"""Well-known repo-relative paths and the fixed default timestamp (design §5.6, §5.8)."""

from __future__ import annotations

from pathlib import Path

__all__ = [
    "DEFAULT_EPOCH_TIMESTAMP",
    "DEFAULT_REPORT_DIR",
    "DEFAULT_TIERS_FILE",
    "PROJECT_ROOT",
    "SLICE_33_FINDINGS_DOC",
]

#: `<repo>/scripts/sbom-survey` — this mini-project's own root.
PROJECT_ROOT = Path(__file__).resolve().parent.parent

#: The tracked tier/exclusion data (§5.3). Rules are DATA, never code.
DEFAULT_TIERS_FILE = PROJECT_ROOT / "tiers.toml"

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
