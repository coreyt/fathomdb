# Design: TypeScript `configureFts` parity with Python (0.5.2 Item 2)

**Release:** 0.5.2
**Scope item:** Item 2 from `dev/notes/0.5.2-scope.md`
**Breaking:** Yes (TypeScript callers of `configureFts` for a kind
without a registered FTS property schema now receive a thrown error
instead of a silent no-op).

---

## Problem

0.5.1 aligned Python `configure_fts` with TypeScript `configureFts`
behavior on the happy path: both now auto-re-register the property FTS
schema under a new tokenizer. The scope decision for the *missing
schema* path accepted a deliberate divergence:

- Python raises `ValueError("no FTS property schema registered for
  kind=<name>")`.
- TypeScript silently skips re-registration, writes the profile, and
  returns.

The review called this out as an acceptable-for-0.5.1 gap. 0.5.2 closes
it by making TypeScript raise to match Python. Rationale:

- Cross-SDK behavior divergence confuses users porting between Python
  and TypeScript.
- Silent success when the caller's intent (re-register with new
  tokenizer) cannot be fulfilled is a footgun. The profile is written,
  but subsequent searches behave as if it weren't.
- 0.x semver permits the breaking change; TypeScript adoption is still
  small enough that the cost of fixing callers is lower than the cost
  of keeping the footgun.

---

## Current state (anchored to 0.5.1 HEAD)

`typescript/packages/fathomdb/src/admin.ts::configureFts`:

```typescript
async configureFts(kind: string, tokenizer: string, ...): Promise<FtsProfile> {
  // ... impact check ...
  const resolvedTokenizer = TOKENIZER_PRESETS[tokenizer] ?? tokenizer;
  const profile = /* setFtsProfile FFI call */;

  const schemaRaw = await this.#core.describeFtsPropertySchema(kind);
  if (schemaRaw !== null && schemaRaw.kind != null) {
    // re-register with resolved tokenizer ...
  }
  // Silent skip when schemaRaw === null.
  return profile;
}
```

Contrast Python (0.5.1 HEAD) `python/fathomdb/_admin.py::configure_fts`:

```python
schema = self._core.describe_fts_property_schema(kind)
if schema is None:
    raise ValueError(
        f"configure_fts({kind!r}): no FTS property schema registered; "
        f"call register_fts_property_schema first"
    )
# ... re-register under new tokenizer ...
```

Also note: 0.5.1 reordered Python so `describe` runs *before*
`set_fts_profile`, avoiding the partial-state hazard on the raise path.
TypeScript still writes the profile *before* the describe call. Any
parity fix must close both gaps: raise on missing schema AND eliminate
the partial-state hazard.

---

## Goal

TypeScript `configureFts` must:

1. Describe the existing schema *before* writing the profile.
2. If no schema is registered, throw a typed error with a message
   structurally matching the Python message.
3. If a schema exists, write the profile, then re-register (current
   behavior for the happy path — unchanged).

---

## Design

### New error type

Add to `typescript/packages/fathomdb/src/errors.ts`:

```typescript
/**
 * Thrown when an admin operation requires an existing FTS property
 * schema registration and none is found for the target kind. Matches
 * the Python SDK's `ValueError` contract.
 */
export class MissingFtsPropertySchemaError extends Error {
  readonly kind: string;
  constructor(kind: string) {
    super(
      `configureFts(${JSON.stringify(kind)}): no FTS property schema registered; ` +
        `call registerFtsPropertySchema first`
    );
    this.kind = kind;
    this.name = "MissingFtsPropertySchemaError";
  }
}
```

Export from `typescript/packages/fathomdb/src/index.ts` so library users
can discriminate via `err instanceof MissingFtsPropertySchemaError`.

### `configureFts` reordering

```typescript
async configureFts(kind: string, tokenizer: string, ...): Promise<FtsProfile> {
  // 1. Impact check (unchanged; may raise RebuildImpactError).
  await this.previewProjectionImpactOr(kind, "fts", options);

  // 2. Require an existing property schema BEFORE any state mutation.
  const schemaRaw = await this.#core.describeFtsPropertySchema(kind);
  if (schemaRaw === null || schemaRaw.kind == null) {
    throw new MissingFtsPropertySchemaError(kind);
  }

  // 3. Resolve preset + write profile.
  const resolvedTokenizer = TOKENIZER_PRESETS[tokenizer] ?? tokenizer;
  const profile = /* setFtsProfile FFI call */;

  // 4. Re-register with resolved tokenizer (unchanged happy path).
  await this.#core.registerFtsPropertySchemaWithEntries(
    kind, schemaRaw.entries, schemaRaw.separator, schemaRaw.excludePaths,
    resolvedTokenizer,
  );

  return profile;
}
```

### TDD approach

Tests live in `typescript/packages/fathomdb/test/admin.test.ts` (or a
new focused file; match the existing 0.5.1 configureFts test
conventions).

1. **Red: missing-schema raises**

   ```typescript
   it("throws MissingFtsPropertySchemaError when no schema registered", async () => {
     const engine = await openEngineWithTempdb();
     await expect(
       engine.admin.configureFts("UnknownKind", "porter")
     ).rejects.toBeInstanceOf(MissingFtsPropertySchemaError);
   });
   ```

   Fails against 0.5.1: current code returns a resolved profile.

2. **Red: missing-schema does not mutate state**

   ```typescript
   it("does not write the profile on the missing-schema error path", async () => {
     const engine = await openEngineWithTempdb();
     await expect(
       engine.admin.configureFts("UnknownKind", "porter")
     ).rejects.toBeInstanceOf(MissingFtsPropertySchemaError);
     const profile = await engine.admin.getFtsProfile("UnknownKind");
     expect(profile).toBeNull();
   });
   ```

   Fails against 0.5.1: profile is written before the describe call.

3. **Green path regression**: existing "configureFts re-registers
   schema under new tokenizer" test continues to pass.

### Documentation

- JSDoc on `configureFts` documents the thrown error type.
- 0.5.2 CHANGELOG Breaking section calls out the change with
  before/after pseudocode.

---

## Out of scope

- Python `configure_fts` surface is already correct; no change there.
- `configureVec` has a different shape (vector profile registration
  lives elsewhere). Parity review of `configureVec` is a separate
  consideration.

---

## Acceptance

- All three new tests pass after the refactor.
- Existing `configureFts` happy-path tests continue to pass.
- Python `configure_fts` tests are not regressed (no Python change).
- Manual: calling `configureFts("Unknown", "porter")` via the TypeScript
  SDK now produces a clear instance-of-checkable error.

---

## Cypher enablement note

N/A. Admin-surface change only; no query AST or engine surface change.
