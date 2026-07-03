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

`is_latest` (bool) / `superseded` (= `¬is_latest`), one indexed column, enforced by:

```sql
UNIQUE(logical_id) WHERE is_latest = 1
```

**This partial unique index is the single "is-latest determination" Q2 asks for** — one authoritative
answer every search path uses, so no caller re-derives (closes the CR-060 divergence).

- **Head-deletion invariant:** currency is **positional** — the latest insert is the head, deleted or not.
  A `logical_id` is *live* iff its **head is admissible**. Deleting the head does **not** resurrect the
  prior version (silent reactivation is forbidden). A revert is a **new write** (append-only history).
- **`superseded` is not "not-live."** An old version is often **historical-but-valid** ("what was true
  then"). It is excluded only by **view-policy** (the default dedup-to-current mode), never treated as a
  deletion. `include_superseded` returns history.
- The write mechanism is **"supersede-insert" / "head-advance"** — **never** called a "tombstone" (see §5).

## 3. Axis: temporal validity

`valid_from` / `valid_until`, one **half-open** interval `[valid_from, valid_until)`, `NULL` = unbounded on
that side, **single window** (not recurrence — "weekdays 9–5" is not expressible; a re-entrant window is
one future window, not a schedule).

- This is **valid time** (world time / "what is in force"), named **distinctly** from **transaction time**
  (write time / version-currency). Full bitemporal query is out of scope, but the schema uses the standard
  names so it is not precluded.
- **"expired" is a predicate, never a stored label:** `¬(valid_from ≤ :now < valid_until)`.
- **Evaluated at query time** via the typed `SearchFilter` seam with a **bound `:now` parameter** (never
  `unixepoch()` inline) — buys testability, replay, and `valid_as_of(t)` for free. At target scale (<~50k
  rows, exact/brute-force vector) a two-column range compare over candidates is microseconds and costs
  **zero recall**; materializing the passage of time into a static index is incoherent and rejected.
- **Boundary crossings are lazy, on next touch** (an embedded library has no daemon). The engine offers a
  **caller-invoked** `crossed_boundary_since(t)` detection hook the host runs opportunistically — a
  detection hook, **not** eager prevention. A never-queried future-window row does silently re-activate
  when finally read; this is stated honestly, not papered over.

## 4. Retrieval contract & read modes

- **Materialize only** `admissible = active ∧ is_latest` (write-stable, flipped **transactionally** — both
  conjuncts change only on writes). This is the single cheap bit the hot path filters on.
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

They must **never share the name**. Additional banned overloads:

- **"tombstone"** off the currency axis — the head-advance mechanism is not a tombstone (existence ⟂ currency).
- **"current"** near the temporal axis — use **`is_latest`** for currency, **in-force** for validity.
- **`t_invalid`** — collides with Memex's **schema-invalid** quarantine concept; use `valid_from`/`valid_until`.

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
