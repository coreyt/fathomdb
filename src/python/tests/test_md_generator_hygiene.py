"""Anti-regression guard: markdown-EMITTING tools must produce gate-compliant output.

Several eval/tooling scripts regenerate `.md` reports. Their output paths live under
`dev/plans/runs/**`, which `.markdownlint-cli2.jsonc` *ignores* — so the normal
`scripts/agent-lint-md.sh` gate never sees a regenerated report. This test closes that
gap: it drives each Python markdown generator with synthetic-but-structurally-faithful
inputs and lints the emitted markdown through `markdownlint-cli2` using the repo's
`.markdownlint.jsonc` rule set. A generator that re-introduces debt (a fence with no
language, a heading/list/table without surrounding blanks, a bare URL, a stray double
blank, a blank line inside a blockquote, ...) fails here.

Covers (end-to-end, real render functions):
  - scripts/perf-experiments/aggregate.py  -> render()
  - src/python/eval/m1_verdict_run.py       -> write_report()  (valid + invalid paths)
  - src/python/eval/s15a_embedder_probe.py  -> write_outputs() (pass + failed-candidate)

Does NOT cover:
  - The two shell generators (scripts/repo-prune/bin/{context,memory}-clarity.sh): they
    require a live repo + a (non-tracked) memory dir to run, so they are linted via
    committed output fixtures by scripts/tests/test_md_generators.sh instead.
  - Arbitrary free-text fields the generators interpolate (e.g. a verdict `rationale`):
    we can only guarantee the *frame* the generator emits is compliant, not caller data
    that happens to contain raw markdown.

Skips (does not fail) only when `markdownlint-cli2` is genuinely absent (local dev without
`scripts/bootstrap.sh`). CI installs node deps, so the gate is real there.
"""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

# --------------------------------------------------------------------------- #
# Locate the repo root, the rule config, and the markdownlint-cli2 binary.
# --------------------------------------------------------------------------- #


def _repo_root() -> Path:
    p = Path(__file__).resolve()
    for parent in p.parents:
        if (parent / ".markdownlint.jsonc").is_file():
            return parent
    raise RuntimeError("could not locate repo root (.markdownlint.jsonc)")


ROOT = _repo_root()
CONFIG = ROOT / ".markdownlint.jsonc"


def _find_markdownlint() -> str | None:
    # Repo node_modules, then the sibling main checkout (worktree case), then PATH.
    candidates = [
        ROOT / "node_modules" / ".bin" / "markdownlint-cli2",
        ROOT.parent / "fathomdb" / "node_modules" / ".bin" / "markdownlint-cli2",
        Path("/home/coreyt/projects/fathomdb/node_modules/.bin/markdownlint-cli2"),
    ]
    for c in candidates:
        if c.is_file():
            return str(c)
    found = shutil.which("markdownlint-cli2")
    return found


BIN = _find_markdownlint()

requires_linter = pytest.mark.skipif(
    BIN is None,
    reason="markdownlint-cli2 not installed (run scripts/bootstrap.sh) — gate is real in CI",
)


