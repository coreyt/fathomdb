# Rubric Audit & Revision Method

> **Status: PROPOSED (design; execution gated on the coverage-audit tooling landing).** How to audit the
> agent-harness evaluation rubric (`agent-harness-evaluation-rubric.md`) against the real transcript
> corpus, in what format to emit the audit so the rubric itself can be judged, by what quality parameters,
> and by what closed-loop mechanism to revise it into a versioned successor. Companion to the rubric and
> its report; consumes the transcript-mining detector suite in `dev/experiments/rubric-stress-test/`.

---

## 1. Purpose

The rubric was built top-down (research + the repo's known-bad episodes). It must now be tested bottom-up
against **what actually happens in the transcripts** — does it cover the failures we can observe, is every
criterion grounded in a real occurrence, and does it score reliably? This doc defines the audit, its output
format, the quality parameters that judge the rubric, and the mechanism that turns a low score into a
principled revision — without overfitting to the corpus or letting the criteria count balloon.

---

## 2. The audit method — test the rubric two directions against the corpus

The detector suite (deterministic transcript miners + LLM-adjudicated candidate-extractors, per the coverage
audit) produces a **corpus of real agentic-failure examples**, each carrying `{detector, family,
failure_class, source(file,line,session), evidence_snippet, needs_adjudication, confirmed}`. The rubric's own
**§2.1 judge-harness** (deterministic checks + non-author LLM adjudication) is then run over that corpus.
Two directions:

**A. Forward — recall / coverage (does the rubric catch what we observe?).** For each *confirmed* failure
example, determine (a) which rubric criterion *should* cover that failure class, and (b) whether that
criterion, given the evidence present in the transcript, *would fire*. Aggregate to: fraction of observed
real failures the rubric covers, per failure class and per dimension. **Uncovered failures are coverage
gaps** — the primary driver of revision (candidate new/generalized criteria).

**B. Reverse — groundedness / liveness (is every criterion earned?).** For each rubric criterion, is there
≥1 real transcript example that exercises it? A criterion with **zero corpus support** is one of: (i)
speculative / over-specified, (ii) guarding a failure that does not occur in this program, or (iii)
undetectable-from-transcript (legitimately — e.g. a rare catastrophic invariant). Classify, don't
auto-delete: dead-and-undetectable-but-catastrophic stays; dead-and-speculative is a retire/merge candidate.

**C. Discrimination — known-good vs known-bad.** Run the rubric-scoring over the four known-bad episodes
(CR-047, 30-N, the 36-h stall, OPP-12 drift) and one known-good release (0.8.16). The rubric must score the
known-bads as failing on the *right* criteria and the known-good as clean — the calibration test from the
rubric's own protocol rule 7. A rubric that cannot separate them is invalid regardless of coverage %.

**Anti-overfitting from the start.** The failure-example corpus is split into a **tuning set** and a sealed
**validation set** (reuse the deterministic sha1-of-source split already staged). Revision is measured on
tuning; the *accepted* improvement must hold on validation.

---

## 3. Audit output format (the artifact that judges the rubric)

Three layers, machine-first so each quality parameter (§4) maps to a field. Emitted under
`dev/experiments/rubric-stress-test/audit/`.

**3.1 Per-example rows** — `rubric_audit_examples.jsonl`:

```jsonc
{ "example_id": "...", "source": {"file": "...", "line": 1234, "session": "..."},
  "detector": "DQ-ASSUME-TEXTUAL", "family": "decision-quality", "failure_class": "unverified-assumption",
  "confirmed": true, "adjudicated_by": "deterministic|llm|reference",
  "mapped_criteria": ["B7", "C1"],            // which rubric criteria own this failure class
  "criterion_fires": {"B7": true, "C1": false},// would each fire on this evidence?
  "coverage_verdict": "COVERED|PARTIAL|GAP",
  "evidence_quote": "…truncated ≤300c…",
  "severity": "hard|high|med|low", "notes": "…" }
```

**3.2 Per-criterion aggregates** — `rubric_audit_criteria.jsonl` (one row per of the 62 criteria):

```jsonc
{ "criterion": "B7", "dimension": "B", "verification_class": "H", "hard": true,
  "corpus_examples": 41, "fires_correct": 33, "false_fire": 4, "missed": 4,
  "groundedness": "live|dead-detectable|dead-undetectable",
  "precision_on_corpus": 0.89, "recall_contribution": 0.12,
  "redundancy_with": ["C7"],                  // criteria whose fire-pattern co-moves ≥ threshold
  "actionability": "high|med|low" }
```

**3.3 Rubric-level scorecard** — `RUBRIC-AUDIT-SCORECARD.md` + `scorecard.json`:

- coverage % of the observed-failure taxonomy (overall + per dimension + per detectability class);
- the **gap list** — uncovered failure classes ranked by frequency × severity (the revision backlog);
- the **dead-criteria list** with their groundedness class;
- the **redundancy graph** — co-moving criterion pairs (candidates to merge);
- the discrimination result (known-bad recall, known-good specificity);
- the **quality-parameter scores** (§4) — one row per parameter, value + how measured.

This scorecard *is* the input to judging the rubric: every quality parameter reads off a field here.

---

## 4. Quality parameters for the rubric (FINALIZED from web research — psychometrics + LLM-eval)

The rubric-as-instrument is judged on **18 parameters** consolidated from measurement science and 2023–26
LLM-as-judge work (safety-audit-specific weighting + the revision-acceptance refinement come from the second
research stream, still landing, and are folded into §5). All are measurable off the §3 scorecard once the
**gold corpus** exists — and it does: the detector corpus + `failure_corpus_split` (§3.4) *is* the
independently-labeled known-failure/known-clean set + repeated judge runs these parameters require. **The two
highest-leverage: Q-DISC (known-groups AUC — does it catch failures?) and Q-IRR (inter-rater κ — do judges
agree?);** score those first.

**A. Reliability (do scores reproduce?) — quantitative**

- **Q-IRR** inter-rater reliability — Cohen's/Fleiss' κ (or Krippendorff's α for missing/ordinal) between ≥2
  independent judge runs, **per item** not just overall; <0.6 on any item = rewrite trigger. *(Cohen 1960;
  Landis & Koch 1977; MAST validated its taxonomy at κ=0.88.)*
- **Q-CONS** judge self-consistency / position-invariance — % identical verdicts across repeated + order-
  swapped runs; order-flipped disagreements = ties. *(Zheng et al. 2023.)*
- **Q-COH** internal consistency — Cronbach's α per intended dimension (α-if-item-deleted flags incoherent
  items); coherence *within* a dimension, traded against Q-REDUN *across* the rubric.

**B. Validity (does it measure the right thing?) — mostly quantitative**

- **Q-DISC** known-groups / discriminant validity — **ROC-AUC + sensitivity/specificity** of the rubric vs
  gold labels; the single most important parameter. *(Cronbach & Meehl 1955; Hattie & Cooksey 1984.)*
- **Q-CRIT** criterion/predictive validity — Spearman/Kendall correlation of rubric scores vs an independent
  outcome (expert gold, or a later real incident/rollback/override). *(G-Eval, FLASK.)*
- **Q-CONTENT** content validity / coverage — Lawshe CVR per item from an expert panel; drop below-critical
  items. *(Lawshe 1975; Jonsson & Svingby 2007.)*
- **Q-CONSTRUCT** construct validity / no-confound — rubric scores don't track a surface proxy (trace length,
  n-grams); shortcut test = can a shallow feature-only classifier predict rubric scores? *(Campbell & Fiske
  1959; Messick 1995.)*
