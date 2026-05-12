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

| Row | Verb              | Engine seam method               | 10a / 10b | Stub status post-Phase-5 |
| --- | ----------------- | -------------------------------- | --------- | ------------------------ |
| R1  | `check-integrity` | `Engine::check_integrity`        | 10a       | scaffolded               |
| R2  | `safe-export`     | `Engine::safe_export`            | 10a       | scaffolded               |
| R3  | `trace`           | `Engine::trace_source_ref`       | 10a       | scaffolded               |
| R4  | `verify-embedder` | (10b) `Engine::verify_embedder`  | 10b       | parser stub needed       |
| R5  | `dump-schema`     | (10b) `Engine::dump_schema`      | 10b       | parser stub needed       |
| R6  | `dump-row-counts` | (10b) `Engine::dump_row_counts`  | 10b       | parser stub needed       |
| R7  | `dump-profile`    | (10b) `Engine::dump_profile`     | 10b       | parser stub needed       |

## Closure protocol

This file's status flips `open` → `closed` only when ALL of:

- R1..R3 ship in Phase 10a with real seam wire-up + `--json` output.
- R4..R7 ship in Phase 10b with real engine seams + CLI wire-up.
- `parser.rs` + `operator_cli.rs` cover every row's `--help` invocation
  asserting exit 0 and `Usage:` substring.

When all rows green, this file's `status` frontmatter flips to `closed`
and a closure note is added (date, commits that landed each row).

## Citations

- `dev/acceptance.md` AC-040a, AC-040b (single assertions; this file is
  the enumerated row-set they cover — no AC split).
- `dev/interfaces/cli.md § Doctor verbs`.
- `dev/plans/0.6.0-implementation.md § Phase 10a / 10b`.
