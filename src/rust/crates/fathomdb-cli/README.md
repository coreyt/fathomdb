# fathomdb-cli

The `fathomdb` operator and diagnostics binary for **FathomDB**, a local-first embedded retrieval
engine built on SQLite.

This is a **tool, not a library API**. It exists so an operator holding a FathomDB database file can
inspect it, verify it and repair it without writing code. It is deliberately *not* a query client:
there is no `search`, no `get`, no `list`. Application access goes through the
[`fathomdb`](https://crates.io/crates/fathomdb) crate or the Python / Node.js SDKs.

## Status: pre-1.0, beta

The 0.8.x line is under active development. Verbs, flags and JSON shapes can change between minor
releases.

## Install

```bash
cargo install fathomdb-cli
```

That installs a binary named `fathomdb`.

## Two roots

```text
fathomdb doctor <verb> [--json] [<db-path>]             # read-only diagnostics
fathomdb recover --accept-data-loss [flags] <db-path>   # lossy repair
```

`doctor` never destroys anything. `recover` can, which is why every `recover` invocation requires an
explicit `--accept-data-loss` acknowledgement: without it the command refuses with
`E_RECOVER_REQUIRES_ACCEPT_DATA_LOSS` and exits 70. There is no way to run a lossy workflow by
accident.

Most `doctor` verbs take a database path; `warm-cache` operates on the shared weight cache and takes
none.

### `doctor` verbs

| Verb | What it does |
| --- | --- |
| `check-integrity` | Structural integrity check; reports typed findings with locators |
| `safe-export` | Materialise a safe export of the database |
| `verify-embedder` | Confirm the embedder identity recorded in the database matches this build |
| `trace` | Trace the resolution chain for a source reference |
| `dump-schema` | Dump the canonical schema definition |
| `dump-row-counts` | Per-table row counts |
| `dump-profile` | The response-cycle profile the engine recorded |
| `dump-mutations` | Page through the operational-mutation log for one collection |
| `orphan-provenance` | Per-`source_id` census; reports rows reachable by **no** erasure verb |
| `warm-cache` | Pre-fetch and verify the pinned default-embedder weights (needs the `default-embedder` feature) |
| `recompute-mean` | Re-derive and re-pin the corpus mean, re-quantizing every vector in one transaction |

### `recover` flags

`--truncate-wal`, `--rebuild-vec0`, `--rebuild-projections`, `--excise-source <id>`, and
`--excise-collection <c> --excise-record-key <k>` (which must be given together).

## Machine-readable output

`--json` is available on every verb and is the normative output contract; the human-readable form is
for humans.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Clean |
| 64 | `recover` completed, but only because lossy action was explicitly accepted |
| 65 | Diagnostics found actionable non-clean state |
| 66 | An artifact-producing verb failed to materialise its export |
| 70 | Unrecoverable command failure |
| 71 | Lock held, or an equivalent precondition blocked the command |

These classes are stable and safe to branch on in scripts.

## Cargo features

| Feature | Effect |
| --- | --- |
| `default-embedder` | Required for `doctor warm-cache` to actually fetch weights. Without it the subcommand is present but cannot load the embedder. |

## License

MIT. See the `LICENSE` file shipped in this crate.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
