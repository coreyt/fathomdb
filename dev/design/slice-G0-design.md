# Slice G0 — instrument hardening: Phase-1 design memo

> **Status:** Phase-1 design (tracer green; implementation gated on design review +
> "proceed to Phase 2"). Grounds E0a/E0b/PRE-1/2/3 in the measured capability map.
> **Contract:** `dev/plans/prompts/0.8.1-G0-instrument-hardening.md` +
> `dev/design/0.8.1-graph-experiment-plan.md` §2/§8.3.
> **Capability map:** `dev/plans/runs/0.8.1-g0-capability-map-20260614T175634Z.json`.

## 0. Capability map headlines (measured 2026-06-14, real binding SCHEMA_VERSION 15)

| Check | Result | Consequence |
|---|---|---|
| **T1** dense/fused reachable | `fused_hit_returned=true`, `dense_branch_present=true`, branches `[vector]`; **path = test-hook `_configure_vector_kind_for_test`**, `production_vector_kind_surface=[]` | PRE-1: dense works but ONLY via the test-hook — decide the production path. |
| **T2** airlock readers | usable: `claude-haiku`, `claude-sonnet`, `claude-opus`, `gemini-3.1-pro`. **`gpt-5` → 429 (quota exhausted)**, **`gpt-5-mini` → 429 (airlock block, retry 299 s)**, **`gemini-3-pro` → 404 (retired: "models/gemini-3-pro-preview is no longer available")** | PRE-2 + a §7 reader-decision escalation (below). |
| **T3** data + scorer | `status=ok`, 4 classes (factoid/temporal/knowledge_update/multi_session), gold parses, multi_session full-set + abstention exclusion both exercised on real LME `s_cleaned` | scorer trustworthy on real data. |
| **T4** embedding throughput | **4.7 sessions/min** (real LME, avg 9,453 chars/session); forecast iteration 7,200 ≈ **25.5 h**, full 19,195 ≈ **68 h** | ~6× the plan §4.3 optimistic 1–4 h / 3–11 h estimate — a planning escalation (below). |

`gate_g0.all_green = true` — the signal path is proven (we CAN emit each signal).
The three sub-findings above do **not** block G0 but materially shape Phase 2 and the
plan's §7 reader decision.

## 1. E0a — graph-arm `source_id` fix (engine, Rust). VERIFIED root-cause.

**Why the graph arm scores 0 (confirmed by reading the code, not assumed):**
`bfs_graph_arm_candidates` (`fathomdb-engine/src/lib.rs:5357`) emits a `SearchHit`
per reached **`canonical_nodes`** row with `id = node.write_cursor`
(`lib.rs:5459-5465`). In the eval adapter, `doc_id_of = cursor_to_doc.get(int(sh.id), str(sh.id))`
(`p0a_base_retrieval.py:390`) maps only **doc-ingest** write-cursors → session ids.
Entity-node cursors are absent from that map → fall back to `str(sh.id)` → never
matches a gold `answer_session_id`. So graph-arm recall is 0 **by construction**.

**Why a string field is the only viable carrier (verified):**
- `SearchHit.id` is a `u64` write_cursor (`lib.rs:1042`) — cannot carry a session-id
  *string*; doc nodes have no stored key beyond their cursor.
- Entity **nodes carry `source_id = NULL`**: `ingest_with_extractor` sets node
  `source_id = entity.get("source_doc_id")` (`lib.rs:2623-2634`), but the ELPS entity
  payload schema is `{name, type, aliases}` only (`elps_live_harness.py:65-66`) — no
  `source_doc_id` → always NULL.
- **Edges DO carry `source_id`**: the edge schema includes `source_doc_id`
  (`elps_live_harness.py:77`), the harness backfills it to the doc id
  (`elps_live_harness.py:213-214`), and ingest stores it (`lib.rs:2748-2778`).
  The `canonical_edges.source_id` column exists since schema step 8 (`lib.rs:198`)
  with an index `canonical_edges_source_id_idx`.

