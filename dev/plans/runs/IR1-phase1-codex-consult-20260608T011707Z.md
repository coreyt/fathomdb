# IR-1 Phase 1 — codex consensus consult log

**Phase:** IR-1 Phase 1 (define the agentic IR/evidence-recall MEASURE + Claude↔codex consensus)
**Doc under review:** `dev/design/ir-recall-measure.md`
**Reviewer:** codex `codex-cli 0.136.0`, model `gpt-5.5`, `codex exec review --base <pre-draft sha> --dangerously-bypass-approvals-and-sandbox`
**Branch:** `IR1-phase1-20260608T011707Z` · **pre-draft (review base):** `8f6262b754a3aa5212066657610f311d63cf35d9`

---

## Round 1

**Draft commit reviewed:** `eae10590260d55edb37219314793fdb806e3b4bd` (draft of `ir-recall-measure.md` + DOC-INDEX row).

**codex verdict:** "documentation-only, but it adds a design note with a materially inaccurate
runnable-mode description and self-contradictory status metadata." Three findings:

### [P2] FTS-only mode description inaccurate — `ir-recall-measure.md:189`
> This mode is documented as "BM25 over FTS5", but the current text branch explicitly orders
> matches by `write_cursor` while only carrying `bm25()` as a score, so a harness following this
> design would not actually be evaluating a BM25-ranked baseline. Either describe the existing
> FTS/write-cursor branch accurately or make the planned eval require a real BM25-ordered mode
> before treating it as runnable.

**Claude assessment:** ACCEPTED — valid and material. Verified against `lib.rs:3905-3928`: the
text branch SQL is `... ORDER BY write_cursor` and selects `bm25(search_index)` only as a score
column; RRF then fuses on that insertion-order rank (`lib.rs:3955-3956`). The original wording
would have led a Phase-3/4 harness author to treat the production FTS branch as a BM25-ranked
baseline, which it is not.

**Resolution:** §(e) rewritten to distinguish (1) the **production FTS `MATCH` branch**
(write-cursor-ordered) from (2) a **BM25-ranked FTS-only baseline** the eval constructs by
ordering on the already-carried `bm25()` score (harness-level `ORDER BY`, no engine change), and
to require the eval to label which is which. Added an explicit note crediting the consult. This
tightens the measure definition's accuracy.

### [P3] Consensus record incomplete vs. "signed" status — `ir-recall-measure.md:264`
> The document and index claim this measure is Claude↔codex consensus-signed, but the consensus
> section still says it is "to be completed after the codex consensus loop."

**Claude assessment:** ACCEPTED — expected (the placeholder predated this loop).
**Resolution:** § Consensus filled with the converged decision + this finding ledger; the status
line is now backed by a real convergence record.

### [P3] Leftover invocation markup — `ir-recall-measure.md:281-282`
> The final `</content>` / `</invoke>` lines appear to be tool-wrapper artifacts.

**Claude assessment:** ACCEPTED — a stray Write-tool artifact, not part of the doc.
**Resolution:** removed.

**Round-1 outcome:** all three accepted and fixed. None was a definitional disagreement (one
accuracy fix to §(e), two cleanups). No HITL residual.

---

## Round 2 (convergence confirmation)

**Commit reviewed:** `b5ac15ce269933bf81dd5174cc3ef8e679330b00` (round-1 fixes), re-reviewed with
`codex exec review --base 8f6262b754a3aa5212066657610f311d63cf35d9`.

**codex verdict:** "The main design document appears internally coherent" — i.e. the [P2] §(e)
FTS-mode accuracy fix landed and **no definitional issue remains in the measure doc.** One
finding, scoped entirely to *this consult log* (not the measure):

### [P3] Round-2 consensus log incomplete — `IR1-phase1-codex-consult-...:57-59`
> The consult log is referenced as the convergence record, but this section is still a
> placeholder saying "see below" and ends with leftover tool markup instead of the Round 2
> result. Either fill in the Round 2 outcome or remove the placeholder/artifact.

**Claude assessment:** ACCEPTED — correct and self-evidently true (the placeholder predated this
very run; the trailing `</content>` was a stray Write-tool artifact, the same class as round-1
finding 3). **Not a definitional issue with the measure** — codex explicitly called the design
doc coherent. **Resolution:** this section now records the round-2 outcome and the stray markup
is removed.

