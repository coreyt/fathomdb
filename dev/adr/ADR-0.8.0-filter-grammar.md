---
title: ADR-0.8.0-filter-grammar
date: 2026-06-06
target_release: 0.8.x
desc: Settle the deferred G4 filtered-list grammar (F3) as a small CLOSED typed enum revived from the v0.5.6 Predicate core — {JsonPathEq, JsonPathCompare{Gt/Gte/Lt/Lte}, ScalarValue{Text/Integer/Bool}} — surfaced under read.list(kind, filter?, limit). Explicitly EXCLUDES the fused predicates (JsonPathFusedEq/JsonPathFusedTimestampCmp/JsonPathFusedBoolEq/JsonPathFusedIn) and all *_unchecked builders (planner internals that bypass the schema gate). Coordinates with — but does not collapse into — the shipped G10 SearchFilter (shared value vocabulary; full single-enum unification needs a Slice 10 reshape → reserved-gap 37). Compiles to parameterized json_extract(body,'$.field') <op> ? over allowlisted paths against the G0-folded canonical_nodes(kind) index — no DSL, no string interpolation, never raw SQL. Zero 0.8.0 code/schema change.
blast_radius: dev/plans/0.8.0-implementation.md (G4 read.list contract); dev/design/agent-memory-impl-strategy.md (G4 seam + injection-safety test); dev/design/slice-10-design.md (G10 SearchFilter coordination); future 0.8.x G4 slice (Predicate enum + json_extract compilation); NO 0.8.0 production code or schema change
status: proposed (awaiting HITL sign-off; the orchestrator routes the decision and flips proposed→accepted at close)
origin: dev/design/0.8.0-v05-feature-triage.md F3 (DEFER 0.8.x; revive CORE as small grammar, drop DSL+fused); dev/plans/0.8.0-implementation.md Slice 35 (HITL-split 2026-06-06); fit-doc §7 Q3 (G4 grammar shape)
inherits: ADR-0.8.0-canonical-identity-substrate (G0 — folded canonical_nodes(kind) index), ADR-0.8.0-supersede-five-verb-surface-cap (read.* namespace), dev/design/slice-10-design.md (G10 SearchFilter, shipped Slice 10)
---

# ADR-0.8.0 — Filter grammar (G4 / F3)

**Status:** 🟡 **proposed** (Slice 35 deliverable, HITL-split 2026-06-06). Awaiting
HITL sign-off; the orchestrator runs the codex adversarial pass, routes this
decision to sign-off, and flips `proposed → accepted` at close. **No 0.8.0 code or
schema change follows from this ADR** — it scopes the *deferred* 0.8.x G4 verb.

> **Decides:** the shape of the deferred G4 filtered-list grammar — F3's
> `read.list(kind, filter?, limit)`. The decision is **data-independent** (proven
> v0.5.6 prior art + the 2025 industry standard the triage already validated), so
> every question below is settled with a concrete decision + a falsifiable 0.8.x
> acceptance criterion — **no "TBD"** (no experiment is required; F3 is dispositioned
> v05-ready + design-ADR, **no profiling**).

---

## 1. Context — what gap, what is inherited

F3 (`dev/design/0.8.0-v05-feature-triage.md` §F3) is a **DEFER-0.8.x** feature.
The triage's **tier split** is the load-bearing finding: the **filtered-list
capability (equality + range core) is table-stakes** (all three named consumers
want "all open to-dos" / "products rated >4★"), while **F3-as-scoped** (the full
JSON-path *DSL* framing + the *fused* predicates) is broader than table-stakes —
the fused predicates are query-planner internals, *below* the differentiating bar.
The 2025 industry standard (Pinecone/Qdrant/Weaviate) is a **FIXED structured
filter grammar** (equality + range, pre-filtered before/alongside vector KNN);
**none expose a JSON-path query DSL to apps**. So: **revive the core as a small
closed grammar; drop the DSL framing and the fused predicates.**

The verb this unlocks: **G4 `read.list(kind, filter?, limit)`** — a governed,
non-destructive, Py+TS-lockstep read over active canonical nodes of a given `kind`,
optionally narrowed by a typed filter.

**Inherited (fixed inputs — cited, never re-litigated):**

- **G0 substrate** (`ADR-0.8.0-canonical-identity-substrate.md`): the **folded
  `canonical_nodes(kind)` index** (`canonical_nodes_kind_idx`, step-12 delta,
  Slice 15) — the `kind` partition G4 lists over; active rows = `superseded_at IS
  NULL` on `logical_id`-alone identity.
