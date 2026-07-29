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

from conftest import (
    FIXTURE_PREFIX,
    REPO_ROOT,
    TIER_VOCABULARY,
    independent_cyclonedx_validator,
    run_cli,
    tracked_manifest_paths,
)


def test_tool_declares_non_ci_gating_and_is_absent_from_ci_wiring() -> None:
    """AC-SBOM-19.

    The tool is recurring by design but explicitly NOT CI-gating. It says so
    itself (`--describe`), and that declaration is cross-checked against the
    real wiring — a self-description nobody verifies is worthless.

    `--describe` also publishes the tier vocabulary (§5.9), which is how
    downstream tooling discovers the ruled values without importing the
    package. Asserting only `ci_gating` / `recurring` / `name` left that half
    of the contract ungraded — the command could omit `tiers` entirely, or emit
    the wrong values, and the criterion stayed green (codex §9 round 4). The
    field is therefore required to equal `TIER_VOCABULARY`, in the ruled order.
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
        ' {"name": "sbom-survey", "ci_gating": false, "recurring": true,'
        f' "tiers": {list(TIER_VOCABULARY)}}} and exit 0.',
    )
    assert proc.returncode == 0, f"--describe exited {proc.returncode}: {proc.stderr}"
    described = json.loads(proc.stdout)
    assert described["ci_gating"] is False
    assert described["recurring"] is True
    assert described["name"] == "sbom-survey"

    assert "tiers" in described, (
        "`--describe` published no `tiers` field, so downstream tooling cannot"
        " discover the ruled tier vocabulary without importing the package"
        f" (§5.9). Keys present: {sorted(described)}"
    )
    assert tuple(described["tiers"]) == TIER_VOCABULARY, (
        f"`--describe` published tiers {described['tiers']!r}; §5.9 and"
        f" sbom_survey.TIER_VOCABULARY make it exactly {list(TIER_VOCABULARY)},"
        " in that ruled order (shipped first — it is the one that outranks the"
        " others in Slice 33's triage)"
    )


def test_cli_writes_all_artifacts_and_exits_zero(tmp_path: Path) -> None:
    """AC-SBOM-20.

    The happy path: an offline run over the real repository writes all three
    artifacts into the requested output directory and exits 0.

    The CLI is the only path a real consumer takes, so the ARTIFACT IT WRITES is
    what has to be a CycloneDX 1.6 document. `AC-SBOM-10` grades
    `Survey.to_cyclonedx()` in process; checking only `bomFormat` /
    `specVersion` here left the written file able to be anything that carries
    those two strings. It is therefore put through the same INDEPENDENT
    upstream validator (which `independent_cyclonedx_validator` proves bites,
    on a known-invalid control, before returning).
    """
    out = tmp_path / "out"
    proc = run_cli(
        ["--repo", str(REPO_ROOT), "--offline", "--out", str(out)],
        "AC-SBOM-20",
        "`sbom-survey --repo R --offline --out DIR` must exit 0 and write"
        " sbom.cdx.json, staleness.json and staleness.md into DIR — and the"
        " sbom.cdx.json it writes must itself validate against the CycloneDX"
        " 1.6 schema.",
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

    problem = independent_cyclonedx_validator("AC-SBOM-20")(doc)
    assert problem is None, (
        "the sbom.cdx.json the CLI WROTE fails CycloneDX 1.6 schema validation"
        f" (independent upstream validator): {problem}. Two `bomFormat` /"
        " `specVersion` strings do not make a document an SBOM, and this file —"
        " not the in-process object AC-SBOM-10 grades — is what a consumer"
        " reads."
    )


def test_cli_exits_two_naming_the_untiered_manifest(tmp_path: Path) -> None:
    """AC-SBOM-21.

    REQ-4 at the CLI boundary. A tier map that does not cover a tracked
    manifest must exit 2 and name the offending path on stderr — never exit 0
    with an untagged component.

    REQ-4 requires *an* offending path, not a particular one. Asserting the
    literal `Cargo.toml` (codex §9 round 5) was the inverse of every earlier
    finding: too STRICT rather than too permissive, and so able to reject a
    CORRECT implementation — one that walks discovered paths in `git ls-files`
    order legitimately fails first on `Cargo.lock`, names that, and satisfies
    REQ-4 in full. The oracle is therefore the SET of currently-untiered tracked
    manifests, derived from git via the same helper the rest of the suite uses;
    a second hand-written literal list is exactly what drifted in rounds 1-3.
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
    # The map above assigns NO tier: its single rule excludes the fixture
    # prefix. So every tracked manifest outside that prefix is untiered, and any
    # one of them is a correct thing for the CLI to name.
    untiered = [p for p in tracked_manifest_paths() if not p.startswith(FIXTURE_PREFIX)]
    assert untiered, (
        "precondition changed: every tracked manifest now lies under"
        f" {FIXTURE_PREFIX!r}, so this tier map covers the whole repository and"
        " the criterion cannot be graded"
    )
    assert any(path in proc.stderr for path in untiered), (
        "stderr must NAME an offending untiered manifest path so the fix is"
        f" obvious, but none of the {len(untiered)} tracked manifests this tier"
        f" map leaves untiered appears in it. Expected one of {untiered};"
        f" got:\n{proc.stderr}"
    )
