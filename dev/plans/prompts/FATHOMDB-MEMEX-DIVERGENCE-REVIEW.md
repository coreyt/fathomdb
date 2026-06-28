# FathomDB ⇄ Memex — Stack-Alignment / Divergence Review (SQLite → Rust → Python → Memex needs)

You are a **FathomDB-side integration reviewer**. Your job: confirm FathomDB and its primary consumer
**Memex** are **not diverging** across the whole stack — **SQLite storage → Rust engine API → Python
binding → Memex's actual needs** — and produce a divergence report with severities + recommended actions
for HITL. **This is a review: read-only / report-only. Change no code in either repo.** You verify the
FathomDB surface yourself, then **spawn and interview a Memex liaison agent** to learn Memex's side, then
reconcile.

## Why this exists

Memex is FathomDB's primary consumer ([[fathomdb-consumer-agents]]); the integration surface spans four
layers and they can silently drift. **One divergence is already known and load-bearing:** Memex installs
`fathomdb` as a `uv` **path dependency on `../fathomdb/python`** — a **recovered, sourceless `0.1.0`
shim** (`.pyc`+`.so`), NOT the current `src/python` (0.8.x maturin SDK). So Memex's *installed* binding
predates everything in 0.8.x (no `ingest_with_extractor`, no graph verbs). Treat confirming/quantifying
this as a primary objective, and look for others like it.

## Roles & rules

- **You (reviewer):** inventory the FathomDB surface from the repo + git (authoritative over any doc),
  run the interview, reconcile, write the report. Cite `file:line`. Do not edit code; do not push.
- **Memex liaison (you spawn it):** answers for Memex's *needs and usage* from the Memex repo + ratified
  records. It must NOT reopen frozen contracts or make product calls — those route to HITL.
- **HITL (coreyt):** owns any product/roadmap/threshold call and any contract change the review surfaces.

---

## Phase A — Inventory the FathomDB stack surface (do this FIRST, from the repo)

Build a layer-by-layer inventory of *what FathomDB actually exposes today on `main`*. For each layer
capture the concrete surface (names, signatures, columns, version) with `file:line` — this is the
"supply" side you'll reconcile against Memex's "demand."

1. **SQLite layer (the storage contract).** `src/rust/crates/fathomdb-engine/src/lib.rs` (+ any schema/
   migration module). Capture: the current **`SCHEMA_VERSION`** (expected 14 — verify) and the full
   migration ladder; the live tables + columns — `canonical_nodes`, `canonical_edges` (incl. the **G11
   enrichment columns** `body`/`t_valid`/`t_invalid`/`confidence`/`extractor_model_id`),
   `operational_mutations`, the FTS5 tables, and the vector/1-bit-binary-quant storage; the single-writer
   `{db}.lock` invariant. Note anything a consumer could depend on at the file/schema level.
2. **Rust engine layer (the API).** The public `Engine` surface + verbs in `lib.rs`: `write`, `search` /
   `search_filtered` / `search_reranked`, the governed `read.*` verbs (`read_get`/`read_get_many`/
   `read_list`/`read_search`), graph `graph_neighbors` / `search_expand`, `ingest_with_extractor`, admin/
   doctor, and the `fathomdb.extract.v1` protocol (cross-check the brief
   `dev/plans/prompts/IR-C-byo-llm-extraction-harness-memex.md`). Capture result/error types.
3. **Python binding layer (what Memex imports).** The public surface of **`src/python/fathomdb/`**:
   `__init__.py` exports (`Engine`, `EngineConfig`, `admin`, `errors`, `graph`, `read`, `types`,
   `IngestWithExtractorReceipt`), and `engine.py` / `graph.py` / `read.py` / `admin.py` / `types.py` /
   `errors.py` / `config.py`. The pyo3 layer `src/rust/crates/fathomdb-py/src/lib.rs`. The packaging:
   `src/python/pyproject.toml` (**maturin, version — expected 0.8.x**, the `features` surface). Note the
   **X1 cross-binding parity** baseline vs TS (`src/ts/src/`) so you can tell whether a Memex need is a
   Python-only gap.
