# Structural lifecycle contract (FathomDB-owned mechanism)

> Part of the [record lifecycle & projection protocol](README.md). **Status: PROPOSED.** Defines the
> three orthogonal engine axes, the read-mode surface, the naming disambiguations, and how Memex composes
> its lifecycle labels on top. Companion: [projection registry & async embed](projection-registry-and-async-embed.md).

## 0. The seam — mechanism vs policy

FathomDB owns, for every axis below: the **type**, **storage**, **indexing**, **invariants**, and
**evaluation**. Memex owns the **values** and the **transition decisions**. FathomDB never decides *whether*
a record is retired or *what* a validity window is; it defines what those axes *are*, keeps their
invariants, and evaluates queries over them cheaply.

Liveness exclusion must be **materialized/indexed**, never derived per query — deriving it defeats the
purpose of excluding rows early, and an un-honored exclusion **surfaces excluded content in FTS/vector
results** (the exclusion has to be enforced inside the index, not only at query time).

## 1. Axis: existence / admission

A mutually-exclusive engine enum, one indexed column:

```text
pending  →  active  →  deleted  →  purged
```

- **`pending`** — present, has a `logical_id`, versioned, but **not admitted to default retrieval** (awaiting
  validation/consolidation). Serves Memex's quarantine/promotion gate for untrusted ELPS/LLM extraction.
- **`active`** — present and admitted.
- **`deleted`** — soft-deleted, **retained + recoverable**; excluded from default reads. **Stays indexed
  behind a flag** so restore/audit search works.
- **`purged`** — terminal, physically erased (see §1.2).

### 1.1 Transition table (engine-enforced; app decides *when*, engine enforces *which are legal*)

| From → To | Name | Notes |
|---|---|---|
| pending → active | **promote** | quarantine/consolidation admits the record |
| pending → deleted | **reject** | quarantine rejection |
| active → deleted | **soft-delete** | recoverable |
| deleted → active | **restore** | |
| deleted → purged | **purge** | terminal, irreversible |

Illegal transitions (e.g. `purged → *`, `active → purged` skipping `deleted`) are refused by the engine.

### 1.2 `purged` = hard, GDPR-grade erasure

- Row content, FTS entries, vector entries, and EAV attributes are **physically removed**.
- **File-level erasure precondition:** `PRAGMA secure_delete=ON` (or a post-purge `VACUUM`) — a plain
  SQLite `DELETE` leaves content in freelist/unallocated pages, recoverable from the file.
- **Opaque-id precondition:** `logical_id` must be an **opaque surrogate**, never content-derived/hashed —
  otherwise a retained referential stub's `logical_id` leaks the erased content.
- Edges **to** a purged node: cascade-remove, **or** convert to a **content-free referential stub**
  `{edge-id, endpoint logical_id, timestamp}` only. A content-retaining "audit-ref" is **forbidden** (it
  defeats erasure). Consequently **no read mode can return purged content** — there is none.

## 2. Axis: version-currency

> **Reconciled to shipped code.** The single is-latest authority this axis needs **already exists** — the
> HITL-signed **G0 `superseded_at`** substrate, not a new `is_latest` column.

Currency is the shipped `superseded_at` (a nullable transaction-time tombstone on `canonical_nodes`): a row
is **current** iff `superseded_at IS NULL`, else **superseded**. Enforced by the shipped partial-unique index
(`fathomdb-schema/src/lib.rs:300`, ADR-0.8.0, HITL-signed, *logical_id-alone* — the G0 keystone):

```sql
CREATE UNIQUE INDEX canonical_nodes_logical_active_idx
  ON canonical_nodes(logical_id) WHERE superseded_at IS NULL
```

**This shipped index is the single "is-latest determination" Q2 asks for** — one authoritative answer every
search path uses; exclusion is a query-time `WHERE superseded_at IS NULL` over the indexed column (**not** a
materialized `is_latest` bit — see §4). `is_latest`/`superseded` are *derived predicates* of `superseded_at`,
not a stored column; keeping the name `is_latest` would be a **rename of a shipped mechanism, not a new one**.
No caller re-derives (closes the CR-060 divergence).

