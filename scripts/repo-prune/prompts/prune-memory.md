<!--
  ╔══════════════════════════════════════════════════════════════════════════╗
  ║  DRAFT — REVIEWED, TOOLING READY, NOT YET EXECUTED (HITL hold 2026-06-26). ║
  ║  Do not run any phase until the user gives the go. Prerequisites now MET:   ║
  ║   - dev/experiments-ledger.md EXISTS (REPOINT targets are live).            ║
  ║   - scripts/repo-prune/bin/memory-clarity.sh    baseline taken (before/after metrics). ║
  ║   - scripts/repo-prune/bin/memory-prune-verify.sh  the invariant gate (run before/after).║
  ║   - scripts/repo-prune/tests/prune-memory-acceptance-tests.md  the tests this prompt must pass.  ║
  ╚══════════════════════════════════════════════════════════════════════════╝
-->

# [DRAFT] Prompt: Prune the FathomDB agent-memory ledger

> **DRAFT — reviewed, tooling-backed, HITL-held.** Sibling to `scripts/repo-prune/prompts/prune-docs.md`, but memory is
> **user data with cross-session effect**, not repo docs — it gets its own rules.
>
> **Learnings carried over from the doc-prune (`scripts/repo-prune/prompts/prune-docs.md` run 2026-06-26):**
> (1) **Snapshot before any irreversible delete** — the memory dir is **not a git repo**, so unlike
> the doc-prune there is *no git history to recover from*; a backup is the only safety net.
> (2) **Cross-ref integrity** — the doc-prune nearly broke an accepted-ADR by-path reference;
> the memory analog is `[[wikilink]]` on the `name:` field — rewrite inbound links before
> retiring/merging, and never trust one regex (verify with the gate script).
> (3) **Distill/repoint before delete**, verifying the target exists (the doc-prune built the
> ledger first; here REPOINT targets must be real anchors).
> (4) **Verify the diff before finalizing** — the doc-prune's `git add -A` silently un-did staged
> deletions; here, run `memory-prune-verify.sh` and re-read the actual changes before declaring done.
>
> **Pre-existing debt this prune must fix** (from `memory-clarity.sh baseline`): 2 unindexed
> files (`airlock-batch-and-provider-protection`, `g0-phase2-schema-gate-signed`), 1 broken
> wikilink (`[[agent-memory-fit]]`), 7 dead repo-path refs, 0 entries pointing at the ledger.

You are pruning the persistent agent memory at
`~/.claude/projects/-home-coreyt-projects-fathomdb/memory/` (the `MEMORY.md` index + its
~46 entry files). Unlike the repo docs, **`MEMORY.md` loads into context every session**, so
its staleness is a *per-turn* tax and an *acting-on-outdated-facts* risk. The same ledger
problem applies: superseded chains (e.g. the multiple overlapping 0.8.3 parity entries) and
findings now better-homed in `dev/experiments-ledger.md`. Goal: a lean index of CURRENT,
verifiable facts, with findings pointing at the durable repo ledger instead of duplicating it.

---

## 0. Hard guardrails (DRAFT — review before trusting)

- **Memory deletions ARE irreversible — the dir is NOT a git repo** (verified 2026-06-26).
  **Before any delete/rewrite, snapshot the whole memory dir** to a timestamped backup
  (`cp -a "$MEM" /tmp/.../memory-snapshot-<ts>`), export `MEMORY_SNAPSHOT=<that dir>` and
  `MEMORY_PRUNE_ACTIVE=1`, and confirm the gate passes INV-5
  (`scripts/repo-prune/bin/memory-prune-verify.sh`) — it FAILS until the snapshot exists and is complete.
  Record the backup path in the summary. No exceptions.
- **The gate is `scripts/repo-prune/bin/memory-prune-verify.sh`** (exit 0 = INV-1 index-rows↔files 1:1,
  INV-2 no unindexed files, INV-3 frontmatter present, INV-4 all `[[wikilinks]]` resolve,
  INV-5 snapshot). Run it BEFORE (audit the pre-existing debt above) and AFTER (must exit 0).
  Measure value with `scripts/repo-prune/bin/memory-clarity.sh post` and diff vs `baseline`.
- **Never touch repo code/docs.** This prompt edits *only* files under the memory dir.
- **Preserve `feedback` and `user` entries by default.** These encode how to work and who the
  user is; they are not "ledger". Only consolidate them if literally duplicated, never drop a
  distinct one.