4. **The shipped-vs-installed delta.** Determine what the **0.1.0 shim** Memex actually imports exposes
   (it's sourceless `.pyc`+`.so` under the gitignored top-level `python/`; inspect symbols if feasible)
   versus the 0.8.x `src/python` surface from step 3. This delta is the core of the version-divergence
   finding.

Record Phase A as inventory tables before interviewing — you'll diff against the answers.

---

## Phase B — Spawn & interview the Memex liaison

Spawn a subagent (general-purpose) that **adopts the role prompt at
`~/projects/memex/dev/fathomdb/MEMEX-LIAISON-AGENT.md`** and operates in `~/projects/memex`. Tell it to
read and follow that prompt, then answer the interview below from the Memex repo + ratified records
(`dev/elps/*`, `dev/fathomdb/*`, `src/memex/fathom_store.py`, `src/memex/fathom_facade.py`), citing
files. Use follow-up messages (SendMessage) to drill into any answer that reveals a divergence.

**Interview question set (maps to the stack layers + known risks):**

1. **Install/version.** How is `fathomdb` installed (confirm the `uv` path dep on `../fathomdb/python`)?
   What FathomDB **version/surface does Memex assume**? Is Memex aware its installed binding is the
   **sourceless `0.1.0` shim**, not 0.8.x? Does Memex's code call anything (`ingest_with_extractor`,
   `graph_neighbors`/`search_expand`, reranked search, the governed `read.*` verbs) that the shim does
   **not** expose? If so, how does it work today (mock? unused path? broken?)?
2. **Consumption surface.** Which **exact** FathomDB Python symbols/signatures does Memex call (inventory
   from `fathom_store.py` + the facades)? For each, the expected args, result shape, and error type.
3. **SQLite-level assumptions.** Does Memex touch the `.db` file directly, assume any table/column/schema
   version, or go **only** through the binding? (Cross-check `dev/fathomdb/design-fathomdb-cutover.md`.)
4. **extract.v1 / ELPS call path.** Does Memex *call* FathomDB's BYO-LLM `ingest_with_extractor`, or only
   *produce* extraction that FathomDB consumes? Where's the boundary, and which side spawns whom?
5. **Needs not yet met.** What does Memex need that FathomDB doesn't expose (missing verb, result field,
   error granularity, batch API, perf characteristic)?
6. **Stability surface.** What does Memex rely on that would **break** if FathomDB changed it (API names,
   result/error shapes, the `source_type` vocab, determinism, the footprint invariant)?
7. **Footprint.** Does Memex's usage assume CPU-only / no-API / local-first / single-writer, and does
   anything in Memex's roadmap pressure that?
8. **Release/versioning.** How does Memex want FathomDB updates delivered — a built **0.8.x wheel**, the
   path dep, a pinned version? What's the plan to retire the `0.1.0` shim?
9. **HITL items.** Anything the liaison flags as `→ HITL (coreyt)` (product call or a frozen-contract
   change) — capture verbatim; do not try to resolve it.

---

## Phase C — Reconcile → divergence report

Diff Phase A (FathomDB supply) against Phase B (Memex demand). Build a **divergence matrix**: for each
Memex usage/need → one of **MET / DIVERGENT / GAP / UNUSED**, with severity (P1 breaks integration · P2
will break on a known-planned change · P3 cosmetic/cleanup) and a recommended action. Specifically
resolve:

- **Version divergence (the `0.1.0` shim vs 0.8.x)** — quantify: which Memex call sites need 0.8.x surface
  the shim lacks; is Memex currently running against a binding that can't satisfy its own code? Recommend
  the **`maturin build` of `src/python` → 0.8.x wheel** cutover and who does it.
- **API/result/error drift** — any verb/field/error Memex calls that FathomDB renamed/changed/removed,
  or shapes that differ between the shim and 0.8.x.
- **Schema/SQLite drift** — any Memex assumption that the current `SCHEMA_VERSION`/tables violate.
- **extract.v1 alignment** — confirm Memex's ELPS side and FathomDB's ingest side agree (should already
  be aligned via the round-1/2 pins + the QD/golden sign-off; verify, don't assume).
- **Footprint pressure** — any Memex direction that would push an LLM/network call into FathomDB (BLOCK-
  worthy; the LLM lives in Memex's BYO harness, never in FathomDB).
- **Unused surface** — FathomDB exposes but Memex doesn't need (low priority; note for API economy).

Write the report to **`dev/notes/fathomdb-memex-divergence-review.md`** with: the Phase-A inventory
tables, the divergence matrix, a prioritized **recommended-actions** list, and a clearly separated
**`→ HITL (coreyt)`** section for anything needing a human decision or a contract change. End with a
one-paragraph **verdict**: are FathomDB and Memex aligned, drifting, or diverged — and the single most
important action to keep them converged.

## Guardrails

- Read-only / report-only; **no code changes, no push, no contract changes.** The review *recommends*;
  HITL and the slice process *act*.
- Trust **repo + git over any doc** (including this prompt) when they disagree; cite `file:line`.
- The liaison must not reopen frozen contracts or make product calls — surface those, don't resolve them.
- If the interview reveals a **P1 that breaks the live integration** (e.g. Memex code that the installed
  shim genuinely cannot run), say so plainly at the top of the report — that's the headline, not a
  footnote.