**Prerequisite — records must carry a `logical_id`, and search must return it (co-requisite, GATING).**
Verified from code: FathomDB has **two** identity carriers (`ADR-0.8.0-canonical-identity-substrate`,
`lib.rs:198`) — the stable `logical_id`, and the **interim `write_cursor`** (a positional `u64` reassigned
on every re-projection). Today **`SearchHit` returns the `write_cursor` as its primary id** (`SearchHit.id`,
`lib.rs:1109`); Cause-A (0.8.11.2) added an *additive* `SearchHit.stable_id` (`lib.rs:1132`, `derive_stable_id`
`:9455`) = `l:<logical_id>` when the active canonical node has one, else `h:<sha256(body)>` for
**doc-seeded/anonymous nodes** (which have **no `logical_id`** — the dominant corpus hit type today), else
`None`. Two consequences the contract must carry:

1. **Co-requisite (GATING): complete the F-8a / ADR-0.8.0 swap** — `SearchHit.id` must become the
   first-class `logical_id` (building on the existing `stable_id`), so a hit carries the identity used to
   dedupe-to-current / check is-latest / delete. Callers must **not** key on the interim `write_cursor`.
   **Lands-together** with the Cause-A id-contract pico. (This is why Memex's per-path reconciliation hacks
   exist — CR-060 closes at the engine, but the *cleanup* Memex promised needs the hit to carry `logical_id`.)
2. **The object-id question (doc-seeded gap) — RESOLUTION.** FathomDB *does* provide a stable object id:
   `logical_id` is present for every **governed** write — caller-supplied, or engine-**derived** from
   `kind`+`name` (`derive_logical_id`, `lib.rs:3069`). It is **absent only for anonymous / doc-seeded**
   bulk-ingest nodes (`PreparedWrite::Node { logical_id: None }`, `lib.rs:7188`), which are content-addressed
   and deliberately never versioned ("anonymous nodes are never superseded-in-batch", `lib.rs:10263`). So the
   engine is **not** missing an object id — governed objects have one; bulk doc-chunks opt out. **Resolution:**
   (i) complete the F-8a swap (above) so `SearchHit` returns the `logical_id`; (ii) to satisfy the expectation
   that *every* object is addressable, the engine **mints an opaque surrogate `logical_id` for anonymous
   nodes** (an auto-assigned primary key), so every canonical node is addressable + version-eligible. Memex's
   world-model records already carry `logical_id`s, so its lifecycle problem is closed by (i) alone; (ii) is
   the general "a database provides an object id" fix (**net-new**).

- **Head-deletion invariant:** currency is **positional** — the latest insert is the head, deleted or not.
  A `logical_id` is *live* iff its **head is admissible**. Deleting the head does **not** resurrect the
  prior version (silent reactivation is forbidden). A revert is a **new write** (append-only history).
