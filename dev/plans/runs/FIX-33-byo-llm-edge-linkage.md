# fix-33 — BYO-LLM extracted edges are orphaned from their nodes [P1]

> **Status:** ✅ LANDED as **fix-33** @ `9b69b67` (2026-06-13), in the Slice-5 post-process combined-diff
> cycle (codex base `101a3b0`). Implemented via the `entities[]` name+alias → (canonical name, type)
> resolution map; un-masked the slice15 `from_type`/`to_type` fixtures; promoted the QD-envelope test to
> a committed linkage regression (`orphaned == 0`; verified RED at fix-32 `6a82c16`: orphaned=1).
> Bindings unchanged (X1 holds — napi/py delegate). Co-landed with fix-34 (derive_logical_id collision
> guard + edge dedup) and fix-35 (subprocess timeout/deadlock) from the same §9 round. **§9 re-review of
> the batch still pending before Slice 5 closes.** HEAD at write time = `6a82c16` (fix-32).
>
> **Provenance:** found 2026-06-13 while validating Memex's QD `result`-envelope sample
> (`~/projects/memex/dev/elps/QD-ENVELOPE-SAMPLE.md`) against FathomDB's deserializer. The deserializer
> itself PASSED (all 8 envelopes parse + ingest); the QD round-trip exposed this **downstream linking**
> defect because the QD sample is the first *contract-faithful* input we've run (edges with no types).

## Severity & blast radius
**[P1] graph integrity.** Every edge produced by `ingest_with_extractor` from contract-conformant
ELPS output points at `from`/`to` `logical_id`s that **no node has**. Consequences:
- G5/G6 traversal (`graph_neighbors`, `search_expand`, Slice 20) sees a disconnected edge set.
- The R3 temporal graph arm (BFS over `canonical_edges` joined to nodes) would retrieve nothing.
- The pre-existing **G8 dangling-edge probe** (`WriteReceipt.dangling_edge_endpoints`, 0.8.0 Slice 20)
  very likely fires on *every* extracted edge — see "RED assertion" below.

The graph keystone silently ingests orphaned edges. Nothing crashes; counts look right; the graph is
just disconnected.

## Root cause (exact)
`src/rust/crates/fathomdb-engine/src/lib.rs`, `ingest_with_extractor` result-mapping:

- **Nodes** (entities loop ~L2527–2601): `logical_id = derive_logical_id(entity.type, entity.name)`
  — uses the entity's **real `type`** ("Person", "Organization", "unknown", …). (L2538, L2543)
- **Edge endpoints** (edges loop ~L2603–2656): `from_lid = derive_logical_id(from_type, from_entity)`
  where `from_type`/`to_type` are read **off the edge JSON** and default to `"entity"`. (L2610–2615,
  L2634–2635)
- `derive_logical_id(kind,name) = sha256(lower(kind)+":"+lower(name))` — **kind participates.** (L7202)

**The protocol defines NO `from_type`/`to_type` on edges.** Per the ratified contract
(`dev/plans/prompts/IR-C-byo-llm-extraction-harness-memex.md`): the `edges[]` schema is
`from_entity/to_entity/relation/body/t_valid/t_invalid/confidence/source_doc_id/source_span` — no
types — and "from_entity/to_entity reference entities **by name**." So `from_type` is *always* absent
⇒ *always* `"entity"`, while nodes carry their real type. The two hashes never match.

Worked example (QD Case 1): node "Alice"/type "Person" → `sha256("person:alice")`; edge endpoint
"Alice" → `sha256("entity:alice")`. Different ids. Holds for **every** case in the QD sample,
including the synthesized endpoint (Case 6 "Grace"/type "unknown" → node `sha256("unknown:grace")`
vs edge `sha256("entity:grace")`).

## Why existing tests missed it
The Slice-15 fixtures put `from_type`/`to_type` **on the edges**, matching the buggy reader — the tests
encode the bug instead of the contract. Real ELPS output (and the QD sample) carry no edge types, so
the masking only shows up against contract-faithful input.

## The fix (FathomDB-side only — do NOT change the protocol)
Resolve each edge endpoint's identity from the **`entities[]` array in the same `result`**, not from a
non-existent edge field:

1. Before the edges loop, build a resolution map from `entities[]`: for each entity, map its `name`
   **and each entry in its `aliases`** → that entity's **`(canonical name, type)`**. (Edges may
   reference an entity by an alias; the endpoint `logical_id` must use the entity's **canonical name
   AND type** — matching type alone is insufficient if the edge used an alias, since `name` is also
   hashed.)
2. For each edge, resolve `from_entity`/`to_entity` through the map → `(canonical_name, kind)`; derive
   `from_lid`/`to_lid` from those. Body/relation/temporal/confidence handling is unchanged.
