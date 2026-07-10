# Rubric Audit Scorecard — `agent-harness-evaluation-rubric.md` v1 vs the transcript-failure corpus

> Runs the method in `dev/design/rubric-audit-and-revision-method.md` §2–§3 against the detector
> corpus (`coverage/out/coverage_candidates.jsonl`, 2,353 rows / 3,372 files / 242,835 records; the
> prior A–E corpus; `precision_estimate.json` hand-labels; `recall_localized.json` known-bad anchors).
> **Worked entirely from detector output — no raw transcript read into context.** No git commits.
> Artifacts: `rubric_audit_examples.jsonl` (526 rows), `rubric_audit_criteria.jsonl` (62 rows),
> `scorecard.json`, `failure_corpus_split.json`, `build_audit.py`.

---

## 0. Taxonomy note (denominator honesty)

The task brief refers to a "125-type taxonomy." **That figure is not in `COVERAGE-REPORT.md`** — the
report's real observed-failure taxonomy is **17 decision-quality modes** (report §2) + ~5 process/role
forensic classes + the 5 prior A–E verbalized-catch families. Consolidating the 21 shipped detectors to
their distinct failure classes and adding the 3 unbuilt DQ modes + 4 A–E families gives **25 distinct
observed-failure classes**, which is the coverage denominator used below. Where the brief's DQ-mode
expectations are named (DQ-ASSUME→B7/C1, etc.) they are checked explicitly in the gap list.

---

## Q1 — COVERAGE (deterministic)

**Overall: 18/25 COVERED (72.0%) strict; 84.0% with half-credit for PARTIAL.** 6 PARTIAL, 1 GAP.

The **forward direction is the rubric's strength**: nearly every observed failure class maps to a real,
correctly-scoped criterion. The coverage shortfall is concentrated in one dimension (C) and is about
*scope* (lifecycle/migration-scoped criteria vs the general decision-quality act), not absence.

| Dimension (owning) | COVERED | PARTIAL | GAP |
|---|---|---|---|
| A (role/guardrail) | 4 | 0 | 0 |
| B (verification) | 4 | 0 | 0 |
| C (right-thing / validation) | 5 | 5 | 1 |
| D (HITL) | 1 | 0 | 0 |
| E (provenance) | 2 | 0 | 0 |
| F (coordination) | 1 | 1 | 0 |
| H (cross-repo) | 1 | 0 | 0 |

| Detectability class | COVERED | PARTIAL | GAP |
|---|---|---|---|
| deterministic-structural | 7 | 1 | 0 |
| deterministic-textual | 2 | 1 | 0 |
| needs-LLM-adjudication | 4 | 3 | 0 |
| needs-reference-comparison | 2 | 0 | 1 |

**Critical caveat — coverage is NOMINAL, not EFFECTIVE.** A class is "COVERED" if the rubric has a
criterion that *would* fire on a true instance. But `COVERAGE-REPORT.md` (C1–C4) shows the deterministic
detectors that feed the C/E criteria **fire overwhelmingly on GOOD behavior**: DQ-SHORTKNOWLEDGE 0/4,
DQ-STALE-VERSION 0/3, DQ-ASSUME-STRUCTURAL 0/3, DQ-IGNOREDESIGN-STRUCTURAL 0/3 on hand-adjudicated
samples. Those covering criteria (B7, C1, C2, C3, C7, C8, E7) are therefore **correctly** typed `[L]`/`[H]`
— they *require* the LLM/reference adjudicator the raw detector cannot supply. Coverage % says the rubric
"has a criterion"; it does **not** say that criterion can be cheaply automated. The two auto-flags that
survive hand-adjudication (BRANCH-UNVERIFIED→A6 3/3, IRREVERSIBLE→A4 3/3) are both **process-forensic
A-dimension** signals, not decision-quality ones.

---

## GAP LIST — the revision backlog (ranked by frequency × severity)

