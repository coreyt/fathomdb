"""Well-known repo-relative paths and the fixed default timestamp (design §5.6, §5.8)."""

from __future__ import annotations

from pathlib import Path

__all__ = [
    "DEFAULT_EPOCH_TIMESTAMP",
    "DEFAULT_REPORT_DIR",
    "SLICE_33_FINDINGS_DOC",
    "TIERS_RELPATH",
    "tiers_file_for",
]

#: Where the tracked tier/exclusion data lives, RELATIVE TO THE SURVEYED
#: REPOSITORY (§5.3). Rules are DATA, never code.
TIERS_RELPATH = "scripts/sbom-survey/tiers.toml"


def tiers_file_for(repo_root: Path | str) -> Path:
    """The tier rules for `repo_root`.

    RESOLVED AGAINST THE SURVEYED REPOSITORY, NOT AGAINST THE INSTALLED PACKAGE.
    This used to be `Path(__file__).parent.parent / "tiers.toml"`, which is the
    source tree when the tool is run from a checkout and
    `site-packages/sbom_survey/` after a non-editable `pip install` — where the
    file does not exist, because `pyproject.toml` declares no package data. The
    CLI then died with a bare `FileNotFoundError` on the very first command of
    the TC-111 install-then-run flow Slice 33 has to use (codex §9 round 2
    `[P1]`).

    Deriving from `repo_root` is the correct fix rather than merely a working
    one, for three reasons:

    1. **The rules are data ABOUT a repository, not about this tool.** Every
       rule is a path prefix into the surveyed tree — `src/rust/crates/`,
       `dev/tools/`, `Cargo.`. A copy baked into a wheel describes whichever
       repository the wheel was built from, which is meaningless (and silently
       wrong) when surveying a different one.
    2. **Otherwise the oracle would stop grading the file the tool uses.**
       `AC-SBOM-08` and `AC-SBOM-11` load the TRACKED `tiers.toml` and compare it
       against the document `run_survey()` produced with no `tier_map`. If the
       default came from the installed package those two could diverge, and the
       criteria would be grading a file the survey never read — exactly the
       boundary class (TC-105) this slice has been fighting throughout. Resolved
       from `repo_root`, they are byte-for-byte the same file.
    3. **A packaged copy can go stale.** Editing the tracked rules without
       reinstalling would silently re-tier every component, which is the event
       REQ-4 exists to make loud.

    The cost, stated rather than hidden: surveying a repository that does not
    track this file now REQUIRES an explicit `--tiers`. That is correct, not
    unfortunate — §5.3 rules that there is deliberately NO catch-all rule, so
    inventing a default rule set for an unknown repository would be precisely
    the silent mis-tag REQ-4 forbids.
    """
    return Path(repo_root) / TIERS_RELPATH

#: Generated reports land here, and this directory is GITIGNORED (REQ-11).
#: Repo-relative on purpose: it is compared against `git check-ignore`.
DEFAULT_REPORT_DIR = "scripts/sbom-survey/out"

#: Slice 33's findings — the TRACKED durable home, deliberately NOT ignored.
#: `dev/plans/runs/` is the house convention for a dated run report, weighed
#: against `dev/design/` and `dev/deps/` in design §5.6.
SLICE_33_FINDINGS_DOC = "dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md"

#: The FIXED default `metadata.timestamp` (§5.8, REQ-13).
#:
#: It is deliberately NOT wall-clock. A wall-clock default makes every re-run of
#: a *recurring* tool diff against the previous one, which destroys the only
#: property that makes re-running it useful. The only overrides are an explicit
#: `now` / `--now` and `SOURCE_DATE_EPOCH`; there is no code path anywhere in
#: this package that calls `datetime.now()` to produce an artifact timestamp.
DEFAULT_EPOCH_TIMESTAMP = "1980-01-01T00:00:00+00:00"
