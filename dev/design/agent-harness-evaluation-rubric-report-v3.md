# Report (v3, TERMINAL) — The FathomDB Agent-Harness Evaluation Rubric: audit, revision, and what was measured

> **Status: PROPOSED (HITL sign-off pending). Terminal — the audit/revision line closes at v3.**
> Companion to `agent-harness-evaluation-rubric-v3.md` (the instrument of record). This report supersedes
> the v1 report (`agent-harness-evaluation-rubric-report.md`, now SUPERSEDED-BY-report-v3). It does **not**
> re-derive the original purpose, external survey, design options, and justification — those are unchanged
> and remain in the v1 report §§1–9, which is still accurate about *why the instrument is shaped the way it
> is*. What this report adds is the part the v1 report could not contain: the bottom-up **audit** of the
> instrument against the real transcript corpus, the **closed-loop revision** v1→v2→v3, and — the thing a
> terminal deliverable must not fake — **what was actually measured versus what remains unmeasured.**

---

## 1. Read-me / provenance

- **Instrument of record:** `agent-harness-evaluation-rubric-v3.md` — 62 criteria, 8 dimensions, 12 HARD,
  the `[D]`/`[L]`/`[H]` judge-harness, and (v3) the bound severity vector.
- **Method:** `rubric-audit-and-revision-method.md` — the audit (§2), the 25 quality parameters (§4), the
  closed-loop revision + acceptance gate (§5), the independent-review charter (§6).
- **Evidence base (0-LLM detection, LLM only at adjudication):** `dev/experiments/rubric-stress-test/` —
  the detector suite, the audit (`audit/`), and the v3 measurement artifacts (`phase_a.py`,
  `adjudication_pack.jsonl`, `judge_A.jsonl`, `judge_B.jsonl`, `irr_result.json`, `severity_vector_v3.json`,
  `failure_corpus_split_v3.json`).
- **Hard discipline, honored throughout:** no raw transcript (`~1 GB`, `/home/coreyt/transcript-data/`) was
  ever read into a working context — every judgment came from detector output + ≤±3-line windows. The corpus
  is never committed and never moved into the repo.

---

## 2. Version history and verdicts (the headline is the self-catch)

| Version | What it was | Verdict |
|---|---|---|
| **v1** (`…rubric.md`) | Built top-down from research + the repo's known-bad RCAs (CR-047, 30-N) | PROPOSED; then **falsified** by the CR-047 calibration episode — a high score would not have moved the event, because once a false premise was HITL-ratified the obedience criteria *protected* it. Drove the B7 premise-witness gate + C7/E7/A9/H6 amendments. |
| **v2** (`…rubric-v2.md`) | Audit-driven generalizations (C6/C7/C8), F2 boundary, B6 routing, §2.1 nominal-vs-effective note, a new Q-SEV scoring rule | **MIXED — acceptance gate FAILED.** Independent Fable-High review kept C7/C8/F2/B6/§2.1 but found: C8 over-fire, Q-SEV uncomputable, C6 ungrounded + wrong polarity, C7 anchor dilution, and — the process defect — **the §5 loop was never run**, so its coverage/severity deltas were asserted. |
| **v3** (`…rubric-v3.md`) | The five reworks **with the §5 loop actually run**; M-additions for terminal quality (this report, supersession, measured Q-IRR/specificity, gap register, citation verification) | PROPOSED; pending independent non-author re-review (method §6). Acceptance is that verdict, not the author's. |

**The most important finding of this whole line of work is not any single criterion — it is that the
loop caught its own author.** v2's changelog asserted "E4 pinned into validation"; E4's key row was in the
*tuning* split. A false split-state premise, in the changelog of the rubric whose flagship criterion (B7)
polices premise-witnessing, is a **B7 UNMET by the rubric's own standard**. It was retracted and logged.
The deliverable worth keeping is an *audited, independently-judged, evidence-grounded loop that surfaces
genuine defects including the evaluator's own* — the capability, not the document.

---

## 3. The audit story and the coverage numbers (nominal vs effective)

The rubric was scored bottom-up against the detector corpus (2,353 examples over ~25 observed-failure
classes; scorecard `audit/RUBRIC-AUDIT-SCORECARD.md`). Three numbers matter, and they must be read together:

