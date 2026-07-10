# Coverage report — does agentic-failure detection cover the BROAD decision-quality space, or is it over-prescribed to the two RCA (premise-substitution) signatures?

Scope: whole staged corpus (both splits = entire corpus), 3372 files, 242,835 records, 0 LLM at detection time.
Question under test: **Is agentic-failure detection over-prescribed to the two RCA (premise-substitution) episodes, and
does the combined suite now cover the broader space — especially DECISION-QUALITY failures (stale artifacts, unverified
assumptions, short-knowledge decisions, dependency-blindness, ignoring architecture/design)?**

> **Revision note (adversarial-review pass 1 — 8 findings).** This report was rebuilt after an 8-finding review (2 BLOCK, 3
> CONCERN, 3 NIT): (a) recall at SESSION-GROUP granularity reported as episode "catches"; (b) "~8/17 fully deterministic"
> with no precision NUMBER, on detectors firing on bare words in explanatory prose; (c) forward dependency-AWARENESS counted
> as blindness; (d) edits TO the design flagged as ignoring it; (e) whole sessions tagged steward/orchestrator from
> skill-preview text; (f) corpus-wide top wall-clock gaps quoted as the E3 stall figures; (g) memex-push and worktree-breach
> safety detectors firing on compliant commands; (h) unrelated numeric premises paired 1000+ lines apart. All fixed and re-run.
>
> **Revision note (adversarial-review pass 2 — findings C1–C4, this pass).** A second review found the pass-1 precision story
> was still not honest: **(C1, BLOCK)** DQ-STALE-DOC was construct-INVALID — it fired on any turn that *mentioned* staleness,
> which is overwhelmingly the agent CORRECTLY detecting+handling stale artifacts (GOOD behavior), the OPPOSITE of the E7
> failure; **(C2, BLOCK)** the "1.000 [tight CI]" precisions were an artifact of a narrow negation-lexicon auto-adjudicator
> with near-zero FP recall (every DQ detector scored FP=0 → precision exactly 1.000), and NO positive was ever independently
> confirmed; **(C3, CONCERN)** DQ-SHORTKNOWLEDGE fired on the agent transcribing a HITL's ratification (agent-as-clerk), where
> the agent is not the decider; **(C4, NIT)** a stale `precision_windows.txt` orphan still showed pre-fix numbers. **All four
> are fixed and the scripts re-run; nothing overridden.** The corrected precision picture (below) is materially LESS favorable
> and is reported honestly.

Verdict, in one line: **YES over-prescribed** (the prior A-E suite reaches the decision-quality space only when a failure is
verbalized-as-caught; 94% of its volume is one reviewer-catch detector), and **the combined suite broadens the candidate
surface but does NOT deliver a trustworthy DQ auto-flag**: after C1/C2/C3, **DQ-STALE-DOC is re-scoped to a candidate and
fires 0** (the 237 were agent-caught staleness — good behavior), **DQ-SHORTKNOWLEDGE drops to 88 auto and scores 0/4 on a
hand-adjudicated sample** (the "1.000" was illusory), and a real spot-check of the auto-flag positives finds **the only
detectors that survive hand-adjudication are the process-FORENSIC pattern detectors (BRANCH-UNVERIFIED 3/3, IRREVERSIBLE 3/3),
NOT the decision-quality failure detectors**. The two ACID episodes are still caught at the FAILING TURN by one novel signal
each (E3 SILENT-STALL, E4 DQ-NETNEW-DRIFT), but those are candidate-extractors, not auto-flags.

All new work is under `dev/experiments/rubric-stress-test/coverage/`. No git commits. Prior `out/*.jsonl` untouched. Reuses
`parse.py`.

---

## 1. The structural finding: the A-E suite is a verbalized-catch suite, so the decision-quality space is systematically out of reach

The five prior families are ALL **verbalized-catch** detectors — they fire only *after* a human (A-hitl-bounce), a reviewer
(C-review-catch), or the agent itself (B/D/E) has already NAMED a failure in prose. None targets the *act* of a bad decision,
and none reads a silent or structural signal. For the decision-quality space — where the whole point is that the failure is
often *not caught and not verbalized* — the answer is therefore **structurally MISSING**, by construction.

**Volume concentration — the suite is effectively one detector wearing five labels.**
From `out/experiment_candidates.jsonl.summary.json` + `out/validation_candidates.jsonl.summary.json`:

