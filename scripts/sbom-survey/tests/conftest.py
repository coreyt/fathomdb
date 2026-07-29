"""Shared support for the `sbom-survey` acceptance suite (0.8.20 Slice 31).

This suite is **RED by construction**: Slice 31 ships requirements, acceptance
criteria, design and tests; Slice 32 ships the code that turns them GREEN.

The one structural rule this file exists to enforce: **nothing imports
`sbom_survey` at module level.** A top-level import of a package that does not
exist yet raises at collection time, which aborts the whole module and hides
every criterion after the first. Tests call `require()` *inside the test body*
instead, so each acceptance criterion produces its own attributable FAILED with
the required behaviour restated in the message.

A `pytest.skip` would be a vacuous green and is used nowhere in this suite.
"""

from __future__ import annotations

import importlib
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

# <repo>/scripts/sbom-survey/tests/conftest.py
TESTS_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = TESTS_DIR.parent
REPO_ROOT = PROJECT_ROOT.parents[1]

# The mini-project is deliberately isolated (its own pyproject.toml, no
# workspace membership), so the suite must be runnable without installing it.
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

# Basenames the survey recognizes as dependency manifests / lockfiles.
#
# ⚠ THIS TUPLE MIRRORS THE RECOGNIZED-BASENAME TABLE IN THE DESIGN (§5.1) 1:1 AND
# MUST BE KEPT IN LOCKSTEP WITH IT. It is the oracle AC-SBOM-03 grades against:
# a name that is in the design table but missing here is a manifest the tool may
# silently skip while the suite still reports green (codex §9 round 1, fix-1
# finding 1 — `setup.cfg` was exactly that).
#
# Split in two: the first group matches tracked paths TODAY (§5.1's 28-path
# enumeration); the second group currently matches nothing and is present so
# that adding one of those files to the repo is DISCOVERED (and then fails REQ-4
# tiering loudly) rather than silently ignored. `requirements*.txt` is a glob and
# so lives in `tracked_manifest_paths()` below rather than in this tuple.
MANIFEST_BASENAMES = (
    # matched by tracked paths at cbb56212
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "package-lock.json",
    "pyproject.toml",
    "uv.lock",
    "setup.py",
    # recognized, forward-looking: nothing tracked matches these today
    "yarn.lock",
    "pnpm-lock.yaml",
    "poetry.lock",
    "Pipfile",
    "setup.cfg",
)

TIER_VOCABULARY = ("shipped", "dev-tooling", "eval-only")

FIXTURE_PREFIX = "dev/release/fixtures/"

# purl (`bom-ref`) prefix per ecosystem — the component identity an advisory feed
# matches against (REQ-6, design §5.5). Every ecosystem the repo tracks must be
# represented in the BOM, and no fourth purl type may appear.
PURL_PREFIX_BY_ECOSYSTEM = {
    "cargo": "pkg:cargo/",
    "npm": "pkg:npm/",
    "pypi": "pkg:pypi/",
}
PURL_PREFIXES = tuple(PURL_PREFIX_BY_ECOSYSTEM.values())

# Cargo crates present in the tracked `Cargo.lock` that are declared by NO
# dependency table of any tracked, non-excluded `Cargo.toml` — i.e. they exist
# ONLY because a lockfile-derived library<->library edge put them there. If none
# of these reaches the BOM tagged `transitive`, the implementation emitted the
# direct set and dropped the dependency graph (codex §9 round 1, fix-1
# finding 3). Verified lockfile-only at cbb56212; see design §5.5 for the
# drift note (a future direct adoption of one of these is a legitimate reason
# for it to leave this set, and the assertion only needs ONE survivor).
KNOWN_TRANSITIVE_ONLY_CARGO = (
    "proc-macro2",
    "quote",
    "syn",
    "unicode-ident",
    "serde_derive",
)

# The NEGATIVE CONTROL for AC-SBOM-10's independent schema oracle.
#
# CycloneDX 1.6 makes `type` REQUIRED on every component (the 1.6 component
# definition is `"required": ["type", "name"]`), so a conforming validator must
# reject this document. A validator that accepts it is not validating anything,
# and a clean result from it would prove nothing — which is the whole failure
# mode AC-SBOM-10 exists to stop.
KNOWN_INVALID_CYCLONEDX_DOC = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.6",
    "version": 1,
    "components": [{"name": "component-with-no-type", "version": "1.0.0"}],
}


def purl_type(purl: object) -> str | None:
    """The purl `type` segment (`pkg:<type>/…`), or None if it is not a purl."""
    if not isinstance(purl, str) or not purl.startswith("pkg:"):
        return None
    return purl[len("pkg:") :].split("/", 1)[0]


def require(module: str, criterion: str, behaviour: str):
    """Import `module`, or FAIL naming the criterion and the missing behaviour.

    Never skips. A missing implementation is a failed acceptance criterion,
    not an absent test.
    """
    try:
        return importlib.import_module(module)
    except Exception as exc:  # noqa: BLE001 - any import-time failure is a RED result
        pytest.fail(
            f"{criterion} is NOT SATISFIED: `{module}` is not implemented yet"
            " (it lands in 0.8.20 Slice 32).\n"
            f"  REQUIRED BEHAVIOUR: {behaviour}\n"
            f"  import {module!r} raised: {exc!r}"
        )


