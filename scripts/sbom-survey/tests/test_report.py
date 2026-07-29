"""AC-SBOM-22 — the Slice-33 consumer contract.

REQ-13 (determinism) and REQ-14 (consumer fields).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.8.

Slice 33 answers exactly two questions and stops: what is stale, and would a
surgical ~1-5 SLOC change likely land it. This report must let it answer both
without re-deriving anything. Nothing in Slice 33 is built here.
"""

from __future__ import annotations

from pathlib import Path

from conftest import REPO_ROOT, require

SLICE_33_ROW_FIELDS = {
    "ecosystem",
    "name",
    "tier",
    "depth",
    "locked_version",
    "latest_version",
    "status",
    "lookup_error",
    "declared_in",
    "edit_sites",
    "edit_site_count",
}


def test_staleness_rows_carry_slice_33_fields_and_are_deterministic(tmp_path: Path) -> None:
    """AC-SBOM-22."""
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-22",
        "each staleness row must carry exactly the Slice-33 consumer fields"
        f" {sorted(SLICE_33_ROW_FIELDS)}, sorted by"
        " (tier, ecosystem, name, locked_version); and two runs over identical"
        " inputs must write BYTE-IDENTICAL artifacts (UUIDv5 serialNumber, no"
        " wall-clock timestamp) so a re-run diffs to nothing.",
    )
    registry = require("sbom_survey.registry", "AC-SBOM-22", "OfflineSource keeps the run hermetic.")
    report = require(
        "sbom_survey.report",
        "AC-SBOM-22",
        "sbom_survey.report.write_reports(survey, out_dir) must emit"
        " sbom.cdx.json, staleness.json and staleness.md deterministically.",
    )

    def once(out: Path) -> dict[str, bytes]:
        survey = survey_mod.run_survey(
            REPO_ROOT, published=registry.OfflineSource(), now="1970-01-01T00:00:00Z"
        )
        report.write_reports(survey, out)
        return {p.name: p.read_bytes() for p in sorted(out.iterdir())}

    first = once(tmp_path / "a")
    second = once(tmp_path / "b")
    assert first.keys() == second.keys()
    for name in first:
        assert first[name] == second[name], (
            f"{name} is not byte-identical across two runs — the report is"
            " non-deterministic, so a recurring re-run cannot diff cleanly"
        )

    survey = survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())
    rows = [r.as_dict() for r in survey.staleness()]
    assert rows, "vacuous-pass guard: zero staleness rows"

    for row in rows:
        assert set(row) == SLICE_33_ROW_FIELDS, (
            "staleness row field set drifted from the Slice-33 contract:"
            f" missing={sorted(SLICE_33_ROW_FIELDS - set(row))}"
            f" unexpected={sorted(set(row) - SLICE_33_ROW_FIELDS)}"
        )
        assert isinstance(row["edit_sites"], list)
        assert row["edit_site_count"] == len(row["edit_sites"])
        for site in row["edit_sites"]:
            assert not site.startswith("/"), "edit_sites must be repo-relative"

    keys = [(r["tier"], r["ecosystem"], r["name"], r["locked_version"] or "") for r in rows]
    assert keys == sorted(keys), "staleness rows are not deterministically sorted"