| family | experiment hits | validation hits | val precision (REPORT.md) | share of validation yield |
|---|---|---|---|---|
| C-review-catch | 1120 | 756 | 0.60 | **93.9%** |
| D-self-correction | 76 | 20 | 0.90 | 2.5% |
| E-halt-scout-falsify | 32 | 5 | 1.00 | 0.6% |
| A-hitl-bounce | 24 | 6 | 0.33 (exp 0.83→val 0.33) | 0.7% |
| B-unwitnessed-premise | 17 | 18 | 0.30 (exp 0.75→val 0.30) | 2.2% |

C is 94% of yield at 0.60 precision. A+B — the two families *built from* the RCA episodes — are 40% of the design surface,
~3% of volume, and demonstrably fail to generalize (A 0.83→0.33, B 0.75→0.30 exp→val). Over the full DQ taxonomy the A-E
suite reaches **0/17 deterministically** and **2/17 only-if-verbalized-catch**.

---

## 2. Coverage matrix over the FULL taxonomy — by detectability class and MEASURED precision

17 decision-quality modes. `comb` counts are per-detector from `coverage/out/coverage_candidates.jsonl.summary.json` after all
fixes. **auto** = a shipped detector with `needs_adjudication=False`; **cand** = a deterministic candidate-extractor that
surfaces rows for later adjudication (`needs_adjudication=True`), NOT an auto-flag.

The `precision` column now reports **two** things: the WEAK negation-lexicon auto-adjudicator estimate (an UPPER BOUND, CI
non-trustworthy — see C2) and, where a seeded random sample of positives was hand-adjudicated against bounded windows, the
**hand-sample precision** (the only defensible figure; small N, wide CI). "1.000 [tight]" figures from pass 1 are withdrawn.

| # | DQ mode | detectability | orig A-E | combined | hits/sess | hand-sample precision (Wilson 95% CI) |
|---|---|---|---|---|---|---|
| 1 | DQ-STALE-DOC | det-textual → adjudication | MISSING | **cand** (re-scoped, C1) | **0** | fires 0 after excluding agent-caught staleness — see C1 |
| 2 | DQ-STALE-MTIME | det-structural (git) | MISSING | *not built* (needs git-mtime) | — | — |
| 3 | DQ-STALE-VERSION | det-textual | MISSING | auto | 5/4 | **0/3 → 0.000 [0.000, 0.562]** (agent diagnosing drift = C1 shape) |
| 4 | DQ-ASSUME-TEXTUAL | needs-LLM | MISSING | cand | 132/55 | not claimed (adjudication) |
| 5 | DQ-ASSUME-STRUCTURAL | det-structural | MISSING | auto | 11/9 | **0/3 → 0.000 [0.000, 0.562]** |
| 6 | DQ-SHORTKNOWLEDGE | det-structural | MISSING | auto (action-grounded) | **88/34** | **0/4 → 0.000 [0.000, 0.490]** † |
| 6b| DQ-SHORTKNOWLEDGE-TEXTUAL | needs-LLM | — | cand (ungrounded + clerk dispositions, C3) | 459/64 | not claimed |
| 7 | DQ-SNAP-DISPOSITION | needs-LLM | PARTIAL (verbalized) | cand | 10/7 | not claimed |
| 8 | DQ-UNVERIFIED-METRIC | det-structural | MISSING | auto (1 bounded pair) + 10 cand | 1 auto / 10 cand | **1/1 → 1.000 [0.207, 1.000]** *(N=1)* |
| 9 | DQ-DEPBLIND-RETRO | det-textual | MISSING | auto (corrective-polarity only) | 15/11 | **2/3 → 0.667 [0.208, 0.939]** *(N<20)* |
| 9b| DQ-DEPBLIND-FORWARD | needs-LLM | — | cand (forward awareness — NOT a failure) | 301/75 | not claimed |
| 10 | DQ-DEPBLIND-STRUCTURAL | needs-reference | MISSING | *not built* (needs symbol→dependents) | — | — |
| 11 | DQ-DEPBLIND-CROSSREPO | needs-reference | MISSING | cand (extractor only) | (in DQ-depblind) | not claimed |
| 12 | DQ-IGNOREDESIGN-TEXTUAL | needs-LLM (polarity-ambig) | MISSING | cand | 24/15 | not claimed |
| 13 | DQ-IGNOREDESIGN-STRUCTURAL | det-structural | MISSING | auto (source/schema only) | 12/1 | **0/3 → 0.000 [0.000, 0.562]** (design-COMPLIANT refactor, 1 session) |
| 14 | DQ-NETNEW-DRIFT | needs-reference | MISSING (A-E MISSED OPP-12) | cand | 94/28 | not claimed |
| 15 | DQ-LIMITED-SAMPLE | needs-LLM | MISSING | cand | 9/9 | not claimed |
| 16 | DQ-INCORRECT | needs-reference | PARTIAL (only-if-caught) | cand (caught-reversal subset) | 11/8 | not claimed |
| 17 | DQ-SILENTSTALL | det-structural | MISSING | **cand** (gap visible; stall-vs-pause needs human) | 51/25 | not claimed (F6) |