- **`read.*` namespace** (`ADR-0.8.0-supersede-five-verb-surface-cap.md`): G4 hangs
  off the governed surface. Fixed input.
- **G10 `SearchFilter`** (`dev/design/slice-10-design.md`, **shipped Slice 10**): a
  **closed struct** `{source_type, kind, created_after, status}` threaded as
  `Option<SearchFilter>` through `Engine::search_filtered`, compiled as an indexed
  pre-KNN `WHERE` over **vec0 metadata columns**. This ADR coordinates with it; it
  does **not** reshape it.

---

## 2. Open questions (the "RED" — each must be settled below, no "TBD")

- **Q-F1 — The closed enum.** Exactly which predicate/value variants does G4 expose?
- **Q-F2 — Exclusions.** Which v0.5.6 builders are explicitly out (and why)?
- **Q-F3 — G10 coordination.** Do G4 and the shipped G10 `SearchFilter` share one
  enum, or stay distinct surfaces?
- **Q-F4 — Compilation target.** What SQL does the grammar compile to, and how is
  injection foreclosed?
- **Q-F5 — Boolean composition.** How are multiple predicates combined?

---

## 3. Decision (one concrete answer per question)

### D-F1 — The CLOSED typed enum (revive the v0.5.6 `Predicate` core)

G4 exposes exactly this **closed** enum (no open DSL, no caller-supplied SQL):

```
Predicate (closed):
  JsonPathEq      { path: <allowlisted>, value: ScalarValue }
  JsonPathCompare { path: <allowlisted>, op: ComparisonOp, value: ScalarValue }

ComparisonOp (closed):  Gt | Gte | Lt | Lte
ScalarValue  (closed):  Text(String) | Integer(i64) | Bool(bool)
```

This is the v0.5.6 core verbatim — `Predicate::JsonPathEq` (`ast.rs:155`),
`Predicate::JsonPathCompare` with `ComparisonOp{Gt/Gte/Lt/Lte}` (`ast.rs:162,260`),
`ScalarValue{Text/Integer/Bool}` (`ast.rs:273`); the v0.5.6 timestamp filters
(`filter_json_timestamp_*`) **delegate to** `filter_json_integer_*`
(`builder.rs:247-266`), so timestamps need **no separate variant** — a timestamp
comparison is `JsonPathCompare` with `ScalarValue::Integer` (unix-seconds), keeping
the enum minimal. Surfaced under **`read.list(kind, filter?, limit)`**.

> **Falsifiable 0.8.x criterion.** An exhaustiveness test enumerates the public
> `Predicate` variants and asserts the set is **exactly** `{JsonPathEq,
> JsonPathCompare}` with `ComparisonOp ∈ {Gt,Gte,Lt,Lte}` and `ScalarValue ∈
> {Text,Integer,Bool}` — and that a timestamp range filter is expressible as
> `JsonPathCompare(Integer)` with no dedicated timestamp variant. Py + TS expose
> the identical closed set (lockstep parity test).

### D-F2 — EXCLUDE the fused predicates and every `*_unchecked` builder

**Explicitly EXCLUDED from the G4 surface** (named so the 0.8.x slice cannot
re-import them):

- **`JsonPathFusedEq`** (`ast.rs:184`), **`JsonPathFusedTimestampCmp`**
  (`ast.rs:193`), **`JsonPathFusedBoolEq`** (`ast.rs:205`), **`JsonPathFusedIn`**
  (`ast.rs:240`) — the *fused* predicate variants.
- **All `*_unchecked` builder methods** — `filter_json_fused_text_eq_unchecked`
  (`builder.rs:319`), `filter_json_fused_timestamp_{gt,gte,lt,lte}_unchecked`
  (`builder.rs:337,356,375,394`), `filter_json_fused_bool_eq_unchecked`
  (`builder.rs:413`), and the rest of the `..._unchecked` family (`builder.rs:279-354`).

**Why:** the fused variants are **query-planner internals classified for the fusion
gate** (`ast.rs:178-184` doc: "classified as … fusion"); the `_unchecked` builders
**bypass the schema gate** ("produces SQL … without validation", `builder.rs:315`).
Exposing either to applications re-opens exactly the planner-internal / unvalidated
surface the triage says to drop (F3: "fused/DSL-framing = drop"). They are **below
the differentiating bar** and a **schema-gate-bypass risk**.

> **Falsifiable 0.8.x criterion.** A surface test asserts **no** symbol matching
> `Fused` or `_unchecked` appears in the G4 public surface (Rust facade + Py + TS);
> the conformance/no-recovery-style allowlist for `read.list` rejects them. A
> grammar test asserts a filter can only be constructed through the validated
> (schema-gated) builders.