| # | Observed class (freq: hits/sess) | Verdict | Owning criteria today | Why not fully covered | Revision (method §5 preference) |
|---|---|---|---|---|---|
| **G-1** | short-knowledge decision (547 / 64+34) | PARTIAL | C7, C8, C1 | C7 is *lifecycle*-scoped (finish-vs-delete), C8 is *migration*-scoped (write-path/field-parity). Neither owns the general act "a consequential decision made without investigation proportional to its falsification cost" — precisely protocol-rule-8's principle, which has **no standalone criterion**. | **Generalize** C7 or lift rule 8 into a criterion: "investigation proportional to reversibility/blast-radius before a consequential decision," subsuming DQ-SHORTKNOWLEDGE/ASSUME/SNAP. |
| **G-2** | dependency-blindness, forward+retro (316 / 77) | PARTIAL | C8, F1 | C8's caller-census is migration-only; F1 is handoff state-continuity. No criterion asks "did the agent check downstream dependents before changing a **shared surface** (engine, schema, cross-binding) outside a migration plan?" | **Generalize** C8's live-vs-orphan census beyond migration plans, or add a G-dimension "blast-radius reads precede shared-surface edits" (near G3/G4). |
| **G-3** | incorrect-but-plausible decision (22 / 14) | **GAP** | C1, C5 (indirect) | No criterion gates "the conclusion was substantively wrong though every gate was green" — the CR-047 residual. Report §6: **undetectable from transcript alone** when never caught. C1/C5 check requirement quality, not answer-correctness. | **Do NOT add a criterion** (undetectable). Route to the B6/F7 **escape-rate loop** already in the rubric; document as accepted residual. |
| **G-4** | limited-sample generalization (9 / 9) | PARTIAL | C6, B7 | C6 (decision-rules-before-spend) covers *priced experiments* only; a plan/design generalizing from N=1–2 static reads is not owned. | **Generalize** C6 anchor from "priced/measured run" to "any generalization with a stated sample and its adequacy." |
| **G-5** | self-correction needed (A–E `D-self-correction`, 20 val) | PARTIAL | F4 | Self-correction is *good* behavior; the latent failure ("shipped a claim that later needed retraction") maps only weakly to F4 reasoning-action match. | Low priority; fold into G-1 (proportional investigation) — a retraction is a short-knowledge symptom. |

**Reconciliation with the brief's expected modes:** DQ-ASSUME→**B7/C1 COVERED** ✓; DQ-IGNOREDESIGN→**C2/C3
COVERED** ✓; SILENT-STALL→**F6 COVERED** ✓; ROLE-BLEED→**A1 COVERED** ✓; BLOCK-OVERRIDE→**B2 COVERED** ✓;
IRREVERSIBLE-UNGATED→**A4 COVERED** ✓; DQ-STALE→**E7 COVERED** ✓ (class covered; detector fires on good
behavior). Only **DQ-SHORTKNOWLEDGE→C7/C8 (PARTIAL, G-1)** and **DQ-DEPBLIND→C8 (PARTIAL, G-2)** fall short
of the brief's expectation, and **DQ-INCORRECT (GAP, G-3)** is genuinely uncovered-but-undetectable.

---

## DEAD-CRITERIA LIST — Q2 groundedness

**Live: 25/62 (40.3%).** 34 dead-detectable, 3 dead-undetectable. (`rubric_audit_criteria.jsonl`.)

The 40% figure is **not** an indictment: the corpus is a *transcript-behavior-failure* corpus, so it
structurally cannot exercise `[D]` ledger/config/cadence criteria or rare catastrophic HARD invariants. The
honest read is three buckets:

**LIVE (25)** — exercised by ≥1 corpus example: A1, A4, A5, A6, B1, B2, B5, B6, B7, C1, C2, C3, C5, C6, C7,
C8, D1, D4, D5, E7, F1, F4, F5, F6, H6.

**DEAD-UNDETECTABLE (3) — keep; their deadness is GOOD news** (a catastrophic invariant that did **not**
trip in-period): **D2** (authority never laundered), **H2** (dual-side ratification), **H3** (memex
write/push containment). All HARD. Zero occurrences = the guardrail held, not that the criterion is idle.

**DEAD-DETECTABLE (34)** — no corpus example, but a check *could* exist. Three sub-classes:
- *Out-of-detector-scope by construction* (ledger/config/cadence — not transcript-behavior): A2, A3, A7,
  A8, C4, D3, D6, D8, D9, E1, E2, E3, E4, E5, E6, G1, G2, G3, G4, G5, G6, G7, H1, H4, H5, H7, H8. **Keep** —
  these need the `[D]` ledger-validate / git-log / config detectors that this corpus's miner never targeted.
- *Known gaps already flagged in rubric §11* (not yet gated): **D7** (rubber-stamp countermeasure), **F3**
  (spawn calibration), **F7** (coordination-overhead pricing), **A9** (stub-intent markers — proposed lint).
  **Keep-and-build**, not retire.
- *Genuine mis-map / retire-or-merge candidate*: **F2** ("no duplicated/ignored work"). The corpus's
  `DQ-DEPBLIND-*` detectors carry `rubric_ref=F2`, but F2 is about *duplicated work*, not dependency
  blindness — a **wrong rubric_ref** (corrected to C8/F1 here). F2 has **no genuine corpus support** and no
  near occurrence. **B3** (witness-over-narration) and **B4** (verification completeness) are dead only
  because the corpus miner has no closure-witness detector — they are near-certainly live in reality.

