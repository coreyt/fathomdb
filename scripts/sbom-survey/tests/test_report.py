"""AC-SBOM-22 — the Slice-33 consumer contract.

REQ-13 (determinism) and REQ-14 (consumer fields).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.8.

Slice 33 answers exactly two questions and stops: what is stale, and would a
surgical ~1-5 SLOC change likely land it. This report must let it answer both
without re-deriving anything. Nothing in Slice 33 is built here.
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

import pytest

from conftest import REPO_ROOT, require, run_cli

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

# A default-path timestamp may not be wall-clock. §5.8 rules the default a FIXED
# epoch, so any stamp within a day of the real clock is a wall-clock default.
# The window is deliberately generous: it must not be tripped by a slow run, a
# timezone slip or a machine whose clock is a few hours out, only by an
# implementation that actually asks the operating system what time it is.
WALL_CLOCK_WINDOW_SECONDS = 24 * 60 * 60


def _assert_identical(first: dict[str, bytes], second: dict[str, bytes], what: str) -> None:
    assert first and second, f"no artifacts were written {what}"
    assert first.keys() == second.keys(), (
        f"a different artifact set was written {what}: {sorted(first)} vs {sorted(second)}"
    )
    for name in first:
        assert first[name] == second[name], (
            f"{name} is not byte-identical {what} — the report is"
            " non-deterministic, so a recurring re-run cannot diff cleanly"
        )


def test_staleness_rows_carry_slice_33_fields_and_are_deterministic(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """AC-SBOM-22.

    REQ-13 is about ORDINARY repeated runs, so the DEFAULT timestamp path is the
    one that has to be covered. Pinning `now` on both runs — all this test used
    to do — leaves an implementation that falls back to wall-clock whenever
    `now` / `--now` is omitted passing (codex §9 round 2, fix-2 finding 3).
    Three determinism legs are therefore run: pinned `now`, the in-process
    default, and the CLI default (argparse has a default of its own and could
    hand `run_survey` a wall-clock `now` explicitly, bypassing the other two).
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-22",
        "each staleness row must carry exactly the Slice-33 consumer fields"
        f" {sorted(SLICE_33_ROW_FIELDS)}, sorted by"
        " (tier, ecosystem, name, locked_version); and two runs over identical"
        " inputs must write BYTE-IDENTICAL artifacts — WITH and WITHOUT an"
        " explicit `now`, in-process and through the CLI (UUIDv5 serialNumber,"
        " a FIXED default epoch, no wall-clock timestamp) so a re-run diffs to"
        " nothing.",
    )
    registry = require("sbom_survey.registry", "AC-SBOM-22", "OfflineSource keeps the run hermetic.")
    report = require(
        "sbom_survey.report",
        "AC-SBOM-22",
        "sbom_survey.report.write_reports(survey, out_dir) must emit"
        " sbom.cdx.json, staleness.json and staleness.md deterministically.",
    )

    # Cleared so the PURE built-in default is exercised, not an environment
    # override — and so the CLI subprocesses below inherit the same posture.
    monkeypatch.delenv("SOURCE_DATE_EPOCH", raising=False)

    def once(out: Path, **kwargs) -> dict[str, bytes]:
        survey = survey_mod.run_survey(
            REPO_ROOT, published=registry.OfflineSource(), **kwargs
        )
        report.write_reports(survey, out)
        return {p.name: p.read_bytes() for p in sorted(out.iterdir())}

    # Leg 1 — an explicit, pinned `now`.
    pinned = "1970-01-01T00:00:00Z"
    _assert_identical(
        once(tmp_path / "a", now=pinned),
        once(tmp_path / "b", now=pinned),
        "across two runs with an explicit `now`",
    )

    # Leg 2 — the DEFAULT path, which is what an ordinary re-run takes.
    default_first = once(tmp_path / "c")
    _assert_identical(
        default_first,
        once(tmp_path / "d"),
        "across two DEFAULT runs (no explicit `now`)",
    )

    # Byte-equality alone does not settle leg 2: two back-to-back runs can share
    # a wall-clock second and look identical by luck. The default stamp must
    # therefore be shown NOT to be wall-clock at all.
    stamp = json.loads(default_first["sbom.cdx.json"])["metadata"]["timestamp"]
    parsed = datetime.fromisoformat(str(stamp).replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    drift = abs((datetime.now(timezone.utc) - parsed).total_seconds())
    assert drift > WALL_CLOCK_WINDOW_SECONDS, (
        f"the DEFAULT run stamped metadata.timestamp {stamp!r}, which is within"
        " a day of the real clock. §5.8 rules the default a FIXED epoch"
        " (SOURCE_DATE_EPOCH or an explicit --now are the only overrides); a"
        " wall-clock default makes every re-run diff, and two runs inside the"
        " same second would still compare byte-identical, so the equality check"
        " above cannot catch it on its own."
    )

    # Leg 3 — the CLI default. `--now` could be defaulted to the current time in
    # the argument parser and passed explicitly to run_survey, which would sail
    # past both legs above.
    cli_runs: list[dict[str, bytes]] = []
    for name in ("cli-a", "cli-b"):
        out = tmp_path / name
        proc = run_cli(
            ["--repo", str(REPO_ROOT), "--offline", "--out", str(out)],
            "AC-SBOM-22",
            "two `sbom-survey --repo R --offline --out DIR` runs with NO --now"
            " must write byte-identical artifacts, and must stamp the same"
            " fixed default timestamp run_survey() uses in-process.",
        )
        assert proc.returncode == 0, (
            f"offline CLI run exited {proc.returncode}\nstderr:\n{proc.stderr}"
        )
        cli_runs.append({p.name: p.read_bytes() for p in sorted(out.iterdir())})

    _assert_identical(*cli_runs, what="across two CLI runs with no --now")

    cli_stamp = json.loads(cli_runs[0]["sbom.cdx.json"])["metadata"]["timestamp"]
    assert cli_stamp == stamp, (
        f"the CLI stamped {cli_stamp!r} where run_survey()'s own default is"
        f" {stamp!r} — `--now` is being defaulted in the argument parser, which"
        " bypasses the deterministic default proved above"
    )

    # --- the Slice-33 consumer field set ------------------------------------
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
