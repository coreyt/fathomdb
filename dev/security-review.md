---
title: 0.6.0 Security Review
date: 2026-05-02
target_release: 0.6.0
desc: Threat model, attack surface inventory, and ranked findings for the locked 0.6.0 design corpus
blast_radius: design/*; interfaces/*; requirements.md; acceptance.md; test-plan.md; roadmap; freeze gate for Phase 4
status: locked
hitl_decision_date: 2026-05-02
---

# Security Review

This file is the Phase 3 output gate that must reach `locked` before Phase 4
freeze. The design corpus (`requirements`, `acceptance`, `needs`,
`traceability`, `design/*`, `interfaces/*`, `test-plan`) was locked
2026-05-02 per `dev/progress/0.6.0-hitl-lock-gate.md`. This review applies
to that frozen corpus.

## HITL decision (2026-05-02)

All 11 findings closed for 0.6.0 lock:

- **Acknowledged design posture (no code change):** SR-001, SR-009.
- **Accepted resolution path + new REQ/AC entries:** SR-002, SR-003, SR-004,
  SR-006, SR-007, SR-008, SR-010. Backed by `requirements.md` REQ-060..REQ-066
  and `acceptance.md` AC-064..AC-070 (added 2026-05-02 as an HITL amendment to
  the locked corpus).
- **Postponed to 0.8.0:** SR-005 (sidecar lock symlink/TOCTOU hardening),
  SR-011 (hardened malicious-file open path). Tracked in
  `dev/roadmap/0.8.0.md` with cross-references back to this file. SR-011
  also remains an acknowledged inherited posture for 0.6.0.

With this disposition, Phase 4 freeze gate clears.

## Scope and threat model

FathomDB 0.6.0 is an embedded SQLite-based library with a small operator CLI
(`fathomdb doctor` / `fathomdb recover`). It runs in-process inside the
application that calls the SDK. There is no network listener, no auth layer,
and no multi-tenant boundary inside one engine handle.

In-scope adversaries:

- **A1 — malicious caller payload.** Application caller sends crafted
  `write`/`search`/`admin.configure` payloads through the SDK FFI boundary.
- **A2 — malicious embedder.** Application supplies an embedder function
  that misbehaves (panics, returns wrong-shape vectors, exhausts time/memory,
  exfiltrates).
- **A3 — malicious or corrupt on-disk file.** Attacker controls bytes in
  `<db>.sqlite`, `<db>.sqlite-wal`, `<db>.sqlite-journal`, or
  `<db>.sqlite.lock` before `Engine.open` runs.
- **A4 — local filesystem race.** Local user with write access to the DB
  parent directory swaps a file for a symlink between check and use.
- **A5 — operator misuse.** Authorized operator runs a destructive
  `recover --accept-data-loss` subflag against the wrong target.

Out of scope (documented as non-goals, see Appendix A):

- Network-borne attackers (no IPC / network surface in 0.6.0 per
  `interfaces/wire.md`).
- Side-channel leakage between callers sharing one process (FathomDB is not a
  multi-tenant boundary).
- Attackers with code execution inside the host process (already game-over).
- Hardened defense against malicious SQLite database files. SQLite's upstream
  position is that opening attacker-controlled DBs is not a supported use
  case; FathomDB inherits this and documents it (SR-011).

## Attack surface inventory

| #   | Surface                                        | Owner doc                                          | Adversary |
| --- | ---------------------------------------------- | -------------------------------------------------- | --------- |
| 1   | `Engine.open(path, ...)` open-path stages      | `design/engine.md`, `design/migrations.md`         | A3, A4    |
| 2   | Embedder dispatch + warmup                     | `design/embedder.md`                               | A2        |
| 3   | Writer accept path (`PreparedWrite`, op-store) | `design/engine.md`, `design/op-store.md`           | A1        |
| 4   | Op-store JSON Schema validation (`schema_id`)  | `design/op-store.md`, `design/errors.md`           | A1        |
| 5   | Vec0 BLOB encoding boundary                    | `design/vector.md`                                 | A1, A2    |
| 6   | FFI string boundary (Python / TypeScript)      | `interfaces/python.md`, `interfaces/typescript.md` | A1        |
| 7   | Sidecar lock file (`<db>.sqlite.lock`)         | `design/engine.md` (open step 2)                   | A3, A4    |
| 8   | CLI parser + `--json` output                   | `interfaces/cli.md`, `design/recovery.md`          | A5        |
| 9   | Error `Display` content                        | `design/errors.md` § Foreign-cause sanitization    | A1, A3    |
| 10  | Migration execution                            | `design/migrations.md`                             | A3        |

## Findings

Severity bar for lock: **zero open findings at severity ≥ medium**. Low
findings may carry to followups with explicit call-out per the prior stub
contract.

---

## SR-001: Embedder runs in-process with no isolation boundary

**Severity:** high (accepted by design)
**Affected doc/component:** `design/embedder.md`; `Engine.open` warmup;
embedder dispatch pool
**Description:** Caller-supplied embedder code executes in the engine
process with full process privileges. A malicious embedder can read/write
arbitrary process memory, exfiltrate the database content, perform network
I/O, or trigger UB. The engine's only runtime guard is
`embedder_call_timeout_ms`, which is a liveness control, not a security
boundary.
**Proposed resolution:** Document explicitly in `design/embedder.md` and
SDK docs that the embedder is application code, trusted at the same level
as the calling process. No code change. Caller-side sandboxing
(subprocess, WASM, separate node) is the application's responsibility and
is out of scope for 0.6.0.
**Status:** acknowledged (HITL 2026-05-02). Documentation-only follow-up
captured in `design/embedder.md` and SDK READMEs.

---

## SR-002: JSON Schema regex ReDoS in op-store payload validation

**Severity:** medium
**Affected doc/component:** `design/op-store.md` § Write contract;
`schema_id` validation path; `SchemaValidationError`
**Description:** `latest_state` and `append_only_log` op-store collections
validate caller-submitted payloads against a `schema_id`-registered JSON
Schema before commit. JSON Schema `pattern` and `patternProperties`
keywords accept arbitrary regex. A backtracking regex engine plus a
crafted input string can cause catastrophic backtracking and stall the
writer thread (compare CVE-2025-69873 in `ajv`). Whether the schema
itself is caller-supplied or operator-supplied does not matter here: the
matched string comes from caller payloads, and the schema author may
unknowingly write a vulnerable pattern.
**Proposed resolution:** Configure the chosen JSON Schema validator to
use a linear-time regex engine for `pattern` / `patternProperties`. For
the Rust `jsonschema` crate this means selecting the `regex` backend
rather than the default `fancy-regex`. Add an op-store implementation
test that verifies a known catastrophic pattern (`^(a|a)*$` against a
non-matching string of length 30) returns a `SchemaValidationError` in
bounded time, not a hang.
**Status:** resolved (HITL 2026-05-02). Backed by REQ-060 + AC-064.

---

## SR-003: JSON Schema `$ref` external URI fetch (SSRF / local file read)

**Severity:** medium
**Affected doc/component:** `design/op-store.md` schema registration;
JSON Schema validator integration
**Description:** Default JSON Schema validators (including some Rust
crate defaults) resolve `$ref` URIs by network fetch (`http://`,
`https://`) and filesystem read (`file://`). A registered schema
containing
`{ "$ref": "http://169.254.169.254/latest/meta-data/" }` or
`{ "$ref": "file:///etc/passwd" }` would cause the engine process to
issue that request during validation. This is the same class of bug as
GHSA `mcp-from-openapi` and the json-schema-org guidance against
runtime external resolution.
**Proposed resolution:** Configure the validator to disable all external
resolvers. Allow only intra-document `$ref` (i.e. `#/$defs/...` or
local fragment refs) and reject any schema that registers a non-fragment
`$ref` at registration time, not at validation time. Add an op-store
test that registers a schema with `http`, `https`, and `file` `$ref`s
and asserts each is rejected by `admin.configure` with a clear error
before any payload validation runs.
**Status:** resolved (HITL 2026-05-02). Backed by REQ-061 + AC-065.

---

## SR-004: Wrong-dimension embedder return must not reach vec0

**Severity:** medium
**Affected doc/component:** `design/embedder.md`; `design/vector.md` (LE-f32
encoding); `EmbedderDimensionMismatchError` row in `design/errors.md`
**Description:** `design/errors.md` already lists
`EmbedderDimensionMismatchError` as a distinct error class. The vec0 BLOB
encoding requires fixed-length LE-f32 vectors; a wrong-length blob
silently corrupts vec0 partition rows. The contract is correct on paper,
but if dimension validation is not enforced strictly between embedder
return and vec0 write, a buggy or hostile embedder can break invariants
at the storage layer rather than surfacing a typed error.
**Proposed resolution:** Place dimension validation at exactly one
boundary: immediately after the embedder returns, before any vec0 write
runs. Surface as `EmbedderDimensionMismatchError`. Add a test that
substitutes a stub embedder returning a wrong-length vector and asserts
(a) no row is written to vec0 partition tables, (b) the error class is
exact, (c) the writer transaction rolls back cleanly.
**Status:** resolved (HITL 2026-05-02). Backed by REQ-062 + AC-066.

---

## SR-005: Sidecar lock file symlink / TOCTOU on POSIX and Windows

**Severity:** low
**Affected doc/component:** `design/engine.md` open-path step 2 (sidecar
lock acquisition)
**Description:** `<db>.sqlite.lock` is a FathomDB-defined sidecar, not the
SQLite-internal byte-range lock on the database file. If the engine
opens it under a path the local attacker can pre-populate as a symlink,
the lock open could follow the symlink and truncate or write to an
arbitrary attacker-chosen file. Similar bug pattern: `filelock`
CVE-2025-68146, CVE-2026-22701. Attack requires local filesystem write
access in the DB parent directory; for a fully embedded DB this is the
same privilege the application already has.
**Proposed resolution:** Open the sidecar lock with
`O_NOFOLLOW | O_CLOEXEC` on POSIX. On Windows, check
`GetFileAttributesW` for reparse points (`FILE_ATTRIBUTE_REPARSE_POINT`)
before `CreateFileW`, and reject if set. If the lock file already exists
and is not a regular file, fail open with `DatabaseLocked` or a typed
open error rather than silently following.
**Status:** postponed to 0.8.0 (HITL 2026-05-02). Tracked in
`dev/roadmap/0.8.0.md` § Security hardening (SR-005). Risk accepted for
0.6.0 because attack requires local filesystem write access in the DB
parent directory, which is the same privilege the application already
holds in the embedded-library trust model.

---

## SR-006: FFI panic-across-boundary safety

**Severity:** low
**Affected doc/component:** `interfaces/python.md`, `interfaces/typescript.md`,
`design/bindings.md`
**Description:** A Rust panic that unwinds across an `extern "C"` FFI
boundary into Python or Node.js is undefined behavior. PyO3 wraps Rust
function bodies in `catch_unwind` and translates panics into Python
exceptions; modern napi-rs does similar via N-API. Risk: any future
hand-written FFI shim that bypasses these wrappers, or any callback
invoked outside the wrapper, would be unsafe.
**Proposed resolution:** Pin the binding crates and enforce no
hand-rolled `extern "C"` Rust→Python or Rust→Node entry points outside
PyO3 / napi-rs macros. Add a clippy lint or grep-based CI check that
fails if `extern "C" fn` appears outside the binding crate boundary
files. Embedder callbacks (Rust→caller→Rust) must wrap caller code in
`catch_unwind` before crossing back into the engine.
**Status:** resolved (HITL 2026-05-02). Backed by REQ-063 + AC-067.

---

## SR-007: Null-byte and surrogate handling in FFI string arguments

**Severity:** low
**Affected doc/component:** `interfaces/python.md`, `interfaces/typescript.md`;
op-store / write payload string fields
**Description:** SQLite's C API treats text values as null-terminated
when bound via `sqlite3_bind_text` with a negative length. A null byte
inside a Python `str` or JavaScript `string` would silently truncate the
field at storage time. PyO3 and napi-rs `&str` conversions do not strip
or reject embedded `\0` by default. Additionally, Python `str` may
contain unpaired surrogates that are not valid UTF-8; these fail Rust
`&str` conversion with errors that should surface as
`WriteValidationError`, not as binding-internal `UnicodeError`.
**Proposed resolution:** Reject embedded `\0` and unpaired
surrogates at the FFI boundary, before payloads reach the writer. Map
both to `WriteValidationError` (caller-fix class). Add a binding-layer
test in Python and TypeScript covering each rejection.
**Status:** resolved (HITL 2026-05-02). Backed by REQ-064 + AC-068a/b.

---

## SR-008: Error `Display` content sanitization is asserted but not yet tested

**Severity:** low
**Affected doc/component:** `design/errors.md` § Foreign-cause sanitization
**Description:** The errors design forbids raw SQL text, absolute host
paths, and parser byte offsets in error `Display` output. This is
load-bearing for not leaking host filesystem layout or parser internals
to operator logs and bindings. The rule lives only in the design today;
no acceptance test asserts it.
**Proposed resolution:** Add at least one acceptance test per top-level
error root (`EngineError`, `EngineOpenError`) that constructs a foreign
cause known to embed a SQL fragment / absolute path / byte offset, and
asserts the `Display` output omits each of those fields. Wire to
`acceptance.md` AC-060\* if it does not already cover this.
**Status:** resolved (HITL 2026-05-02). Backed by REQ-065 + AC-069.

---

## SR-009: `recover --accept-data-loss` has no second confirmation gate

**Severity:** low (acknowledged operator-only surface)
**Affected doc/component:** `design/recovery.md`, `interfaces/cli.md`
**Description:** `--purge-logical-id`, `--excise-source`, and the other
lossy `recover` subflags execute on a single command line; there is no
interactive `--confirm` gate or `--dry-run` mode. An operator running
the wrong command against the wrong path destroys data. This is a UX
boundary, not a security boundary, and the CLI is documented as
operator-only (not application-callable).
**Proposed resolution:** Document explicitly in CLI help text and the
operator manual that `--accept-data-loss` is the entire confirmation
gate. No code change for 0.6.0. A future release may add `--dry-run` to
emit the JSON progress stream without committing the lossy step.
**Status:** acknowledged (HITL 2026-05-02). CLI help text + operator
manual documentation follow-up.

---

## SR-010: Migration partial-commit / corruption-on-failure risk

**Severity:** low
**Affected doc/component:** `design/migrations.md`,
`design/engine.md` open-path step 5
**Description:** Migrations advance `PRAGMA user_version`. A failure
mid-migration that does not roll back cleanly could leave the DB at an
in-between schema with the user_version sentinel still pointing at the
prior version, or worse, advanced past the failed step. Bindings would
then see `MigrationError` once but a subsequent `Engine.open` could read
inconsistent schema.
**Proposed resolution:** Confirm in the migrations implementation that
each migration runs in a single SQLite transaction with `BEGIN
IMMEDIATE`, that `user_version` advancement is the last statement before
`COMMIT`, and that failure rolls back to the prior `user_version`. Add a
test that injects a failing statement mid-migration and verifies the
DB's `user_version` is unchanged after the open returns
`MigrationError`.
**Status:** resolved (HITL 2026-05-02). Backed by REQ-066 + AC-070.

---

## SR-011: Opening attacker-controlled SQLite files is not hardened

**Severity:** informational
**Affected doc/component:** `design/engine.md` open-path; SDK docs
**Description:** SQLite upstream explicitly states that opening
attacker-controlled database files is outside the project's threat
model: most historical SQLite CVEs (e.g. CVE-2024-0232,
CVE-2025-29087, CVE-2025-6965) require a malicious DB or
attacker-supplied SQL. FathomDB layers on top of SQLite + sqlite-vec and
inherits this posture. The 0.6.0 open path's always-on detection
(`design/engine.md`) catches structural corruption but does not promise
robustness against adversarial bit patterns.
**Proposed resolution:** Document this inherited limitation in the SDK
README and `design/engine.md` ("Untrusted-file posture" subsection).
The `Engine.open` path parameter is application-controlled and
FathomDB does not add path-traversal protection beyond OS enforcement.
**Status:** acknowledged for 0.6.0 + postponed hardening to 0.8.0
(HITL 2026-05-02). Documentation note added now; sandboxed-open or
pre-flight static validation tracked in `dev/roadmap/0.8.0.md` §
Security hardening (SR-011).

## Resolution criteria

| SR     | Severity | Disposition                              | Backing                                 |
| ------ | -------- | ---------------------------------------- | --------------------------------------- |
| SR-001 | high\*   | acknowledged (design posture)            | doc note in `embedder.md` + SDK READMEs |
| SR-002 | medium   | resolved                                 | REQ-060 + AC-064                        |
| SR-003 | medium   | resolved                                 | REQ-061 + AC-065                        |
| SR-004 | medium   | resolved                                 | REQ-062 + AC-066                        |
| SR-005 | low      | postponed to 0.8.0                       | `dev/roadmap/0.8.0.md`                  |
| SR-006 | low      | resolved                                 | REQ-063 + AC-067                        |
| SR-007 | low      | resolved                                 | REQ-064 + AC-068a/b                     |
| SR-008 | low      | resolved                                 | REQ-065 + AC-069                        |
| SR-009 | low      | acknowledged (operator-only UX)          | CLI help text follow-up                 |
| SR-010 | low      | resolved                                 | REQ-066 + AC-070                        |
| SR-011 | info     | acknowledged + postponed hardening 0.8.0 | doc note + `dev/roadmap/0.8.0.md`       |

\* SR-001 is severity high but accepted as design posture; lock bar
treats it as resolved once HITL acknowledges acceptance.

All findings disposed; this file flips to `status: locked` and Phase 4
freeze gate clears (per `dev/progress/0.6.0-hitl-lock-gate.md`
Non-Blocking Followups item 3).

## Finding format reference

```markdown
## SR-NNN: <short title>

**Severity:** critical | high | medium | low | informational
**Affected doc/component:** <path>
**Description:** <what>
**Proposed resolution:** <how>
**Status:** open | resolved
```

## Appendix A — out-of-scope and post-1.0 surfaces

These are not 0.6.0 findings. They are recorded so the next reviewer can
see which adversaries were considered and rejected, and so the v1.0
review has a starting list rather than starting cold.

- **A.1 Network / IPC adversary.** 0.6.0 has no wire protocol
  (`interfaces/wire.md`). If a future release ships a server mode,
  re-open the threat model.
- **A.2 Multi-tenant isolation inside one engine handle.** Out of scope:
  the `Engine` is a single trust domain. A v1.0 multi-tenant variant
  would need per-tenant key separation, query budget enforcement, and a
  vec0 partition isolation review.
- **A.3 Embedder supply-chain integrity.** 0.6.0 verifies stored
  embedder identity at open
  (`EmbedderIdentityMismatchError`), but not the binary integrity of the
  caller's embedder. A v1.0 review may add caller-supplied embedder
  attestation.
- **A.4 Vector similarity timing side-channel.** Speculative for an
  embedded library; relevant only if FathomDB is later wrapped behind a
  shared service that returns search timing to an untrusted caller.
- **A.5 Fuzz-test infrastructure.** Both write-path and open-path are
  natural fuzz targets (codec round-trips, malformed WAL, malformed
  schema rows). A 0.6.x or 0.7 effort can add a `cargo fuzz` harness;
  not required for 0.6.0 lock.
- **A.6 Hardened malicious-file open path.** Treating attacker-supplied
  SQLite files as a supported input would require either (a) sandboxed
  open and probe before main-process open, or (b) static validation of
  the file format ahead of SQLite. Both are post-1.0 work and depend on
  upstream SQLite posture.
- **A.7 Side-channel via projection scheduler progress.** Operator-
  visible counters expose write throughput and projection lag; a
  co-tenant could in principle infer write activity. Not exploitable
  in the 0.6.0 single-trust-domain model.