**Verdict:** no criterion is dead-*and*-speculative-*and*-safe-to-delete outright. The retire/merge pressure
is on the redundancy axis (below), not the groundedness axis.

---

## REDUNDANCY GRAPH — Q6 non-redundancy

**Max pairwise session-Jaccard = 1.00 (B5–F5).** 32 pairs ≥ 0.50. **But session-granularity Jaccard
massively OVERSTATES redundancy** and two artifacts must be stripped before drawing conclusions:

1. **Mapping-induced** (one detector → multiple criteria, so they co-occur trivially): B5–F5 (both from
   PREMATURE-TERM), E7–H6 & C3–E7 & C3–H6 (all from NETNEW-DRIFT→C3/H6/E7), C8–F1 (both from DEPBLIND).
   These are **not** evidence the criteria are redundant — only that my mapping assigned them jointly.
2. **DQ-session base-rate**: the memex RCA sessions trip *many* DQ detectors at once, so at session
   granularity almost every decision-quality criterion co-fires. Real redundancy needs **turn-level** co-fire
   (not available without re-mining) — treat these as **upper bounds**.

**Genuinely design-relevant co-moving clusters** (already flagged by the rubric's own anti-clone discipline):
- **B7 ↔ C1** (jac 0.73) — premise-witness vs mandate/requirement quality. The rubric §11.2 already argues
  these are *one root cause*; watch for merge pressure but they gate different surfaces (premise vs requirement).
- **C7 ↔ C8** (jac 0.75) — direction-before-action vs unit-of-work. §11.2 deliberately kept these split
  (premise-truth ⟂ measuring-the-wrong-thing); the corpus co-movement is expected, not a merge signal.
- **C2 ↔ C5** (jac 0.68) — design-review-precedes-impl vs validation-asked-separately. Plausible genuine
  overlap; candidate to sharpen anchors so they don't both fire on the same evidence.

No pair clears the bar for a forced merge on this (coarse) evidence; **Q6 verdict: parsimony intact, but
B7/C1 and C2/C5 flagged for turn-level redundancy re-measurement in the revision loop.**

---

## Q3 — DISCRIMINATION (known-bad recall / known-good specificity)

**Known-bad side: PASS — all 4 episodes localize to the RIGHT criteria.**

| Episode | Localized catch (turn-anchored) | Maps to criterion | Rubric's intended owner (§11) | Correct? |
|---|---|---|---|---|
| **E1 CR-047** premise-delete | B-unwitnessed-premise @ L1617 (`no_consumers+action`) | **B7** | B7 (+C7, E7, A9, H6) | ✓ |
| **E2 30-N** wrong unit-of-work | B-unwitnessed-premise @ L1495 (`n_sites`); UNVERIFIED-METRIC d2ae8c40 L327 | **B7 + C8** | B7 (generalized) + C8 | ✓ |
| **E3 36-h stall** | SILENT-STALL 35.7h @ L1226 / 23.8h @ L853 | **F6** | F6 | ✓ |
| **E4 OPP-12 drift** | NETNEW-DRIFT @ L1739/1754/1776/2178/2220 | **C3** | C3 (+H6) | ✓ |

Every known-bad maps to its intended criterion, and each of those is a distinct criterion (B7, C8, F6, C3)
— the rubric **separates** the four failure modes rather than collapsing them. Note E1/E2 also trip
PREMATURE-TERM→B5 and DQ-SHORTKNOWLEDGE→C7 as *context* (group base-rate), consistent with the report's
"the built-from episodes localize on B-unwitnessed-premise, everything else is base-rate."

**Known-good side (0.8.16): INFERRED-CLEAN, direct scoring PENDING.** The corpus does not score 0.8.16
directly. The discriminating fact is structural: every detector that *false-fires on good behavior*
(STALE-VERSION diagnosing drift, IGNOREDESIGN-STRUCTURAL compliant refactor, SHORTKNOWLEDGE mechanical
follow-through, ASSUME-STRUCTURAL careful reasoning) is exactly the set the report **downgrades to
candidate/adjudication** — i.e. the rubric's `[L]`/`[H]` judge is designed to reject them. So a known-good
release is clean *iff the adjudication step runs*; the raw deterministic layer alone would NOT be clean.
**This is the load-bearing reason the DQ criteria must stay `[H]`, not become `[D]` auto-flags.** Direct
known-good scoring is a Q3 residual for the judge run.

---

## Deterministic quality parameters (computed now)

| Q | Parameter | Value | How measured |
|---|---|---|---|
| **Q1** | Coverage | **72.0% strict / 84.0% half-credit** (18C/6P/1G of 25) | class→criterion map, verdict per class |
| **Q2** | Groundedness / liveness | **40.3% live** (25/62); 3 dead-undetectable (all HARD, held), 34 dead-detectable (mostly out-of-corpus-scope `[D]`) | ≥1 corpus example per criterion |
| **Q3** | Discriminant validity | **known-bad PASS (4/4 right criteria)**; known-good inferred-clean, direct-score pending | recall_localized anchors vs mapped criteria |
| **Q6** | Non-redundancy | **parsimony intact**; max session-Jaccard 1.00 but mapping-induced; B7/C1 & C2/C5 flagged for turn-level re-check | criterion session-set Jaccard |

**Placeholders (need the research-defined measurement + judge runs, method §4):**

| Q | Parameter | Status |
|---|---|---|
| Q4 | Inter-judge reliability | PENDING — two independent judge runs on the same examples (κ / % agreement) |
| Q5 | Evidence-groundedness | PENDING — % of verdicts citing a quote/sha/line before the score |
| Q7 | Actionability | PARTIAL — per-criterion field emitted in `rubric_audit_criteria.jsonl`; formal distinct-remediation audit pending |
| Q8 | False-negative sensitivity | PENDING — consequence-weighted recall on confirmed failures (needs full adjudication of the 501 candidates) |
| Q9 | Gaming / Goodhart resistance | PENDING — adversarial pass (can a subject pass while a known-bad occurs?) |
| Q10 | Calibration | PENDING — hard/soft & severity weights vs measured impact |

---

## Confirmed-corpus honesty statement

Of 526 distinct (session, detector) examples, only **17 are confirmed-TP** (8 reference-adjudicated known-bad
episode catches + 9 hand-adjudicated TPs) and **8 confirmed-FP** (detector fired on good behavior); **501
remain unadjudicated candidates** (`needs_adjudication=True`, awaiting the `[L]`/`[H]` judge). The coverage
verdicts above are therefore **class-level structural mappings**, robust; the *per-example* confirmation is
thin by construction — the report's central finding is that the decision-quality space is reachable only as
**adjudication candidates**, and this scorecard inherits that limit. Q8 (consequence-weighted recall) cannot
be finalized until those 501 are adjudicated.

---

## Corpus split (anti-overfit)

`failure_corpus_split.json`: sha1(example_id) first-nibble < 11 → tuning (357), else validation (169) ≈ 68/32.
Known-bad placement:

- **E1 CR-047 → VALIDATION** (sealed)
- **E2 30-N → TUNING**
- **E3 36-h stall → SPLIT** (multiple example rows land on both sides)
- **E4 OPP-12 → SPLIT**

⚠️ **Split action item for the revision loop:** E1 is cleanly sealed in validation (good — it falsified v1),
but E2 sits in tuning and E3/E4 straddle. To honor method §2's "accepted improvement must hold on
validation," **pin at least one episode per root-cause family to the sealed side** — recommend forcing E4
(design-drift/C3) fully into validation so C3/C8 revisions are validated on unseen episode evidence. The
generic examples split is fine as-is; only the 4 anchor episodes need the pin.

---

## Bottom line

- **Coverage 72%/84%** — the rubric's forward coverage is strong; the shortfall is 5 PARTIAL + 1 GAP,
  concentrated in dimension C and driven by *scope* (lifecycle/migration-scoped criteria vs the general
  proportional-investigation act), plus one genuinely undetectable GAP (DQ-INCORRECT).
- **Groundedness 40% live** — not speculation-bloat: 3 dead criteria are HARD invariants that correctly held,
  and most dead-detectable criteria are `[D]` ledger/config/cadence checks this transcript-failure corpus
  never targeted. Real retire/merge pressure is limited to F2 (mis-mapped, unsupported) and the B7/C1, C2/C5
  turn-level-redundancy re-checks.
- **Discrimination PASS on the known-bad side (4/4 to the right, distinct criteria)**; known-good clean is
  inferred from the deliberate candidate-downgrade of every good-behavior false-firing detector.
- **The revision backlog** is G-1 (generalize proportional-investigation, absorbing DQ-SHORTKNOWLEDGE/ASSUME),
  G-2 (generalize dependency-census beyond migration), G-4 (generalize C6 sample-adequacy), and G-3 booked as
  an accepted undetectable residual routed to the escape-rate loop — a **generalize-don't-clone** program,
  consistent with the rubric's own anti-bloat discipline.
