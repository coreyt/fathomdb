# 06 — Operational store (`operational_state` / `operational_mutations`)

**Component:** the KV / append-log substrate. `operational_state` (upsert by PK
`(collection_name, record_key)`, latest-value) and `operational_mutations`
(append-only log), governed by `operational_collections` (kind +
`schema_json` + retention). Migration `004_op_store.sql`.

## Why it matters

The op-store carries fast-changing app state (schedules, plans, the
`projection_failures` log, the `excise_source_audit`). It is write-path only
today (no SDK read), but G3 (`admin.read_collection`) and G7
(`history`/`mutations`) will expose reads over it — so its read shape is a 0.8.0
profiling target even though it's invisible to apps now.

## Ingest path — what to measure

- **JSON-schema validation cost.** Op-store writes validate `body` against the
  collection's `schema_json` **at save time, inside the writer lock, pre-commit**
  (AC-060b). This is the op-store's distinctive cost — it extends writer-lock hold
  (see `01-writer-thread.md`). Measure validate-time vs INSERT-time; complex
  schemas or large payloads make validation the dominant op-store cost.
- **`latest_state` upsert vs `append_only_log` append** — different cost shapes:
  upsert is an INSERT…ON CONFLICT DO UPDATE on the PK (B-tree lookup + write);
  append is a plain INSERT with AUTOINCREMENT. Profile separately.
- **Retention enforcement** — `enforce_provenance_retention` can delete oldest
  rows from `operational_mutations` if a cap is set (test-feature today). If
  active, it adds a delete pass per commit.

## Retrieval path — what to measure (G3/G7 baseline)

- **Read seams already exist** as `*_for_test` queries
  (`projection_failure_count_for_test`, `oldest_provenance_record_key_for_test`,
  ~`lib.rs:2245-2297`) — the exact SELECT shapes G3/G7 will promote. Baseline
  them now.
- **`append_only_log` unboundedness** — `operational_mutations` can reach ~1M
  rows. Any read MUST be `LIMIT` + after-id cursored; profile a full-collection
  scan vs a paged read to prove the cursor is necessary (materializing a
  huge Vec across FFI is the failure mode — see `10-bindings-ffi.md`).
- **`latest_state` is PK-bounded** — point/range reads on the PK are cheap;
  profile the by-key get vs the by-collection list.

## Sharp edges

- No index beyond the PK on `operational_state` and the autoincrement id on
  `operational_mutations` — a `WHERE collection_name=?` scan on mutations without
  the id cursor is O(rows). G3/G7 design must account for this; the profiler
  proves the cost.
- Validation is single-source-of-truth at the writer (bindings don't pre-validate)
  — so validation cost is entirely on the writer thread, not the binding.

## Scaling expectation

Op-store write cost is dominated by JSON validation for non-trivial schemas;
otherwise ~O(log N) per row. Read cost is fine for `latest_state` (PK-bounded)
and dangerous for `append_only_log` without paging — that asymmetry is the
profiling headline.
