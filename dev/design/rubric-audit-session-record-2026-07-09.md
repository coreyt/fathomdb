# Session Record — Agentic-Failure Rubric + Transcript-Audit Loop (2026-07-09)

> **Durable index for this line of work.** Re-read this first; it points at every artifact and states
> where things stand. This is not a chronological log — it is the goal, the relevant work, the findings
> that matter, and what to do next.

---

## 1. Goal

Build and *validate* an evidence-grounded system for evaluating the FathomDB multi-agent harness (Steward,
release Orchestrators, Implementers, LBS/LBO, the ledger tooling, the HITL protocol, the Memex cross-repo
seam) for **agentic failures** — not just guardrail breaches, but **decision-quality failures**: acting on
stale docs/plans/designs, unverified assumptions, deciding important things on short/local knowledge,
dependency-blindness, ignoring architecture/design. The deliverable is two coupled things: a **rubric**
(the instrument) and a **transcript-mining audit loop** that tests the rubric against what actually happens
and drives its revision.

## 2. Work done (relevant only)

- **A rubric** (`agent-harness-evaluation-rubric.md`, v1): 62 binary criteria across 8 dimensions (role
  integrity, verification gating, building-the-right-thing, HITL coherence, provenance, coordination,
  process-to-scale, cross-repo), 12 HARD invariants, a judge specified as a **harness** (deterministic +
  LLM-adjudicated, each criterion tagged `[D]`/`[L]`/`[H]`). Hardened against two real RCAs (CR-047
  finish-vs-delete premise failure; 30-N wrong-unit-of-work) which drove the premise-witness gate (B7) and
  the cost-center criterion (C8). Companion report + the CR-047/30-N calibration analysis.
- **A deterministic transcript-mining suite** (`dev/experiments/rubric-stress-test/`): scans ~750 MB /
  ~2,000 session transcripts + 2,600 subagent outputs at **0 LLM at detection**, surfacing agentic-failure
  examples across 7 families incl. the decision-quality modes. Built via two dynamic workflows; the second
  fixed an **over-prescription** problem in the first (94% one family; 40% of design aimed at ~12% of the
  failure space).
- **An audit of the rubric against the mined corpus** (`.../audit/`): coverage, groundedness, discrimination,
  redundancy, and a tuning/validation split — the scorecard that judges the rubric.
- **A finalized quality-parameter framework + revision method** (`rubric-audit-and-revision-method.md`): 25
  parameters (psychometrics + LLM-eval + safety-audit), a closed-loop revision mechanism with a
  DO-178C/NIST/OWASP-grounded acceptance gate.
- **A revised rubric** (`agent-harness-evaluation-rubric-v2.md`) + an **independent Fable-High comparative
  review** of v1 vs v2.

## 3. Findings (what actually matters)

**F1 — One root cause dominates, at every altitude.** Both documented RCAs are the *same* failure —
**an asserted/surface premise substituted for verified ground truth** — recurring at the disposition level
(CR-047), the plan level (30-N), and, tellingly, in this session's own v2 changelog (a false split-state
claim, §F5). It is the program's single most recurrent failure class, and the rubric now gates it
surface-generally (B7 covers findings, dispositions, *and* plan/scope premises).

**F2 — Coverage is *nominal*, not *effective* — the load-bearing caveat.** The rubric nominally covers ~72%
of observed failure classes, but a criterion *existing* for a failure ≠ that failure being *reliably
detectable*. Decision-quality signals are **polarity-ambiguous** (the same words mark the *catch* as the
*failure*), so their detectors fire ~0 precision on good behavior. Only process-forensic guardrail breaches
(A4 irreversible-action, A6 branch-check, role-bleed, block-override, stall) auto-flag deterministically;
everything else rests on an LLM-adjudication step. Any coverage % must be read with this distinction.

**F3 — Detection is deterministic and cheap; the verdict is where the LLM cost lives.** ~245K records in
36s, 0 LLM, ~2,350 candidates. Confident verdict splits roughly **60% fully deterministic / 37%
needs-adjudication / 3% undetectable-from-transcript**. This is the token-efficiency lever the program
wanted: push *detection* to scripts (free), spend LLM only on *adjudicating* the ambiguous candidates — and
never for scanning.

**F4 — Over-prescription is itself a failure mode of failure-detection.** Building detectors from two RCA
examples produced a suite over-fit to one signature and blind to the broad decision-quality space. The fix
was to *first-class* the decision-quality modes (derive the taxonomy independently, not from the examples)
and to add structural/silent detectors (the 36-h stall has no verbalized catch at all). Recall on the four
known-bad episodes: CR-047 HIT, 30-N HIT, OPP-12 HIT, silent-stall PARTIAL.

