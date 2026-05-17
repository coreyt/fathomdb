# Config

Engine-owned runtime knobs. The same five knobs are exposed by every
binding in idiomatic spelling (Python snake_case, TS camelCase, Rust
snake_case).

Authoritative spec: [`dev/design/engine.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/design/engine.md);
cross-binding symmetry pinned by `dev/design/bindings.md` § 6.

## Knob matrix

| Knob                          | Python                     | TypeScript                | Type             | Notes                                                                |
| ----------------------------- | -------------------------- | ------------------------- | ---------------- | -------------------------------------------------------------------- |
| Embedder pool size            | `embedder_pool_size`       | `embedderPoolSize`        | `int \| None`    | Max concurrent embedder calls. `None` = engine default.              |
| Scheduler runtime threads     | `scheduler_runtime_threads`| `schedulerRuntimeThreads` | `int \| None`    | Threads in the scheduler runtime. `None` = engine default.           |
| Provenance row cap            | `provenance_row_cap`       | `provenanceRowCap`        | `int \| None`    | Max provenance rows retained.                                        |
| Embedder call timeout (ms)    | `embedder_call_timeout_ms` | `embedderCallTimeoutMs`   | `int \| None`    | Per-call timeout for embedder invocations.                           |
| Slow query threshold (ms)     | `slow_threshold_ms`        | `slowThresholdMs`         | `int \| None`    | Profiling event emission threshold. Mutable via `set_slow_threshold_ms`. |

`None` (Python) / `undefined` (TS) → engine default. Defaults are
internal-tunable; callers should not depend on specific numeric
defaults.

## Python — two equivalent forms

Object form:

```python
from fathomdb import Engine, EngineConfig

config = EngineConfig(embedder_pool_size=4, slow_threshold_ms=200)
engine = Engine.open("./mydb.fdb", config=config)
```

Keyword form:

```python
engine = Engine.open(
    "./mydb.fdb",
    embedder_pool_size=4,
    slow_threshold_ms=200,
)
```

The two forms are mutually exclusive within a single `Engine.open`
call. Unknown keyword arguments are rejected with `TypeError`.

## TypeScript

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("./mydb.fdb", {
  engineConfig: {
    embedderPoolSize: 4,
    slowThresholdMs: 200,
  },
});
```

`EngineOpenOptions` may carry a TS-binding-specific
ThreadsafeFunction handoff-pool sizing option **beside** `engineConfig`.
That option is a TS-runtime concern, not a canonical engine config
field, and has no Python counterpart by design.

## Non-fields

Python executor usage is caller-owned and is not an engine config
field. Path is positional on `Engine.open` and is not a config field.

## Mutable-at-runtime

Only `slow_threshold_ms` / `slowThresholdMs` is mutable post-open via
`engine.set_slow_threshold_ms` / `engine.setSlowThresholdMs`. All
other knobs are open-time only.

## See also

- [Python API — Engine.open](python-api.md)
- [TypeScript API — Engine.open](typescript-api.md)
- Authoritative spec: [`dev/design/engine.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/design/engine.md)
