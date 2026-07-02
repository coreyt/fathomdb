# FathomDB 0.8.12 — Plan of Record (Memory-quality) · Steward-owned

> **This is the single source of truth for 0.8.12.** Owner: **Program Steward (PDS)**.
> Canonical location: **`main:dev/plans/plan-0.8.12.md`** (here). It **supersedes** the
> prior branch-local completion plan `plan-0.8.12-finish.md` (now a redirect stub on
> `0.8.12-memory-quality`) and the original build-ladder framing of this same file.
> Authoritative contracts remain in `0.8.12-implementation.md`; live slice state in
> `runs/STATUS-0.8.12.md` (on `0.8.12-memory-quality`).
>
> **Scope:** 0.8.12 only (not the wider 0.8.x line). **Label-only** — manifests stay
> `0.8.9`, no `v*` tag, no publish. Executed by orchestrator/implementer sessions in
> worktrees off `origin/main`; merged to `main` only at Phase 5 (HITL-gated).

## Theme

The two memory-quality capabilities at the head of the retrieval virtuous loop, riding the
0.8.6 provider protocol (#8) and governed-verb boundary (#9): lift ELPS extraction coverage
(#6) and add the consolidation/recency provider (#7, the Mem0-parity update/temporal axis).
Both are **caller-side BYO-LLM** seams — no LLM enters the in-library query path; the
write/index path stays CPU-only and deterministic.

## Current state (verified from git 2026-07-02)

- Slices **0 / 5 / 15 / 20 CLOSED** (codex §9 each). **Slice 40 PARTIAL** — X1 Py-live ✓,
  X2 `mkdocs --strict` ✓, X3 ✓; **R-COV-3 gated** on the Phase 1 verdict.
- **OPP-6 census discharged:** entity coverage **solved** (local GLiNER matches the frontier);
  the residual gap is **edges/relations** — strict recall **0.227**, CI95 `[0.157, 0.306]`.
- **Consolidation provider (#7) built;** value-test verdict = **STAY-OFF / opt-in**. Named
  default-ON blocker = the **`t_invalid` FTS/vec projection filter** (recency-exclusion is not
  rebuild-durable).
- **EXP-COV-1 priced extraction DONE** (`$4.79` of a `$20` cap, 272/272 docs, 0 failures,
  resilience proven) and **preserved**: manifest + checksums committed on
  `0.8.12-expcov1-sweep` (`6daf2d94`); the extraction cache is gitignored in the main tree
  (`data/corpus-data/eval-cache/exp-cov1/…`), sha256-verified against the manifest.
- **Downstream sufficiency verdict = `CEILING-ABSORBED`** (Phase 1 GPU re-run, 2026-07-02,
  `813d9a22` on `0.8.12-expcov1-sweep`): on the full held-fixed GPU stack, every powered Δ vs
  the same-stack C-none is negative (multi_session Δgip@10 **−0.123** [−0.167,−0.078], ΔMRR
  −0.227; temporal −0.069 / −0.244) — added coverage **degrades** retrieval; the
  embedder/retrieval ceiling binds, not coverage. 1-bit CPU↔CUDA parity held; `$0`.
  ⇒ **OPP-6 #6 de-prioritized** (HITL-accepted 2026-07-02). The earlier ENVIRONMENT-BLOCKED
  read (CPU-only `.so`) is resolved by the GPU build
  (`notes/0.8.12-cpu-embedder-defect-blocks-dense-eval.md`).
- Branches (nothing merged; manifests `0.8.9`): dev **`0.8.12-memory-quality`** (`8a2a1006`),
  sweep **`0.8.12-expcov1-sweep`** (`6daf2d94`), GPU features on **`0.8.14-gpu-rerank`**
  (`d9e61c66`, folds into 0.8.14).

## Completion path

### Phase status (canonical progress tracker — update as phases close)

> A completion run records P1→P5 progress here (mirror to `runs/STATUS-0.8.12.md`).
> Legend: `NOT STARTED` · `IN FLIGHT` · `BLOCKED` (on a predecessor) · `DONE`.
> Last updated: **2026-07-02**.

| Phase | Deliverable | Status | Gate / next |
|-------|-------------|--------|-------------|
| **P1** | EXP-COV-1 GPU verdict re-run (`$0`) | **DONE** — verdict **`CEILING-ABSORBED`** (`813d9a22`, 2026-07-02) | verified from git; parity held; `$0` |
| **P2** | Slice-10 disposition (record verdict) | **DONE** — OPP-6 #6 de-prioritized (HITL 2026-07-02); master reconciled (F-15) | R-COV-3 = resolved-negative; Slice 10 CLOSED |
| **P3** | `t_invalid` durability fix + live TS X1 | **IN FLIGHT** — orchestrator commissioned 2026-07-02 | code slices → codex §9 each |
| **P4** | Slice 40 + release DoD (R-COV-3 resolved) | **BLOCKED** on P1–P3 | X1/X2/X3 + R-COV/R-CON AC gate |
| **P5** | Label-only merge → `main` | **BLOCKED** on P4 | **HITL-gated**; retire the `-finish` stub |

### Phase 1 — Resolve the EXP-COV-1 sufficiency verdict (the Slice-10 gate) — `$0`

**Status: DONE (2026-07-02)** — verdict **`CEILING-ABSORBED`**, verified from git (`813d9a22`).
All powered Δ vs same-stack C-none negative; 1-bit CPU↔CUDA parity held; `$0` (cache reused,
272/272). Two GPU-eval build traps were worked around (bundled `libcuda.so.1` → silent CPU
fallback; stale CPU `.so` shadowing the CUDA wheel) — captured to steward memory.

Re-run the downstream sufficiency sweep on a **GPU embedder** reusing the preserved cache at
**`$0`** (completeness guard; NO re-extraction). Composition (each verified from git — they
live in three places): CUDA engine wheel built from **`0.8.14-gpu-rerank`**, the `exp_cov1`
harness from **`0.8.12-expcov1-sweep`**, and the preserved 272/272 cache in the main tree.
Hold the embedder **FIXED** (CLS-corrected bge-small, GPU-accelerated) with a **CPU↔CUDA
~1-bit parity check + STOP-and-escalate** if it diverges. Three PDS-required additions:

1. **GPU allocation hygiene** — pin a 3090 (index 0/1), exclude the K620 (index 2), honor the
   vLLM/GPU mutex.
2. **Build FROM `0.8.14-gpu-rerank`** (the `embed-cuda` + rerank-CUDA path already exists there,
   rebased + green) — not a fresh build off main.
3. **Same-stack `C-none`** — the verdict compares against a full-GPU-stack `C-none` re-run in
   this sweep; the degraded FTS-only `C-none` (multi_session gold-in-pool@10 `0.468`) is a
   prior data point only.

→ **HARD-STOP: report the verdict (SUFFICIENT / CEILING-ABSORBED) to the Steward/HITL.**

### Phase 2 — Slice-10 disposition (record only)

**Taken (2026-07-02): the `CEILING-ABSORBED` branch.** OPP-6 #6 de-prioritized (HITL-accepted);
**R-COV-3 = resolved-negative**; **Slice 10 CLOSED**; master reconciled (§4 0.8.12 row + finding
**F-15**). Do **not** fund the ~$340 full-corpus relation-extraction pass on a coverage-lift
premise.

Record the verdict on `STATUS-0.8.12.md` + `EXP-COV-1-results.md` (§4/§5, §0/§2).

- **CEILING-ABSORBED** → resolve OPP-6 #6 (coverage is not the lever; entity solved, edge lift
  ceiling-absorbed); **R-COV-3 = resolved-negative**; Slice 10 CLOSED.
- **SUFFICIENT** → record the finding; **R-COV-3 = verdict-computed**; Slice 10 CLOSED-as-recorded.

**Productization is OUT of 0.8.12 either way** — the productize/defer decision happens only
after Phase 5 (a separate HITL call).

### Phase 3 — Consolidation loose ends (parallel to Phases 1–2)

- **`t_invalid` FTS/vec projection filter fix** — make the recency-exclusion rebuild-durable
  (the named Slice-20 default-ON blocker). A small codex-§9-reviewed slice.
- **Live TS X1** — complete the TS binding functional harness for `consolidate_with_provider`
  (was build-gated in Slice 40; needs `node_modules`/build).

### Phase 4 — Complete Slice 40 + release DoD

Finish verification with R-COV-3 resolved: **X1** (Py-live ✓ + TS-live), **X2**
(`mkdocs build --strict`), **X3** (docs + DOC-INDEX), full **R-COV / R-CON AC gate**. Confirm
release DoD.

### Phase 5 — Label-only merge to `main`

Merge `0.8.12-memory-quality` → `main` — **label-only** (manifests stay `0.8.9`; no `v*` tag;
no publish). Fold in the preserved sweep artifacts (manifest/harness) as appropriate; reconcile
`STATUS`; record the EXP-COV-1 verdict + Slice-10 disposition. Retire the branch-local
`plan-0.8.12-finish.md` redirect stub as part of the merge.

## Decision rule (Phase 1) — FIXED, no goalpost movement

**SUFFICIENT** iff the paired-bootstrap CI lower bound of Δ(gold-in-pool@10) **or** Δ(MRR) vs
the **same-stack `C-none`** is **> +0.04** on **≥1 powered class** (multi_session, temporal);
**else CEILING-ABSORBED**. LOCOMO is CC-BY-NC — persist only derived metrics; never commit
corpus payloads or fact spans.

## Requirements / DoD (frozen at Slice 0)

Full contracts in `0.8.12-implementation.md`. Headline signals: **R-COV-1** ($0 LLM-free
coverage probe gates any priced run), **R-COV-2** (coverage lift measured + pre-registered, CI,
no under-powered claims), **R-COV-3** (downstream sufficiency verdict — resolved in Phases 1–2),
**R-CON** (consolidation provider value-test + `t_invalid` durability). Track by G-gap + TDD
test names, not invented AC ids.

## Constraints

Label-only (manifests `0.8.9`, no tag/publish) · **fathomdb-only push, never memex** ·
`13` forbidden as minor+micro · two-tier numbering (`x.y.z` real / `x.y.z.p` pico) ·
**V-7 held** · don't rewrite history · **verify from git before narrating** · codex §9 on every
code slice · **one writer per worktree** · shared `.venv`/`maturin` build mutex ·
**background-agent spend needs the user's own direct authorization** (pre-authorize the envelope
in the spawn prompt; Phase 1 is `$0`).

## Execution artifacts (branch map)

- **`0.8.12-memory-quality`** (`8a2a1006`): `runs/STATUS-0.8.12.md` (live state), slice reviews
  (`runs/0.8.12-slice{0,5,15,20,40}-*.md`), `notes/0.8.12-cpu-embedder-defect-blocks-dense-eval.md`.
- **`0.8.12-expcov1-sweep`** (`6daf2d94`): `runs/EXP-COV-1-downstream-GPU-replan.md` (the Phase 1
  spawn mandate), `runs/EXP-COV-1-results.md` (filled in Phase 1–2), `runs/EXP-COV-1-extraction-manifest.json`,
  and the `exp_cov1_*` harness under `src/python/eval/`.
- Preserved cache (gitignored, EVAL-ONLY, main tree):
  `data/corpus-data/eval-cache/exp-cov1/relation.claude-haiku.cov1-relation-1.ndjson` (+ ledger).

## After 0.8.12 (out of scope here)

Productization of relation-focused extraction — a separate HITL call, only if the Phase 1
verdict was SUFFICIENT.
