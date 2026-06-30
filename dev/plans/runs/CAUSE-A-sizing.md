# Cause-A — sizing report (size-it-first; NO code changes)

**Item:** Cause-A — stable hit-id for real-gold adoption (pico `0.8.11.2`).
**Worktree:** `/home/coreyt/projects/fathomdb-worktrees/0.8.11.2` · branch `0.8.11.2-pico-umbrella` (base `main` 34af4bbd).
**Source-of-record read first:** `dev/plans/plan-0.8.11.2.md` §1 (scope), §4 (exit criteria), §5 (X1 parity);
`dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` F-8a (§ line 379); `dev/plans/runs/NOTE-0.8.8-to-steward-id-contract.md`.
**Verdict (TL;DR):** **GO** — additive-only is confirmed at the API surface *provided* the telemetry id-space flip is
deferred (emit the stable id as a NEW parallel field, keep `result_ids`) and the `logical_id = NULL` doc-node case is
consciously dispositioned. Details below.

---

## 1. The `SearchHit` type — definition + construction across all layers

### Core engine (Rust) — `fathomdb-engine`

- **Definition:** `src/rust/crates/fathomdb-engine/src/lib.rs:1107-1122`
  `#[derive(Clone, Debug, PartialEq)] pub struct SearchHit { id: u64, kind: String, body: String, score: f64,
  branch: SoftFallbackBranch, source_id: Option<String>, ce_score: Option<f64> }`.
  **Note:** `SearchHit` is **NOT** `#[non_exhaustive]` (unlike its container `SearchResult`, `lib.rs:1190`). The
  doc-block at `lib.rs:1086-1089` records that `id` is the canonical row's `write_cursor` — the *interim* identity
  carrier per `dev/adr/ADR-0.8.0-canonical-identity-substrate.md`, which "swaps to `logical_id` at the G0 keystone
  (Slice 15) with no carrier reshape." That swap is the F-8a substrate work; Cause-A is the **lighter additive
  alternative** (add a field, do not reshape the `id` carrier).
- **Construction sites (8, all in-crate):**
  - `lib.rs:5513` — `rerank_passages` synthetic passage hits (`id` = caller-supplied passage id).
  - `lib.rs:5982` — **vector branch, node hit** (`id = rowid as u64`, rowid == canonical `write_cursor`).
  - `lib.rs:5992` — **vector branch, edge-fact hit**.
  - `lib.rs:6061` — **text (FTS) branch** (`id = write_cursor`).
  - `lib.rs:6106` — **edge-FTS branch**.
  - `lib.rs:6417` — **graph-arm, depth-0 seed** (`id = write_cursor`; **`logical_id` is in scope here as `lid`**).
  - `lib.rs:6508` — **graph-arm, BFS neighbor** (`id = write_cursor`; **`logical_id` in scope as `neighbor`**).
  - `ce_rerank` (`lib.rs:5585-5614`) clones existing hits, does not construct fresh — no new field needed beyond the clone.
- **Test-fixture literal constructions (must compile against any new field):**
  `tests/pr_g1_search_hits.rs` (e.g. :73, :109, :139, :148), `tests/pr_g9_rrf_fusion.rs`, `tests/pr_g10_reranker.rs`,
  `tests/pr_g10_reranker_ce.rs`, `tests/pr_g12_recency.rs`.

### Binding 1 — Rust→Python (pyo3) `fathomdb-py`

- `src/rust/crates/fathomdb-py/src/lib.rs:380-394` `#[pyclass(name="SearchHit", frozen, get_all)] struct PySearchHit`
  (own copy of the fields). Mapping `PySearchHit::from_rust` at `:396-413` (reads `RustSearchHit` fields only).

### Binding 2 — Rust→Node (napi) `fathomdb-napi`

- `src/rust/crates/fathomdb-napi/src/lib.rs:397-416` `#[napi(object)] pub struct SearchHit` (own copy). Mapping
  `SearchHit::from_rust` at `:418-434`. napi maps snake_case→camelCase (`source_id`→`sourceId`, `ce_score`→`ceScore`).

### Binding 3 — TypeScript SDK `src/ts`