**Design (additive, byte-stable):**
1. Add `pub source_id: Option<String>` to `SearchHit` (`lib.rs:1041`) and to
   `PySearchHit` (`fathomdb-py/src/lib.rs:362`, populated in `from_rust` :371).
2. Add `source_id: str | None` to the Python SDK `SearchHit` dataclass
   (`fathomdb/types.py:47`) and pass it through in `engine.py:175-181`.
   (TS binding mirror only if the parity suite covers `SearchHit`; otherwise note it.)
3. **`None` for vector/text/text_edge hits** — set at every existing `SearchHit { … }`
   construction site (search_inner two-arm path) so the two-arm output is
   byte-identical (the only new field is always `None` when `use_graph_arm=false`).
4. In `bfs_graph_arm_candidates`, when emitting a GraphArm hit for a reached node
   `lid`, resolve its source via an **incident active edge** (reusing the BFS's own
   edge_stmt context or a dedicated prepared stmt):
   ```sql
   SELECT source_id FROM canonical_edges
   WHERE (from_id = ?1 OR to_id = ?1)
     AND superseded_at IS NULL AND source_id IS NOT NULL
   LIMIT 1;
   ```
   Set `source_id = Some(resolved)` on the GraphArm `SearchHit`. (A node reached by
   BFS is reached *via* an edge, so an incident edge with a non-NULL `source_id`
   exists for every real reached node.)
5. **BFS frontier instrumentation** (returned alongside, or via a debug accessor —
   keep out of the byte-stable result): seeds-by-branch count, resolved-seed rate
   (seeds whose cursor resolved to a `logical_id`), non-empty-frontier rate. This
   tests the seed-type-mismatch hypothesis (plan P0-B). Surface as a side-channel
   (e.g. an optional struct on the internal outcome / a counter), NOT a new public
   `SearchHit`/`SearchResult` field, to preserve byte-stability.
6. **Eval-side**: update `doc_id_of` to prefer `sh.source_id` when present, falling
   back to the cursor map — so graph-arm hits resolve to a session id that can match
   gold.
- **Does NOT** flip `use_graph_arm` default (stays HITL-blocked, decided at G2).

