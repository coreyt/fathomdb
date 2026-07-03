# Consolidated API surface (record-lifecycle protocol)

> Part of the [record lifecycle & projection protocol](README.md). **Status: PROPOSED.** The net verb +
> signature delta after the **two consolidation-review rounds** (verbs C1–C11 + signature/type S1–S10; the ADJUST fixes
> are folded in below). Baseline = **16 governed verbs** (`src/conformance/governed-surface-allowlist.json`).
> This exists so the *surface* is reviewable independently of the mechanism docs.

## Verbs — net **+3** (16 → ~19), plus one reuse and one fold

**New verbs:**

1. **`transition(logical_id, to_state: LifecycleState, reason?)`** — the *reversible* lifecycle moves
   (soft-delete `active→deleted`, **undelete** `deleted→active`, promote `pending→active`, reject
   `pending→deleted`). The engine enforces the legal-transition table. **(C1 fix)** `to_state` is a **typed
   enum** (not a string; the legal `to_state` domain via `transition` is effectively `{active, deleted}` —
   `purged` is `purge()`-only and `pending` is not a transition target). An illegal move returns a typed
   **`IllegalTransitionError { from_state, to_state, legal: [...] }`** — fields **`from_state`/`to_state`, NOT
   `from`/`to`** (**S7 fix:** `from` is a Python reserved keyword → `err.from` is a `SyntaxError`; this must be a
   real typed pyo3 exception + napi error-with-fields, parity-checked) — that *enumerates the legal targets*
   (the state machine is discoverable via the error + introspection, not hidden until runtime). `reason` (a Memex-owned
   label discriminator) is optional at the engine but **expected on the delete-family** transitions — it is
   what distinguishes `retired` (reason=governance) from a user `deleted`.
2. **`purge(logical_id)`** — **kept separate** from `transition()` (C2): irreversible, GDPR-grade hard-erase
   (all versions + FTS/vector/EAV rows; `secure_delete`/`VACUUM`; content-free edge stubs). A gated, explicit,
   hard-to-misfire verb. (`purge` is *not* on the `recovery_denylist`.)
3. **`configure_projections(spec, drop?)`** — the projection registry as a **declarative, idempotent** apply
   (the engine diffs + backfills). **(C3 fix)** Drop is **EXPLICIT**: omission from `spec` does **not** drop a
   projection; removal requires an explicit `drop: [...]` list (or `prune=true` with the destructive delta
   surfaced first). Pairs with `read.projections` so callers can see current state before applying — never
   silent data-loss on an expensive-to-rebuild resource.

**Reused / folded (no new top-level verb):**

1. **`drain(timeout_ms)`** *(shipped, `lib.rs:4360`)* — reused as the projection/embed **quiescence barrier**
   in place of a new `flush_embeddings()` (C4). *Rider:* `drain` is a **barrier** (wait-for-idle), **not** a
   trigger — deferred/backfill rows must be **enqueued on the same projection runtime** `drain` waits on.
2. **`read.projections`** — projection introspection / drift-check, folded into the `read.*` namespace (C5).

**Deferred:** `crossed_boundary_since(t)` (C6) — a read-mode/filter later, not a verb now.

> **Naming:** the `deleted→active` move is `transition(to_state = Active)` ("undelete") — **never** `restore`,
> which is one of the five SDK-absent `recovery_denylist` names (`{recover, restore, repair, fix, rebuild}`).

## Signature / type surface

- **`ReadView` (net-new) — a *sibling* struct to `SearchFilter`, NOT merged (C7 fix).** Carries the visibility
  modes `include_deleted`, `include_superseded`, `valid_as_of(t)`, `ignore_validity` (**S1 note:** name it
  `valid_as_of` uniformly — reconcile the docs' `as_of`; if `ignore_validity` **and** `valid_as_of(t)` are both
  set, `valid_as_of` wins). A single optional arg (an
  options-object / kwargs — never positional) on `search`, `read.get`, `read.list`, `graph.neighbors`. Default
  = `None` ⇒ active-only, **byte-stable**. `SearchFilter` (content: `source_type/kind/created_after/status`)
  stays **byte-frozen and untouched** — visibility and content are orthogonal axes. `read.get(id, view?)`
  sensibly takes a `ReadView`: it is active-only today, so a `ReadView` is the *only* way to fetch a
  deleted/superseded/as-of node — that is visibility, not result-set filtering.
