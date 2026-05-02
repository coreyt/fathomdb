---
title: 0.6.0 User Needs
date: 2026-05-02
target_release: 0.6.0
desc: Customer, functional, and non-functional user needs for the 0.6.0 rewrite
blast_radius: dev/requirements.md; dev/acceptance.md; dev/architecture.md; dev/design/*; dev/interfaces/*; dev/test-plan.md; docs/positions/*
status: locked
---

# User Needs

This file captures the user-needs framing for the 0.6.0 rewrite: who FathomDB
is for, what outcomes they need, and what constraints matter. It is intentionally
not a requirements list and does not own numerical targets.

Conventions:

- Each need is a stable id `NEED-###` and a single falsifiable statement.
- Needs are outcome- and constraint-oriented, not implementation prescriptions.
- This document stands on its own. It is not derived from `requirements.md`.

## Primary Persona (HITL 2026-05-02)

**Application engineer (not a DB expert)** building agent/application workloads
that store hard-to-recreate canonical data, and need FTS + similarity search
over that data. They need strong reliability, transparent backup/restore, and
worst-case recoverability without becoming database operators.

Secondary personas (covered insofar as they overlap the primary):

- Operator/platform engineer deploying the embedded engine in constrained
  environments.
- SDK consumer integrating from Python/TypeScript (and a Rust facade).

## Customer Needs

NEED-001: **Trustworthy storage for hard-to-recreate canonical data.**
Users need confidence that canonical data is stored durably and that the system
fails loudly and diagnostically rather than silently corrupting or serving
incorrect results.

Primary canonical data types (HITL 2026-05-02; non-exhaustive examples):

- web pages / crawls
- meeting transcripts
- media metadata
- e-mails and messages
- "world models" (structured representations derived from canonical sources)

NEED-002: **Local-first privacy and offline operation.**
Users need the core system to work without a network connection and without
implicitly transmitting user data.

NEED-003: **Low-ceremony embedded deployment.**
Users need a deployable artifact that does not require running a server or
operating a separate database service. They want "a file path + a library" as
the default experience.

NEED-004: **Operator-grade backup/export and worst-case recovery.**
Users need a clear backup/export story and a recovery story that can be executed
from operator tooling rather than bespoke application code.

NEED-004a: **Backup workflows that do not impose long service interruptions.**
Users need backup workflows that preserve read availability and minimize write
interruption for practical database sizes.

Voice-of-customer (HITL 2026-05-02; expectation, not a locked requirement):

- For a ~100 MB database, "no reads" longer than ~2 minutes is not acceptable.
- For a ~100 MB database, "no writes" longer than ~5 minutes is not acceptable.

NEED-005: **Predictable licensing compatibility.**
Users need permissive-license compatibility suitable for embedding into a broad
range of applications (including commercial distribution), and do not want
surprise licensing constraints introduced through core dependencies.

HITL 2026-05-02 license posture (constraints, not requirements):

- Acceptable (permissive): MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC,
  Unlicense/CC0, PSF.
- Use with caution (weak copyleft): LGPL, MPL.
- Not acceptable (restrictive/copyleft): GPLv2/GPLv3, AGPL, SSPL, CC BY-NC,
  CC BY-ND.

NEED-006: **Release quality bar suitable for a database.**
Users need conservative releases with strict CI and a strong bias toward "it
keeps working" rather than relying on frequent micro-releases to patch basic
reliability issues.

## Functional Needs

NEED-010: **Small, learnable SDK surface.**
Users need a small, learnable application runtime API surface that is easy to
learn and hard to misuse, with consistent behavior across supported languages.

NEED-011: **Clear boundary between runtime SDK and operator tooling.**
Users need a hard boundary between the application runtime API and operator
recovery/inspection workflows, so application code cannot accidentally invoke
lossy repair operations.

NEED-011a: **A dedicated operator surface for inspection/export/recovery.**
Users need a dedicated operator surface that supports inspection, export, and
recovery workflows without requiring bespoke application code changes.

NEED-012: **Hybrid retrieval over canonical data.**
Users need to write canonical entities/relationships and retrieve them via text
search, similarity search, and hybrid paths without maintaining separate storage
systems.

NEED-013: **Graph-oriented model compatibility.**
Users need a model that supports graph-oriented application/agent workflows (at
minimum node/edge style relationships and queryable traversals) without forcing
a separate graph database.

NEED-014: **Explicit "catch-up" barrier for derived work.**
Users need a way to determine when derived indexing/projection work has caught
up to canonical writes for workflows like tests and batch ingest.

NEED-015: **Low-burden upgrades with explicit reporting.**
Users need upgrades to be low-operator-burden and to provide an explicit,
machine-readable report of what changed (or why the upgrade failed).

NEED-016: **Cross-platform support for common developer environments.**
Users need the project to run on the major desktop/server platforms they
develop and ship on, without OS-specific footguns.

Deployment envelope (HITL 2026-05-02):

- desktop/server: in-scope
- mobile: nice-to-have, not a requirement for 0.6.0

NEED-017: **No implicit model download or network dependency.**
Users need embedder/model lifecycle to remain caller-owned; the system should
not surprise them by downloading model weights or requiring external services.

## Non-Functional Needs

NEED-020: **Reliability: bounded shutdown and no deadlocks.**
Users need engine lifecycle behavior that does not hang processes on exit and
does not deadlock under expected concurrency patterns.

NEED-020a: **Multi-process concurrency without unacceptable query latency.**
Users need to support concurrent application querying while a single writer is
active, specifically to keep query response latency low.

Deployment constraint (HITL 2026-05-02):

- This must work on Linux (the three most-used local filesystems), macOS, and
  Windows (NTFS).

NEED-021: **Safety: corruption detection with actionable diagnostics.**
Users need corruption and incompatibility to be detected early with structured
errors that make next steps obvious (including when the correct next step is an
operator recovery/inspection workflow).

NEED-022: **Performance that supports interactive agent/application use.**
Users need search and write performance that is "fast enough" for interactive
use without external services, with explicit perf gates defined elsewhere
(requirements/ADRs/acceptance).

NEED-023: **Usability for non-DB experts.**
Users need the system to be operable with minimal database expertise: clear
errors, stable diagnostics, and documented workflows for the scary cases.

NEED-024: **Operational transparency.**
Users need enough observability (lifecycle events, counters, profiling) to
understand "what is slow" and "what is broken" without guessing.

NEED-025: **Minimal migrations and bounded upgrade complexity.**
Users need upgrade posture that avoids long chains of accumulated migrations and
avoid compatibility shims that hide breaking changes.

## Out of Scope / Non-Needs (0.6.0)

These are intentionally not user needs for 0.6.0 and should be treated as
explicit non-goals unless a successor decision revises scope:

- Running as a networked server or hosting a wire protocol endpoint.
- Automatic cloud sync or multi-device replication.
- Implicit embedder/model downloads, hosting, or lifecycle management by the
  engine.
- Runtime application API recovery/repair operations (repair is an operator
  concern, not an application concern).

## Notes (HITL)

The needs above are intended to be stable "what" statements. Lower-level
user-experience and API behavior choices (for example, what happens when a
writer is busy) are intentionally out of scope for this document.

## Appendix: External Context (Non-Binding)

These are not requirements. They are external reference points used only to
validate that the needs match real-world expectations for embedded, local-first
systems:

- SQLite WAL is designed to support concurrency where readers do not block a
  writer and a writer does not block readers (with a single writer at a time).
  See <https://www.sqlite.org/wal.html>
- SQLite WAL locking behavior is explicitly exclusive for writers; embedded
  deployments must define a clear lock and concurrency contract.
  See SQLite WAL mode format notes: <https://sqlite.org/walformat.html>
- SQLite provides an "online backup" API intended to create backups of live
  databases incrementally, reducing the need to hold a read-lock for the entire
  copy duration.
  See <https://www.sqlite.org/backup.html>
- SQLite vector extensions exist specifically to enable local-first ANN search
  without external services; this reinforces the "offline + embedded" posture.
  See sqlite-vec overview: <https://builders.mozilla.org/project/sqlite-vec/>
