# Hand-off Prompt — Rubric v3 rework + audit-loop continuation

> Paste the block below to a fresh session to resume this work. It is self-contained; it points at the
> durable record and states the immediate task, the discipline, and the constraints.

---

You are continuing the **agentic-failure rubric + transcript-audit loop** for FathomDB. Start by reading
`dev/design/rubric-audit-session-record-2026-07-09.md` — it is the durable index (goal, findings, artifact
map, constraints). Then read, in order: `dev/design/rubric-audit-and-revision-method.md` (the audit method,
the 25 quality parameters §4, the revision loop + acceptance gate §5), and the v2 changelog §13 in
`dev/design/agent-harness-evaluation-rubric-v2.md` (it carries the review verdict + the rework list).

**Where things stand.** An independent Fable-High review judged rubric **v2 = MIXED, acceptance gate FAILED**;
it returns to §5 step 3 for a **v3**. The keepers (do NOT re-litigate): the C7 and C8 generalizations, the F2
boundary fix, the B6 routing of the undetectable class, and the §2.1 nominal-vs-effective note. v1 is still
PROPOSED (no HITL sign-off).

**Your task — produce v3, then get it independently re-reviewed. Do NOT self-assert acceptance.**

1. Author `dev/design/agent-harness-evaluation-rubric-v3.md` (copy v2, apply the five fixes as a dated delta,
   stable core untouched, per §5 step 7). The five fixes (from the v2 §13 review outcome):
   - **C8** — replace the over-firing "retrospective dependency-surprise markers = a missed-dependency UNMET"
     rule with a qualified one: UNMET requires (a) a consequential change, (b) a dependent knowable pre-change
     (grep/one-read), (c) no pre-change census; explicitly exempt in-loop test/debug iteration. Restore v1's
     dropped "deferring the topology map to execution time = UNMET" clause + the survey-content check.
   - **Rule 9 (Q-SEV)** — make it computable: bind the audit's per-example severity tiers
     (`hard|high|med|low`, already in `audit/rubric_audit_examples.jsonl`) into a per-criterion severity +
     weight vector, OR demote rule 9 to a §11 known-gap. An uncomputable scoring rule must not ship.
   - **C6** — either ground the generalization (adjudicate the 9 `DQ-LIMITED-SAMPLE` candidates → ≥1 confirmed
     TP) or mark the clause provisional-pending-a-real-occurrence; fix its polarity (the failure is the
     *absence* of scope-naming before a general claim, not the *presence* of a locality qualifier).
   - **C7** — restore the "round count to resolve" anchor; replace the read-paddable "deliberation depth =
     context-gathering tool-uses" proxy with "the decision-relevant artifact (design/plan/handler body) was
     read before the disposition."
   - **Keep 62 criteria / 12 HARD** unless a fix genuinely requires a structural change — generalize/merge,
     never clone.

2. **Actually run the §5 loop** (v2 skipped it — that was a flagged process defect). Reuse the audit runner in
   `dev/experiments/rubric-stress-test/audit/build_audit.py` + the detector output; do NOT read raw
   transcripts. Re-score v3 on the **sealed validation split**; run the turn-level redundancy re-checks the
   scorecard flagged (B7/C1, C2/C5, and now the broader C7/C8); **pin known-bad E4's `DQ-NETNEW-DRIFT` row
   fully into validation** first (the split currently straddles it — this was the false claim v2 was caught on).

3. **Acceptance gate (§5 step 6) — all must hold, measured not asserted:** every change traces a trigger;
   Q-COV ↑ on validation; no parameter regresses (Q-IRR, Q-REDUN, Q-DISC, Q-GAME, Q-CONSEQ, Q-SEV); decorrelation
   clean; severity integrity intact; HARD + parsimony preserved. Record measured deltas in the v3 changelog.

4. **Independent re-review** by a non-author agent (`model: fable`, adversarial, per method §6) against the
   sealed validation split; return a material-improvement verdict v2→v3. Only on a clean pass is v3 acceptable;
   otherwise loop again with a targeted rework.

**Hard constraints.** Never read a raw transcript `.jsonl`/`.output` into context (multi-MB → overflow) — work
from detector output + ≤±3-line windows; detection is deterministic, LLM only adjudicates. Scripts live in
`dev/experiments/rubric-stress-test/`; the ~1 GB staged corpus is `/home/coreyt/transcript-data/` — never
commit it, never move it into the repo. Anti-bloat: generalize, don't clone. Evidence before verdict — and
apply that to your own changelog (the last version was caught asserting a false split-state premise, a B7
violation by the rubric's own standard; verify every factual claim you write about the split/scores).

**Optional parallel tracks** (if directed): productize the Tier-1 deterministic detectors as standing
`[D]` auto-feeders (~0 LLM); instrument the rubric's §11 gaps (D7 rubber-stamp tracking, H7 OPP-12
conformance gate, F6 stall heartbeat). HITL sign-off on the rubric + adoption cadence remain open HITL calls —
propose, don't decide.