- **`SearchHit` — net *smaller* (C11 fix).** `id` **subsumes** the prefix-tagged stable identity
  (`l:<logical_id>` for canonical nodes · `h:<content-hash>` for doc-seeded/anonymous nodes · `p:<passage>`
  for synthetic rerank hits) — **non-null and id-space-tagged (S2 fix)** (all three hit classes carry an
  id-space, so `id` is total; prefer a typed `{ space: IdSpace, value }` newtype over a magic-prefixed string).
  The interim `write_cursor` id **and** the `stable_id` field are **retired — subsumed into `id`, not dropped**
  (real-gold keying continues on the `id` value; `h:`/`p:` hits keep an identity). **Lifecycle verbs
  (`transition`/`purge`/`read.get`) key on the bare `logical_id` — the `l:` space only; `h:`/`p:` hits are not
  lifecycle-addressable** (ungoverned/unversioned by design). **Co-requisite
  (GATING, lands-together):** the F-8a swap + the anonymous-node **surrogate-`logical_id` minting** (README §2)
  ship in the same change.
- **`PreparedWrite` — extend the existing enum (C10; S3 fix).** (`PreparedWrite` is a `#[non_exhaustive]` enum —
  `Node`/`Edge` — not a struct; the `Edge` variant already carries `t_valid`/`t_invalid`/`confidence`.) The
  `Node` variant gains an **initial** `state` typed as an admission subset **`InitialState ∈ {pending, active}`**
  (default `active`) — you cannot *create* a `deleted`/`purged` node — plus a `reason` (meaningful for
  `pending` = quarantine cause). The **natural-key declaration is a per-kind schema fact in the registry**
  (declared once; the engine already has `derive_logical_id(kind, name)`), **NOT a per-write field** — a write
  supplies `name` and the engine upserts. `logical_id: None` legacy default preserved.
- **Existence columns (net-new):** one `state` enum column + one `reason` column on `canonical_nodes` (C9) —
  not per-transition columns.
- **Temporal validity columns:** node-level `valid_from`/`valid_until` **reuse the shipped edge
  `t_valid`/`t_invalid` shape + semantics** (ISO-8601), extended from `canonical_edges` to `canonical_nodes`
  (C8). The ELPS wire keeps the frozen `t_valid`/`t_invalid`; a seam maps wire ↔ storage.
- **New types:** `LifecycleState` enum (`pending | active | deleted | purged`); `IllegalTransitionError`
  (`{ from_state, to_state, legal }`); the `configure_projections` **`ProjectionSpec`** (**S8 fix**) —
  `{ name, roles: Set<ProjectionRole>, fts?: { tokenizer }, vector?: { embedder } }` + explicit `drop: [...]`,
  where `ProjectionRole ∈ {filterable, rankable, searchable}` is a **set** (an attribute can be filterable AND
  searchable), named **`roles`, not `kind`** (`kind` is the node/edge type discriminator — collision), and the
  `fts`/`vector` sub-objects select the `searchable` sub-target (FTS-only / vector-only / both). Note `purged`
  rows are content-free stubs — no read mode returns purged content; `purge()` requires deleted-first + is idempotent.

## Net delta

- **Verbs:** `+3` (`transition`, `purge`, `configure_projections`) + `drain` reused + `read.projections` folded
  into `read.*`. Deferred: `crossed_boundary_since`. **Nothing removed** from the verb surface.
- **Types/fields (≈+5 governed; AC-074 29 → ~34–36):** `+ReadView`, `+LifecycleState`, `+IllegalTransitionError`, `+ProjectionSpec`, `+dense_readiness` (read metadata); `PreparedWrite`
  extended; **`SearchHit` net-smaller** (`id` subsumes `stable_id`; `write_cursor` retired). Node validity +
  existence columns reuse existing shapes/vocabulary.
- **SDK impact:** every item above threads through **both** `fathomdb-py` and `fathomdb-napi` with parity (X1);
  the SDKs stay thin pass-throughs (no client-side logic).