**F5 — The meta-loop works: it caught its own author.** v2 was judged **MIXED / acceptance-gate FAILED** by
an independent adversarial review. Its sharpest catch was that v2's changelog asserted "E4 pinned into
validation" when E4's key row is in the *tuning* split — a false premise in the changelog of the rubric that
polices premise-witnessing, i.e. a **B7 UNMET by the rubric's own standard** (now retracted + logged). The
real result of the session is not v2; it is that an *audited, independently-judged, evidence-grounded* loop
surfaces genuine defects including the evaluator's own. That is the capability worth keeping.

**F6 — Anti-bloat is achievable and was achieved.** v2 improved coverage on the two largest gaps purely by
*generalizing* over-narrow criteria (C7, C8, C6) — 0 net criteria added, criterion IDs byte-identical to v1.
The right way to grow a rubric is to generalize/merge, not clone; the audit confirmed the gaps were
over-narrow *scope*, not absence.

## 4. Status & next steps

**Current state:** v2 is the reviewed artifact, verdict MIXED, **not accepted** — it returns to the revision
loop for a **v3**. v1 remains PROPOSED (no HITL sign-off yet). The detector suite, audit, and staged corpus
are reproducible and durable.

1. **v3 rework** (the reviewer's five items; keepers = C7/C8 generalizations, F2 fix, B6 routing, §2.1 note):
   (i) qualify C8's over-firing "retro-marker = UNMET" evidence rule (stakes + pre-change knowability + debug
   exemption; restore v1's dropped clauses); (ii) make rule 9 (Q-SEV) computable — bind the audit's per-example
   severity tiers into a weight vector, or demote to a known-gap; (iii) ground or mark-provisional C6 (0
   confirmed examples) and fix its polarity; (iv) restore C7's "round count" anchor and replace the
   read-paddable deliberation-depth proxy with "the decision-relevant artifact was read before the disposition";
   (v) **actually run the §5 loop** — re-score v2 on validation, run the flagged redundancy re-checks (B7/C1,
   C2/C5, C7/C8), pin E4 fully into validation — before any v3 is accepted. Then independent re-review.
2. **Productize the deterministic detectors** as standing tooling: the Tier-1 auto-feeders (irreversible-action,
   branch-check, role-bleed, block-override, worktree-breach, stall) → the rubric's `[D]` criteria, at ~0 LLM
   (the token-efficiency payoff, F3).
3. **Instrument the rubric's own flagged gaps** (rubric §11): D7 HITL rubber-stamp/acceptance-rate tracking,
   H7 OPP-12 contract-conformance gate before the 0.8.20 co-land, F6 background-agent stall heartbeat, B6
   reviewer escape-rate (the home for undetectable-from-transcript wrong-but-plausible decisions).
4. **HITL decisions pending:** sign-off on the rubric (still PROPOSED); adoption + scoring cadence; whether the
   audit loop runs per-release.

## 5. Artifact index

| Artifact | What it is |
|---|---|
| `dev/design/agent-harness-evaluation-rubric.md` | **v1 rubric** — 62 criteria, 8 dims, judge-harness §2.1, CR-047/30-N calibration §11 |
| `dev/design/agent-harness-evaluation-rubric-report.md` | v1 report — purpose, survey, options, justification, appendices (source signal-eval) |
| `dev/design/agent-harness-evaluation-rubric-v2.md` | **v2** — audit-driven; §13 changelog carries the MIXED verdict + the 5 rework items + the B7 self-correction |
| `dev/design/rubric-audit-and-revision-method.md` | **The method** — audit (§2), output format (§3), 25 quality parameters (§4), revision loop + acceptance gate (§5), Fable-High charter (§6) |
| `dev/experiments/rubric-stress-test/COVERAGE-REPORT.md` | detector suite: 20 detectors, taxonomy, over-prescription analysis, known-bad recall |
| `dev/experiments/rubric-stress-test/coverage/detectors_coverage.py` + `out/coverage_candidates.jsonl` | the coverage detectors + ~2,350 candidate rows |
| `dev/experiments/rubric-stress-test/detectors.py` + `out/*_candidates.jsonl` | the original A–E catch-event detectors |
| `dev/experiments/rubric-stress-test/audit/` | the rubric audit: scorecard (md+json), per-example + per-criterion jsonl, `failure_corpus_split.json` |
| `/home/coreyt/transcript-data/` | staged corpus (~1 GB, both projects) + the deterministic experiment/validation split — **durable working data, never committed, never in-repo** |

**Two RCAs this is grounded in** (Memex-side): `memex/dev/steward/retro-CR047-finish-vs-delete-RCA.md`,
`memex/dev/steward/retro-30N-plan-delta-RCA.md`.

## 6. Operating constraints (carry forward)

- **Never read a raw transcript into an agent's context** — work from detector output + ≤±3-line windows.
  Detection is deterministic; only adjudication uses an LLM.
- **Anti-bloat:** grow the rubric by generalizing/merging, not cloning; one root cause → one criterion.
- **Don't self-assert acceptance:** run the §5 loop and get an *independent, non-author* review before
  accepting a version. Evidence before verdict — the loop exists because authors miss their own premise errors.
