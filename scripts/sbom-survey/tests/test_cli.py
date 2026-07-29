"""AC-SBOM-19 .. AC-SBOM-21 — the CLI contract.

REQ-4 (loud gaps, CLI surface) and REQ-12 (not CI-gating).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.9.

Every test here drives the tool OUT OF PROCESS. That is deliberate: an absent
entry point becomes an ordinary process result instead of a collection error,
so each criterion still reports its own FAILED.
"""

from __future__ import annotations

import json
from pathlib import Path

from conftest import REPO_ROOT, run_cli


def test_tool_declares_non_ci_gating_and_is_absent_from_ci_wiring() -> None:
    """AC-SBOM-19.

    The tool is recurring by design but explicitly NOT CI-gating. It says so
    itself (`--describe`), and that declaration is cross-checked against the
    real wiring — a self-description nobody verifies is worthless.
    """
    for wiring in ("scripts/agent-test.sh", ".github/workflows/ci.yml"):
        text = (REPO_ROOT / wiring).read_text(encoding="utf-8")
        assert "sbom-survey" not in text and "sbom_survey" not in text, (
            f"{wiring} references the survey tool. It is informational and must"
            " NOT gate CI (plan-0.8.20.md §3a)."
        )

    proc = run_cli(
        ["--describe"],
        "AC-SBOM-19",
        "`sbom-survey --describe` must print JSON declaring"
        ' {"ci_gating": false, "recurring": true} plus the tier vocabulary,'
        " and exit 0.",
    )
    assert proc.returncode == 0, f"--describe exited {proc.returncode}: {proc.stderr}"
    described = json.loads(proc.stdout)
    assert described["ci_gating"] is False
    assert described["recurring"] is True
    assert described["name"] == "sbom-survey"


def test_cli_writes_all_artifacts_and_exits_zero(tmp_path: Path) -> None:
    """AC-SBOM-20.

    The happy path: an offline run over the real repository writes all three
    artifacts into the requested output directory and exits 0.
    """
    out = tmp_path / "out"
    proc = run_cli(
        ["--repo", str(REPO_ROOT), "--offline", "--out", str(out)],
        "AC-SBOM-20",
        "`sbom-survey --repo R --offline --out DIR` must exit 0 and write"
        " sbom.cdx.json, staleness.json and staleness.md into DIR.",
    )
    assert proc.returncode == 0, (
        f"offline survey exited {proc.returncode}\nstdout:\n{proc.stdout}\n"
        f"stderr:\n{proc.stderr}"
    )
    for artifact in ("sbom.cdx.json", "staleness.json", "staleness.md"):
        assert (out / artifact).is_file(), f"{artifact} was not written to {out}"

    doc = json.loads((out / "sbom.cdx.json").read_text(encoding="utf-8"))
    assert doc["bomFormat"] == "CycloneDX"
    assert doc["specVersion"] == "1.6"


def test_cli_exits_two_naming_the_untiered_manifest(tmp_path: Path) -> None:
    """AC-SBOM-21.

    REQ-4 at the CLI boundary. A tier map that does not cover a tracked
    manifest must exit 2 and name the offending path on stderr — never exit 0
    with an untagged component.
    """
    incomplete = tmp_path / "tiers.toml"
    incomplete.write_text(
        "schema = 1\n\n"
        "[[rule]]\n"
        'prefix = "dev/release/fixtures/"\n'
        'action = "exclude"\n'
        'reason = "fixture"\n',
        encoding="utf-8",
    )

    out = tmp_path / "out"
    proc = run_cli(
        [
            "--repo",
            str(REPO_ROOT),
            "--offline",
            "--out",
            str(out),
            "--tiers",
            str(incomplete),
        ],
        "AC-SBOM-21",
        "with a tier map that covers no real manifest, the CLI must exit 2 and"
        " NAME the untiered path on stderr (never exit 0, never emit an"
        " untagged component).",
    )
    assert proc.returncode == 2, (
        f"expected exit 2 for an untiered manifest, got {proc.returncode}\n"
        f"stderr:\n{proc.stderr}"
    )
    assert "Cargo.toml" in proc.stderr, (
        "stderr must name the offending manifest path so the fix is obvious;"
        f" got:\n{proc.stderr}"
    )