def _lint(md_path: Path) -> None:
    """Lint a single file with the repo rules; fail with the findings on any violation.

    `md_path` lives under pytest's tmp dir (outside the repo), so markdownlint-cli2 does
    not auto-discover `.markdownlint-cli2.jsonc` (whose globs/ignores would otherwise
    re-scope the run); we pass the rule set explicitly via `--config`.
    """
    assert BIN is not None
    proc = subprocess.run(
        [BIN, "--config", str(CONFIG), str(md_path)],
        cwd=str(md_path.parent),
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise AssertionError(
            f"markdownlint-cli2 flagged generator output {md_path.name}:\n"
            + (proc.stdout or "")
            + (proc.stderr or "")
        )


# --------------------------------------------------------------------------- #
# Synthetic, structurally-faithful generator inputs.
# --------------------------------------------------------------------------- #


def _perf_records() -> list[dict]:
    return [
        {
            "experiment_id": "EXP-1",
            "lever_id": "L1",
            "verdict": "GO",
            "canonical_ci": {
                "workflow_url": "https://github.com/x/y/actions/runs/123",
                "branch": "main",
                "head_sha": "abcdef1234",
                "results": {"ac012": {"p50_ms": 1, "p99_ms": 2}},
            },
            "dev_box_pre_screen": {"results": {"ac012": {"p50_ms": 1, "p99_ms": 2, "n": 5}}},
        },
        {"experiment_id": "EXP-2", "lever_id": "L2", "verdict": "PENDING"},
    ]


def _m1_art(*, run_valid: bool) -> dict:
    per_hop = {
        h: {
            "n": 10,
            "f1_delta": 0.01,
            "f1_ci_low": -0.02,
            "f1_ci_high": 0.04,
            "em_delta": 0.0,
        }
        for h in ("2", "3", "4")
    }
    arms = ("bm25", "passage_dense", "fused", "fused_rerank", "ppr_fusion")
    art: dict = {
        "run_valid": run_valid,
        "n_questions": 300,
        "musique_hash": "deadbeef",
        "reader_model": "test-reader",
        "ppr_divergence": {
            "n_ppr_differs_from_bm25_topk": 120,
            "n_questions": 300,
            "fraction_differs": 0.4,
        },
        "five_arm_pooled_ge3hop": {a: {"f1": 0.4, "em": 0.2, "n": 144} for a in arms},
        "primary_endpoint": {
            "comparator_arm": "fused",
            "treatment_arm": "ppr_fusion",
            "n_boot": 1000,
            "pooled_ge3hop": {
                "f1_delta": -0.04,
                "f1_ci_low": -0.08,
                "f1_ci_high": 0.0,
                "n": 144,
                "em_delta": -0.01,
                "em_ci_low": -0.03,
                "em_ci_high": 0.01,
            },
            "per_hop": per_hop,
            "trend": {"slope": -0.01, "neg_significant": False},
        },
        "cost": {
            "model": "test-reader",
            "n_calls": 1500,
            "n_errors": 0,
            "prompt_tokens": 1000,
            "completion_tokens": 500,
            "usd": 9.5,
        },
        "decide_inputs": {"delta": -0.04, "power": "underpowered"},
        "verdict": "NO_GO",
        "power_status": "UNDERPOWERED",
        "decision_rule_note": "Power gate vetoes at N<1165.",
        "confident_wrong_status": "UNEVALUATED (no unanswerable contrast set).",
        "stage2_recommendation": {
            "recommendation": "DO NOT run stage 2",
            "run_stage2": False,
            "rationale": "The effect size is negative; stage 2 is not warranted.",
        },
    }
    if not run_valid:
        art["answer_completeness"] = {
            "completeness": "0.50",
            "n_errors": 750,
            "expected_calls": 1500,
        }
    return art


def _s15a_result() -> dict:
    return {
        "smoke": False,
        "corpus_hash": "cafef00d",
        "corpus_resolved_count": 500,
        "qrels_version": "v1",
        "hard_subset": {"cap": 100, "count": 80, "k1": 0.9, "b": 0.4, "tokenizer": "unicode"},
        "base": {"name": "bge-small", "eu8": 0.55, "hard_r@10": 0.40},
        "per_candidate": {
            "cand-good": {
                "eu8": 0.57,
                "hard": {"r@10": 0.42},
                "projected_eu7": 0.91,
                "projected_eu7_ci": {"lo": 0.88, "hi": 0.94},
                "eu8_margin_ci": {"lo": 0.01},
                "hard_margin_ci": {"lo": 0.005},
                "cpu_latency": {"feasible": True},
                "in_library_feasible": True,
                "probe_15a_pass": True,
            },
            "cand-bad": {
                "measurement_status": "failed",
                "in_library_feasible": False,
            },
        },
        "measurement_failures": [{"name": "cand-bad", "error": "timeout loading weights"}],
        "chosen_embedder": "cand-good",
        "no_swap": False,
        "ranking": ["cand-good", "base"],
        "surpass_flag": True,
        "stage_split": "Stage split: candidate clears the projected-eu7 floor.",
        "caveats": ["projected_eu7 is a transparency CI", "smoke mode disabled"],
    }


# --------------------------------------------------------------------------- #
# Tests.
# --------------------------------------------------------------------------- #


@requires_linter
def test_aggregate_output_is_gate_compliant(tmp_path: Path) -> None:
    import sys

    sys.path.insert(0, str(ROOT / "scripts" / "perf-experiments"))
    import aggregate  # type: ignore[import-not-found]

    out = tmp_path / "perf-results.md"
    out.write_text(aggregate.render(_perf_records()), encoding="utf-8")
    _lint(out)


@requires_linter
@pytest.mark.parametrize("run_valid", [True, False], ids=["valid", "invalid"])
def test_m1_verdict_report_is_gate_compliant(tmp_path: Path, run_valid: bool) -> None:
    from eval.m1_verdict_run import write_report

    out = tmp_path / "m1-report.md"
    write_report(_m1_art(run_valid=run_valid), out)
    _lint(out)


@requires_linter
def test_s15a_report_is_gate_compliant(tmp_path: Path) -> None:
    from eval.s15a_embedder_probe import write_outputs

    out_md = tmp_path / "s15a-report.md"
    out_json = tmp_path / "s15a-report.json"
    write_outputs(_s15a_result(), out_json=out_json, out_md=out_md)
    # sanity: json sidecar parses (the generator writes both)
    json.loads(out_json.read_text(encoding="utf-8"))
    _lint(out_md)