---

## Round 3 (re-review after the round-2 log completion)

**Commit reviewed:** `<round-2 log fix>` re-reviewed with
`codex exec review --base 8f6262b754a3aa5212066657610f311d63cf35d9`.

**codex verdict:** one **substantive [P2] methodology finding** on the measure doc (the log
issues were gone):

### [P2] Seed qrels with known positives outside pooled results — `ir-recall-measure.md:221-224` (§(f))
> When the required evidence for a query is not returned in any compared mode's top-N, this
> pooling-only procedure never labels that evidence as required, so the Recall@K denominator
> omits the exact misses the eval is supposed to catch. This makes the metric self-confirming
> for hard queries and can overstate product recall; the pool should be augmented with existing
> `expected_top_k_doc_ids` / hand-authored `required_evidence` positives, with pooling used to
> discover additional judgments.

**Claude assessment:** ACCEPTED — a real and important methodology refinement (not a definitional
disagreement; it sharpens, not contradicts, the measure). A pooling-only qrels would drop
unsurfaced required evidence from the denominator and overstate recall on precisely the hard
queries the gate must catch — the same "don't let the measurement confirm itself" hygiene that
the GA recall halt taught. **Resolution:** §(f) rewritten to **seed-then-pool**: the Recall@K
denominator is the **authored** `required_evidence` + `expected_top_k_doc_ids` (independent of
retrieval, always present); pooling **only augments** discovery of additional judgments and can
never remove a seeded required positive. This is the kind of design improvement the independent
consensus loop is for.

---

## Round 4 (re-review after the round-3 §(f) fix)

**codex verdict:** the §(f) seed-then-pool fix landed; two **[P2] internal-consistency** findings
on the schema/scoring (no definitional disagreement — both are "make the doc consistent with
itself / with eu8"):

### [P2] Use the existing `query` key in the additive schema — `ir-recall-measure.md` (b)
> The current eu8 migration path parses each `ground_truth_queries` entry with `q.get("query")`
> and skips entries without it, but this additive schema introduces `query_text` instead… use
> `query` or explicitly require the parser to accept both keys.

**Claude assessment:** ACCEPTED — verified `corpus_subset.rs:239` reads `q.get("query")` and
skips entries lacking it. The draft's `query_text` would have broken the "additive superset of
today's chain shape" claim. **Resolution:** schema now uses **`query`** (the eu8 key), with
`query_id` as the additive new field.

### [P2] Define graded recall over one evidence set — `ir-recall-measure.md` (a)/(b)
> The graded metric is defined as `|required ∩ retrieved@K| / |required|`, but the scoring
> contract later says `supporting` units feed graded recall… clarify whether graded recall is
> over required-only evidence or over required plus supporting evidence.

**Claude assessment:** ACCEPTED — a genuine self-contradiction between (a) and (b).
**Resolution:** **graded recall is over the `required` set only** (same denominator as strict;
they differ only all-or-nothing vs. fractional). `supporting` units are removed from both recall
numbers and reported as a separate **supporting-coverage** diagnostic. (a) and (b) now agree.

---

## Convergence

**CONVERGED (after round 4; round-5 confirmation below).** The measure definition in
`dev/design/ir-recall-measure.md` (a)–(g) is Claude↔codex consensus-signed. Trajectory:
round 1 = one §(e) accuracy fix + two cleanups; round 2 = design doc confirmed "internally
coherent" (only this log's completeness outstanding, fixed); round 3 = §(f) seed-then-pool
methodology refinement (recall not self-confirming); round 4 = two schema/scoring consistency
fixes (eu8 `query` key; graded recall over `required`-only, supporting → separate diagnostic).
Every finding across all rounds was **accepted and resolved**; **none required a definitional
reversal and none is escalated.** **Residuals escalated to HITL: none.** Threshold numbers, the
corpus snapshot, and the gate/no-gate decision are deliberately out of Phase-1 scope (Phase 4
experiments + IR-2 / HITL).

## Round 5 (final confirmation)

_(re-review after the round-4 fixes — recorded below.)_
