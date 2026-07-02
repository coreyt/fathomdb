# EXP-COV-1 downstream re-run — GPU embedder REPLAN (spawn prompt)

> **Purpose.** The OPP-6 EXP-COV-1 *priced* work (relation-focused extraction over
> LOCOMO) is **COMPLETE** (`$4.79`/`$20`, 272/272 docs, cache preserved). The
> *downstream* coverage->outcome verdict is **ENVIRONMENT-BLOCKED** on the installed
> CPU-only embedder `.so` (see `EXP-COV-1-results.md` §6). This doc is the ready-to-spawn
> prompt to finish the downstream read on a **GPU embedder build**, where the fixed
> retrieval + CE-rerank stack is tractable. **Date:** 2026-07-02.

---

## Prompt to the downstream implementer (paste as the slice mandate)

You are the implementer for the **EXP-COV-1 downstream GPU re-run**. The priced
extraction is already done and cached; your job is the `$0`-additional-spend downstream
sufficiency sweep on a GPU embedder. Operate ONLY in your assigned worktree.

### Context you inherit (all on-machine, gitignored EVAL-ONLY)
- **Preserved priced extraction cache** (272/272 LOCOMO sessions, relation-focused
  `claude-haiku`, prompt_version `cov1-relation-1`), reusable via `--resume` at **$0**:
  `data/corpus-data/eval-cache/exp-cov1/relation.claude-haiku.cov1-relation-1.ndjson`
  and its ledger `.../ledger.ndjson`.
- **The runner code** (already merged/committed): `src/python/eval/exp_cov1_common.py`,
  `exp_cov1_extract.py`, `exp_cov1_replay_harness.py`, `exp_cov1_sweep.py`,
  `tests/test_exp_cov1.py`. The sweep already supports `--use-embedder`,
  `--rerank-depth`, `--classes`, per-condition ranks checkpointing, and a completeness
  guard on the priced cache.
- **The `C-none` baseline** already measured (FTS + structural graph; see results §4):
  multi_session gold-in-pool@10 = 0.468 (n=269), temporal 0.913 (n=321), factoid 0.979.
- **The design + decision rule** are FIXED (do not move goalposts): OPP-6 §7 —
  SUFFICIENT iff paired-bootstrap CI lower bound of Δ(gold-in-pool) or Δ(MRR) vs `C-none`
  **> +0.04** on ≥1 powered class; else CEILING-ABSORBED.

### The one prerequisite that unblocks everything: a GPU embedder build
The shared-venv `_fathomdb.abi3.so` is CPU-only (no `embed-cuda`). Build + install a GPU
engine wheel so dense embed + CE-rerank run on the GPU (the machine has RTX 3090s;
`FATHOMDB_EMBED_DEVICE=cuda` is honored only when the `embed-cuda` feature is compiled
in — see `src/rust/crates/fathomdb-embedder/src/candle_bge.rs`). Concretely:
1. Build the pyo3 extension with the CUDA embed feature enabled (e.g. the maturin build
   with `--features embed-cuda` on the engine/py crate; confirm the reranker CUDA path
   too). Do NOT `maturin develop` from a stale-base worktree — build from a clean
   worktree off `origin/main` per the worktree-trap memory, install the wheel into a
   dedicated venv.
2. Verify: `FATHOMDB_EMBED_DEVICE=cuda` + a smoke embed of a ~700-token body should be
   **<100 ms** (vs ~13 s on CPU). Confirm CPU↔CUDA embeddings are ~1-bit identical
   (0.8.7 established this) so the "held-fixed embedder" invariant holds.
3. Sanity: re-run `pytest tests/test_exp_cov1.py` (pure, no GPU) — must stay green.

### Steps (all `$0` — reuse the cache)
1. Copy the preserved cache into your run dir (or point `--relation-cache` at it). The
   sweep's completeness guard will confirm 272/272 present; NO re-extraction, NO spend.
2. Run the **full held-fixed stack** now that GPU makes it tractable — restore dense +
   CE that the CPU run had to drop:
   ```bash
   FATHOMDB_EMBED_DEVICE=cuda python -m eval.exp_cov1_sweep \
     --conditions C-none,C0-floor,C-relation \
     --use-embedder --rerank-depth 50 --classes multi_session,temporal \
     --relation-model claude-haiku \
     --relation-cache <cache.ndjson> --c0-cache <c0.ndjson> \
     --db-dir <db> --ranks <ranks.json> --out-json <out.json>
   ```
   - `--use-embedder` restores dense doc + edge_fact vectors; `--rerank-depth 50`
     restores CE-rerank (the shipped precision lever). Consider also registering the
     doc vector kind for a full fused stack (the sweep dropped it only for CPU speed —
     on GPU it is cheap; add it back in `build_condition_engine` if you want doc-dense).
   - Keep `--classes multi_session,temporal` for the powered relation classes; add
     `factoid` if you want the negative control (no headroom expected).
   - Ranks are checkpointed per condition, so a crash resumes.
3. Compute the paired-bootstrap verdict (built in; `compute_verdicts`) and the precision
   guard, then **fill `EXP-COV-1-results.md` §4/§5 and set the §0/§2 verdict** to
   SUFFICIENT or CEILING-ABSORBED per the pre-registered rule. Also report the
   `C0-floor` anchor and the graph-arm latency on GPU.

### Guardrails
- **No new spend expected** (cache reuse). If for any reason you must re-extract, the
  remaining envelope is **$20 − $4.79 = $15.21**; the resilience preconditions + ledger
  auto-stop are already in `exp_cov1_extract`. Do NOT exceed $20 cumulative.
- LOCOMO is CC-BY-NC — **never commit corpus payloads or extracted fact spans**; persist
  only derived metrics. The extraction cache stays gitignored on-machine.
- Hold the embedder FIXED (CLS-corrected bge-small, GPU-accelerated, byte-identical to
  CPU) — this is NOT an embedder-swap experiment (EXP-M4 is separate).
- If GPU embeddings are NOT ~1-bit identical to CPU, STOP and escalate — the held-fixed
  invariant would be violated.

### Definition of done
- `EXP-COV-1-results.md` §4/§5 filled with the at-power GPU downstream numbers +
  paired-bootstrap CIs; §0/§2 carries the SUFFICIENT / CEILING-ABSORBED verdict per the
  pre-registered rule; the graph-arm latency + precision guard reported; `output.json`
  written. No goalpost movement.

---

## Why replan rather than push the CPU run
The CPU `.so` embeds a ~700-token body in ~13 s and *stalls* mid edge_fact-queue, and
CE-rerank is ~8 s/query — so the fixed dense+CE stack is hours-to-intractable per
condition, and even the degraded FTS+structural-graph fallback runs the graph-arm BFS at
~2-3 s/query (~18-30 min/condition). A GPU build removes the embed/CE bottleneck and lets
the sweep run the *actual* fixed stack, which is required to make the sufficiency verdict
credible rather than a triply-degraded-stack artifact.