def require_external(module: str, criterion: str, distribution: str, why: str):
    """Import a THIRD-PARTY oracle module, or FAIL. Never skips.

    Distinct from `require()`: that one imports the code UNDER TEST, and a
    missing module there is the expected Slice-31 RED. This one imports a
    module the test grades WITH, and a missing module there means the criterion
    would go **ungraded**.

    Which is why it must fail rather than skip. `pytest.skip` on a missing
    oracle is the textbook vacuous green: the suite still reports success while
    the assertion it was built for silently stopped running. Every distribution
    reached through this helper is a declared dependency of the mini-project
    (design §5.7), so its absence is a broken environment, not a licence to
    pass.
    """
    try:
        return importlib.import_module(module)
    except Exception as exc:  # noqa: BLE001 - any import failure means "ungraded"
        pytest.fail(
            f"{criterion} CANNOT BE GRADED: the independent oracle `{module}` is"
            " not importable, so the criterion would go UNGRADED.\n"
            f"  INSTALL: {distribution} — a declared dependency of this"
            " mini-project (design §5.7).\n"
            f"  WHY THE ORACLE IS REQUIRED: {why}\n"
            "  This is a FAILURE and never a skip: an oracle allowed to vanish"
            " turns the criterion into a vacuous green.\n"
            f"  import {module!r} raised: {exc!r}"
        )


def independent_cyclonedx_validator(criterion: str):
    """A CycloneDX 1.6 schema validator that does NOT come from `sbom_survey`.

    Returns `validate(doc) -> str | None` — `None` when `doc` is schema-valid,
    a diagnostic string otherwise.

    AC-SBOM-10 exists to stop a hand-rolled document that no consumer will
    validate. Grading it with the implementation's own
    `sbom_survey.cyclonedx.validate()` is **self-certification**: an
    implementation whose `validate()` returns `None` unconditionally would pass
    while emitting invalid JSON (codex §9 round 2, fix-2 finding 1). The oracle
    is therefore the upstream library's own validator, bound to the normative
    1.6 schema shipped by the `cyclonedx-python-lib[json-validation]` extra
    (design §5.7 already declares that dependency for exactly this purpose).
    """
    validation = require_external(
        "cyclonedx.validation.json",
        criterion,
        "cyclonedx-python-lib[json-validation]",
        "AC-SBOM-10 must be graded by a validator INDEPENDENT of the code under"
        " test; the implementation's own validate() cannot certify itself.",
    )
    schema = require_external(
        "cyclonedx.schema",
        criterion,
        "cyclonedx-python-lib[json-validation]",
        "SchemaVersion.V1_6 selects the normative 1.6 schema the emitted"
        " document is graded against.",
    )
    try:
        validator = validation.JsonStrictValidator(schema.SchemaVersion.V1_6)
    except Exception as exc:  # noqa: BLE001 - a validator we cannot build grades nothing
        pytest.fail(
            f"{criterion} CANNOT BE GRADED: cyclonedx-python-lib imported, but its"
            " JSON schema validator could not be constructed — the"
            " `json-validation` extra (the JSON-schema engine plus the bundled"
            " normative schemas) is almost certainly missing. Install"
            " `cyclonedx-python-lib[json-validation]`, per design §5.7."
            f" Raised: {exc!r}"
        )

    def validate(doc) -> str | None:
        problem = validator.validate_str(json.dumps(doc))
        return None if problem is None else str(problem)

    return validate


def run_cli(args: list[str], criterion: str, behaviour: str) -> subprocess.CompletedProcess:
    """Drive the CLI as a subprocess, or FAIL naming the criterion.

    Driving the CLI out-of-process is the second half of the no-module-level-
    import rule: an absent entry point becomes a normal process result rather
    than a collection error.
    """
    env = dict(os.environ)
    env["PYTHONPATH"] = os.pathsep.join(
        [str(PROJECT_ROOT), env.get("PYTHONPATH", "")]
    ).rstrip(os.pathsep)
    proc = subprocess.run(  # noqa: S603 - fixed argv, no shell
        [sys.executable, "-m", "sbom_survey", *args],
        capture_output=True,
        text=True,
        env=env,
        cwd=str(REPO_ROOT),
    )
    if "No module named" in proc.stderr and "sbom_survey" in proc.stderr:
        pytest.fail(
            f"{criterion} is NOT SATISFIED: the `sbom-survey` CLI does not exist yet"
            " (it lands in 0.8.20 Slice 32).\n"
            f"  REQUIRED BEHAVIOUR: {behaviour}\n"
            f"  `python -m sbom_survey {' '.join(args)}` exited {proc.returncode}"
            f" with: {proc.stderr.strip().splitlines()[-1] if proc.stderr.strip() else '<no stderr>'}"
        )
    return proc


def tracked_paths() -> list[str]:
    """Every path tracked in this repository, straight from git."""
    out = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["git", "-C", str(REPO_ROOT), "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [p for p in out.split("\0") if p]


def tracked_manifest_paths() -> list[str]:
    """The tracked paths whose basename is a recognized manifest name."""
    return sorted(
        p
        for p in tracked_paths()
        if Path(p).name in MANIFEST_BASENAMES
        or (Path(p).name.startswith("requirements") and p.endswith(".txt"))
    )


def is_gitignored(path: str) -> bool:
    proc = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["git", "-C", str(REPO_ROOT), "check-ignore", "-q", "--", path],
        capture_output=True,
        text=True,
    )
    return proc.returncode == 0


@pytest.fixture()
def repo_root() -> Path:
    return REPO_ROOT


@pytest.fixture()
def project_root() -> Path:
    return PROJECT_ROOT


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def load_json(path: Path):
    return json.loads(read_text(path))
