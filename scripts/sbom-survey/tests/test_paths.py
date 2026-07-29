"""AC-SBOM-18 — gitignored reports, tracked findings home.

REQ-11. Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.6.
"""

from __future__ import annotations

from conftest import REPO_ROOT, is_gitignored, require


def test_report_dir_is_gitignored_and_findings_home_is_tracked() -> None:
    """AC-SBOM-18.

    Generated reports are gitignored (HITL-ruled). Slice 33's *findings*
    therefore need a separate, deliberately NOT-ignored durable home — the raw
    tool output is not one.
    """
    paths = require(
        "sbom_survey.paths",
        "AC-SBOM-18",
        "sbom_survey.paths must expose DEFAULT_REPORT_DIR (repo-relative,"
        " GITIGNORED — Slice 32 adds the `scripts/sbom-survey/out/` rule) and"
        " SLICE_33_FINDINGS_DOC (repo-relative, NOT ignored — the tracked"
        " durable home for the survey findings).",
    )

    report_dir = paths.DEFAULT_REPORT_DIR
    findings_doc = paths.SLICE_33_FINDINGS_DOC

    assert report_dir == "scripts/sbom-survey/out"
    assert findings_doc == "dev/design/0.8.20-slice-33-library-sweep-3-findings.md"

    probe = f"{report_dir}/sbom.cdx.json"
    assert is_gitignored(probe), (
        f"{probe} is NOT gitignored — generated reports must never be"
        " committable (add `scripts/sbom-survey/out/` to .gitignore)"
    )
    assert not is_gitignored(findings_doc), (
        f"{findings_doc} is gitignored — it is the TRACKED durable home for"
        " Slice 33's findings and must be committable"
    )
    assert (REPO_ROOT / "dev" / "design").is_dir()
