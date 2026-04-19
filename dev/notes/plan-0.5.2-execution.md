# Execution Plan: v0.5.2

**Date written:** 2026-04-18
**Based on:** `dev/notes/0.5.2-scope.md` and
`dev/notes/design-0.5.2-check-semantics-weighted-fts.md`
**Runbook:** `dev/notes/agent-harness-runbook.md`

---

## Pre-flight checklist (run before first launch)

```bash
cd /home/coreyt/projects/fathomdb
./scripts/preflight.sh --baseline
```

Record:

- `BASE_COMMIT` = HEAD hash on main (expected: `e62b73c` or the
  next doc commit on top of it).
- Pre-existing clippy warnings (do not count against the pack).
- Test count baseline: `cargo nextest run --workspace 2>&1 | tail -5`.

---

## Pack summary

| Pack | Scope items | Key files touched | Phase |
|---|---|---|---|
| A | Item 1 (`check_semantics()` weighted FTS fix) | `admin/mod.rs`, `writer/fts_extract.rs` (re-export if needed), new Rust + Python tests | 1 |

0.5.2 is a single-pack hotfix. No Phase 2.

---

## Phase 1

### Pack A: `check_semantics()` shape-aware drift detection

**Design doc:**
`dev/notes/design-0.5.2-check-semantics-weighted-fts.md`
**Branch:** `fix/0.5.2-check-semantics-weighted-fts`
**Agent:** `implementer` (worktree)

**MODIFY:**

- `crates/fathomdb-engine/src/admin/mod.rs` —
  `count_drifted_property_fts_rows` becomes shape-aware:
  - Probe per-kind FTS table columns via `PRAGMA table_info({table})`
    (mirror the idiom at `bootstrap.rs:1025-1029`).
  - Branch on presence of `text_content` column.
  - Non-weighted branch: keep existing query verbatim.
  - Weighted branch: new helper that builds a dynamic
    `SELECT fp.node_logical_id, fp.<col0>, fp.<col1>, ..., n.properties
    FROM {table} fp JOIN nodes n ON ...` using column names derived
    from the PRAGMA probe (excluding `node_logical_id`), then compares
    each row against
    `writer::extract_property_fts_columns(&props, schema)`. Per-row
    mismatch increments `drifted` by 1 (stays consistent with the
    non-weighted arm).
- `crates/fathomdb-engine/src/writer/mod.rs` or
  `crates/fathomdb-engine/src/writer/fts_extract.rs` — widen the
  `pub(crate)` visibility of `extract_property_fts_columns` if it is
  not already reachable from the `admin` module. Re-export as needed.
  Do not change its signature or behavior.
- New Rust tests (in-tree, `#[cfg(test)]` block in
  `crates/fathomdb-engine/src/admin/mod.rs` alongside the existing
  drift tests at line 4197):
  - `check_semantics_clean_on_weighted_fts_schema_does_not_panic`
  - `check_semantics_detects_drifted_property_fts_text_weighted`
  - `check_semantics_mixed_weighted_and_non_weighted_schemas`
- New Python test — `python/tests/test_admin.py` (create if not
  present) with
  `test_check_semantics_survives_weighted_fts_registration`.

**DO NOT TOUCH:**

- Search path (`crates/fathomdb-engine/src/coordinator.rs`) — already
  handles both shapes.
- Rebuild actor (`crates/fathomdb-engine/src/rebuild_actor.rs`) —
  already handles both shapes.
- Schema bootstrap / migrations
  (`crates/fathomdb-schema/src/bootstrap.rs`).
- Registration API (`crates/fathomdb-engine/src/admin/fts.rs`).
- Python / TypeScript / Go SDK surface. The fix is entirely
  internal.

**Target test:**

```bash
cargo nextest run -p fathomdb-engine admin
uv run pytest python/tests/test_admin.py -x
```

**TDD approach (red → green → refactor):**

1. **Red (Rust)**: add
   `check_semantics_clean_on_weighted_fts_schema_does_not_panic`
   first. Confirm it reproduces the Memex failure
   (`SqliteError: no such column: fp.text_content`). This is the
   regression gate — the test must fail on `main` before the fix
   lands.