- **`superseded` is not "not-live."** An old version is often **historical-but-valid** ("what was true
  then"). It is excluded only by **view-policy** (the default dedup-to-current mode), never treated as a
  deletion. `include_superseded` returns history.
- The write mechanism is the shipped **tombstone-then-insert** (`superseded_at` is a transaction-time
  *tombstone*; the term is load-bearing in schema + engine). It is a *different* tombstone from the
  *deletion* tombstone (existence axis) — same word, two axes; disambiguate by axis, don't ban the word (§5).

## 3. Axis: temporal validity

> **Reconciled to shipped code (mostly NET-NEW).** Today validity exists **only on edges** — `canonical_edges`
> `t_valid`/`t_invalid` as **ISO-8601 text** (`fathomdb-schema/src/lib.rs:351`), evaluated by **inline**
> `datetime(t_invalid) > datetime('now')` (`lib.rs:5430, 6690`), **not** via `SearchFilter` (fields:
> `source_type/kind/created_after/status`, `lib.rs:1499`). **Nodes have NO validity columns.** So record-level
> validity, integer windows, the bound-`:now`/`SearchFilter` seam, and `valid_as_of` below are **net-new**. The
> ELPS wire keeps the frozen `t_valid`/`t_invalid` (`fathomdb.extract.v1`); the seam maps wire ↔ storage.

`valid_from` / `valid_until` (**net-new**, record-level), one **half-open** interval `[valid_from, valid_until)`,
`NULL` = unbounded on that side, **single window** (not recurrence — "weekdays 9–5" is not expressible; a
re-entrant window is one future window, not a schedule).

- This is **valid time** (world time / "what is in force"), named **distinctly** from **transaction time**
  (write time / version-currency). Full bitemporal query is out of scope, but the schema uses the standard
  names so it is not precluded.
- **"expired" is a predicate, never a stored label:** `¬(valid_from ≤ :now < valid_until)`.
- **Evaluated at query time.** *Today (edges):* inline `datetime(t_invalid) > datetime('now')`. *Proposed
  (net-new):* replace the inline clock with a **bound `:now` parameter** (buys testability, replay,
  `valid_as_of(t)`). Either way, a range compare over candidates is microseconds and costs **zero recall** at
  target scale; materializing the passage of time into a static index is rejected.
- **Boundary crossings are lazy, on next touch** (an embedded library has no daemon). The engine offers a
  **caller-invoked** `crossed_boundary_since(t)` detection hook the host runs opportunistically — a
  detection hook, **not** eager prevention. A never-queried future-window row does silently re-activate
  when finally read; this is stated honestly, not papered over.

## 4. Retrieval contract & read modes

- **Exclusion is a query-time predicate over the shipped index — not a materialized bit (today).** The hot
  path filters `WHERE superseded_at IS NULL` (indexed, cheap) + the inline edge-validity check. A materialized
  `admissible` column — and the `active` existence-state it references — is **net-new**; since the shipped
  predicate already rides a partial index, materialization is an optional optimization, **not** a requirement,
  and the earlier "must never be per-query" framing is dropped.
- **Validity** is the query-time range check (§3).
- **View membership** (archived/unpublished) is separate flag column(s) with a defined recompute protocol
  when policy changes.
- **Read modes are orthogonal, composable boolean relax-flags** (not a fixed enum):
  `include_deleted`, `include_superseded`, and `valid_as_of(t)` / `ignore_validity`.
  - **Default** (no flags) = `admissible ∧ in-force-now`, **deduped to current-per-`logical_id`** → this is
    the mode all search paths use, closing CR-060 by construction.
  - `include_deleted ∧ include_superseded` = restore/audit over history.
  - No mode returns **purged** content (physically gone). A distinct `include_existence_stubs` may surface
    the content-free referential stubs if ever needed.

## 5. The supersession landmine + naming hygiene

Two different things wear the word "superseded":

- **version-supersession** — structural: `superseded` = not the latest version of the **same** `logical_id`.
  A **state** on the currency axis (§2).
- **meaning-supersession** — interpretive: record A `obsoleted-by` / `corrected-by` record B — **different**
  records/`logical_id`s. An **edge**, in Memex's vocabulary (§B of the A/B/C map). **Never** an enum value.

They must **never share the name**. On the other overloads — reconciled to shipped vocabulary:

- **"tombstone" is NOT banned** (the earlier ban was wrong): `superseded_at` is a shipped "transaction-time
  tombstone" and "tombstone-then-insert" is pervasive engine/schema vocab. Disambiguate by **axis** instead —
  a *currency* tombstone (`superseded_at`) vs a *deletion* tombstone (the net-new existence axis).
- **"current"** near the temporal axis — do use `superseded_at IS NULL` / "is-latest" for currency and
  "in-force" for validity, but this is a doc-writing caution, not a code rename.
- **`t_invalid` stays** — it is a frozen, load-bearing `canonical_edges` column and the ELPS wire name; the
  net-new **node**-validity columns may be `valid_from`/`valid_until`, with a seam mapping to the edge wire.

## 6. Exclusion vs ranking — different machinery, kept apart

- The **axes** drive **binary EXCLUSION** (the `admissible` + read-mode filter). "Not live" ⇒ excluded.
- **Graded attributes** (confidence / salience / decay / relevance) drive **RANKING**, via the F9 importance
  signal + the recency-reweight seam. A low-confidence record is still *live* — it is down-ranked, not
  excluded. The engine's **signal algebra** (value range, monotonicity, missing-value default, combination
  law with BM25·vector·RRF·recency) is an **open contract item** (deferred to F9, ~0.8.16).

## 7. Memex composition — labels are predicates, not engine enum values

Memex's lifecycle labels are **composed predicates** over the structural axes + its own reason/view-policy:

- `live` = `active ∧ is_latest ∧ in-force ∧ ¬view-excluded`
- `retired` = `deleted` **∧ reason = governance**
- `deleted` (user) = `deleted` **∧ reason = user**
- `expired` = validity predicate (structural), typically surfaced as a view over `active`
- `archived` = view-policy over `active`
- `superseded` = `¬is_latest` — a **logical query**, not a lifecycle value

Memex therefore **deletes its three hand-rolled not-live encodings** (resolves CR-056) and stores
reason/status as registry-projected or read-time-resolved attributes (resolves CR-057 — see the
[projection registry](projection-registry-and-async-embed.md)).