- **Findings move to pointers, not deletion of knowledge.** A `project`/experiment memory whose
  result now lives in `dev/experiments-ledger.md` is **rewritten to a one-line pointer**
  (`see [[…]] / dev/experiments-ledger.md#<anchor>`), not silently dropped — unless it is fully
  duplicated by another memory.
- **Verify before asserting CURRENT.** A memory may name a file/flag/default that no longer
  exists (the memory system already warns of this). Re-check against the live tree before
  marking an entry CURRENT; if it's stale-but-historically-true, mark it superseded, don't
  "fix" it into a false present-tense claim.
- **Index integrity.** `MEMORY.md` must end as a true one-line-per-file index of exactly the
  files that exist. No orphan rows, no rows for deleted files.

---

## 1. The taxonomy (per memory entry)

| Verdict | Definition | Action |
|---------|------------|--------|
| **KEEP** | A current, verifiable fact; or any `feedback`/`user` entry. | Leave as-is (refresh wording only if stale). |
| **CONSOLIDATE** | One of several entries covering the same evolving topic (e.g. the 0.8.3 CE-rerank / parity chain). | Merge the chain into a single current entry that states the resolved outcome + links the superseded ones via `[[…]]`; collapse the index rows to one. |
| **REPOINT** | A `project`/experiment finding now durably captured in `dev/experiments-ledger.md`. | Rewrite the body to a short pointer to the ledger anchor; keep a one-line index row. |
| **RETIRE** | Fully superseded and fully duplicated by a KEEP/CONSOLIDATE entry; no unique fact left. | Delete the file + its index row (after the dir snapshot). |

Tie-breakers: unsure KEEP vs CONSOLIDATE → CONSOLIDATE. Unsure REPOINT vs RETIRE → REPOINT
(never lose a unique fact). Any `feedback`/`user` → KEEP.

---

## 2. PHASE 1 — map (no edits)

Produce `MEMORY-CLEANUP-MAP.md` (in the memory dir): one row per entry —
`file · type · topic · verdict · reason · (CONSOLIDATE) merge-group · (REPOINT) ledger anchor`.
Group the obvious superseded chains explicitly (e.g. all 0.8.3 parity/rerank entries →
one merge-group). List counts per verdict. **Stop for HITL sign-off.**

## 3. PHASE 2 — execute (after approval)

1. Snapshot the memory dir to a backup (guardrail §0).
2. Apply CONSOLIDATE merges (write the merged entry; update `[[…]]` links).
3. Apply REPOINT rewrites (body → ledger pointer).
4. RETIRE deletions (file + index row).
5. Rebuild `MEMORY.md` so every row maps 1:1 to an existing file; fix the 2 baseline unindexed
   orphans + the broken `[[agent-memory-fit]]` link; clean the 7 dead repo-path refs (re-point
   to `dev/experiments-ledger.md`/archive or drop the dead path).
6. **Gate + measure:** run `scripts/repo-prune/bin/memory-prune-verify.sh` — must **exit 0**. Then
   `scripts/repo-prune/bin/memory-clarity.sh post` and diff vs `baseline` (expect: index tokens down,
   broken wikilinks 0, dead refs reduced, `files_citing_experiments_ledger` up). Re-read the
   actual changed files (do not trust a single command's summary). Summarize: per-verdict counts,
   the snapshot path, every retired entry, and the clarity delta.

> **Naming convention (do NOT "fix"):** memory filenames may use dots (`0.8.3-…`) where the
> `name:` field uses dashes (`0-8-3-…`); these are intentionally distinct identifiers (index links
> by filename, `[[wikilinks]]` by `name:`). A mass-rename to "align" them would break both the
> index rows and every inbound `[[link]]`. The gate treats name≠filename as advisory, not a defect.

---

## 4. Open questions to resolve before promoting this draft

- Is the memory dir under any git/versioning, or is the scratchpad snapshot the only safety net?
- Should `reference` entries (external URLs/dashboards) be KEEP-always like `feedback`/`user`?
- Exact anchor scheme in `dev/experiments-ledger.md` for REPOINT targets.
- Whether to add codex review of this prompt (as was done for `scripts/repo-prune/prompts/prune-docs.md`) before first use.