3. **Fallback** when a referenced name is not in the map (should not happen — QA=(A) synthesize lists
   dangling endpoints in `entities[]` with type "unknown" — but be defensive): derive from the raw
   referenced name with kind `"entity"` (preserves today's behavior for the truly-unresolvable case
   rather than dropping the fact).
4. **Stop reading `from_type`/`to_type` off the edge object** (L2610–2615) — they are not in the
   protocol.

This makes endpoint `logical_id == node logical_id` for every listed/synthesized entity.

## TDD

**RED first** — new file `tests/pr_fix33_edge_node_linkage.rs` (mirror the stub-provider harness in
`tests/slice15_byo_llm_ingest.rs`). Feed a contract-faithful `result`: entities with real types +
empty aliases, and an edge referencing them **by name with no `from_type`/`to_type`** (QD Case-1
shape). Assert end-to-end linkage. Two good assertion options (use whichever is ergonomic; prefer the
first):
- **G8 probe:** after ingest, `WriteReceipt.dangling_edge_endpoints == 0`. On current code this should
  be non-zero (every endpoint orphaned) — clean RED.
- **Traversal:** `graph_neighbors(<Alice's node>)` returns the "Acme Corp" neighbor / the edge (as the
  Slice-20 tests do). Currently returns nothing.

Capture the RED output + the sha you ran it against. Then implement the map; confirm GREEN.

**Un-mask fixtures.** Grep `from_type`/`to_type` under `tests/` (and any binding tests); remove edge
`from_type`/`to_type` so fixtures are contract-faithful and rely on `entities[]` resolution. Ensure
`slice15_byo_llm_ingest.rs` + the Slice-20 traversal tests pass via real resolution.

**Bindings (X1).** Confirm edge linking lives only in the engine `ingest_with_extractor`; check
`src/rust/crates/fathomdb-napi/src/lib.rs` and `src/rust/crates/fathomdb-py/src/lib.rs` delegate (no
duplicated buggy derivation). If a binding duplicates it, fix there too. Expected: engine-only, so no
SDK-surface change and X1 holds without binding edits — but verify, don't assume.

**Scratch test.** An untracked `tests/qd_envelope_deserialize.rs` (counts-only) exists from the
verification pass; either fold a linkage assertion into it or supersede it with the pr_fix33 test —
don't lose coverage.

## Gates
- `cargo fmt` (pre-commit runs `--check`) + `cargo clippy` clean on the engine crate (pre-push runs
  clippy).
- `cargo test -p fathomdb-engine --test pr_fix33_edge_node_linkage --test slice15_byo_llm_ingest`
  (+ the Slice-20 traversal test; add `--features default-embedder` only if a test's header requires
  it). **Read the real `test result: ok. N passed` lines, not a wrapper echo**
  ([[background-exit-masks-real-exit]]).

## Scope guards
- Minimal change: the ingest mapping + the masked fixtures. No unrelated refactor.
- **Protocol/golden are NOT affected.** Edges stay name-referenced; do **not** add edge types. Memex
  has already been told the QD sample validated and is cleared to freeze the Slice-25 golden
  (`~/projects/memex/dev/elps/FATHOMDB-QD-VALIDATION.md`) — this fix is entirely on our consumer side.
- This is a **fix-N on the Slice-15 keystone diff**, which is still open under the combined codex
  review (base `101a3b0`) — appropriate as fix-33 in the running cycle rather than a post-hoc
  reserved-gap slice. codex may not flag it on its own (it can't know the protocol forbids edge types),
  so it must be added deliberately.

## Suggested commit (on `main`, no push)
```
fix(edges): resolve BYO-LLM edge endpoints via entities[] name→type map so edges link to nodes (fix-33)

Edge endpoints derived logical_id with a default kind "entity" while nodes used
the entity's real type, so every contract-faithful extracted edge (the protocol
has no edge types; edges reference entities by name) pointed at a logical_id no
node had — orphaning edges from nodes and tripping the G8 dangling probe. Build a
name+alias -> (canonical name, type) map from entities[] and resolve each endpoint
through it. Un-masks slice15 fixtures that carried from_type/to_type on edges.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
```

## Board updates the orchestrator should make on close (single-writer = you)
Add to `dev/plans/runs/STATUS-0.8.1.md`:
- A fix-33 row under the Slice-5 §1 cycle: `fix-33 (<sha>): BYO-LLM edge endpoints resolved via
  entities[] name→type map (were orphaned via default kind "entity"); un-masked slice15 edge-type
  fixtures; G8 dangling==0 regression added.`
- A §7 decision note: QD-sample validation (Memex round-2) surfaced the orphaned-edge [P1]; fixed
  FathomDB-side, protocol unchanged, golden unaffected.