† **DQ-SHORTKNOWLEDGE — the flagship DQ auto-flag does NOT survive spot-check (C2/C3).** The pass-1 "1.000 [0.965,1.000]" was
the negation-lexicon artifact. A seeded random sample of the 88 auto positives, hand-adjudicated against bounded windows,
scored **0/4** (all four sampled were mechanical "delete the moved converter / delete models.py" refactor follow-through steps
in the memex 0.5.3 worktree move-and-delete work — action-grounded dispositions, but NOT important decisions made on limited
knowledge, which is the C7/C8 construct). C3 additionally split out **17 agent-as-clerk rows** (transcribing a HITL's
ratification — "Recording the ratification decision", "you approved it") that the ≤1-prior-read proxy fired on trivially;
those now route to DQ-SHORTKNOWLEDGE-TEXTUAL. Net: DQ-SHORTKNOWLEDGE is at best a **deterministic disposition-with-thin-local-
context EXTRACTOR whose failure-precision spot-checks near zero** — it should be treated as a candidate, not fed as an
E7/C7/C8 auto-flag.

### Coverage fractions (restated honestly)

| | det-textual | det-structural | needs-reference | needs-LLM | total |
|---|---|---|---|---|---|
| modes in taxonomy | 3 (1,3,9) | 5 (2,5,6,8,13,17→now cand) | 4 (10,11,14,16) | 4 (4,7,12,15) | 17 |
| **ORIGINAL A-E reaches** | 0 | 0 | 0 | 0 (2 verbalized-only) | **0/17 det** |
| **COMBINED — has a `needs_adj=False` auto-flag** | 2 (3,9) | 4 (5,6,8-partial,13) | 0 | 0 | **6/17** (C1 drops #1 STALE-DOC to candidate) |
| **COMBINED — auto-flag that SURVIVES hand-adjudication** | 1 (9 partial 2/3) | 1 (8, N=1) | 0 | 0 | **0–2/17** (only DEPBLIND-RETRO 2/3 and UNVERIFIED-METRIC 1/1; SHORTKNOWLEDGE/STALE-VERSION/ASSUME/IGNOREDESIGN all 0/n) |
| **COMBINED — + candidate-extractors** | +STALE-DOC | +SILENT-STALL | 3 | 4 | **13/17 reached; 2 unbuilt; 2 residue** |

The prior report's "**14/17 (82%); ~8/17 (47%) fully deterministic**" AND its own pass-1 replacement "**7/17 auto-flag, 2 with
tight CIs**" are **both withdrawn**. Corrected: **6/17** modes have a `needs_adjudication=False` auto-flag detector (C1 moved
DQ-STALE-DOC to a candidate), and when those auto-flags are spot-checked against a hand-adjudicated random sample of their own
positives, **NONE of the DQ failure detectors reaches a clean confirmed-failure precision at meaningful N** — the flagship
DQ-SHORTKNOWLEDGE is 0/4, DQ-STALE-VERSION / DQ-ASSUME-STRUCTURAL / DQ-IGNOREDESIGN-STRUCTURAL are 0/3, DQ-DEPBLIND-RETRO is
2/3, DQ-UNVERIFIED-METRIC is 1/1 (N=1). The only auto-flags that hold up are the **process-forensic pattern detectors**
BRANCH-UNVERIFIED-BEFORE-COMMIT (3/3 sample; auto-adjudicator 0.842) and IRREVERSIBLE-ACTION-UNGATED (3/3), which are rubric
A-dimension process signals, not DQ failure detectors. DQ-SILENTSTALL remains a candidate-extractor (F6).

