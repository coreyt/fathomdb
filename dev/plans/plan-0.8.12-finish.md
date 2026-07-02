# Plan — Finish 0.8.12 (Memory-quality) to closure + label-only merge

> **Status: APPROVED (HITL 2026-07-02).** Fresh completion plan for the in-flight 0.8.12 release.
> **Target = 0.8.12 only** (not the wider 0.8.x line). Executed by a fresh orchestrator/session in a
> worktree off `origin/main`; **label-only** (manifests stay `0.8.9`, no `v*` tag, no publish).
> Inputs: `runs/0.8.12-handoff.md`, `runs/STATUS-0.8.12.md`,
> `runs/EXP-COV-1-downstream-GPU-replan.md` (+ the 3 PDS-required additions in Phase 1),
> `runs/EXP-COV-1-extraction-manifest.json`, `notes/0.8.12-cpu-embedder-defect-blocks-dense-eval.md`.

## State (verified from git 2026-07-02)

- Slices **0 / 5 / 15 / 20 CLOSED** (codex §9 each); **Slice 40 PARTIAL** (X1 Py-live, X2 mkdocs-strict, X3 ok; R-COV-3 gated).
- **OPP-6 census discharged:** entity coverage solved (local GLiNER matches the frontier), gap = **edges/relations** (strict recall 0.227, CI95 [0.157, 0.306]).
- **Consolidation provider (#7) built;** value-test verdict = **STAY-OFF / opt-in**; named default-ON blocker = the **`t_invalid` FTS/vec projection filter** (recency-exclusion not rebuild-durable).
- **EXP-COV-1 priced extraction DONE** (`$4.79`/`$20`, 272/272, 0 failures, resilience proven) and **preserved**: cache gitignored on-machine, manifest + checksums committed on branch `0.8.12-expcov1-sweep` (`6daf2d94`).
- **Downstream sufficiency verdict = ENVIRONMENT-BLOCKED** (CPU-embedder `.so` defect, no GPU build) → deferred to a GPU re-run.
- Branches: dev **`0.8.12-memory-quality`** (`db1565b6`), sweep **`0.8.12-expcov1-sweep`** (`6daf2d94`). **Nothing merged; manifests `0.8.9`.**

## Phases

### Phase 1 — Resolve the EXP-COV-1 sufficiency verdict (the Slice-10 gate) — `$0`

Execute the GPU downstream re-run per `runs/EXP-COV-1-downstream-GPU-replan.md`, reusing the preserved
cache at **`$0`** (completeness guard; NO re-extraction). Hold the embedder FIXED (CLS-corrected
bge-small, GPU-accelerated) with a **CPU↔CUDA ~1-bit parity check + STOP-and-escalate** if it diverges;
restore the full dense + CE-rerank stack; **pre-registered decision rule unchanged** (SUFFICIENT iff
paired-bootstrap CI-lower-bound of Δ(gold-in-pool) or Δ(MRR) vs same-stack `C-none` > +0.04 on ≥1 powered
class; else CEILING-ABSORBED). **Three PDS-required additions (review 2026-07-02):**

1. **GPU allocation hygiene** — pin a 3090 (cuda:0/1), exclude the K620 display GPU (`FATHOMDB_GPU_EXCLUDE`), honor the vLLM/GPU mutex (no collision with other GPU users).
2. **Build FROM branch `0.8.14-gpu-rerank`** (`embed_batch_cls` / `rerank-cuda` already exist there, rebased + green) — not a fresh `embed-cuda` build.
3. **Same-stack comparison explicit** — the verdict compares against a full-GPU-stack `C-none` re-run in this sweep; the degraded FTS-only `C-none` (multi_session gip@10 0.468) is a prior data point only.

→ **HARD-STOP: report the verdict (SUFFICIENT / CEILING-ABSORBED) to HITL.**

### Phase 2 — Slice-10 disposition (record only — productization is NOT decided here)

- Record the verdict on `STATUS-0.8.12.md` + `EXP-COV-1-results.md` (§4/§5, §0/§2).
- **CEILING-ABSORBED** → resolve **OPP-6 #6** (coverage is not the lever; entity solved, edge lift ceiling-absorbed); **R-COV-3 = resolved-negative**; Slice 10 CLOSED.
- **SUFFICIENT** → record the finding; **R-COV-3 = verdict-computed**; Slice 10 CLOSED-as-recorded. **No productized extraction in 0.8.12.**
- **Either way, productization is OUT of 0.8.12 scope** — the productize/defer decision happens **only after Phase 5** (HITL 2026-07-02).

### Phase 3 — Consolidation loose ends (parallel to Phase 1–2) — INCLUDED

- **`t_invalid` FTS/vec projection filter fix** — make the recency-exclusion rebuild-durable (the named Slice-20 default-ON blocker). A small codex-§9-reviewed slice. **(HITL: keep IN.)**
- **Live TS X1** — complete the TS binding functional harness for `consolidate_with_provider` (was build-gated in Slice 40; needs `node_modules`/build).

### Phase 4 — Complete Slice 40 + release DoD

- Finish verification with R-COV-3 resolved: **X1** (Py-live ✓ + TS-live), **X2** (`mkdocs build --strict`), **X3** (docs + DOC-INDEX), full **R-COV / R-CON AC gate**. Confirm release DoD.

### Phase 5 — Label-only merge to `main`

- Merge `0.8.12-memory-quality` → `main` — **label-only** (manifests stay `0.8.9`; no `v*` tag; no publish). Fold in the preserved sweep artifacts (manifest/harness) as appropriate; reconcile `STATUS`. Record the EXP-COV-1 verdict + Slice-10 disposition.

### After Phase 5 (OUT of this plan's scope)

- **Productization decision** (only if Phase-1 verdict was SUFFICIENT) — a separate HITL call.

## Gates & constraints

- **Sequencing:** Phase 1 → Phase 2 → R-COV-3 (Phase 4). **Phase 3 runs in parallel** with 1–2. **Phase 5 last**, HITL-gated merge.
- **Constraints:** label-only (0.8.9 manifests, no tag/publish) · fathomdb-only push (never memex) · **V-7 held** · don't rewrite history · codex §9 on every code slice · one-writer-per-worktree off `origin/main` · shared `.venv`/`maturin` build mutex.