- **Q-REDUN** parsimony / non-redundancy — inter-item correlation matrix; pairs >0.80 = merge candidates;
  latent-factor count vs stated-criterion count. *(the anti-bloat parameter — enforces the CR-047/30-N
  discipline quantitatively.)*

**C. Design & operational — mixed; the safety-critical ones for a rubric scoring *optimizing agents***

- **Q-ANCHOR** behavioral anchoring / binary decomposition — the lever that *raises* Q-IRR (CheckEval: +0.45
  agreement); already the rubric's design, verify it held.
- **Q-COV** failure-mode coverage — gap-% of the independently-built taxonomy (the 125-type detector taxonomy
  - MAST's 14 modes) mapped to criteria; **this is the audit's forward direction (§2A).**
- **Q-GROUND** groundedness/liveness — % criteria with ≥1 real corpus example (dead-undetectable excepted);
  **the audit's reverse direction (§2B).**
- **Q-CALIB** calibration — do hard/soft + severity weights track real failure impact (convergence across
  anchor exemplars).
- **Q-GAME** ⚠ resistance to gaming / Goodhart — can a subject pass the rubric while a real known-bad occurs?
  Red-team each item for the cheat + its catching companion; **first-class because the rubric scores agents
  that optimize.** *(Goodhart 1975; reward-hacking-in-rubric-RL literature.)*
- **Q-ROBUST** perturbation robustness — score stability under paraphrase/reorder/rename (surface-invariant).
- **Q-SPREAD** discriminative power / anti-saturation — does the rubric spread genuinely different runs apart,
  or cluster at floor/ceiling.
- **Q-ACT** actionability — % of UNMET verdicts that imply a concrete, distinct remediation.
- **Q-BURDEN** efficiency / rater burden — time-per-case vs rubric length; reliability peaks then falls past
  ~5–9 items per dimension (working-memory limit).
- **Q-FACE** face validity — expert read-through; screen only, never sufficient.
- **Q-CONSEQ** ⚠ consequential validity — do the *decisions the rubric drives* cause adverse responses
  (teams routing around it, suppressed reporting, over-flagging safe runs)? *(Messick 1995 — the facet most
  teams skip; first-class here because this rubric gates real work.)*

**D. Safety-audit-specific (assurance-grade — what makes it an audit, not an opinion survey)**

- **Q-SEV** ⚠ severity-weighted / recall-prioritized scoring — a missed high-severity failure must cost far
  more than a false alarm; encode false-negative cost asymmetry rather than flat pass/fail. *(FMEA RPN scores
  low detectability as high risk; toxicology weights a false negative ~20× a false positive. The rubric's
  hard/soft split is a start — v2 should make severity × detectability explicit.)*
- **Q-FALS** falsifiability / evidence-boundedness — every criterion resolves to a checkable true/false claim
  terminating in an artifact (GSN claim→solution), not an impression.
- **Q-TRACE** sufficiency-via-traceability — completeness is *walkable* (failure → criterion → evidence
  chain) so a second auditor can re-walk it, not asserted in prose. *(DO-178C bidirectional trace; NASA
  SWE-141.)*
- **Q-INDEP** auditor independence — judge structurally separated from the authored work, impairments
  disclosed. *(DO-178C: 30 of 71 Level-A objectives "with independence"; IIA 1100.)*
- **Q-HELDOUT-GAME** held-out gaming resistance — part of the audit is unpredictable to the scored agent
  (rotated/withheld criteria), because frontier agents adjust behavior to the eval. *(Auditing-Games-for-
  Sandbagging.)*
- **Q-CONTINUOUS** non-checklist continuous discipline — applied as ongoing risk management, not a one-time
  gate that retires attention (the structural antidote to compliance theater). *(NIST AI RMF.)*
- **Q-TAXCOMPLETE** taxonomy-anchored completeness — every criterion traces to a pre-declared *named* failure
  taxonomy (the 125-type detector taxonomy / MAST-14), answerable as "which objective does this instantiate?"
  *(DO-178C fixed-objective structure.)*

§6/§5 acceptance requires **no parameter regresses** on the sealed validation split, with **Q-DISC, Q-IRR,
Q-REDUN, Q-GAME, Q-SEV, and Q-CONSEQ** as the non-negotiable ones. (Verify 2026-preprint citations before
external use; the classical anchors — Messick, Lawshe, Cohen/Fleiss/Krippendorff, DO-178C, FMEA, IIA,
NIST AI RMF — are solid.)

---

## 5. Revision mechanism — closed loop, anti-overfit, versioned

Synthesized from the DO-178C / ISO-26262 / NIST-AI-RMF / OWASP revision precedents (research stream 2).

1. **Measure.** Run §2 → §3 scorecard + §4 quality scores on the **tuning** split.
2. **Named trigger — no unmotivated changes.** Every revision traces to a *named trigger*: an audit coverage
   gap, a dead/redundant criterion, a low-reliability item, or a real incident the rubric missed. A tightening
   or loosening with no trigger is rejected on principle (the DO-178C B→C discipline: an implicit expectation
   is only promoted to an explicit criterion once field practice shows it is interpreted inconsistently).
3. **Diagnose.** Rank: (a) coverage gaps by frequency × **severity** (Q-SEV weighting — a miss on a
   catastrophic mode outranks many low-severity gaps), (b) dead-speculative criteria, (c) co-moving redundant
   pairs, (d) low-actionability / low-reliability items.
4. **Revise — bounded, with the anti-bloat discipline already used for CR-047/30-N.** Preference order:
   **generalize** an existing criterion to absorb a gap > **split** only genuinely orthogonal axes >
   **merge** co-moving pairs > **retire** dead-and-undetectable-and-non-catastrophic > **re-weight** for
   false-negative asymmetry (Q-SEV) > **sharpen anchors** for reliability (Q-IRR/Q-ANCHOR). Every change cites
   its trigger + driving audit evidence (`example_id` / scorecard field). One root cause → one criterion.
5. **Re-score on the sealed VALIDATION split** — never tune and validate on the same examples. A revision that
   only improves the cases that inspired it does not ship (anti-overfitting guardrail; RedBench/held-out
   precedent).
6. **Acceptance gate (ALL must hold, else reject/rework):**
   (a) every changed/added criterion traces to a Step-2 trigger (no orphan changes);
   (b) coverage (Q-COV) ↑ on validation;
   (c) **no quality parameter regresses** — non-negotiable: Q-IRR reliability, Q-REDUN parsimony, Q-DISC
       discrimination, Q-GAME gaming-resistance, Q-CONSEQ consequences;
   (d) **decorrelation check** — a new criterion that correlates >0.80 with an existing one (or with a
       cheaply-gameable proxy) is merged/reweighted/rejected, not added;
   (e) **severity-weighting integrity** — the revision must not dilute the false-negative asymmetry (Q-SEV) by
       padding with low-severity criteria;
   (f) hard-item integrity preserved; criteria count grows only if the coverage gain justifies it (parsimony
       budget — Q-BURDEN: reliability falls past ~5–9 items per dimension).
7. **Version + change-control (stable core + dated delta).** Save as a new file
   `agent-harness-evaluation-rubric-vN.md` with an append-only changelog table: per change, {what, trigger,
   driving evidence, quality-parameter delta, superseded-criterion}. The stable core is untouched; changes
   ship as an enumerated dated delta (DO-178C-supplement pattern), nothing edited silently, prior version
   retained. Commit a re-review cadence (per release), not an open-ended "living document."

---

## 6. Independent comparative review (Fable-High)

After v_next is produced, an **independent Fable-model high-effort agent** (not the reviser) scores **both**
versions on the §4 parameters against the **same sealed validation corpus**, and returns a **material-
improvement verdict** with justification: per-parameter delta, coverage delta, whether new/changed criteria
are grounded in real examples, whether anti-bloat held (criteria count vs coverage), and whether any
regression sneaked in. "Materially improved" requires a real coverage/quality gain that survives validation —
not merely more criteria or better prose. A non-improvement verdict sends it back to §5 step 3.

---

## 7. Execution order (gated)

1. ✅ Coverage-audit tooling landed — 20 detectors, 125-type taxonomy, 0-LLM detection, known-bad recall
   3 HIT / 1 PARTIAL (`COVERAGE-REPORT.md`).
2. ✅ Rubric-quality-parameter research returned (two streams) — §4 finalized: 18 psychometric/LLM-eval +
   7 safety-audit-specific parameters, §5 revision loop hardened with the DO-178C/NIST/OWASP acceptance gate.
3. ⏳ Audit runner (§2/§3) — **in progress**: maps the detector corpus onto the 62 criteria, emits the
   scorecard + `failure_corpus_split`. Deterministic layers first (Q-COV/Q-GROUND/Q-REDUN off detector output
   at ~0 LLM); LLM adjudication only for `criterion_fires` on ambiguous [L]/[H] examples.
4. Score the rubric (§4); run the revision loop (§5) → `agent-harness-evaluation-rubric-v2.md`.
5. Fable-High comparative review (§6) → material-improvement verdict.
