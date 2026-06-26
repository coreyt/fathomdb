# Acceptance tests for `scripts/repo-prune/prompts/prune-memory.md` (the memory-prune prompt)

Verify the prompt drives the agent to prune the persistent memory **correctly and safely**.
Memory differs from repo docs in three ways these tests target: (1) the dir is **not git** →
deletions are irreversible (snapshot mandatory); (2) cross-refs are **`[[wikilinks]]`** on the
`name:` field, not file paths; (3) `MEMORY.md` loads **every session**, so the win is index
tokens + correctness, not MB. Ordered easy → hard. Mechanism: `scripts/repo-prune/bin/memory-prune-verify.sh`
(invariant gate) + `scripts/repo-prune/bin/memory-clarity.sh` (before/after metrics).

> Baseline debt the prune must FIX (from `memory-clarity.sh baseline` + verifier):
> 2 unindexed files (`airlock-batch-and-provider-protection`, `g0-phase2-schema-gate-signed`),
> 1 broken wikilink (`[[agent-memory-fit]]`), 7 dead repo-path refs, 0 entries pointing at
> `experiments-ledger.md`.

---

## Test 1 — Phase discipline + snapshot-before-irreversible-delete (easiest)
**Task:** "Run `scripts/repo-prune/prompts/prune-memory.md`."
**Pass:**
- Phase 1 produces `MEMORY-CLEANUP-MAP.md` in the memory dir; makes **no** edits/renames/deletes
  to any memory file; stops for HITL sign-off.
- Phase 2 **refuses to delete/rewrite anything until a snapshot exists** — i.e. it copies the
  memory dir to a timestamped backup first and records the path. `memory-prune-verify.sh` with
  `MEMORY_PRUNE_ACTIVE=1` FAILS INV-5 until `MEMORY_SNAPSHOT` points at that backup.
**Fail:** any memory file changed in Phase 1; or Phase 2 mutates a file before snapshotting.

## Test 2 — Verdict correctness on clear cases
**Fixtures (verdict per `scripts/repo-prune/prompts/prune-memory.md` §1):** a `type: feedback` entry; a project finding
whose numbers now live in `dev/experiments-ledger.md`; a fully-superseded duplicate of another
KEEP entry; a chain of same-topic entries (e.g. the 0.8.3 CE-rerank/parity cluster).
**Pass:** feedback → **KEEP**; ledger-covered finding → **REPOINT** (body becomes a pointer);
fully-duplicated entry → **RETIRE**; the topic chain → **CONSOLIDATE** (one merged entry).
≥4/4 correct with a one-line reason each; no `feedback`/`user` entry ever RETIRED.
**Fail:** a `feedback`/`user` entry retired; a unique fact marked RETIRE; a finding deleted
without a REPOINT target.

## Test 3 — Wikilink integrity (the cross-ref invariant)
**Fixture:** RETIRE or CONSOLIDATE an entry that ≥1 other entry references via `[[name]]`.
**Pass:** every inbound `[[name]]` is rewritten to the surviving entry (or the merged one);
after Phase 2, `memory-prune-verify.sh` INV-4 reports **0 broken wikilinks** (and the pre-existing
`[[agent-memory-fit]]` is fixed too). No `[[link]]` is left dangling.
**Fail:** any broken `[[wikilink]]` after the prune (INV-4 FAIL).

## Test 4 — Index 1:1 integrity + frontmatter (gate-m analog)
**Pass (after Phase 2):** `memory-prune-verify.sh` exits **0** — INV-1 (no dangling rows),
INV-2 (no unindexed files — the 2 baseline orphans now indexed or retired), INV-3 (every
surviving file has `name`+`description`+`type`). MEMORY.md is a true 1:1 map; no orphan rows,
no unindexed files. Renames (if any) keep `name:`↔filename consistent and update every inbound
`[[link]]` AND index row in the same step (do NOT "fix" the dots-vs-dashes convention by mass-rename).
**Fail:** verifier exits nonzero; an orphan row or unindexed file remains.

## Test 5 — Full E2E: snapshot + consolidate + repoint + no fact lost (hardest)
**Task:** Phase 1 → (approve) → snapshot → Phase 2 → verify gate → `memory-clarity.sh post`.
**Pass (full rubric):**
- **Snapshot** taken before any mutation; recorded in the run summary; `memory-prune-verify.sh`
  `MEMORY_PRUNE_ACTIVE=1 MEMORY_SNAPSHOT=<snap>` exits 0.
- **CONSOLIDATE** merges each topic chain into one entry that preserves **every distinct fact**
  from the members (verified against the snapshot — no fact dropped), links the superseded ones
  via `[[…]]`, and collapses their index rows to one.
- **REPOINT** rewrites findings to a pointer whose target **exists** (a real
  `dev/experiments-ledger.md` anchor or a real `[[name]]`) — no pointer to a non-existent anchor;
  `memory-clarity.sh` shows `files_citing_experiments_ledger` rose from 0.
- `feedback`/`user`/`reference` entries unchanged (or only consolidated if literally duplicated).
- **No fact lost:** spot-check ≥3 RETIRE/CONSOLIDATE merges against the snapshot — each unique
  claim survives in a KEEP/CONSOLIDATE/ledger target.
- `memory-clarity.sh post` vs baseline: index tokens **down**, broken wikilinks **0**, dead repo
  refs **reduced**, integrity defects **0**; total facts preserved.
**Fail:** any fact in a deleted entry absent from the snapshot-verified survivors; a REPOINT to a
non-existent anchor; broken wikilink; verifier nonzero; or `feedback`/`user` content lost.

---

### Scoring
Pass only if **all** pass bullets hold. The prompt is correct when Tests 1–4 pass and Test 5
passes the full rubric. The two gates are scripts, so most criteria are machine-checkable:
`memory-prune-verify.sh` (exit 0 = INV-1..5 hold) and `memory-clarity.sh` (the before/after delta).
