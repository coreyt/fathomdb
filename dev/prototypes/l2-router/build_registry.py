#!/usr/bin/env python3
"""Build the L2-router per-intent tuple REGISTRY (the 0.8.15 dispatcher pre-stage
artifact, R-L2-2 / DP-B) from the committed 0.8.11 experiment outputs.

CALLER-SIDE, $0, no LLM, no engine import. Reads:
  - dev/plans/runs/expb-joint-tune-output.json  (EXP-B' per-intent tuples + B'.5 guard)
  - dev/plans/runs/gate2-oracle-output.json     (per-arm cost tiers)

Emits:
  - dev/prototypes/l2-router/registry.json

The registry carries provenance HONESTLY: each intent is tagged measured vs
provisional, and a confidence header states this is 0.8.11 *screening* data, NOT
validated config; 0.8.15 must re-validate. Re-run after the inputs change.
"""
from __future__ import annotations

import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
RUNS = REPO / "dev" / "plans" / "runs"
EXPB = RUNS / "expb-joint-tune-output.json"
GATE2 = RUNS / "gate2-oracle-output.json"
OUT = HERE / "registry.json"

# Per-arm cost-tier rank -> bucketed cost_tier in {low, medium, high} (contract §4).
_RANK = {"low": 0, "low-medium": 1, "medium": 2, "high": 3}


def _arm_rank(tier_str: str) -> int:
    # gate2 tiers can carry trailing prose (e.g. "low-compute / ~0-value"); take
    # the leading token before any space/slash.
    head = tier_str.split("/")[0].split()[0].strip()
    return _RANK.get(head, 0)


def _bucket(rank: int) -> str:
    if rank <= 0:
        return "low"
    if rank >= 3:
        return "high"
    return "medium"


def stack_cost_tier(stack: list[str], per_arm: dict) -> tuple[str, dict]:
    """cost_tier = bucket(max per-arm rank over the stack's operators)."""
    arms = {}
    worst = 0
    for op in stack:
        info = per_arm.get(op)
        if info is None:
            # operator with no measured tier (e.g. community_summary rides with
            # map_reduce_qfs as an LLM-tier sensemaking op) -> inherit high.
            arms[op] = {"tier": "high", "evidence": "LLM-tier sensemaking op (no separate measurement; rides map_reduce_qfs)"}
            worst = max(worst, _RANK["high"])
            continue
        arms[op] = {"tier": info["tier"], "evidence": info["evidence"]}
        worst = max(worst, _arm_rank(info["tier"]))
    return _bucket(worst), arms


def main() -> None:
    expb = json.loads(EXPB.read_text(encoding="utf-8"))
    gate2 = json.loads(GATE2.read_text(encoding="utf-8"))
    per_arm = gate2["per_arm_cost_tiers"]

    measured = set(expb["intents_measured"])
    provisional = set(expb["intents_provisional"])

    intents = {}
    for tup in expb["per_intent_tuples"]:
        intent = tup["intent"]
        cost_tier, arm_breakdown = stack_cost_tier(tup["stack"], per_arm)
        provenance = "measured" if intent in measured else "provisional"
        # cross-check the tuple's own provisional flag against the input lists.
        assert tup["provisional"] == (intent in provisional), (
            f"provisional mismatch for {intent}: tuple={tup['provisional']} "
            f"intents_provisional={intent in provisional}"
        )
        intents[intent] = {
            "intent": intent,
            "provenance": provenance,
            "provisional": tup["provisional"],
            "stack": tup["stack"],
            "config": {
                "index": tup["index"],
                "retrieval": tup["retrieval"],
                "alpha": tup["alpha"],
                "pool_n": tup["pool_n"],
                "mmr": tup["mmr"],
                "recency": tup["recency"],
                "forbidden_ops": tup["forbidden_ops"],
            },
            "cost_tier": cost_tier,
            "cost_tier_breakdown": arm_breakdown,
            "source_exp": tup["source_exp"],
            "ci": tup["ci"],
        }

    registry = {
        "schema": "0.8.11-l2-router-registry-v1",
        "slice": "0.8.11/slice-35",
        "generated_from": {
            "per_intent_tuples": "dev/plans/runs/expb-joint-tune-output.json",
            "cost_tiers": "dev/plans/runs/gate2-oracle-output.json",
        },
        "confidence_header": (
            "SCREENING DATA — NOT VALIDATED CONFIG. These per-intent tuples are "
            "0.8.11 screening results: a SINGLE-CORPUS LME measurement (needle/"
            "multi_session/temporal), rerank knobs DERIVED from the 0.8.3 CE-pass "
            "(the in-session .venv build had the CE block gated OFF), 2 of 5 classes "
            "(global, multi_hop) PROVISIONAL EXP-0-global pins with no per-class "
            "measurement, and a LEXICAL fallback classifier (lower-bound proxy, "
            "macro 0.768). The 0.8.15 dispatcher MUST re-validate every tuple "
            "(fresh default-reranker CE build, LOCOMO/MuSiQue corroboration, "
            "AP-News decide_084 win-rate for global) before relying on it."
        ),
        "intents_measured": sorted(measured),
        "intents_provisional": sorted(provisional),
        "feedback_arm": False,
        "feedback_arm_rationale": (
            "EXP-AF (Slice 30) verdict = KILL: even a stronger agent (claude-sonnet) "
            "on the break-even cells did NOT beat internal ce_score net of round-trip "
            "(depth-1 reranking lift NET of one round-trip c_rt=0.02 = -0.0126 "
            "[-0.0274,0.0022], GO=False). The router stays on internal ce_score; the "
            "agent-signal escalation loop is DROPPED; record_feedback stays "
            "instrumentation (overrides any F-8b promote)."
        ),
        "intents": intents,
        "forbidden_composition_guard": expb["expb5_forbidden_composition_guard"],
    }

    OUT.write_text(json.dumps(registry, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(REPO)}")
    print(f"  intents: {len(intents)} "
          f"(measured={sorted(measured)}, provisional={sorted(provisional)})")
    for name, rec in intents.items():
        print(f"  - {name:14s} cost_tier={rec['cost_tier']:6s} "
              f"provenance={rec['provenance']:11s} forbidden={rec['config']['forbidden_ops']}")


if __name__ == "__main__":
    main()
