# Design: TypeScript feedback fast-path allocation (0.5.2 Item 6)

**Release:** 0.5.2
**Scope item:** Item 6 from `dev/notes/0.5.2-scope.md` (GH #33)
**Breaking:** No (internal refactor; public API unchanged)

---

## Problem

Every SDK method that supports progress callbacks constructs a
`runWithFeedback` options object on every call, even when no callback
is provided:

```typescript
#run<T>(
  operationKind: string,
  operation: () => T,
  progressCallback?: ProgressCallback,
  feedbackConfig?: FeedbackConfig,
): T {
  return runWithFeedback({
    operationKind,
    metadata: {},
    progressCallback,
    feedbackConfig,
    operation,
  });
}
```

`runWithFeedback` has an early return when `progressCallback` is
undefined, but the call-site allocation (options bag + `metadata: {}`
literal) still occurs on every call.

---

## Impact

Low. Single small short-lived object per call; V8 optimizes these
well. Matters only at very high call rates (thousands of
writes/queries per second). The fix is still worth doing because:

- It's a one-line change per call site.
- It removes a meaningless allocation from the hot path, making future
  micro-optimization cleaner.
- It keeps the engine / query / admin surfaces cohesive — if we want
  to add telemetry hooks later, the fast-path bypass is the right
  place to gate them.

---

## Current state (anchored to 0.5.1 HEAD)

Three files, each with a `#run<T>(...)` method:

- `typescript/packages/fathomdb/src/engine.ts`
- `typescript/packages/fathomdb/src/query.ts`
- `typescript/packages/fathomdb/src/admin.ts`

All call `runWithFeedback` with the full options bag unconditionally.

---

## Goal

Skip the options-bag allocation entirely when no progress callback is
passed. Short-circuit to `operation()` directly.

---

## Design

### Shared helper

Extract the pattern into a shared helper to avoid repeating it in three
places:

```typescript
// typescript/packages/fathomdb/src/feedback.ts (existing file)

/**
 * Run `operation` with feedback instrumentation if a progress callback
 * is provided; otherwise short-circuit to `operation()` to avoid the
 * options-bag allocation on the fast path.
 */
export function runWithOptionalFeedback<T>(
  operationKind: string,
  operation: () => T,
  progressCallback?: ProgressCallback,
  feedbackConfig?: FeedbackConfig,
): T {
  if (!progressCallback) {
    return operation();
  }
  return runWithFeedback({
    operationKind,
    metadata: {},
    progressCallback,
    feedbackConfig,
    operation,
  });
}
```

### Call-site updates

Replace `#run` in each of engine.ts / query.ts / admin.ts:

```typescript
// before
#run<T>(operationKind: string, operation: () => T, pc?: ProgressCallback, fc?: FeedbackConfig): T {
  return runWithFeedback({ operationKind, metadata: {}, progressCallback: pc, feedbackConfig: fc, operation });
}

// after
#run<T>(operationKind: string, operation: () => T, pc?: ProgressCallback, fc?: FeedbackConfig): T {
  return runWithOptionalFeedback(operationKind, operation, pc, fc);
}
```

`#run` stays as a thin wrapper rather than inlining `runWithOptionalFeedback`
at the call sites — preserves the class-internal indirection seam so
subclasses or future instrumentation can override one spot.

### TDD approach

1. **Red: fast-path does not invoke `runWithFeedback`**

   Mock `runWithFeedback` via vitest spy:

   ```typescript
   it("skips runWithFeedback when no progress callback is provided", () => {
     const spy = vi.spyOn(feedbackModule, "runWithFeedback");
     const db = openEngineWithTempdb();
     db.write(someRequest); // no callback
     expect(spy).not.toHaveBeenCalled();
   });
   ```

   Fails against 0.5.1: `runWithFeedback` is called on every write.

2. **Red: slow-path still invokes `runWithFeedback`**

   ```typescript
   it("invokes runWithFeedback when a progress callback is provided", () => {
     const spy = vi.spyOn(feedbackModule, "runWithFeedback");
     const db = openEngineWithTempdb();
     db.write(someRequest, () => {});
     expect(spy).toHaveBeenCalledOnce();
   });
   ```

   Passes already; guard against regression.

3. **Green**: extract `runWithOptionalFeedback`, update all three
   `#run` methods. Both tests pass.

### Benchmarks (optional)

If time permits, add a synthetic microbenchmark that confirms the
fast-path is ~10-20% faster for trivial operations. Not a release gate
— correctness tests suffice.

---

## Out of scope

- GH #32 — TypeScript SDK options-bag *pattern* consistency for feedback
  parameters across all public methods. That's an API surface question;
  this is an internal hot-path optimization.
- Python SDK equivalent. Python's `run_with_feedback` already has the
  same shape; audit whether the fast-path bypass is also missing there
  but address separately if found.

---

## Acceptance

- Both new vitest tests pass.
- Existing engine / query / admin tests continue to pass.
- No behavior change visible from outside the SDK.

---

## Cypher enablement note

N/A. Internal SDK refactor.
