# CLI

Binary: `fathomdb`. Operator-only in 0.6.0; the CLI does **not**
ship application-surface verbs like `search`, `get`, or `list`. Use
the SDK for those.

Authoritative spec:
[`dev/interfaces/cli.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/interfaces/cli.md);
recovery semantics owned by `dev/design/recovery.md`.

## Roots

- `fathomdb doctor <verb> ...` — read-only or artifact-producing
  diagnostics.
- `fathomdb recover --accept-data-loss <sub-flag> ...` — the only
  lossy / non-bit-preserving root.

## Output

- `--json` is the normative machine-readable contract on every verb.
- `--pretty` is a human-only formatter on verbs that explicitly
  document it.
- `recover --json` emits an NDJSON progress stream plus a final
  summary object. All other verbs emit a single JSON object.
- Doctor wrap shape: `{ "verb": "<verb-name>", ...flattened_report_fields... }`.
- Field names: serde `snake_case`.

## Doctor verbs

| Verb              | Synopsis                                                                       | Exit codes        |
| ----------------- | ------------------------------------------------------------------------------ | ----------------- |
| `check-integrity` | `fathomdb doctor check-integrity [--quick] [--full] [--round-trip] [--pretty]` | `0` / `65` / `70` / `71` |
| `safe-export`     | `fathomdb doctor safe-export <out> [--manifest <path>]`                        | `0` / `66` / `71` |
| `verify-embedder` | `fathomdb doctor verify-embedder --identity <s> --dimension <n>`               | `0` / `65`        |
| `trace`           | `fathomdb doctor trace --source-ref <id>`                                      | `0` / `65` / `70` / `71` |
| `dump-schema`     | `fathomdb doctor dump-schema`                                                  | `0` / `65` / `70` / `71` |
| `dump-row-counts` | `fathomdb doctor dump-row-counts`                                              | `0` / `65` / `70` / `71` |
| `dump-profile`    | `fathomdb doctor dump-profile`                                                 | `0` / `65` / `70` / `71` |

`check-integrity --full` may emit doctor-only finding codes such as
`E_CORRUPT_INTEGRITY_CHECK`.

## Recover root

```text
fathomdb recover --accept-data-loss
  [--truncate-wal]
  [--rebuild-vec0]
  [--rebuild-projections]
  [--excise-source <id>]
```

Exit codes: `0` / `64` / `70` / `71`.

`--accept-data-loss` is declared on the `recover` parser only;
`doctor` verbs reject it as unknown.

`--rebuild-projections` is the canonical 0.6.0 regenerate workflow
for failed or stale projections. There is no separate
`fathomdb regenerate` command.

### Client workaround — bulk delete by source

Until logical-id verbs land in 0.7.x, the canonical 0.6.0 path for
deleting all rows from a single ingestion source is:

```bash
fathomdb recover --accept-data-loss --excise-source <id> --json
```

## Exit-code classes

| Code | Meaning                                                              |
| ---- | -------------------------------------------------------------------- |
| `0`  | successful completion; no findings requiring non-zero exit           |
| `64` | recovery completed because lossy action was explicitly accepted      |
| `65` | doctor / verification surface found actionable non-clean state       |
| `66` | export / materialization failure on an artifact-producing doctor verb|
| `70` | unrecoverable command failure                                        |
| `71` | lock-held or equivalent precondition-blocked outcome                 |

The full engine-error → exit-code mapping is in the locked spec.

## Logical-id verbs (deferred)

`purge_logical_id` and `restore_logical_id` are deferred to **0.7.x**
(HITL re-confirmed 2026-05-17). The canonical-identity substrate is
design-only in 0.6.0. See
[release notes § Logical-id verbs](../release-notes/0.6.0.md).

## See also

- [Errors](errors.md)
- [Install — Rust / CLI](../install/rust.md)
- Locked spec: [`dev/interfaces/cli.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/interfaces/cli.md)
