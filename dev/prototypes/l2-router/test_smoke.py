"""Smoke test for the agent-side L2 router prototype (0.8.11 Slice 35).

$0, no LLM, no engine import. Run:  python dev/prototypes/l2-router/test_smoke.py

Covers the four requirements from the §4 contract:
  R-L2-1  every one of the 5 classes returns a Recommendation
  R-L2-2  each class carries a registered EXP-B' tuple (the 0.8.15 pre-stage artifact)
  R-L2-3  agent_hint override is respected VERBATIM, with NO classifier fallback
  R-L2-4  no engine mutation — `fathomdb` is never imported; recommend() executes
          no retrieval
Plus the EXP-B'.5 forbidden-composition refusal and the Slice-36 default-off
feedback_arm seam: default == explicit-off (byte-identical regression, EXP-AF
KILL stays shipped) and the feedback_arm=True on-path reaches the no-op
escalation stub with the base plan unchanged (the V-3 wiring point).
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from router import (  # noqa: E402
    ForbiddenCompositionError,
    L2Router,
    Recommendation,
    UnknownIntentError,
)

CLASSES = ("needle", "multi_session", "temporal", "global", "multi_hop")
_FAILS: list[str] = []
_PASSES = 0


def check(name: str, cond: bool, detail: str = "") -> None:
    global _PASSES
    if cond:
        _PASSES += 1
        print(f"  PASS  {name}")
    else:
        _FAILS.append(f"{name} :: {detail}")
        print(f"  FAIL  {name} :: {detail}")


def main() -> int:
    router = L2Router()

    print("R-L2-1 + R-L2-2: all 5 classes return a Recommendation with a registered tuple")
    for intent in CLASSES:
        rec = router.recommend("placeholder query", agent_hint=intent)
        check(f"  {intent}: returns Recommendation", isinstance(rec, Recommendation), repr(rec))
        check(f"  {intent}: intent matches", rec.intent == intent, rec.intent)
        check(f"  {intent}: carries a non-empty stack", bool(rec.stack), rec.stack)
        check(f"  {intent}: carries a config tuple",
              {"index", "retrieval", "alpha", "pool_n", "mmr", "recency", "forbidden_ops"}
              <= set(rec.config), sorted(rec.config))
        check(f"  {intent}: cost_tier in low/medium/high",
              rec.cost_tier in {"low", "medium", "high"}, rec.cost_tier)
        check(f"  {intent}: feedback_arm is False (EXP-AF KILL)",
              rec.feedback_arm is False, rec.feedback_arm)

    print("R-L2-3: agent_hint is verbatim with confidence 1.0 and NO classifier fallback")
    # A query whose text screams 'global', but the agent says 'needle' -> agent wins.
    rec = router.recommend(
        "across the entire dataset what are the overall themes and trends",
        agent_hint="needle",
    )
    check("  agent_hint overrides query text", rec.intent == "needle", rec.intent)
    check("  agent_hint confidence == 1.0", rec.confidence == 1.0, rec.confidence)
    # Bad hint must raise (no silent fallback to the classifier).
    try:
        router.recommend("q", agent_hint="not_a_class")
        check("  unknown agent_hint raises", False, "no error raised")
    except UnknownIntentError:
        check("  unknown agent_hint raises", True)

    print("Internal classifier fallback (preference #3) returns a valid class when no hint")
    for q in [
        "what is the capital of france",
        "across the dataset what are the main themes",
        "which company employs the person who founded the charity",
    ]:
        rec = router.recommend(q)
        check(f"  classify -> registered class :: {q[:32]!r}", rec.intent in CLASSES, rec.intent)
        check(f"  classify confidence in [0,1] :: {q[:32]!r}",
              0.0 <= rec.confidence <= 1.0, rec.confidence)

    print("EXP-B'.5 forbidden-composition refusal (the 0.8.15 validator seam)")
    # needle must refuse a sensemaking op...
    try:
        router.check_forbidden("needle", ["fts_bm25", "vector_ann", "map_reduce_qfs"])
        check("  needle refuses map_reduce_qfs", False, "no error raised")
    except ForbiddenCompositionError:
        check("  needle refuses map_reduce_qfs", True)
    # ...but global allows it.
    try:
        router.check_forbidden("global", ["fts_bm25", "vector_ann", "map_reduce_qfs", "community_summary"])
        check("  global allows map_reduce_qfs", True)
    except ForbiddenCompositionError as e:
        check("  global allows map_reduce_qfs", False, str(e))
    # the emitted needle/global stacks themselves are isolation-clean (recommend()
    # calls check_forbidden internally, so this is already exercised above).

    print("R-L2-4: caller-side — `fathomdb` engine/SDK is never imported (no retrieval run)")
    check("  fathomdb not in sys.modules", "fathomdb" not in sys.modules,
          [m for m in sys.modules if m == "fathomdb" or m.startswith("fathomdb.")])

    print("Slice 36: default-off feedback_arm seam (EXP-AF KILL stays the shipped default)")
    # Byte-identical-when-off regression: L2Router() == L2Router(feedback_arm=False).
    default_router = L2Router()
    off_router = L2Router(feedback_arm=False)
    check("  default feedback_arm is False", default_router.feedback_arm is False,
          default_router.feedback_arm)
    off_queries = [
        ("placeholder query", "needle"),
        ("placeholder query", "multi_session"),
        ("placeholder query", "temporal"),
        ("placeholder query", "global"),
        ("placeholder query", "multi_hop"),
        ("what is the capital of france", None),
        ("across the dataset what are the main themes", None),
        ("which company employs the person who founded the charity", None),
    ]
    for q, hint in off_queries:
        rec_default = default_router.recommend(q, agent_hint=hint)
        rec_off = off_router.recommend(q, agent_hint=hint)
        check(f"  default == explicit-off :: {q[:24]!r}/{hint}", rec_default == rec_off,
              f"{rec_default!r} != {rec_off!r}")
        check(f"  feedback_arm is False off :: {q[:24]!r}/{hint}",
              rec_default.feedback_arm is False, rec_default.feedback_arm)

    # On-path: feedback_arm=True returns a valid Recommendation, reaches the no-op
    # stub, and leaves the base plan/stack unchanged (the stub is a no-op today).
    on_router = L2Router(feedback_arm=True)
    for q, hint in off_queries:
        rec_on = on_router.recommend(q, agent_hint=hint)
        rec_off = off_router.recommend(q, agent_hint=hint)
        check(f"  on-path returns Recommendation :: {q[:24]!r}/{hint}",
              isinstance(rec_on, Recommendation), repr(rec_on))
        check(f"  on-path feedback_arm is True :: {q[:24]!r}/{hint}",
              rec_on.feedback_arm is True, rec_on.feedback_arm)
        check(f"  on-path base plan unchanged :: {q[:24]!r}/{hint}",
              (rec_on.intent, rec_on.stack, rec_on.config, rec_on.confidence,
               rec_on.cost_tier) ==
              (rec_off.intent, rec_off.stack, rec_off.config, rec_off.confidence,
               rec_off.cost_tier),
              f"{rec_on!r} vs {rec_off!r}")
    # _maybe_escalate is reached and is a no-op when on (returns the same object).
    base = on_router.recommend("placeholder query", agent_hint="needle")
    check("  _maybe_escalate no-op when on", on_router._maybe_escalate("q", base) is base,
          "stub mutated the recommendation")
    check("  _maybe_escalate no-op when off", off_router._maybe_escalate("q", base) is base,
          "off stub mutated the recommendation")

    print("\nProvenance split (honest hand-off):")
    for intent in CLASSES:
        r = router.intents[intent]
        print(f"  {intent:14s} {r['provenance']:11s} cost_tier={r['cost_tier']}")

    print(f"\n{_PASSES} passed, {len(_FAILS)} failed")
    if _FAILS:
        print("FAILURES:")
        for f in _FAILS:
            print("  -", f)
        return 1
    print("ALL SMOKE TESTS PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
