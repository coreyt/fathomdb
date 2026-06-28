# NOTE → Program Steward — 0.8.8 telemetry/gold id-contract dependency on the identity substrate

**From:** 0.8.8 Release Orchestrator · **Date:** 2026-06-28 · **HITL decision:** ACCEPT-as-documented + track.

## The carried dependency (HITL-ratified accept, with follow-up)

0.8.8 EXP-OBS landed telemetry capture (Slice 15) + the real-gold pipeline (Slice 20) keying every
id (`TelemetryEvent.result_ids`/`arm_of`/`feedback.*_ids`, `GoldRecord.candidate_ids`/`labels`) on
**`SearchHit.id`**. The ratification artifact (§3d) named that the "stable `logical_id`", but per
`ADR-0.8.0-canonical-identity-substrate` **`SearchHit.id` is today the interim `write_cursor`**
(it "swaps to `logical_id` at the G0 keystone with no carrier reshape"); doc nodes also carry
`logical_id = NULL`. This is consistent with the already-shipped explain `PerHitExplain.id`.

**HITL ruling (2026-06-28):** ACCEPT as documented for 0.8.8 — within a single capture session the
ids are byte-identical to the search/telemetry ids that produced them, so the gold pipeline is
correct **session-scoped**. The distinction is invisible until the substrate swap.

## Action required of the Steward when the identity-substrate work makes `logical_id` hit-stable

When `SearchHit.id` (and thus telemetry/gold ids) becomes the stable `logical_id`:

1. **Revisit the gold id contract.** Either:
   - provide a **remap** from any pre-swap captured gold (`write_cursor`-keyed) to `logical_id`, OR
   - consciously **accept pre-swap gold as session-scoped only** (do not reuse it across a rewrite/
     supersession boundary; regenerate gold post-swap).
2. Update `GoldRecord.id_space` semantics + `eval/gold_capture.py` / `eval/frozen_candidate_scorer.py`
   accordingly, and reconcile `dev/plans/runs/0.8.8-explanation-fieldset-ratification.md` §3d.
3. Same review applies to `PerHitExplain.id` and `SearchHit.id` consumers generally.

**Tracking:** this is a known, accepted carry — not a 0.8.8 defect. Surface it on the identity-
substrate work item so the gold contract is revisited at swap time.
