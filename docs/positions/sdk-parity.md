# SDK Parity

FathomDB aims for a shared high-level SDK shape across Python and TypeScript,
while keeping the CLI as a distinct operator surface. Public docs should
describe parity at the behavior level rather than exposing internal planning
artifacts.

## What parity means, concretely (0.8.20)

Parity is **enforced**, not aspirational. A single governed-surface allowlist
(`src/conformance/governed-surface-allowlist.json`) is loaded by both the
Python and the TypeScript conformance suites, so the two bindings cannot carry
divergent copies. Each suite introspects its live command surface and asserts
it is a subset of that allowlist, and that the core verbs are present.

Both bindings therefore expose:

- the same governed commands, in each language's idiomatic spelling
  (`read.get_many` / `read.getMany`, `erase_source` / `eraseSource`);
- the same 27-class error taxonomy under a single catch-all root
  (`EngineError` in Python, `FathomDbError` in TypeScript), with the same
  typed payload fields in snake_case / camelCase;
- the same result shapes, including the typed `SearchHit.id`.

## What parity does *not* mean

- **Not equal maturity.** Python is the more heavily exercised binding and is
  the recommended choice for production pilots.
- **Not identical ergonomics.** `drain` takes **seconds** in Python and
  **milliseconds** in TypeScript; TypeScript is Promise-returning throughout;
  TypeScript accepts both camelCase and snake_case for dual-spelled write keys.
- **Not identical to the Rust facade.** Rust is a *different consumer
  contract* — a set of re-exported types plus inherent `Engine` methods, not a
  verb namespace — so it is governed by its own allowlist and is not asserted
  membership-equal to the binding verb set.