- **Nominal coverage: 72% strict / 84% half-credit** (18 COVERED / 6 PARTIAL / 1 GAP of 25 classes). The
  forward direction is the instrument's strength — nearly every observed failure class maps to a correctly-
  scoped criterion. The shortfall is concentrated in dimension C and is about *scope* (lifecycle/migration-
  scoped vs the general decision-quality act), not absence.
- **Effective auto-detection is far lower** — this is the load-bearing caveat (§2.1 of the instrument). Of
  the detectors, only the **process-forensic** ones survive hand-adjudication as deterministic auto-flags
  (BRANCH-UNVERIFIED→A6 3/3, IRREVERSIBLE→A4 3/3). Every **decision-quality** detector fires overwhelmingly
  on *good* behavior and can only surface *candidates* for `[L]`/`[H]` adjudication. So a high nominal % is
  **not** high detection power. v3 measured the other half of this claim (see §4): the adjudication step
  rejects good-behavior candidates at 100% on the tested sample — the design works, but it is adjudication,
  not free auto-scoring.
- **Confident-verdict split ≈ 60% deterministic / 37% needs-adjudication / 3% undetectable-from-transcript.**
  This is the token-efficiency thesis: push detection to scripts (free), spend the LLM only on the 37%, and
  route the 3% *incorrect-but-plausible* residual to the B6 escape-rate loop (it has no in-transcript catch
  signal, so it is correctly **not** a new in-line criterion).

Groundedness: 25/62 criteria are exercised by ≥1 corpus example. The 40% is not speculation-bloat — the
corpus is a *transcript-behavior* corpus, so it structurally cannot exercise `[D]` ledger/config/cadence
criteria or rare catastrophic HARD invariants; 3 dead HARD invariants (D2/H2/H3) are dead precisely because
they *held* in-period.

---

## 4. What v3 measured (the §5 loop), and the measured-vs-unmeasured table

The v2 review's process defect was that the loop was skipped. v3 ran it. Two independent non-author judges
(one Opus-tier, one Fable-High) adjudicated a reproducible 27-candidate pack (9 C6-grounding + 14 good-
behavior specificity + 4 positive-control), each with a ±3-line window. One sample served four measurements:

- **C6 grounding:** **0/9** DQ-LIMITED-SAMPLE candidates were real failures (both judges, independently).
  Every one was a scoped sample, a checkpoint plan, or a witnessed claim — confirming the detector fires on
  the *catch*, and confirming the v2 C6 generalization was motivated by a coverage "gap" (G-4) that is a
  polarity artifact. v3 fixes the polarity and marks the clause **corpus-unattested**, not falsely grounded.
- **Q-DISC known-good specificity:** **14/14 = 100%** (both judges) — the adjudication step correctly
  rejected every good-behavior candidate. This is the measured proof of the §2.1 design and upgrades the
  scorecard's "known-good: INFERRED-CLEAN, pending" to a measured result.
- **Q-IRR inter-rater reliability:** **27/27 identical labels, 0 disagreements** across two *different-model*
  judges. κ = 1.0, but honestly **base-rate-limited** (26/27 GOOD → near-zero variance), so the load-bearing
  claim is **100% observed agreement**, which validates the binary-decomposition design (Q-ANCHOR) on this
  sample — *not* a stress-tested full-instrument κ.
- **Turn-level redundancy (Q-REDUN):** session-Jaccard overstated redundancy exactly as the scorecard
  warned. At turn (line-window) granularity: **B7/C1 0.727→0.251, C7/C8 0.75→0.667, C2/C5 0.682→0.636** —
  all below the 0.80 merge line. No forced merge; the 62/12 structure is measured-safe.
- **Q-SEV:** now bound and computable (§2.2 of the instrument); demo on the corpus shows the HARD gate +
  weighted asymmetry both function.