> **Two independent precision measures (C2), kept separate.** `precision_estimate.py` now reports (A) a WEAK negation-lexicon
> auto-adjudicator over the full auto-flag population — disclosed as an UPPER BOUND with UNMEASURED FP sensitivity and a
> NON-trustworthy CI (it only measures "the negation lexicon matched nothing", which is why pass-1 got 1.000 everywhere); and
> (B) a hand-adjudicated precision over a seeded random sample of POSITIVES (`out/handlabels.tsv`, 24 windows, single
> adjudicator), whose Wilson CI reflects the true small sample. Only (B) is a defensible precision claim. Full data:
> `out/precision_estimate.json`, `out/precision_windows.txt` (regenerated each run to surface the next unlabelled sample).

Process/role forensics (rubric dims A–H, not DQ modes) after fixes: BRANCH-UNVERIFIED-BEFORE-COMMIT (139/22, hand-sample
**3/3 → 1.000 [0.438,1.000]**; auto-adjudicator upper bound 0.842), IRREVERSIBLE-ACTION-UNGATED (18/11, hand-sample **3/3 →
1.000 [0.438,1.000]**), WORKTREE-DISCIPLINE-BREACH (1/1 after F7; hand-sample 0/1), PREMATURE-TERM (553, candidate),
BLOCK-OVERRIDE (265, candidate), **ROLE-BLEED-SOURCE-EDIT (144, ALL candidate — 0 auto-flags; see F5)**. These process-pattern
auto-flags are the ones that survive spot-check; the DQ failure auto-flags do not (§2 matrix, C2).

---

## 3. Recall acid test — TURN-LOCALIZED (F1/F6), with group base-rate reported SEPARATELY

Prior recall scored a "catch" if a detector fired ANYWHERE in a 72–87-file session group — but these detectors already fire
in 40–77 of the corpus's session groups, so a group-level fire is base-rate, not recall. `coverage/recall_localized.py` now
defines each episode's ACTUAL FAILING TURN (file + anchor line + bounded window = its immediate causal cluster) and scores
ONLY rows overlapping that window as a catch. Group fires are reported separately as "fired-in-group (base rate)".

