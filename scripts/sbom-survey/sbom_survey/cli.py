"""The `sbom-survey` command line (design §5.9).

```text
sbom-survey --repo PATH [--offline | --online] [--out DIR] [--tiers FILE] [--now ISO8601]
sbom-survey --describe
```

| Exit | Meaning |
|------|---------|
| `0`  | survey written |
| `2`  | a tracked manifest has no tier assignment (an offending path on stderr) |
| `3`  | a tracked manifest could not be parsed, or the tier rules could not be read |
| `1`  | unexpected internal error |
| `64` | bad command line (`EX_USAGE`) — NOT one of §5.9's ruled codes, on purpose |

§5.9 rules the first four. `64` is added rather than reusing one of them because
a malformed argument is none of those things, and argparse's own default for a
usage error is `2` — which would collide head-on with "untiered manifest" and
make that signal ambiguous for anyone reading exit codes.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import TIER_VOCABULARY, __version__
from .parse import ManifestParseError
from .paths import DEFAULT_REPORT_DIR
from .registry import HttpRegistrySource, OfflineSource
from .report import write_reports
from .survey import run_survey
from .tiers import TierRuleFileError, UntieredManifestError, load_tier_map
from .util import TimestampFormatError, normalize_timestamp

__all__ = ["build_parser", "console_main", "describe", "main"]

EXIT_OK = 0
EXIT_INTERNAL = 1
EXIT_UNTIERED = 2
EXIT_UNPARSEABLE = 3

#: A BAD COMMAND LINE, and deliberately NOT one of §5.9's four ruled codes.
#:
#: §5.9 fixes 0 = written, 2 = untiered manifest, 3 = unparseable manifest,
#: 1 = unexpected internal error. A malformed argument is none of those, and
#: argparse's own default for a usage error is 2 — which would collide head-on
#: with "a tracked manifest has no tier assignment" and make `AC-SBOM-21`'s
#: signal ambiguous for any consumer reading exit codes.
#:
#: 64 is `EX_USAGE` from BSD `sysexits.h`, the long-standing convention for
#: exactly this case: outside the ruled set, so it cannot overload a ruled
#: meaning, and self-documenting rather than arbitrary. `_Parser` below routes
#: EVERY argparse usage error here, so the CLI is internally consistent — an
#: unknown flag and a malformed `--now` report the same way.
EXIT_USAGE = 64


class _Parser(argparse.ArgumentParser):
    """`argparse.ArgumentParser` whose usage errors exit `EXIT_USAGE`, not 2."""

    def error(self, message: str):  # noqa: D102 - argparse contract
        self.print_usage(sys.stderr)
        print(f"{self.prog}: error: {message}", file=sys.stderr)
        raise SystemExit(EXIT_USAGE)


def describe() -> dict:
    """The machine-readable "this is informational" declaration (AC-SBOM-19).

    `tiers` publishes the ruled vocabulary IN THE RULED ORDER so downstream
    tooling can discover it without importing this package.
    """
    return {
        "name": "sbom-survey",
        "version": __version__,
        "ci_gating": False,
        "recurring": True,
        "tiers": list(TIER_VOCABULARY),
    }


def build_parser() -> argparse.ArgumentParser:
    parser = _Parser(
        prog="sbom-survey",
        description=(
            "CycloneDX 1.6 dependency survey over the manifests tracked on main."
            " Informational and NOT CI-gating."
        ),
    )
    parser.add_argument("--repo", default=".", help="repository root to survey")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--offline",
        action="store_true",
        help="do not consult any registry; every row degrades to `unknown`",
    )
    mode.add_argument(
        "--online",
        action="store_true",
        help="consult crates.io / the npm registry / PyPI for published versions",
    )
    parser.add_argument("--out", default=None, help=f"output directory (default: {DEFAULT_REPORT_DIR})")
    parser.add_argument("--tiers", default=None, help="tier rule file (default: the tracked tiers.toml)")
    # DELIBERATELY `None`, NEVER a wall-clock stamp. `None` defers to
    # `survey.resolve_timestamp()`, which is the SAME function `run_survey` uses
    # for its in-process default, so the CLI default and the in-process default
    # are equal by construction and `SOURCE_DATE_EPOCH` keeps working. An
    # argparse default of `datetime.now()` here would sail straight past an
    # in-process determinism test, which is why §5.8 grades this path
    # separately.
    parser.add_argument("--now", default=None, help="ISO-8601 timestamp (default: a FIXED epoch)")
    parser.add_argument(
        "--describe",
        action="store_true",
        help="print the tool's self-description as JSON and exit",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    # Validate the advertised `--now ISO8601` contract HERE, before any work,
    # so a typo is a usage error rather than a survey that runs to completion
    # and writes artifacts disagreeing about when it happened.
    if args.now is not None:
        try:
            normalize_timestamp(args.now, source="--now")
        except TimestampFormatError as exc:
            parser.error(str(exc))

    if args.describe:
        print(json.dumps(describe(), indent=2, sort_keys=True))
        return EXIT_OK

    repo_root = Path(args.repo).resolve()
    out_dir = Path(args.out) if args.out else repo_root / DEFAULT_REPORT_DIR

    published = OfflineSource() if not args.online else HttpRegistrySource()

    try:
        tier_map = load_tier_map(args.tiers) if args.tiers else None
    except TierRuleFileError as exc:
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE
    except OSError as exc:
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE

    try:
        survey = run_survey(
            repo_root,
            published=published,
            tier_map=tier_map,
            now=args.now,
        )
    except UntieredManifestError as exc:
        # REQ-4 at the CLI boundary: name AN offending path so the fix is
        # obvious, and never exit 0 with an untagged component.
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNTIERED
    except TierRuleFileError as exc:
        # The DEFAULT tier-rule file is resolved from `--repo`, so this is
        # reachable without `--tiers` and must not fall through to the bare
        # `Exception` handler below: an unreadable rule file is a legible input
        # problem, not an internal error (codex §9 round 2 [P1]).
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE
    except TimestampFormatError as exc:
        # Reachable via SOURCE_DATE_EPOCH, which `--now` validation above does
        # not cover. Same door, same code.
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_USAGE
    except ManifestParseError as exc:
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE
    except Exception as exc:  # noqa: BLE001 - anything else is an internal error
        print(f"sbom-survey: unexpected internal error: {exc!r}", file=sys.stderr)
        return EXIT_INTERNAL

    written = write_reports(survey, out_dir)
    summary = survey.summary()
    print(
        f"sbom-survey: {summary['components']} components"
        f" ({summary['direct']} direct, {summary['transitive']} transitive),"
        f" {summary['unknown']} unknown, {summary['excluded_manifests']} manifests excluded"
    )
    for path in written:
        print(f"  wrote {path}")
    return EXIT_OK


def console_main() -> int:  # pragma: no cover - console-script shim
    return main(sys.argv[1:])
