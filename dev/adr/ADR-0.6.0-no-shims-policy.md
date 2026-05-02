---
title: ADR-0.6.0-no-shims-policy
date: 2026-04-27
target_release: 0.6.0
desc: No 0.5.x→0.6.0 deprecation shims; no within-0.6.x multi-release deprecation cycles
blast_radius: every binding (Python, TS, Rust, CLI); every public API; release policy; release-checklist; future ADR amendments
status: accepted
---

# ADR-0.6.0 — No shims / no deprecation policy

**Status:** accepted (HITL 2026-04-27, decision-recording).

Promoted from critic-3 M-2. The "0.6.0 has no compat path with 0.5.x"
posture is in `dev/notes/0.6.0-rewrite-proposal.md` but is not yet an
ADR. Cross-cuts every other Phase 1 ADR; needs a single citable
artifact so the policy is not re-litigated under release pressure.

## Context

0.5.x carried deprecation shims to keep API compat across releases.
Concrete example from commit `b4fe850` (0.5.6):

```python
admin.configure_vec(embedder)  # legacy 0.4.x form
# routed through admin.configure_embedding (modern form)
# synthesized a VecProfile-shaped return value from the embedder identity
```

That shim regressed in 0.5.6 because rename-passes shipped without
shim-specific tests. Memory `feedback_release_verification` codified
"deprecation shims are first-class code paths with their own tests."

The 0.6.0 rewrite is a clean break (per existing non-goals: no data
migration, no upgrade path for 0.5.x users in 0.6.0). The same
posture applies to API: no 0.5.x→0.6.0 shims at all.

## Decision

### Across-major (0.5.x → 0.6.0): zero shims

- 0.6.0 ships **no** deprecation shims for 0.5.x APIs.
- No `legacy_*` modules. No `compat_v0_5` features. No
  `#[allow(deprecated)]` in crate roots.
- No "raw_sql escape hatch for migration" (would also violate
  ADR-0.6.0-typed-write-boundary).
- No re-routing of old verbs through new ones (e.g. no
  `configure_vec(embedder)` → `configure_embedding` adapter).
- 0.5.x clients cannot import 0.6.0; 0.6.0 clients cannot run against
  0.5.x DBs (already fresh-DB-only).

### Within 0.6.x (0.6.0 → 0.6.x patch / minor)

- Patch releases (0.6.0 → 0.6.1): no API breaks. Bugfix-only.
- Minor breaks within 0.6.x are discouraged. When they happen, change
  is announced + removed in the **same release**. No multi-release
  deprecation cycles. No "soft-removal then hard-removal" pattern.

### Across-major (0.6 → 0.7+): out of scope

- Future major-version transitions are decided in their own ADRs.
- This ADR does **not** prejudge 0.7's posture.

## Options considered

**A — Zero shims at major boundary; no within-major deprecation
cycles (chosen).** Pros: cleanest possible boundary; matches
already-decided non-goals (no data migration, no upgrade path);
removes the "speculative knobs / silent feature-gated fallbacks"
Stop-doing class at the source. Cons: 0.5.x users have no migration
helper. Acceptable per existing non-goal.

**B — Allow shims under feature flag `legacy_v0_5`.** Pros: gentler
break for 0.5.x users. Cons: re-introduces the exact pattern
0.5.6 hit (untested shim, regression); feature-flag fallbacks are
the Stop-doing class; expectation creep ("just one more shim, one
more release"); doubles test surface for the 0.6.0 lifetime.

**C — Provide a 0.5.x → 0.6.0 migration tool, not shims.** Pros:
no in-engine compat code; a separate binary that reads 0.5.x state
and emits 0.6.0 inputs. Cons: still requires reading 0.5.x schema
(the rewrite proposal explicitly drops this); duplicates the
"no data migration" non-goal in tool form. Rejected.

## Consequences

- Every other ADR may cite this ADR when refusing a "but can we add
  a shim for $client?" request.
- ADR-0.6.0-typed-write-boundary cannot be softened by
  `legacy_sql_compat` flag (cross-cite).
- ADR-0.6.0-default-embedder cannot have a `python_st_compat` flag
  (cross-cite).
- ADR-0.6.0-operator-config-json-only cannot accept `.toml` "for one
  more release" (cross-cite).
- `release-policy.md` updated: changelog discipline includes the
  "announced + removed in same release" rule for within-0.6.x
  breaks.
- Followup `release-checklist.md`: add a release-gate question
  ("does this release introduce any deprecation? if yes, refuse").
- Memex / OpenClaw integration narratives (already dropped per
  Phase 1a disposition) cannot re-emerge as "wire-compat shim
  because $client uses 0.5.x"; cite this ADR.

## Non-consequences (what this ADR does NOT do)

- Does not forbid internal renames during 0.6.0 pre-freeze drafting.
- Does not decide wire-format versioning (separate decision-index #15).
- Does not decide 0.7+ posture.
- Does not prohibit deletion of unused fields / methods within a
  release (deletions are encouraged per net-negative-LoC Keep-doing).

## Citations

- HITL decision 2026-04-27.
- `dev/notes/0.6.0-rewrite-proposal.md` § "What we got wrong" #5;
  § "Architectural invariants".
- `b4fe850` (0.5.6 atexit + legacy `configure_vec` shim regression).
- Memory `feedback_release_verification` (deprecation shims as
  first-class code paths).
- Stop-doing entries: speculative knobs / silent feature-gated
  fallbacks; defect deferral patterns.
