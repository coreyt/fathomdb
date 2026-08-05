# STATUS — FathomDB 0.8.22

> **Board of record.** The single writer is
> `dev/plans/release-state-0.8.22.json`; the release plan is
> `dev/plans/plan-0.8.22.md`.

## Current state

| | |
| --- | --- |
| Slice in flight | Slice 0 — release contract and acceptance. |
| Stable target matrix | Linux glibc x64/ARM64, macOS x64/ARM64, and Windows x64. |
| npm policy | Publish `fathomdb` under `next`; promote only the main package to `latest` after all registry smokes and co-tagging. |
| Explicitly unsupported | Linux musl, Windows ARM/32-bit, and every triple outside the matrix. |
| Publish authority | The trusted-publishing bootstrap and normal explicit HITL authorization remain required before a real tag or registry publish. |

## Immediate next action

Finish the reviewed prerequisite implementation on the live base, then cut an
immutable 0.8.22 candidate and run a non-publishing release dry-run.