**RED tests (engine, `cargo test -p fathomdb-engine`):**
- `test_graph_arm_hit_carries_source_id`: ingest a doc + extracted entity/edge with a
  known `source_doc_id`; a `use_graph_arm=true` search returns a GraphArm hit whose
  `source_id == that doc id` (RED today: field doesn't exist / is None).
- `test_two_arm_search_byte_stable_with_source_id_field`: `use_graph_arm=false` results
  are unchanged and every hit has `source_id == None`.
- `test_graph_arm_frontier_instrumentation`: a seeded search reports non-zero
  resolved-seed / non-empty-frontier counts.

## 2. E0b — extraction robustness (caller-side; NOT the protocol).

**Today:** the runner truncates each session to `_ELPS_MAX_CHARS=2000`
(commit `d949a10`) → feeds ~1/6 of a ~12k-char session; truncated-JSON responses are
dropped (`elps_live_harness.py:225-227` returns `[]` with a warning).

**Design (runner/harness orchestration only — no `extract.v1` change, no Slice-25
golden impact):**
- In the runner's document builder, **chunk a long session into N sub-documents**
  (e.g. ~3,000-char windows with small overlap, distinct `source_doc_id` suffixes
  that all map back to the same session id) and submit them across extractor calls
  (the protocol already takes a batch of docs; `_MAX_DOCS_PER_REQUEST=8`).
- Raise/repair `max_tokens` (`elps_live_harness.py:144` currently 4096) and **retry
  on truncated JSON** (parse-fail or `finish_reason=="length"`): re-request the same
  sub-doc, bounded retries, before giving up.
- Target the G1 AC: **≥90 % of each session's content fed, <2 % truncation.**

**RED tests (`pytest src/python`):**
- `test_long_session_chunked_full_coverage`: a >2,000-char session → multiple
  sub-docs whose concatenated content covers ≥90 % of the original.
- `test_truncated_json_retried_not_dropped`: a stubbed truncated-JSON response is
  retried and ultimately yields entities/edges (not an empty drop).

## 3. PRE-1 — vector-kind binding. DECISION.

**Measured:** dense works, but only through `_configure_vector_kind_for_test`; there
is no production surface (`production_vector_kind_surface=[]`).

**Recommendation: (b) sanction the test-hook seam for eval, this slice.** Rationale:
- The eval needs *a* dense path now; the engine + vectors are real (nothing mocked).
- A production "register a node vector kind" SDK surface is a genuine API-design
  decision (naming, profile binding, migration of `_fathomdb_vector_kinds`) that is
  **out of scope** for an instrument-hardening slice and risks scope creep.
- Record the sanction + rationale in `output.json` as a `forced_deviation`, and file
  the production binding as a reserved follow-up (already noted in
  `p0a_base_retrieval.py` `reserved_followups: vector-kind-binding-gap`).
- If review prefers (a), the minimal surface is `Engine.configure_vector_kind(kind)`
  wrapping the existing native path used by the test-hook — small, but it is new
  public API and should be its own governed decision.

**Test:** the existing T1 tracer already asserts the fused arm returns a hit via the
sanctioned path; add a non-`_for_test`-named eval helper that wraps it so the eval
code path is explicit about using a sanctioned seam.

## 4. PRE-2 — airlock reliability (answerer AND extractor).

**Measured:** zero resilience today; `gpt-5`/`gpt-5-mini` 429'd on single calls.

**Design:** add **exponential backoff + retry-on-429 (honoring `Retry-After` when
present) + bounded concurrency** (configurable; conservative default, e.g.
concurrency≤2, base delay ~1 s, cap ~5 retries) to **both**:
- the answerer path (`AirlockAnswerer._complete`, `p0a_base_retrieval.py:465`), and
- the ELPS extractor (`_call_llm_httpx`, `elps_live_harness.py:130`).
Share one small retry helper (FathomDB-local; no protocol change).

> **Escalation (not solvable by backoff):** `gpt-5` returned 429 **"exceeded your
> current quota"** — a billing/quota exhaustion, not a transient limit. Backoff will
> *not* recover it. `gpt-5-mini`'s 429 is a transient airlock block (retry 299 s) that
> backoff *will* survive. See §7 reader-decision escalation.

**RED tests:** mock a 429 (then 200) on each path → assert the call retries and
ultimately succeeds; assert it gives up after the bound and surfaces a clear error.

## 5. PRE-3 — extraction provenance (local-first; schema 15→16; HITL schema-gate).

**Today:** only a generic alias (`"claude-haiku"`) on **edges only**
(`extractor_model_id`, step 14), no effort/temperature/prompt-version, nothing on
nodes. T2 confirms the airlock returns a **resolved** `response.model` distinct from
the requested alias — the provenance source.

**Design:**
- **Harness (`elps_live_harness.py`):** capture `response.model` (resolved) from the
  airlock response; assemble a `provenance` object `{model_requested, model_resolved,
  effort/reasoning_effort, temperature, prompt_version (hash of `_SYSTEM_PROMPT`),
  protocol, schema_version, timestamp}`; attach it to an **additive `ready`
  `provenance` field** (the `ready` handshake at `:286-297`; `ready.model` already
  exists). **No per-result protocol field** (cross-repo guard).
- **Protocol/ADR:** `ready`-only additive extension; update
  `dev/adr/ADR-0.8.1-byo-llm-extraction-protocol.md` to note it.
- **Engine (`lib.rs`):** stamp **every node and edge row** from the session/`ready`
  provenance (one ELPS process = one config → per-result *storage* without a
  per-result protocol field): set `extractor_provenance` (JSON) on both tables and
  `extractor_model_id` = resolved model on nodes too (edges already have it).
- **Schema step 16 (SCHEMA_VERSION 15→16):** exact SQL in §6 below.

**RED tests:**
- engine round-trip: an ingested **entity node AND edge** each carry
  `extractor_provenance` JSON with `model_resolved` + `effort` + `temperature`, and a
  non-NULL `extractor_model_id`; legacy (pre-16) rows read NULL.
- migration test: applying step 16 to a step-15 DB adds the three columns nullable;
  pre-existing rows read NULL; idempotent re-run.
- harness test: a stubbed airlock response with `model="claude-haiku-4.5-xxxx"` yields
  a `ready.provenance.model_resolved` equal to that resolved id (not the alias).

## 6. Step-16 migration SQL (verbatim — for the HITL schema-gate `proposed_sql`)

```sql
-- MIGRATION-ACCRETION-EXEMPTION: PRE-3 extraction provenance (additive nullable)
ALTER TABLE canonical_nodes ADD COLUMN extractor_provenance TEXT;
ALTER TABLE canonical_edges ADD COLUMN extractor_provenance TEXT;
ALTER TABLE canonical_nodes ADD COLUMN extractor_model_id TEXT;  -- edges already have it (step 14)
```

Added as `Migration { step_id: 16, sql: … }` after step 15 in
`fathomdb-schema/src/lib.rs:375` and bump `SCHEMA_VERSION: u32 = 16` (`:6`). All three
columns are nullable; pre-16 rows read NULL (no data migration). The
`-- MIGRATION-ACCRETION-EXEMPTION: ` marker is REQUIRED (the accretion guard rejects
`ADD COLUMN` without it, `:485-487`). **Merge/close BLOCKED on HITL schema-gate
sign-off** (same governance as SCHEMA-GATE-1 / the step-14/15 bumps).

## 7. Risks & escalations

1. **§7 strong-reader anchor (GPT-5) is currently unavailable.** Decision #2 pins
   GPT-5.4 as the leaderboard-comparable headline reader, but `gpt-5` is 429'd on a
   hard quota and `gpt-5-mini` is transiently blocked. PRE-2 backoff does not fix a
   quota exhaustion. **HITL/orchestrator input needed:** either restore GPT-5 airlock
   quota, or re-anchor the strong-reader headline on an available strong reader
   (`claude-opus`/`claude-sonnet`/`gemini-3.1-pro` all returned 200). This does not
   block G0 (recall@K is LLM-free) but blocks the end-to-end headline at G2.
2. **`gemini-3-pro` is retired (404).** The plan/leaderboard reference cites
   "Gemini-3-Pro"; the live airlock id is **`gemini-3.1-pro`**. Update reader id lists
   to `gemini-3.1-pro` wherever `gemini-3-pro` appears.
3. **Embedding wall-clock ~6× the plan estimate (4.7 sess/min).** Iteration-set fused
   ≈ 25.5 h, full-corpus fused ≈ 68 h — both scheduled-batch territory, not
   interactive. Strengthens the plan §4.3 caution: weigh whether the gate needs
   full-19k fused at all (vs iteration + held-out) given §3.5 power limits. Throughput
   may improve with batching/GPU (not in this slice's scope); the 4.7/min figure is
   the conservative CPU baseline measured on this host.
4. **Byte-stability risk on E0a.** The additive `source_id` field must be `None` at
   every two-arm construction site; a missed site changes two-arm output. The
   `test_two_arm_search_byte_stable_with_source_id_field` RED test guards this.
5. **Frontier instrumentation must not leak into the byte-stable result** — keep it a
   side-channel (counter/debug accessor), not a new public result field.
6. **Phase-2 rebuild repoints the shared `.venv`.** E0a/PRE-3 are Rust changes →
   Phase 2 will `maturin develop`, which repoints the shared binding. Per the ENV
   contract, the orchestrator re-points the canonical binding afterward; flag before
   rebuilding.

## 8. Baseline note (preflight)

The worktree was cut at `658863f`; canonical `main` is `8a4e68a`, which is **one
doc-only commit ahead** (the env/worktree-hardening edit to this slice's own prompt).
`git diff main HEAD -- src/ scripts/ Cargo.toml` is empty — zero code/data delta — so
the code baseline is current. A worktree-local gitignored symlink of the
canonical-built `_fathomdb.abi3.so` into `src/python/fathomdb/` resolves PYTHONPATH
shadowing so `eval/` edits import against the from-main binding (no rebuild).
