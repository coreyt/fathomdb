# backups/ — local-only prune snapshots (NOT in git)

Snapshots of the agent memory dir (and any other not-git-tracked target) taken **before** a
destructive prune. The memory dir (`~/.claude/.../memory`) is not under version control, so a
snapshot is the **only** recovery path for a memory prune.

Everything here except this file and `.gitignore` is **git-ignored** — snapshots are local-only
and must never be committed. Keep the relevant snapshot on disk until you're confident in the
pruned state, then delete it.

Convention: `memory-snapshot-<YYYYMMDD>/` is a full `cp -a` of the memory dir. The memory prune
gate enforces one exists (`memory-prune-verify.sh` INV-5 with `MEMORY_PRUNE_ACTIVE=1`
`MEMORY_SNAPSHOT=scripts/repo-prune/backups/memory-snapshot-<ts>`).

Current local snapshot (not tracked): `memory-snapshot-20260626/` — pre-prune memory state for
the 2026-06-26 memory prune.