Anchors (grounded in `coverage/out/episode_hits.jsonl`): E1 memex/2fa060bc L1613 ±40 ("no live consumers"/"already
superseded"); E2 memex/2fa060bc L1475 ±40 ("~98 call sites"/"ScheduledTask is a duplicate"); E3 fathomdb/ebec94c7 L1226 &
L853 ±10 (the wall-clock gaps); E4 fathomdb/60b48af5 L2204/L1758 & ebec94c7 L83 ±40 (exists-vs-net-new audit).

| episode | turn-localized ON-target catch (the recall credit) | group base-rate context (NOT a catch) |
|---|---|---|
| **E1** CR-047 (BUILT-FROM) | **A-E B-unwitnessed-premise @ L1617** (`no_consumers+action`, ON the premise turn); DQ-SHORTKNOWLEDGE @ L1603/1642 | PREMATURE-TERM 28, DQ-SHORTKNOWLEDGE-TEXTUAL 18 (3 localized), DQ-DEPBLIND-FORWARD 13 across 72 files — DQ-STALE-DOC now **0** (C1), was 12 pre-fix |
| **E2** 30-N (BUILT-FROM) | **A-E B-unwitnessed-premise @ L1495** (`n_sites` = the ~98-call-sites premise); SILENT-STALL 27.8h @ L1496 | PREMATURE-TERM 28, DQ-DEPBLIND-FORWARD 19, DQ-SHORTKNOWLEDGE-TEXTUAL 18 across 87 files — DQ-STALE-DOC now **0** (C1), was 12 pre-fix |
| **E3** 36h stall (**ACID**) | **SILENT-STALL @ L1226 (35.7h) + L853 (23.8h)** — the genuine novel catch; **A-E fire ZERO on the gap** (C-review-catch @ L1222 is a nearby review finding, not the stall) | DQ-SHORTKNOWLEDGE-TEXTUAL 27, BLOCK-OVERRIDE 15 across 32 files — base rate |
| **E4** OPP-12 drift (**ACID**) | **DQ-NETNEW-DRIFT @ L1739/L1754/L1776/L2178/L2220** (the "~90% net-new" / exists-vs-net-new audit); A-E C-review-catch/D-self-correction also localize at the AUDIT turn | DQ-SHORTKNOWLEDGE-TEXTUAL 49 (only 6 localized), DQ-NETNEW-DRIFT 35 (only 5 localized) across 76 files |

**Corrected recall verdict (F1).** The affirmative "the two ACID episodes are caught by the new additions" holds **only** for
the turn-localized catches: **E3 = SILENT-STALL** (structural, A-E blind) and **E4 = DQ-NETNEW-DRIFT** at the audit turn. The
built-from episodes localize on their B-unwitnessed-premise signature (**E1 @ L1617, E2 @ L1495 sig=n_sites**). The prior
report's E1/E2 "DQ-axis caught: DQ-SHORTKNOWLEDGE 27, DQ-DEPBLIND-RETRO 13, DQ-STALE-DOC 12" figures were **group base-rate,
not catches** (and DQ-STALE-DOC is now **0** corpus-wide under the C1 re-scope), and are now shown in the right-hand column
with their `localized=` counts (mostly 0). Full data:
`coverage/out/recall_localized.json`.

**E3 gap figures corrected (F6).** The prior "36.3h + 46.9h + 41.8h + spawn-anchored" was **corpus-wide TOP gaps from
unrelated sessions, misattributed to E3**. The actual localized E3 signal is **two bare large-gaps in ebec94c7: 35.7h @ L1226
and 23.8h @ L853; f57b5dee has none; NEITHER is spawn-anchored** (the earlier side of each gap is a text-less system/history
line). It is still a real, valuable novel catch — the ~36h shape is present — but the honest figure is one ~35.7h gap, not
three inflated ones.

---

## 4. What changed vs the reviewed version — the 8 findings, each with its re-run evidence

**F1 (BLOCK) — recall was session-granularity, reported as episode catches.** Fixed: `recall_localized.py` scores only rows
inside the failing-turn window; group fires reported separately with per-detector `localized=` vs `in_group=` vs
`corpus_session_groups=`. Recall verdict restated (§3) to credit only E3 SILENT-STALL, E4 NETNEW-DRIFT, E1/E2 n_sites/premise.

**F2 (BLOCK, pass 1) — faked precision on "auto-flag" detectors.** Pass-1 fix (a) added a per-detector negation lexicon and (b)
made DQ-SHORTKNOWLEDGE require an action target + exclude explanatory prose (632 → 105 auto + 442 candidate; the cited FP
9dfc869c L324 no longer emitted). **This was only cosmetically resolved and is superseded by C2 below.** The negation-lexicon
adjudicator has near-zero FP recall, so every DQ detector scored FP=0 → precision 1.000 with an illusory CI, and no positive
was ever confirmed.

**C2 (BLOCK, pass 2) — the 1.000 CIs were an auto-adjudicator artifact.** Fixed: `precision_estimate.py` now (A) reports the
negation-lexicon estimate explicitly as a WEAK UPPER BOUND with UNMEASURED FP sensitivity and a NON-trustworthy CI, and (B)
draws a **seeded random sample of the auto-flag POSITIVES** per detector, emits bounded ±2-line windows
(`out/precision_windows.txt`), and computes a **hand-adjudicated precision** from `out/handlabels.tsv` with a Wilson CI that
reflects the true small sample. Result (24 windows, single adjudicator): DQ-SHORTKNOWLEDGE **0/4**, DQ-STALE-VERSION **0/3**,
DQ-ASSUME-STRUCTURAL **0/3**, DQ-IGNOREDESIGN-STRUCTURAL **0/3**, DQ-DEPBLIND-RETRO **2/3**, DQ-UNVERIFIED-METRIC **1/1**;
process patterns BRANCH-UNVERIFIED **3/3**, IRREVERSIBLE **3/3**, WORKTREE 0/1. The DQ auto-flags' "1.000 tight CI" is withdrawn.

**C1 (BLOCK, pass 2) — DQ-STALE-DOC was construct-invalid.** Fixed: removed from the auto-flag textual banks; re-scoped into a
candidate-extractor `d_stale_doc` (`needs_adjudication=True`) that fires ONLY when a staleness mention is co-located with a
RELIANCE/decision signal AND there is NO correction/verification language in the turn AND no verify tool_use in the adjacent
turns — i.e. UNWITTING reliance, the actual E7 failure. Effect: **237 auto → 0** (after fixing a word-boundary bug in the
correction lexicon that had let 3 agent-handled cases through). The 237 were the agent CORRECTLY detecting+handling staleness
("verified before proceeding", "local main is stale — 70 commits behind", "I'll supersede", "I'll update -A in place") — GOOD
behavior, the opposite of the failure. E7 no longer receives a DQ-STALE auto-flag.

**C3 (CONCERN, pass 2) — DQ-SHORTKNOWLEDGE fired on agent-as-clerk.** Fixed: a `CLERK_RE` guard routes decision-keyword turns
where the disposition is the HITL's ("recording your/the ratification", "per your decision", "you approved", "Approved —
merging") to DQ-SHORTKNOWLEDGE-TEXTUAL (candidate, `clerk_disposition=True`) instead of auto-flagging. Effect: 105 → **88 auto
+ 17 clerk-rerouted** (verified: "Recording the ratification decision", "you approved it").

**C4 (NIT, pass 2) — stale precision_windows.txt orphan.** Fixed: `precision_windows.txt` is now owned and regenerated by
`precision_estimate.py` each run (post-fix content, 4-column label format); the pre-fix generator `precision_sample.py` was
deleted so it can't re-orphan.

**F3 (CONCERN) — DEPBLIND counted forward awareness as blindness.** Fixed: `DQ_DEPBLIND` split into `DQ_DEPBLIND_RETRO`
(corrective polarity: didn't-account / missed-that / broke-because / forgot-that → **auto**, 15/11) and `DQ_DEPBLIND_FORWARD`
(depends-on / blocked-by / requires-first → **candidate**, 301/75, `needs_adjudication=True`). The cited 022c250f L104
"WM2 depends on advancing FathomDB" now routes to the FORWARD candidate bank, not the failure tally.

**F4 (CONCERN) — IGNOREDESIGN flagged edits TO the design.** Fixed: removed the bare `record-lifecycle` token from
`GOVERNED_PATH` and added a `DOC_PATH_EXCLUDE` (dev/design/, dev/adr/, /adr, record-lifecycle, /docs/) applied to the edited
path BEFORE the governed predicate. Effect: 37/4 → **12/1**; the OPP-12 design-doc edits (60b48af5 projection-registry…md,
ebec94c7/00b4c81c OPP-12-leverage-ledger-update.md) no longer fire. **The prior E4 "structurally caught:
DQ-IGNOREDESIGN-STRUCTURAL 24" claim is withdrawn** — E4 is caught by DQ-NETNEW-DRIFT, not by governed-edit structure.

**F5 (CONCERN) — ROLE-BLEED was whole-session misclassification.** Fixed: role now derives from an EXPLICIT command-mode
marker (`<command-name>/steward|orchestrate`) and is scoped to fathomdb sessions for any auto-flag; the leaky prose keywords
("implementer subagents in worktrees", "codex §9 gate" — verbatim in the /orchestrate skill description echoed in nearly
every transcript) were removed. Result: **0 command-mode auto-flags** (the 5 explicit-steward fathomdb sessions have no
main-thread source edits — stewards correctly delegate), and the residual **144 prose-only rows are ALL candidates**
(`needs_adjudication=True`). Precision of the auto-flag tier = **0/0 → the detector auto-flags nothing**; the prior 216/11 was
exactly the whole-session misclassification the review described (011b5a59 writing eval/locomo_loader.py, etc.). Re-tiered to
the candidate column in §2/§5.

**F6 (CONCERN) — SILENT-STALL evidence mismatch + mis-tallied as deterministic.** Fixed: E3 gaps corrected to the actual
localized values (§3). SILENT-STALL emits `needs_adjudication=True`, so it is now counted as a **candidate-extractor**, not
toward the "fully deterministic" fraction (which now counts only `needs_adjudication=False` detectors with a measured
precision).

**F7 (NIT) — safety detectors fired on compliant commands.** Fixed. `push_memex` now requires the git-push REMOTE (positional
name after flags, or a `github.com/coreyt/memex` URL) to be memex — not any ref text; the cited f2f071a5 push of the FATHOMDB
branch `docs/0.8.11-memex-liaison-handoff-prompt` (remote = fathomdb) **no longer fires** (0 push_memex fires corpus-wide).
`WORKTREE-DISCIPLINE-BREACH` now requires a shared-.venv/shared-checkout mutation marker AND excludes any command naming an
isolated venv / "NOT shared": 17/10 → **1/1**; the cited 5b3699c3 and agent-aefd69fd isolated builds are no longer breaches.

**F8 (NIT) — UNVERIFIED-METRIC over-paired.** Fixed: an assert/contra pair must now share a REFERENT (subject noun and/or
number, extracted from a ±120c window around each match) AND sit within a **40-line** window to be emitted as
deterministic-structural; shared-referent-but-far pairs drop to candidate; no-shared-referent pairs are dropped. Effect: the
genuine 30-N pair **d2ae8c40 L327→L352 (dist=25) stays as the ONE auto-flag**; the cross-topic pairs (eb9a1e78 $0.75 "refuted"
1166 lines later; b9422d49 "no consumer" 1694 lines later) become candidates. 15 → **1 auto + 10 candidate**.

---

## 5. Auto-feed mapping (post-fix) — which detectors survive hand-adjudication → rubric criteria

Feed as an auto-flag ONLY where the hand-adjudicated sample supports it. `precision` below is the **hand-sample** figure
(defensible); the auto-adjudicator upper bound is in parentheses and is NOT trustworthy on its own (C2).

| detector | N (hits/sess) | hand-sample precision (95% CI) | rubric | feed as |
|---|---|---|---|---|
| BRANCH-UNVERIFIED-BEFORE-COMMIT | 139/22 | **3/3 → 1.000 [0.438,1.000]** (auto-adj 0.842) | A6 | **auto-flag** (process pattern — holds up) |
| IRREVERSIBLE-ACTION-UNGATED | 18/11 | **3/3 → 1.000 [0.438,1.000]** | A4 | **auto-flag** (process pattern; benign clean-worktree resets) |
| DQ-DEPBLIND-RETRO (corrective) | 15/11 | 2/3 → 0.667 [0.208,0.939] | F2,F6,C8 | auto-flag with caution (small N; 1/3 was a review finding about others) |
| DQ-UNVERIFIED-METRIC (bounded pair) | 1 | 1/1 → 1.000 [0.207,1.000] | C8,B7 | auto-flag (single instance — the genuine 30-N pair) |
| DQ-SHORTKNOWLEDGE (action-grounded) | 88/34 | **0/4 → 0.000 [0.000,0.490]** | C7,C8,B7 | **DOWNGRADE to candidate** (mechanical-disposition FPs; C2/C3) |
| DQ-STALE-VERSION | 5/4 | 0/3 → 0.000 [0.000,0.562] | E7 | **DOWNGRADE to candidate** (agent diagnosing drift; C1 shape) |
| DQ-ASSUME-STRUCTURAL | 11/9 | 0/3 → 0.000 [0.000,0.562] | B7,C1 | **DOWNGRADE to candidate** (careful-reasoning FPs) |
| DQ-IGNOREDESIGN-STRUCTURAL (src/schema) | 12/1 | 0/3 → 0.000 [0.000,0.562] | C2,C3 | **DOWNGRADE to candidate** (design-COMPLIANT refactor; 1 session) |
| WORKTREE-DISCIPLINE-BREACH | 1/1 | 0/1 → 0.000 [0.000,0.793] | A5 | candidate (corrective/preventive commit; single instance) |
| DQ-STALE-DOC (re-scoped, C1) | **0** | fires 0 after excluding agent-handled staleness | E7 | candidate → adjudication (no positives; the 237 were good behavior) |
| DQ-SHORTKNOWLEDGE-TEXTUAL / DQ-ASSUME-TEXTUAL / DQ-SNAP-DISPOSITION / DQ-LIMITED-SAMPLE / DQ-IGNOREDESIGN-TEXTUAL | 459 / 132 / 10 / 9 / 24 | not claimed | C1/C2/C5/C7/C8/G | candidate → LLM adjudication |
| DQ-DEPBLIND-FORWARD | 301/75 | not claimed | F2,C8 | candidate (forward awareness — usually NOT a failure) |
| DQ-NETNEW-DRIFT / DQ-INCORRECT / DQ-DEPBLIND-CROSSREPO / DQ-UNVERIFIED-METRIC (far) | 94 / 11 / — / 10 | not claimed | C3,C1,C5,F1 | candidate → code-grounded exists-vs-net-new / reference audit |
| SILENT-STALL | 51/25 | not claimed | F6 | candidate → human confirms stall-vs-pause |
| ROLE-BLEED-SOURCE-EDIT | 144/7 | 0 auto (all candidate) | A1 | candidate → human confirms role |
| PREMATURE-TERM / BLOCK-OVERRIDE | 553 / 265 | not claimed | D, C-anchor | candidate → human review |

**Net auto-feed recommendation after C1–C4:** only two auto-flags are supported by their own spot-check, and both are
process-FORENSIC (BRANCH-UNVERIFIED → A6, IRREVERSIBLE-ACTION-UNGATED → A4). Every DECISION-QUALITY detector should be fed as a
**candidate to adjudication**, not as an E7/C-dimension auto-flag — the hand sample shows their firing rows are dominated by
GOOD behavior (agent detecting staleness, doing design-compliant refactors, reasoning carefully) or mechanical dispositions.

---

## 6. Genuinely not-yet-built and undetectable (unchanged, still honest)

**Not built — need reference/git infra:** DQ-STALE-MTIME (#2, cited-doc git-mtime vs described-code recency),
DQ-DEPBLIND-STRUCTURAL (#10, symbol→dependents graph), DQ-DEPBLIND-CROSSREPO (#11, memex↔fathomdb counterparty state — a
deterministic candidate-extractor exists but the reference comparison does not).

**Genuinely UNDETECTABLE from transcripts alone:** DQ-INCORRECT when never caught; the absence-of-notification fact behind a
silent stall (the GAP is visible via SILENT-STALL, but "auto-resume never fired / no alert sent" is machinery outside the
transcript); any DQ failure whose ground truth lives only in external artifacts not cited in the transcript.

---

## 7. Answer

- **Over-prescribed?** Yes. The A-E suite is 94% one reviewer-catch detector; its two RCA-derived families are 40% of the
  design for ~12% of the space and fail to generalize; it reaches **0/17** DQ modes deterministically and catches the two
  ACID episodes on-target **nowhere**.
- **Does the combined suite cover the broader space?** It broadens the CANDIDATE surface honestly (13/17 modes reached as
  deterministic candidate-extractors; 2 need unbuilt reference/git infra; a small residue is undetectable) — but after C1–C4
  it delivers **no trustworthy DECISION-QUALITY auto-flag**. Only **6/17** modes have a `needs_adjudication=False` auto-flag,
  and a hand-adjudicated spot-check of their own positives shows the DQ ones fail as failure signals (DQ-SHORTKNOWLEDGE 0/4,
  DQ-STALE-VERSION/ASSUME/IGNOREDESIGN 0/3, DQ-STALE-DOC re-scoped to 0). The only auto-flags that survive are the
  process-forensic patterns BRANCH-UNVERIFIED (3/3) and IRREVERSIBLE-ACTION-UNGATED (3/3), which are rubric A-dimension
  signals, not DQ detectors. **The honest conclusion: the DQ space is reachable as adjudication CANDIDATES, but the "verbalized"
  lexical/structural signals mostly fire on GOOD behavior (agent detecting staleness, design-compliant refactors, careful
  reasoning) — they must be adjudicated, not auto-flagged.** The two ACID episodes are still caught at the FAILING TURN by one
  novel candidate signal each — **E3 SILENT-STALL (35.7h/23.8h gaps), E4 DQ-NETNEW-DRIFT** — both invisible to the A-E suite.
  Group base-rate fires are reported as context, never as recall.

Artifacts (all under `coverage/`): `detectors_coverage.py` (C1 `d_stale_doc` + STALE_RELIANCE/STALE_CORRECTION; C3 `CLERK_RE`),
`run_coverage.py`, `recall_localized.py` (F1/F6), `precision_estimate.py` (C2 — weak auto-adjudicator + hand-sample), `dq_probe.py`,
`locate_episodes.py`, and `out/{coverage_candidates.jsonl(.summary.json), recall_localized.json, precision_estimate.json,
precision_windows.txt, handlabels.tsv, episode_hits.jsonl}`. `precision_sample.py` was deleted (C4 — superseded orphan
generator). Prior `recall_combined.py`/`recall_combined.json` retained for the group base-rate reference. No git commits; prior
top-level `out/` untouched.
