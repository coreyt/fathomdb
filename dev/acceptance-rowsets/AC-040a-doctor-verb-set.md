---
title: AC-040a / AC-040b doctor-verb row-set
date: 2026-05-12
target_release: 0.6.0
desc: Authoritative verb enumeration for AC-040a (--help exits 0) and AC-040b (--help contains Usage)
blast_radius: dev/acceptance.md AC-040a/AC-040b; src/rust/crates/fathomdb-cli; Phase 10
status: open
---

# AC-040a / AC-040b — doctor verb row-set

Each row below MUST satisfy both AC-040a (`fathomdb doctor <verb> --help`
exits 0) and AC-040b (`--help` output contains a `Usage:` section).

| Row | Verb              | Engine seam method              | 10a / 10b | Stub status post-Phase-5 |
| --- | ----------------- | ------------------------------- | --------- | ------------------------ |
| R1  | `check-integrity` | `Engine::check_integrity`       | 10a       | scaffolded               |
| R2  | `safe-export`     | `Engine::safe_export`           | 10a       | scaffolded               |
| R3  | `trace`           | `Engine::trace_source_ref`      | 10a       | scaffolded               |
| R4  | `verify-embedder` | (10b) `Engine::verify_embedder` | 10b       | parser stub needed       |
| R5  | `dump-schema`     | (10b) `Engine::dump_schema`     | 10b       | parser stub needed       |
| R6  | `dump-row-counts` | (10b) `Engine::dump_row_counts` | 10b       | parser stub needed       |
| R7  | `dump-profile`    | (10b) `Engine::dump_profile`    | 10b       | parser stub needed       |

## Closure protocol

This file's status flips `open` → `closed` only when ALL of:

- R1..R3 ship in Phase 10a with real seam wire-up + `--json` output.
- R4..R7 ship in Phase 10b with real engine seams + CLI wire-up.
- `parser.rs` + `operator_cli.rs` cover every row's `--help` invocation
  asserting exit 0 and `Usage:` substring.

When all rows green, this file's `status` frontmatter flips to `closed`
and a closure note is added (date, commits that landed each row).

## 2026-05-12 — Phase 10a partial closure

Rows R1..R3 (`check-integrity`, `safe-export`, `trace`) satisfied by real
engine wire-up: every verb invokes its landed engine seam, emits a single
JSON object under the `{ "verb": "<name>", ... }` envelope, and maps
`EngineError` through the `cli.md § Error → exit-code mapping` table.
Rows R4..R7 (`verify-embedder`, `dump-schema`, `dump-row-counts`,
`dump-profile`) satisfied by parser stub path only: `--help` exits 0 with
a `Usage:` line (AC-040a/b), runtime invocation surfaces lock-held / open
errors then emits the `not_implemented` JSON envelope and exits 70.

Full row-set closure (status `open` → `closed`) requires Phase 10b engine
seam landings for R4..R7.

Commits (10a closure):

- `1f824a6` — `feat(engine): RebuildReport + RebuildKind ...`
- `98a779c` — `docs(interfaces/rust): reconcile recovery seam ...`
- `573e351` — `test(engine): assert vec0 rebuild rows_rebuilt==0 ...`
- `e163153` — `feat(facade): re-export recovery seam types ...`
- `53e58eb` — `feat(cli): wire doctor + recover verbs to landed engine seams`
- `30cb007` — `test(cli): bind AC-035d/036/037/038/045 runtime + flip Phase-9-deferred ignores`
- `29aa705` — `fix(cli): safe-export errors map to exit 66; strip doctor: prefix`

Status: remains `open` until 10b lands.

## Citations

- `dev/acceptance.md` AC-040a, AC-040b (single assertions; this file is
  the enumerated row-set they cover — no AC split).
- `dev/interfaces/cli.md § Doctor verbs`.
- `dev/plans/0.6.0-implementation.md § Phase 10a / 10b`.
