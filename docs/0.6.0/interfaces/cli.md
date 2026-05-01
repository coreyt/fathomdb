---
title: CLI Public Interface
date: 2026-04-24
target_release: 0.6.0
desc: Public CLI surface for 0.6.0
blast_radius: TBD
status: draft
---

# CLI Interface

Public CLI surface for the 0.6.0 operator binary. The canonical verb table and
recovery semantics are owned by `design/recovery.md`; this file owns concrete
flag spelling, root command paths, and exit-code classes.

## Roots

- `fathomdb recover --accept-data-loss <sub-flag>...`
- `fathomdb doctor <verb> ...`

The CLI is **operator-only** in 0.6.0. It does not mirror the SDK five-verb
application surface and does not ship `search` / `get` / `list` query verbs.

## Output posture

- `--json` is the normative machine-readable contract on every verb.
- `doctor check-integrity` emits a single JSON object.
- `doctor check-integrity --full` may emit doctor-only finding codes such as
  `E_CORRUPT_INTEGRITY_CHECK`.
- `recover` JSON output is a progress stream plus summary, owned by
  `design/recovery.md`.
- `--pretty` is a human-only formatter on verbs that explicitly document it;
  it is not a separate machine schema.

## Doctor verbs

| Verb | Synopsis | Exit class |
|---|---|---|
| `check-integrity` | `fathomdb doctor check-integrity [--quick] [--full] [--round-trip] [--pretty]` | `doctor-check-*` = 0 / 65 / 70 / 71 |
| `safe-export` | `fathomdb doctor safe-export <out> [--manifest <path>]` | `doctor-export-*` = 0 / 66 / 71 |
| `verify-embedder` | `fathomdb doctor verify-embedder` | `doctor-check-*` = 0 / 65 |
| `trace` | `fathomdb doctor trace --source-ref <id>` | `doctor-check-*` |
| `dump-schema` | `fathomdb doctor dump-schema` | `doctor-check-*` |
| `dump-row-counts` | `fathomdb doctor dump-row-counts` | `doctor-check-*` |
| `dump-profile` | `fathomdb doctor dump-profile` | `doctor-check-*` |

## Recover root

`recover` is the only lossy / non-bit-preserving root in 0.6.0.

```text
fathomdb recover --accept-data-loss
  [--truncate-wal]
  [--rebuild-vec0]
  [--rebuild-projections]
  [--excise-source <id>]
  [--purge-logical-id <id>]
  [--restore-logical-id <id>]
```

Exit class: `recover-*` = 0 / 64 / 70 / 71.

`--accept-data-loss` is declared on the `recover` parser only. `doctor` verbs
reject it as unknown.

`--rebuild-projections` is the canonical 0.6.0 regenerate workflow for failed
or stale projections. The docs may refer to "regenerate" as the workflow name,
but there is no separate `fathomdb regenerate` command in 0.6.0.