- Internal native shape: `src/ts/src/binding.ts:62-73` `interface NativeSearchHit` (`id`, `sourceId?`, `ceScore?`).
- **Public** type: `src/ts/src/index.ts:84-107` `export interface SearchHit` (`id`, `branch`, `sourceId`, `ceScore`).
- Native→public mapping: `src/ts/src/index.ts:636-637` (search) and `:881-883` (searchExpand).

### Binding 4 — pure-Python wrapper `src/python/fathomdb`

- Dataclass: `src/python/fathomdb/types.py:49-72` `@dataclass class SearchHit` (`id: int`, `source_id`, `ce_score`).
  Re-exported via `src/python/fathomdb/__init__.py:28,49`.
- Native→dataclass mapping: `src/python/fathomdb/engine.py:294-302`.

**The "4 bindings"** = pyo3 (`fathomdb-py`) · napi (`fathomdb-napi`) · TS SDK (`src/ts`) · pure-Python wrapper
(`src/python/fathomdb`), all hanging off the one core `RustSearchHit`.

---

## 2. Proposed stable-id field

- **Name / type:** `stable_id: Option<String>` on `RustSearchHit` (parity: `stableId: string | null` in TS/napi,
  `stable_id: str | None` in pyo3/py-dataclass). String, not `u64` — the stable identity is the `logical_id` (a
  `String`), and it must be nullable (doc-node case, §3).