**Adjudication-pack provenance (stated for reproducibility and honesty).** The 27-candidate pack is **not**
validation-only: by `failure_corpus_split_v3.json` it is 16 tuning / 11 validation (C6-grounding 5T/4V,
specificity 10T/4V, positive-control 1T/3V). No result flips on the validation-only subsets — C6 grounding
is 0/4 on validation (and 0/9 as a full-corpus *census*, which is the correct denominator for a groundedness
claim); specificity is 4/4 on the validation subset (14/14 overall); controls are 3/4 in validation. The C6
0/9 is deliberately a census over *both* splits because groundedness asks "does any real occurrence exist
anywhere," not "on the held-out side only." **Judge provenance:** two independent runs, one Opus-tier and one
Fable-High, 2026-07-09, recorded in `audit/judge_provenance.json`; the two runs are independently worded
(every rationale differs in phrasing and quoted evidence) and non-author (neither judge authored v3).

**Measured vs unmeasured (the honesty table a terminal deliverable owes its reader):**

| Parameter | Status in v3 | Value / note |
|---|---|---|
| Q-COV coverage | measured (structural) | 72%/84% nominal; effective far lower (§3); C6's generalized coverage honestly withdrawn |
| Q-DISC discrimination | **measured both sides** | known-bad 4/4 to right criteria (scorecard Q3); known-good specificity 14/14 = 100% (v3) |
| Q-IRR reliability | **measured (bounded)** | 100% observed agreement, 2 judges, N=27; κ base-rate-limited |
| Q-REDUN parsimony | **measured (turn-level)** | all flagged pairs < 0.80; no merge |
| Q-SEV severity | **measured (computable)** | bound vector 12/16/26/8; formula runs |
| Q-GROUND groundedness | measured | 25/62 live; dead classes explained (not speculation) |
| Q-ANCHOR binary decomposition | measured (indirect) | the 100% agreement is its signature |
| Q-GAME gaming-resistance | **NOT measured** | known-gap; C7 proxy de-padded, but no full adversarial pass — acceptable-as-stated |
| Q-CONSEQ consequential validity | **NOT measured** | known-gap; the routing (B6) and honesty notes are the mitigation |
| Q-CALIB calibration | **NOT measured** | known-gap; severity tiers are principled, not impact-fitted |
| Q-CRIT predictive validity | **NOT measured** | needs a future real incident/rollback to correlate against |

---

## 5. Residual risk / limitations (stated, because there is no v4)

1. **Q-IRR is base-rate-limited.** The adjudication sample was 26/27 GOOD, so the perfect agreement is a
   strong *observed-agreement* result but a weak κ stress test. A harder, class-balanced pack would test κ
   properly; not run here.
2. **Positive controls under-reproduced (1/4).** Episode-associated rows are mostly base-rate *context*
   lines, not the pinpoint catch line, so the ±3-window pack is the wrong surface to re-confirm known-bad
   discrimination — that lives at the localized-catch anchors (scorecard Q3), which stand.
3. **Confirmed-TP count is thin by construction (17).** The decision-quality space is reachable only as
   adjudication candidates; 501 candidates remain unadjudicated. v3 adjudicated the decision-relevant subset
   (the 9 C6 rows + the specificity sample), not all 501 — full adjudication was neither required by the
   acceptance gate nor affordable, and is documented, not hidden.
4. **Three quality parameters (Q-GAME/Q-CONSEQ/Q-CALIB) are unmeasured**, carried as known-gaps. For a
   *terminal instrument* this is acceptable *iff stated* — which it now is (this table + §11 register). A
   world-class terminal deliverable is honest about its edges, not silent about them.
5. **The §11 mechanisms are tracked, not built.** Converting the discipline-enforced criteria (D7/H7/F6/
   B6/A2/G1 and the CR-047/30-N amendments) into gates is program work outside the scope of shipping the
   instrument.

---

## 6. Citation verification (M7 — the instrument's own premises, witnessed)

