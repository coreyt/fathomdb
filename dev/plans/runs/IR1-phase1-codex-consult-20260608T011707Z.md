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

_(see below — re-ran codex over the corrected doc to confirm the [P2] fix lands and no new
definitional issue was introduced.)_
</content>
