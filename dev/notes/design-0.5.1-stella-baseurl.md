# Design: StellaEmbedder baseUrl fix (0.5.1 Item 6)

**Release:** 0.5.1  
**Scope item:** Item 6 from `dev/notes/0.5.1-scope.md`  
**Breaking:** Minor (constructor now requires explicit `baseUrl` when not using local model)

---

## Problem

`StellaEmbedder` in TypeScript hardcodes `baseUrl: "https://api.stella.ai/v1"` as
default. No public hosted API exists at that address for `stella_en_400M_v5`.
Any caller not providing an explicit `baseUrl` gets silent network failures.

---

## Current state (anchored to HEAD)

`typescript/packages/fathomdb/src/embedders/stella.ts`:
- `StellaEmbedder implements QueryEmbedder`
- `StellaEmbedderOptions` has `baseUrl` optional, defaults to `https://api.stella.ai/v1`
- `model: stella_en_400M_v5`

---

## Design

Two acceptable implementations; pick one:

**Option A — throw at construction time (recommended):**

```typescript
export interface StellaEmbedderOptions {
    baseUrl?: string;  // keep optional in interface
    // ...other options
}

constructor(options: StellaEmbedderOptions = {}) {
    if (!options.baseUrl) {
        throw new Error(
            "StellaEmbedder: no hosted API exists for stella_en_400M_v5; " +
            "provide baseUrl pointing to your local inference server"
        );
    }
    // ...
}
```

**Option B — make `baseUrl` required in interface:**

```typescript
export interface StellaEmbedderOptions {
    baseUrl: string;  // required — no default
    // ...other options
}
```

Option A gives a clearer error message for callers who forget. Option B is caught
at compile time if TypeScript strict mode is on, silent at runtime if not. 

**Decision: Option A.** Error message directs the caller to the fix.
Option B may silently break callers who destructure without `baseUrl`.

### JSDoc update

Add to `StellaEmbedder` class JSDoc:

```
 * @throws {Error} if `baseUrl` is not provided — no public hosted API exists
 *   for stella_en_400M_v5; configure a local inference server and pass its URL.
```

---

## Acceptance criteria

1. `new StellaEmbedder()` (no options) throws with message containing `"no hosted API exists"`.
2. `new StellaEmbedder({ baseUrl: "http://localhost:8080" })` constructs without error.
3. TypeScript unit test covers both cases.
4. No other embedder classes affected.