- **Derivation (precise):** the **active canonical node's `logical_id`** — the post-G0 supersession-stable identity.
  This is what makes it stable across re-runs/re-ingestion: `write_cursor` (today's `id`) is reassigned on every
  re-projection/re-ingest, whereas `logical_id` is preserved by the tombstone-then-insert supersession contract
  (`canonical_nodes.logical_id`, queried e.g. at `lib.rs:6399-6400`, `:6458-6459`).
- **Availability at hit-construction time:**
  - **Graph-arm** (`lib.rs:6417`, `:6508`): `logical_id` is already in hand (`lid` / `neighbor`) — **zero extra query**.
  - **Vector node / text / edge-FTS** (`lib.rs:5982`, `:6061`, `:6106`): the construction queries fetch only
    `kind, body` (`node_stmt` at `lib.rs:5973`: `SELECT kind, body FROM canonical_nodes WHERE write_cursor = ?1`).
    Deriving `logical_id` is a **one-column additive SELECT** (`SELECT kind, body, logical_id ...`) — no new round-trip.
  - **`rerank_passages`** (`lib.rs:5513`): synthetic passages have no canonical identity → `stable_id = None`.
- **Optional non-null fallback (recommend documenting, not necessarily shipping):** `sha2` is already a dependency
  used for "stable identity hashes" (`dev/deps/sha2.md`). For the `logical_id = NULL` doc-node case a content-hash
  fallback `Some(format!("h:{}", sha256(body)))` would make the id non-null for every hit. Costs one hash per hit; only
  worth it if the doc-node arms actually need stabilized keying (see §3).

---

## 3. The `logical_id = NULL` doc-node case (plan §4 flag)

- **When a hit lacks a `logical_id`:** **doc nodes carry `logical_id = NULL`** by design — confirmed at
  `lib.rs:1127-1128` (the `GraphFrontierStats` doc: "doc nodes carry `logical_id = NULL`, so `seeds_resolved == 0`")
  and `lib.rs:1045` (`WriteReceipt` "NULL `logical_id`"). The current corpus is **doc-seeded**, so the *dominant* hit
  type today is exactly the one with a NULL stable id. Vector/text branches surface doc nodes; the graph arm currently
  seeds an empty frontier precisely because doc nodes are NULL-keyed.
- **What the stable id does then:** with `stable_id: Option<String>` it is **`None`** for doc-node hits. That is
  correct and additive, but it means the field **does not actually stabilize the most common hit type** unless the
  content-hash fallback (§2) is adopted. This is the single most important design call for the "real-gold adoption"
  goal: a `None` stable id for doc hits leaves real-gold keying session-scoped for exactly the arms Cause-A is meant to
  unblock. **Recommendation:** ship `stable_id` carrying `logical_id`, and adopt the `h:<sha256(body)>` content-hash
  fallback for the NULL doc-node case so doc-seeded real-gold is cross-session-stable too — keep that fallback behind a
  clearly-labelled derivation so the two id spaces (`logical_id` vs `content-hash`) are distinguishable by prefix.
- **Code path:** the NULL originates at ingest (doc nodes inserted without a `logical_id`); at hit time it surfaces
  through the `canonical_nodes.logical_id` column read added in §2. Edge-fact hits (`lib.rs:5992`, `:6106`) resolve
  from `canonical_edges`, whose identity is keyed `(logical_id, kind)` — those carry an edge `logical_id`.

---

## 4. F-8a "gold-id-contract revisit" touchpoint

- **The finding:** `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md:379` (**F-8a — gold/telemetry id ↔ identity-substrate**)
  — telemetry + real-gold reference hits by the interim `SearchHit.id` (= `write_cursor`): within-session consistent
  (gold pipeline valid) but **not cross-session-stable**. CONSTRAINT: at the identity-substrate swap the gold-id
  contract MUST be revisited — either remap interim→stable `logical_id`, or consciously accept pre-swap gold as
  session-scoped (documented data-loss).
- **The steward action record:** `dev/plans/runs/NOTE-0.8.8-to-steward-id-contract.md` (full remediation surface:
  `GoldRecord.id_space`, `eval/gold_capture.py`, `eval/frozen_candidate_scorer.py`, and reconcile
  `dev/plans/runs/0.8.8-explanation-fieldset-ratification.md` §3d).
- **Where gold/eval keying consumes a hit id today (all keyed on `SearchHit.id` == `write_cursor`):**
  - `src/python/eval/gold_capture.py:20-28` — explicit drift note: `GoldRecord.id_space` is tagged
    `"engine-logical-id"` **forward-looking**, but the carrier is *today* `write_cursor`; `:186` reads
    `candidate_ids = result_ids`, `:188-189` builds `labels` from `relevant_ids`/`irrelevant_ids`.
  - `src/python/eval/frozen_candidate_scorer.py:55` — scores the frozen `rec.candidate_ids` (same id space).
  - `GoldRecord.id_space = ID_SPACE = "engine-logical-id"` (`gold_capture.py:81, :203`).
- **DOC-vs-CODE drift to flag (critical):** the engine telemetry already *claims* logical-id stability in comments —
  `lib.rs:3354` ("Captures ONLY ids (stable `logical_id`)"), `lib.rs:3387` ("Ids are the stable `logical_id`"),
  mirrored in `fathomdb-napi/src/lib.rs:996`, `fathomdb-py/src/lib.rs:904`, `src/ts/src/index.ts:715`. The **code**
  (`lib.rs:3377` `result_ids: ... map(|h| h.id)`) emits `write_cursor`. Cause-A is the work that makes those comments
  true — but only if it actually rewires keying to the stable id (which is the non-additive part — see §7).

---

## 5. Telemetry / gold-keying emission (0.8.8 EXP-OBS) — where the stable id attaches

- **Capture (event emission):** `lib.rs:3356-3383` `capture_telemetry(&self, query, result)`. Emits per search:
  `result_ids` (`:3377`, `Vec<u64>` from `h.id`) and `arm_of` (`:3367-3370`, `{ id.to_string(): branch }`). Off by
  default (single atomic load fast-path `:3360`).
- **Feedback:** `lib.rs:3389-3423` `record_feedback(query_id, relevant_ids: &[u64], irrelevant_ids: &[u64], ...)` →
  JSONL `relevant_ids` / `irrelevant_ids` (`:3418-3419`).
- **Surfaced through bindings:** pyo3 `record_feedback` (`fathomdb-py/src/lib.rs:904`), python wrapper
  `engine.py:308-324` (`enable_telemetry` / `last_telemetry_query_id`) + `:326+` (`record_feedback`), TS
  `index.ts:696, :715`, napi `:996`.
- **Where the stable id attaches:** the **additive-safe** attachment is a NEW parallel field on the telemetry event —
  e.g. `result_stable_ids: Vec<Option<String>>` alongside the existing `result_ids` at `lib.rs:3377`, and an
  analogous `relevant_stable_ids` accepted by `record_feedback`. Then `gold_capture.py` can adopt the new field and
  flip `id_space` as a **separate, opt-in** step (the F-8a remap) without disturbing existing `write_cursor`-keyed
  sinks. Replacing `result_ids` in place is the non-additive path (§7) and is **not** recommended for this pico.

---

## 6. SDK parity (X1, plan §5) — symbols that must stay in lockstep

Adding `stable_id` must land **simultaneously** in all of:

| Surface | Symbol / type | File:line |
|---|---|---|
| Core | `SearchHit` struct field | `fathomdb-engine/src/lib.rs:1108` |
| pyo3 | `PySearchHit` field + `from_rust` | `fathomdb-py/src/lib.rs:382`, `:397` |
| napi | `SearchHit` field + `from_rust` (`stableId`) | `fathomdb-napi/src/lib.rs:398`, `:419` |
| TS native | `NativeSearchHit` | `src/ts/src/binding.ts:62` |
| TS public | `SearchHit` interface + 2 mappers | `src/ts/src/index.ts:84`, `:636`, `:881` |
| Py wrapper | `SearchHit` dataclass + mapper | `src/python/fathomdb/types.py:49`, `engine.py:294` |

Parity tests already exist and must be extended: `src/python/tests/test_telemetry_parity.py`,
`src/ts/tests/telemetry-parity.test.ts`, `src/python/tests/test_functional_search.py`,
`src/ts/tests/functional-search.test.ts`. Precedent: `source_id`/`ce_score` were added with exactly this lockstep.

---

## 7. Additive-only confirmation

**Purely additive (byte-stable default query path):**

- New field is `Option<String>`, default-absent on paths that cannot derive it (synthetic `rerank_passages` → `None`,
  doc nodes → `None` unless content-hash fallback). It **never participates in ranking/scoring** — same posture as
  `source_id` (`lib.rs:1102-1103`) and `ce_score` (`lib.rs:1119-1120`), both of which were proven byte-stable
  additions. Result ordering, `score`, `projection_cursor` are unchanged.
- Binding consumers only **read** `RustSearchHit` fields (`from_rust` mappers) — no external struct-literal break.
- Telemetry stays **OFF by default**; adding a parallel `result_stable_ids` field does not change the default (no-sink)
  behavior.

**Places it is NOT purely additive — call-outs:**

1. **`SearchHit` is not `#[non_exhaustive]`** → every in-repo struct literal must add the field to compile: 8 engine
   sites (§1) + ~5 test fixtures. Mechanical and compile-enforced, but it *is* a touch of every construction site.
   (Could additionally mark `SearchHit` `#[non_exhaustive]` to future-proof, mirroring `SearchResult:1190` — optional.)
2. **Telemetry id-space flip is NOT additive if done in place.** Replacing `result_ids`/`relevant_ids` semantics with
   the stable id (a) changes the sink byte-output, and (b) invalidates pre-swap gold (the F-8a remap obligation,
   §4/NOTE). **Keep it additive** by emitting the stable id as a NEW parallel field and deferring the `id_space` flip
   to a conscious, separately-recorded step.
3. **Doc-node `None`** (§3): additive but functionally weak for the doc-seeded real-gold arms unless the content-hash
   fallback is adopted.

---

## 8. CUT PLAN (smallest ordered set of edits) + sizing

> Two clearly-separated tracks. **Track C** is Cause-A proper (the stable-id field + bindings + telemetry).
> **Track E** is the bundled OOB eval-support (`margin` + distractor/rank knobs) — python-eval-only, independent.

### Track C — Cause-A stable-id (ordered)

1. **Core field + derivation** — `fathomdb-engine/src/lib.rs`: add `stable_id: Option<String>` to `SearchHit:1108`;
   extend the two node SELECTs (`:5973`, `:6458`/`:6399`) to fetch `logical_id`; populate at the 8 construction sites
   (graph-arm `:6417`/`:6508` use the in-scope `lid`/`neighbor`; vector/text/edge use the new column; `rerank_passages`
   `:5513` = `None`); update the 5 test fixtures. *(optionally add `#[non_exhaustive]`.)* — **M**
2. **Telemetry additive emit** — `lib.rs:3377` add `result_stable_ids`; `record_feedback` accepts optional
   `relevant_stable_ids`/`irrelevant_stable_ids` (additive, `write_cursor` keys retained). Fix the now-true doc
   comments `:3354`, `:3387`. — **S/M**
3. **pyo3** — `fathomdb-py/src/lib.rs:382/:397` add field + map. — **S**
4. **napi** — `fathomdb-napi/src/lib.rs:398/:419` add field + map (`stableId`). — **S**
5. **TS** — `src/ts/src/binding.ts:62` + `src/ts/src/index.ts:84/:636/:881` add field + mappers. — **S**
6. **Py wrapper** — `src/python/fathomdb/types.py:49` + `engine.py:294` add field + map. — **S**
7. **Gold pipeline (opt-in adoption, additive)** — `src/python/eval/gold_capture.py`: read `result_stable_ids` when
   present, keep `id_space` semantics honest (only tag `"engine-logical-id"` when the stable id is actually populated;
   doc-node `None` stays session-scoped per the NOTE). Reconcile
   `dev/plans/runs/0.8.8-explanation-fieldset-ratification.md` §3d + the NOTE. — **S/M**
8. **Tests** — extend `test_telemetry_parity.py` / `telemetry-parity.test.ts` (new field present, doc-node `None`,
   stable across a simulated re-projection), `pr_g1_search_hits.rs` (field populated for node hits, `None` for doc),
   `test_functional_search.py` / `functional-search.test.ts` (parity). — **S/M**

**Track C size:** core **M**, each of the 4 bindings **S**, telemetry+gold **S/M**. Overall **M**.

### Track E — bundled OOB eval-support (KEEP SEPARATE; plan §0.4, §8.3)

- **`margin` as a measurement** — attaches in the **python eval harness**, not the engine/bindings. The retrieval
  `margin` (gold-vs-top-distractor score gap) is computed from `SearchResult.results[*].score`; the decision-side
  `margin` plumbing already exists in `src/python/eval/decision_rule_083.py:210-269` (paired-CI margins) and
  `decision_rule_084.py:196` (`decide_084`). New measurement rides a harness module (e.g. alongside
  `frozen_candidate_scorer.py`). **Decoupled from the V-7 verb-shape decision** per plan §0.4. — **S**
- **Distractor-injection / gold-rank-demotion knobs** — eval-harness knobs (corpus/candidate manipulation), python
  only; attach in the OPP-3 eval runners (`r2_parity_eval.py` / a new knob module). No engine or binding change. — **S/M**
- **Confirm per-corpus `decide_08x`** — `decision_rule_084.py` / `decision_rule_083.py` already per-corpus; just verify.

**Track E size:** **S/M**, fully independent of Track C (touches no Rust, no bindings, no `SearchHit`).

---

## GO / NO-GO

**GO — Cause-A is confirmed additive-only and ready to cut**, with two conditions baked into the cut. (1) The field
`stable_id: Option<String>` carrying the active node's `logical_id` is a clean additive mirror of the proven
`source_id`/`ce_score` pattern: it never touches ranking, binding consumers only read it, and the default query path
stays byte-stable; the only mechanical cost is updating the 8 in-crate construction sites + 5 test fixtures (because
`SearchHit` is not `#[non_exhaustive]`), plus lockstep field additions across the 4 bindings (X1). (2) Additivity holds
**only if** the telemetry stable id is emitted as a NEW parallel field (`result_stable_ids`) while `result_ids`
(`write_cursor`) is retained, deferring the F-8a `id_space` flip to a conscious, separately-recorded step — an in-place
replacement would break sink byte-output and invalidate pre-swap gold. **Top 2 risks:** (a) **the `logical_id = NULL`
doc-node case** — for the doc-seeded corpus that is the *dominant* hit type, so `stable_id = None` leaves the real-gold
adoption arms session-scoped unless the `h:<sha256(body)>` content-hash fallback is adopted; (b) **telemetry id-space
drift** — the engine comments already (falsely) claim `logical_id` stability, so the cut must either make them true via
the additive parallel field or the F-8a remap, and must not silently flip the gold `id_space`. Both are dispositionable
within the additive envelope; sizing is **M** for Track C (core M, bindings S each, telemetry/gold S/M) and a separable
**S/M** for the bundled Track E eval-support.