2. **Green**: implement the shape-aware branch in
   `count_drifted_property_fts_rows`. The clean-DB test flips to
   passing.
3. **Red → Green for drift detection**: add
   `check_semantics_detects_drifted_property_fts_text_weighted`.
   Manually `UPDATE fts_props_<kind>` to mutate a column. Confirm the
   test fails (drift not yet detected if the weighted branch silently
   short-circuited), then verify the per-column comparison produces
   `drifted_property_fts_rows == 1`.
4. **Mixed schema**: add
   `check_semantics_mixed_weighted_and_non_weighted_schemas` to
   exercise both branches in a single call.
5. **Python smoke**: add
   `test_check_semantics_survives_weighted_fts_registration`,
   mirroring the Memex reproduction. Confirm red on `main`, green
   after Rust fix.
6. **Refactor**: extract the weighted branch into a private helper
   (`count_drift_weighted_property_fts_rows`) and keep the
   non-weighted branch as-is (`count_drift_non_weighted_property_fts_rows`,
   renamed from the current body). Keep the public helper
   (`count_drifted_property_fts_rows`) as a tiny dispatcher — this
   matches the per-branch structure in `rebuild_actor.rs:340-409`.

**Known risks:**

- Dynamic SQL: the weighted branch interpolates column names. Names
  come from `fathomdb_schema::fts_column_name`, which already
  sanitizes paths (tests at `bootstrap.rs:1956-1983`). Still worth a
  code-review pass on the review gate.
- Shape-probe correctness: if a per-kind table ever contains
  **both** `text_content` and per-path columns (hypothetical future
  mixed shape — no such schema exists in 0.5.1/0.5.2), the fix falls
  back to the non-weighted branch and would miss drift on the
  per-path columns. Call this out in the PR description so 0.6.x
  migrations that add such a shape know to revisit this helper.
- Escape: if the per-column reconstruction path turns out to be
  non-trivial during implementation, fall back to "skip weighted
  tables, return 0, leave a TODO" — covered as the emergency option
  in the design doc.

---

## Phase 1 merge and gate

After Pack A merges:

```bash
./scripts/preflight.sh --release
cargo nextest run --workspace 2>&1 | tail -15
uv run pytest python/tests -x
```

All gates must pass before release tagging.

---

## Release mechanics

Standard 0.5.x flow, intentionally minimal:

1. **CHANGELOG.md** — add a 0.5.2 section with a single `### Fixed`
   entry describing the weighted-FTS `check_semantics()` regression
   (exact wording in the design doc's CHANGELOG entry section).
2. **Version bump** across all four manifests + `Cargo.lock`:
   - `Cargo.toml` (workspace root)
   - `crates/fathomdb/Cargo.toml`
   - `python/pyproject.toml`
   - `typescript/packages/fathomdb/package.json`
   - Regenerate `Cargo.lock` via `cargo check --workspace`.
3. **Preflight gates**:
   - `scripts/preflight.sh --release` (clippy tracing/python, nextest
     tracing — all three must pass).
   - `scripts/preflight-CI.sh` for the full CI-equivalent set before
     tagging.
4. **Tag** `v0.5.2` on HEAD. Release workflow gates on
   `all-builds-passed` (aggregator added in 0.5.1).

---

## Coordination

- Main thread plans and verifies; coding is delegated to an
  `implementer` agent in a worktree (per the
  `feedback_orchestrate_releases.md` memory).
- Reviews go to a `code-reviewer` agent before merge.
- Memex team is unblocked immediately via Option A
  (`xfail test_check_semantics_clean` pending 0.5.2) — no gating
  coordination required from this side beyond shipping 0.5.2
  promptly.

---

## Out of scope (see `dev/notes/0.5.3-scope.md`)

- scale.rs race
- TS `configureFts` parity
- FTS5 metachar escape (GH #31)
- TS feedback fast-path (GH #33)
- Node.js 20 action upgrade
- Python click helper audit
- All previously-drafted 0.5.3 items (test gaps, tmp-root hygiene,
  missing-docs sweep, FTS5 nested/weighted query surface)