### D-F3 — G10 coordination: **shared value vocabulary; distinct surfaces; full unification = reserved-gap 37**

G4 and the shipped G10 `SearchFilter` **share the scalar value + comparison-op
vocabulary** (`ScalarValue{Text/Integer/Bool}`, `ComparisonOp{Gt/Gte/Lt/Lte}`) as
a common primitive — the strongly-preferred coordination. They **do not collapse
into one enum**, because the two surfaces compile against **different targets**:

- **G10 `SearchFilter`** is a closed **struct of four fixed fields**
  (`source_type, kind, created_after, status`) filtering **indexed vec0 metadata
  columns pre-KNN** (`slice-10-design.md:96-126`) — the indexed pre-filter is the
  whole point (and the reason it is *not* `json_extract`).
- **G4 filter** is a **list of JSON-path predicates over node `body`** via
  `json_extract` (D-F4) — arbitrary allowlisted paths, not four fixed columns.

Collapsing them into one enum would force G10 onto `json_extract` (losing its
indexed pre-KNN filter — a perf regression) or G4 onto only the four metadata
fields (losing arbitrary JSON-path). Per the triage, **G10 sharing is "coordination,
not a hard blocker"** — G4 and G10 ship independently. **Full single-enum
unification requires a Slice 10 reshape and is therefore named as
reserved-gap 37** (the "coordination slice if the filter grammar and G10 can't
share", `0.8.0-implementation.md:1320`) — **not** done here, **not** a 0.8.x G4
blocker.

> **Falsifiable 0.8.x criterion.** G4 introduces `ScalarValue`/`ComparisonOp` as a
> **standalone shared vocabulary** (one definition a future G10 adoption *can*
> import) and lands **without** modifying the shipped `SearchFilter` struct or its
> compilation (a test asserts `SearchFilter` is unchanged). Retrofitting G10 to
> actually reference those shared types is **reserved-gap 37** (recorded in the
> DOC/plan), **not** required for G4 — so the two requirements do not conflict:
> sharing is vocabulary-definition reuse now, full single-grammar unification later.

### D-F4 — Compilation: parameterized `json_extract` over allowlisted paths, never raw SQL

Each predicate compiles to **`json_extract(body, '$.field') <op> ?`** with the
value passed as a **bound parameter (`?`)** and the field path drawn from a
**server-side allowlist** of permitted JSON paths (whitelisted indexed-field set),
**not** caller-supplied SQL or an interpolated string. The `kind` partition of
`read.list` uses the **G0-folded `canonical_nodes(kind)` index**
(`canonical_nodes_kind_idx`); the active-row predicate is `superseded_at IS NULL`.
**No DSL, no string interpolation, never raw SQL** — a non-allowlisted path is a
typed rejection, not a passthrough.

> **Falsifiable 0.8.x criterion.** An injection-safety test (modeled on
> `fts5_injection_safety.rs` + the new G4 grammar test, impl-strategy `:414`)
> asserts: (a) every compiled clause is parameterized — the value reaches SQLite as
> a bound `?`, never interpolated; (b) a non-allowlisted path (or an attempted
> `'; DROP …` in path/value) is **rejected with a typed error**, not compiled; (c)
> `EXPLAIN QUERY PLAN` of `read.list(kind=…)` shows the `canonical_nodes(kind)`
> index drives the scan (no full-table `SCAN`).

### D-F5 — Boolean composition: **implicit AND of stacked typed predicates**

Multiple predicates in one `filter` are combined by **implicit AND** (all must
hold) — the v0.5.6 `QueryStep::Filter` stacking model (`builder.rs:214,238`). **No
OR / no nested boolean** in 0.8.x: it is unnecessary for the table-stakes workloads
("all open to-dos rated >4★" = two ANDed predicates) and OR/nesting is a clean
**reserved-additive** future grammar extension (a new `BoolGroup` variant) with no
reshape of the closed scalar enum.

> **Falsifiable 0.8.x criterion.** A test asserts `read.list(kind, filter=[p1,p2])`
> returns only rows satisfying **both** predicates (AND semantics); the grammar
> exposes **no** OR/nesting constructor in 0.8.x.

---

## 4. EXCLUDE list (explicitly out of scope — named so the 0.8.x slice does not drift)

- **The fused predicate variants** — `JsonPathFusedEq`, `JsonPathFusedTimestampCmp`,
  `JsonPathFusedBoolEq`, `JsonPathFusedIn` (planner internals / fusion-gate;
  `ast.rs:184,193,205,240`).
- **All `*_unchecked` builders** — they bypass the schema gate (`builder.rs:279-354`).
- **A JSON-path query DSL** — the 2025 industry standard exposes a fixed grammar,
  not a DSL (triage §F3); G4 is a closed enum.
- **`JsonPathIn` / IN-set membership** — not in the table-stakes core; **deferred as
  reserved-additive** (a clean future `JsonPathIn` variant, the common next
  addition per the Qdrant finding) — **not** the fused `JsonPathFusedIn`, which is
  permanently excluded.
- **OR / nested boolean composition** — deferred reserved-additive (D-F5).
- **Reshaping the shipped G10 `SearchFilter`** — full unification is reserved-gap 37
  (D-F3), not done here.
- **Raw SQL / string-interpolated paths or values** — foreclosed by D-F4.
- **Any migration / schema change** — the `canonical_nodes(kind)` index is already
  folded into G0.

---

## 5. Cross-ADR consistency (required self-check)

- **G6 references G4.** The companion
  [`ADR-0.8.0-graph-traversal-scope.md`](./ADR-0.8.0-graph-traversal-scope.md)
  defines `G6 = G1 + G4 + G5 + G9`; the G4 it composes is **this** closed
  `Predicate` enum on `read.list`. The `search(expand=)` filter is the same enum —
  both ADRs agree on what G4 is.
- **G10 reconciliation.** G4 (this ADR) and G10 `SearchFilter` (Slice 10) share the
  `ScalarValue`/`ComparisonOp` vocabulary; full single-enum unification is
  **reserved-gap 37** (D-F3) — the explicit "cannot share one enum without a
  Slice 10 reshape" note the prompt requires.
- **Valid-time naming.** Not referenced by this ADR (G4 filters node `body`, not
  edge valid-time); where the companion ADR references it, the pair is
  `t_valid`/`t_invalid`.
- **No re-opening.** This ADR does not touch G0 identity, the `read.*` namespace, or
  the Slice-32 graph model — all cited as fixed inputs (§1).

---

## 6. Inheritance (upstream decisions built on, not re-opened)

| Inherited decision | Source | How this ADR uses it |
|---|---|---|
| Folded `canonical_nodes(kind)` index; active = `superseded_at IS NULL` (logical_id-alone) | `ADR-0.8.0-canonical-identity-substrate` (Slice 15/31) | `read.list` kind-partition + active-row filter (D-F4) |
| `read.*` governed namespace | `ADR-0.8.0-supersede-five-verb-surface-cap` Q1/Q2 | G4 surface home |
| G10 `SearchFilter` closed struct, indexed pre-KNN | `dev/design/slice-10-design.md` (shipped Slice 10) | Coordinated vocabulary; not reshaped (D-F3) |

---

## 7. Consequences / reserved follow-on

- **For 0.8.0:** nothing ships — this ADR scopes deferred 0.8.x work. Zero code,
  zero schema, zero `acceptance.md` change.
- **For 0.8.x:** the G4 slice revives the closed `Predicate` core behind
  `read.list`, compiling to parameterized `json_extract` over allowlisted paths
  against the already-present `canonical_nodes(kind)` index, no migration.
- **Reserved-additive (no reshape):** `JsonPathIn` (IN-set), OR/nested boolean
  composition — clean future enum extensions.
- **Reserved-gap 37:** full single-enum unification of G4 + G10 `SearchFilter`
  (needs a Slice 10 reshape).

---

## 8. Sources

Repo: `dev/design/0.8.0-v05-feature-triage.md` §F3 (tier split; 2025 industry
standard; revive core / drop fused+DSL); `dev/adr/ADR-0.8.0-canonical-identity-substrate.md`
(folded `canonical_nodes(kind)` index, step-12 delta);
`dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md` (`read.*`);
`dev/design/slice-10-design.md:13,91-126,189` (G10 `SearchFilter`);
`dev/design/agent-memory-impl-strategy.md:321,414` (G4 seam + injection-safety
test); `dev/plans/0.8.0-implementation.md:1207,1320` (Slice 35 split; reserved-gap
37). v0.5.6 prior art: `git show v0.5.6:crates/fathomdb-query/src/ast.rs`
(`Predicate:149`, `JsonPathEq:155`, `JsonPathCompare:162`, fused variants
`:184,193,205,240`, `ComparisonOp:260`, `ScalarValue:273`) &
`.../builder.rs:214-280` (core builders), `:247-266` (timestamp delegates to
integer), `:279-354` (the EXCLUDED `_unchecked` fused builders).
