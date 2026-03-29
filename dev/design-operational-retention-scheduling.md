# Design: Operational Retention Scheduling Documentation

## Purpose

Address the verified finding that operational mutation retention requires
manual scheduling (M-3). The engine provides `plan_operational_retention`
and `run_operational_retention` primitives but no automatic trigger.

---

## Current State

`crates/fathomdb-engine/src/admin.rs:1120-1151`
`crates/fathomdb/src/lib.rs:227-250`

Both methods are public API but require explicit invocation. Without
operator-scheduled calls, `operational_mutations` grows without bound for
collections with `retention_json` policies.

---

## Assessment

The current design is intentional: the engine provides plan/run
primitives and scheduling is the operator's responsibility. This follows
the same principle as provenance retention (C-5) — the engine should not
own a background scheduler thread.

The finding is accurate but the fix is documentation, not code. An
operator who declares `retention_json` on a collection must also arrange
periodic invocation of the retention primitives.

---

## Design

### Documentation requirements

1. **Collection registration docs:** When documenting `retention_json` in
   the operational collection registration API, state explicitly that
   declaring a retention policy does not enable automatic enforcement.
   The operator must schedule periodic calls to `plan_operational_retention`
   and `run_operational_retention`.

2. **Operator runbook entry:** Add a retention section to the operational
   playbook showing a cron-based scheduling example:

   ```
   # Run every hour
   0 * * * * fathom-integrity retention run --database /path/to/db
   ```

3. **Python surface:** Document the `plan_operational_retention()` and
   `run_operational_retention()` methods on `FathomAdmin` with a note
   that they should be called periodically.

### Future: optional auto-retention interval

A later enhancement could add an `auto_retention_check_interval` option
to the engine that runs retention planning on a timer from the writer
thread. This is explicitly deferred — it adds background thread
complexity and the current manual approach is correct for v1 where
operator visibility is more important than convenience.

---

## Not in scope

- Implementing an engine-internal scheduler.
- Changing the retention API.

---

## Test Plan

No code changes. Review documentation for clarity and completeness.