The instrument's foundation cites three 2026 preprints; per the method's own instruction ("verify 2026-
preprint citations before external use"), leaving them unwitnessed would be an unwitnessed premise by B7's
standard. Web-verified 2026-07-09:

| Citation | Verdict | Note |
|---|---|---|
| Autorubric — arXiv:2603.00077 | **VERIFIED** | Rao & Callison-Burch (UPenn); content (binary/ordinal/nominal, one call per criterion, ensemble) matches the instrument's use. |
| Rulers — arXiv:2601.08654 | **VERIFIED, title corrected** | Formal title *"From Rubrics to Reliable Scores: Evidence-Grounded Text Evaluation with LLM Judges"* (RULERS = the framework). Supports evidence-anchored scoring. |
| "Catching One in Five" — arXiv:2606.10315 | **VERIFIED, usage corrected** | Zhang et al. — about **LLM-judge blind spots** (<25% of confirmed issues caught; misses cross-turn/structural). Now anchors B6 + §2.1, not the composition claim (v3 fix). |

**No fabricated citation was found.** The classical anchors (Messick, Lawshe, Cohen/Fleiss/Krippendorff,
DO-178C, FMEA, IIA, NIST AI RMF, MAST) were already primary-verified.

---

## 7. §11 gap-disposition register — HITL-gated ledger entries (drafted, not written)

The instrument's §11 now carries a register giving every known gap a tracking home + reopen trigger (M6),
so none orphans at terminal close. The tracking homes are *ledger/plan appends*, which are **governance
writes** — under the repo's discipline these require HITL approval and are **not** written unilaterally by
the rubric author. Drafted here, ready to append on HITL approval:

```jsonl
{"ledger":"steward","kind":"tracked-gap","ref":"D7","summary":"rubber-stamp countermeasure absent; tally HITL acceptance rate","reopen":"N unmodified-approval streak, or first per-release audit","decider":"HITL-PENDING"}
{"ledger":"steward","kind":"tracked-gap","ref":"H7","summary":"no OPP-12 contract-conformance gate; Pact-style can-i-deploy at 0.8.20 co-land","reopen":"0.8.20 co-land scheduled","decider":"HITL-PENDING"}
{"ledger":"steward","kind":"tracked-gap","ref":"A2","summary":"main-thread role tools not physically gated; PreToolUse deny on src/tests per role","reopen":"any main-thread source edit, or next hooks revision","decider":"HITL-PENDING"}
{"ledger":"steward","kind":"tracked-gap","ref":"F6","summary":"no liveness heartbeat tooling; convert 36h-stall discipline to a watchdog","reopen":"any bg agent dark > 1 working session","decider":"HITL-PENDING"}
{"ledger":"steward","kind":"tracked-gap","ref":"B6/F7","summary":"escape rate + coordination cost unmeasured","reopen":"first per-release audit run","decider":"HITL-PENDING"}
{"ledger":"steward","kind":"tracked-gap","ref":"G1","summary":"changed-LOC per slice untracked","reopen":"first release scored under this rubric","decider":"HITL-PENDING"}
```

---

## 8. Adoption handoff (proposed — HITL decides)

- **Status:** the instrument is **PROPOSED**, not adopted. Sign-off is an HITL call.
- **How to run it** (when adopted): score per method §2 + protocol; detection via the deterministic suite
  (`dev/experiments/rubric-stress-test/`), adjudication via a non-author `[L]`/`[H]` judge; aggregate with
  the §2.2 severity vector under the HARD gate.
- **Proposed cadence** (HITL to confirm): per-release retro at each `0.8.z` close; per-sweep for LBS/LBO;
  meta-oversight (D7/D8) at least quarterly. Whether the audit loop itself re-runs per release is an open
  HITL call — the loop is reproducible (`build_audit.py` + `phase_a.py`), so it *can*, cheaply.
- **The internal version-terminology collision** in the v1 report §5.2 ("v2"/"v3" meaning CR-047/30-N
  drafts) is reconciled by this report's unambiguous version table (§2); the v1 report is stamped
  SUPERSEDED with a pointer.

---

## 9. Bottom line

v3 is the terminal instrument. The five v2 defects are closed with **measured** results, not assertions;
the additions a *terminal* deliverable needs — a truthful report, supersession hygiene, measured Q-IRR and
specificity, a computable severity rule, a tracked (not orphaned) gap register, and witnessed citations —
are folded in. The honest edges (base-rate-limited κ, three unmeasured parameters, discipline-not-yet-gates)
are stated, not deferred. What survives as the durable asset is the **loop**: evidence-grounded, independently
judged, and demonstrated to catch even its author's own premise error. Acceptance now rests on an independent
non-author re-review (method §6) and HITL sign-off — proposed here, decided by neither the author nor this
document.
