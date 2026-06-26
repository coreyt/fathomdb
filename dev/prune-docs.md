<!--
  LEDGER-PRUNE PROMPT — separate from update-docs.md.
  update-docs.md  = keep docs in sync with CODE (epoch-diff).
  prune-docs.md   = THIS — separate CURRENT-STATE from HISTORICAL LEDGER,
                    preserve experiment fidelity, archive/delete the rest.
  Last prune run: 2026-06-26 (baseline 25541d88). Phase 1 → dev/CLEANUP-MAP.md.
    Phase 2: built dev/experiments-ledger.md (0.6.x→0.8.4 + research/); DELETEd 507 transient
    run artifacts (recover ≤ 25541d88); ARCHIVE-relocated low-entanglement trees
    (interface-inventory study, context-research, psd, reports) → dev/archive/ + manifest;
    remaining ARCHIVE (plans/prompts, design slice-memos, runs STATUS boards) signaled
    IN PLACE via staleness indexes (plans/README, design/README) to preserve ~120 by-path
    refs and avoid editing accepted ADRs. Deferred: dev/research/ (R-2, untracked, HOLD —
    distilled not deleted) + 0.7.0 AC-020 closure gap — see experiments-ledger Deferred section.
-->

# Prompt: Prune the FathomDB documentation ledger (current-state vs history)

You are pruning the **FathomDB engineering documentation** so that an agent
cold-starting on `dev/` sees **current state**, not a five-month ledger. The repo
has accreted ~725 `dev/*.md` plus ~660 files under `dev/plans/runs/` across releases
0.6.0 → 0.8.5-WIP (2026-03 → 2026-06): live specs sit beside closed-release slice
prompts, superseded designs, superseded decisions, raw per-run experiment outputs,
and codex review logs. This **clogs context**. Your job is to map what is current,
**preserve the experiment results with fidelity**, and archive or delete the rest.

This is a **curation/archival** task, complementary to `dev/update-docs.md` (which
syncs docs to *code*). Do not duplicate that work; assume code↔doc drift is handled
elsewhere. Read this file fully before acting.

---

## 0. Hard guardrails (read first — violating any one fails the run)

- **No source code, no tests.** This is a docs task. `git status --porcelain src/`
  must stay empty (aside from pre-existing changes). Test files are read-only
  (`AGENTS.md` §5). If you spot a code/doc contradiction, log it in `dev/learnings.md`
  and the live STATUS board — do not fix code.
- **Never lose an experiment result.** Raw run artifacts may be *deleted* only
  **after** their findings (hypothesis · design/prereg · N & power · numbers + CI ·
  verdict/decision · what superseded it · $ cost · pointer to git SHA) are distilled
  into the durable experiment ledger (§4). When in doubt, archive rather than delete.
- **Locked IDs.** `dev/acceptance.md` is `status:locked`; `dev/requirements.md` IDs are
  a contract. Never mint, renumber, or withdraw REQ-*/AC-*/SR-* IDs. Archiving a doc
  never deletes an ID's definition — if a doc is the *home* of a live ID, it is CURRENT.
- **ADRs are never touched by this prune.** Any file under `dev/adr/` is **never moved,
  archived, deleted, edited, or bannered** here. A superseded ADR stays in place — its
  `status:` field and the decision index already record the supersession; successors are
  *proposed*, never substituted. Classify every ADR **CURRENT** (a live decision) or
  **REFERENCE** (superseded-but-recorded); ADRs are categorically ineligible for
  ARCHIVE/DELETE. (Non-ADR design memos that merely *discuss* a decision are normal docs.)
- **Preserve history.** Relocate with `git mv` (never copy-delete). Deletions use
  `git rm` so the artifact remains recoverable from history.
- **DOC-INDEX is the keystone — every commit is self-consistent.** `dev/DOC-INDEX.md`
  must remain a true map of the live surface — Slice-40 **gate-m** fails the release
  otherwise. **No commit may leave a stale/dangling DOC-INDEX, README, or manifest row.**
  Each archive/delete batch carries its matching DOC-INDEX/README/manifest/link repairs in
  the **same commit** (see §4). Leave the index accurate at *every* commit or you have failed.
- **Stale > missing** (`AGENTS.md` §1): a wrong/orphaned doc is worse than its
  absence. Prefer archiving with a banner over leaving a stale duplicate in a live path.

---

## 1. Inputs

| Input | How to obtain | Default |
|-------|---------------|---------|
| `$CURRENT_RELEASE` | The live/in-flight release line. Read the top of `dev/plans/` (`plan-0.8.x.md`) + the newest `runs/STATUS-0.8.x.md`. | `0.8.x` (currently 0.8.4 closed / 0.8.5 in flight) |
| `$HEAD` | Tree you classify against. | `HEAD` |
| `$SCOPE` | Optional directory/subsystem filter (e.g. "only dev/plans/runs/", "only the 0.7.x ledger"). | whole `dev/` tree + root docs |

`docs/` (user-facing mkdocs) is **out of archival scope** — it is already lean and is
contract. Touch it only to fix a link that this prune breaks.

---

## 2. The classification taxonomy (assign exactly one verdict per doc)

Walk **vertically** (directory by directory) and **horizontally** (release by release,
oldest → newest). For every `.md`/`.json`/`.log`/`.txt` under scope assign one verdict:

| Verdict | Definition | FathomDB examples |
|---------|------------|-------------------|
| **CURRENT** | Describes the live shipped/in-flight state; an agent needs it to understand the system *today*. Stays in place. | `dev/architecture.md`, `dev/interfaces/*`, `dev/requirements.md`, `dev/acceptance.md`, accepted ADRs, cross-cutting design specs (`retrieval.md`, `vector.md`, `op-store.md`, `migrations.md`, `engine.md`…), the **$CURRENT_RELEASE** plan/STATUS/slice docs, the live experiment decision-tree (`dev/design/0.8.x-portfolio-features-and-experiment-tree.md`, `0.8.x-parity-portfolio-strategy.md`), `DOC-INDEX.md`, `update-docs.md`, this file. |
| **REFERENCE** | Historical but **load-bearing**: still cited, or the durable record of a decision/experiment an agent may need to re-read. Preserved with fidelity — distilled into a ledger and/or kept with a banner. | Experiment findings (`runs/0.8.2-m1-FINDINGS.md`, `0.7.0-perf-experiments-results.md`, `runs/0.7.2-EU-8-ir-recall-results.md`, `runs/0.8.x-capability-status-report.md`), frozen pre-registrations, HITL decision docs, superseded-but-cited ADRs, root-cause learnings. |
| **ARCHIVE** | Real authored doc, now superseded/closed, low future-read value. `git mv` → `dev/archive/<release>/` + SUPERSEDED banner + manifest row. | Closed-release slice design memos (`dev/design/slice-5-design.md`, `0.7.0-vector-quant-pack1.md`…), closed-release plans/implementations (`0.6.0-implementation.md`, `0.7.0-perf-experiments.md`), closed-release STATUS boards (`STATUS-0.7.x`, `STATUS-0.8.0..0.8.3`), per-slice prompts for closed slices in `dev/plans/prompts/`. |
| **DELETE** | Pure transient machine artifact with no standalone narrative value, **and** its result is already (or will be) captured in the experiment ledger. `git rm` (history retains). | Per-run `*-output.json`, codex `*-review-<timestamp>.log` / `.md`, `*-transcript.txt`, rerun spam (`*-r1..-rN-output.json`), `*.log` capture files under `runs/`. |

**Tie-breakers.** Cited by a CURRENT doc → at least REFERENCE. Home of a live REQ/AC/SR
or an accepted/cited ADR → CURRENT. A `*-output.json` whose numbers are the *only* copy
of an experiment result → REFERENCE (distill first), not DELETE. Unsure between ARCHIVE
and DELETE → ARCHIVE.

**Machine-read data files are never DELETE (R-3), regardless of extension.** Any file
consumed by a tool, test, or binary at runtime is CURRENT/REFERENCE, never a transient.
Known instances: `dev/perf-history/*.json` (append-only baselines read by the
perf-regression-check binary), `dev/release/fixtures/**` and `dev/release/tests/**` (release-
skew test infra — read-only per §0, out of doc-prune scope). Before classifying any `.json`
DELETE, confirm nothing reads it programmatically.

---

## 3. PHASE 1 — produce the classification map (NO file moves)

Do not move, delete, or edit any doc in this phase. Produce **`dev/CLEANUP-MAP.md`**:

1. **Per-directory tables**, oldest-release-first within each directory:
   `path · release · last-touched (git) · verdict · one-line reason · (if REFERENCE/DELETE) where its fidelity is preserved`.
   Cover every file under `$SCOPE`. Do not sample — a silent omission reads as "keep".
2. **Fidelity-extraction plan**: the list of experiment/decision results to be distilled
   into the experiment ledger (§4), grouped by release, each with the source file(s) and
   the headline result to capture. This is the contract for Phase 2.
3. **Counts & footprint**: files and bytes per verdict, per directory — so the reviewer
   sees the de-clog impact before approving.
4. **Risk list**: anything you were unsure about, every ADR you propose to archive (with
   its successor), and any doc that is the sole home of a live ID.

Then **STOP and present `dev/CLEANUP-MAP.md` for HITL sign-off.** Do not proceed to
Phase 2 until the user approves (they may edit verdicts first). This mirrors the repo's
HITL-gated culture.

---

## 4. PHASE 2 — execute (only after approval)

Steps 1–6 are a **work order** (later steps depend on earlier ones), **not** a commit
boundary. **Commit discipline (overrides any other reading):** organize commits so that
**every commit leaves the tree green and DOC-INDEX/READMEs/manifest accurate** — i.e. an
archive/delete batch and its matching index/link/manifest repairs land in the **same**
`docs(prune): …` commit. Do **not** commit a batch of moves/deletes and defer the index
fixes to a later commit (that breaks gate-m in between). The ledger build (step 1) is
fully committed before any deletion commit.

1. **Build the durable experiment ledger FIRST** (before any deletion). Create/extend
   **`dev/experiments-ledger.md`** — a per-release index of what was learned. One entry
   per experiment/decision with: hypothesis · design/prereg pointer · N & power · headline
   numbers + CI · **verdict/decision** · what superseded or closed it · $ cost (if priced) ·
   **original path(s) + the exact commit/blob SHA** of every raw source. This is the single
   place an agent reads to recover "what did 0.7.0 perf / 0.8.2 M1 / 0.8.3 CE-rerank /
   0.8.4 GraphRAG actually find." Verify each number against the source before deleting it.
   **Exhaustive accounting, not sampling:** drive the work from `dev/CLEANUP-MAP.md` as a
   per-file checklist — **every** file marked DELETE/REFERENCE with result-bearing content
   must have a ledger row carrying its original path + recoverable SHA + the mandatory
   fields above *before* it can be deleted. A DELETE with no corresponding complete ledger
   row is a blocking error, not a judgment call.
2. **Set up the archive.** Ensure `dev/archive/README.md` exists as a **manifest**
   (table: archived path · original path · release · date archived · superseded-by ·
   one-line what-it-was). Subfolder per release: `dev/archive/<release>/`.
3. **ARCHIVE — two modes.** Prepend this banner to every ARCHIVE doc either way:
   ```
   > **SUPERSEDED / ARCHIVED <date>.** Historical — describes <release>. Current state: <link>.
   > Preserved for history; not maintained — the project has moved on; details may be STALE.
   > See dev/experiments-ledger.md for distilled results.
   ```
   - **(default) Relocate:** `git mv` the doc into `dev/archive/<release>/`, add its row to
     the `dev/archive/README.md` manifest, and fix inbound links from CURRENT docs to the
     new path or the ledger (same commit).
   - **(R-1) Archive-in-place** for directories whose files are cross-referenced *by path*
     from elsewhere (ADRs / design / run logs) — **notably `dev/plans/prompts/` and
     `dev/plans/*.md`** (~120 by-path references). Do **NOT** `git mv` these and do **NOT**
     create a second archive manifest for them. Instead: add the banner in place, and record
     their archived/stale status in the directory's **existing index, `dev/plans/README.md`**
     (the single source of truth for that tree — do not duplicate into `dev/archive/README.md`
     or duplicate DOC-INDEX rows). Update `dev/plans/README.md` to (i) state plainly that many
     of its docs are **archived in place**, (ii) warn that archived docs may be **stale** (the
     project has moved on), and (iii) serve as the **staleness index** — marking which
     plans/prompts are archived/stale vs live. This keeps the ~120 by-path links intact.
4. **DELETE.** Only after step 1's checklist confirms a **complete** ledger row exists
   (original path + recoverable SHA + mandatory fields) — or the file demonstrably carried
   no result: `git rm` the transient artifacts. Record the count per release **and the
   per-file checklist disposition** in the ledger/commit message so the deletion is fully
   auditable (not just spot-checkable).
   - **(R-2) Untracked / git-ignored trees are HOLD-on-delete this pass.** `dev/research/`
     is not in git, so `git rm` gives **no recovery** — a delete there is irreversible. For
     this pass: **distil its results into the ledger and do NOT delete anything** under
     `dev/research/` (leave all files in place). The open question — whether to (a) delete the
     regenerable artifacts (e.g. `*.npy` vectors), (b) move the tree into an `archive/`
     subdir, or (c) leave it — is **deferred**, not decided. Record it as a deferred item per
     step 6. Apply the same HOLD to any other untracked tree you encounter.
5. **Refresh the indexes** (keystone step):
   - `dev/DOC-INDEX.md` — remove rows for deleted files; re-point rows for archived files
     to their new path (or fold a release's archived set into one archive row); add the
     `experiments-ledger.md` row. Leave it a true map. (gate-m)
   - `dev/README.md`, `dev/design/README.md`, `dev/plans/README.md`, `dev/adr/…decision-index.md`,
     `dev/archive/README.md`, and any subtree `README.md` — list only files that still exist
     at their stated path.
   - `dev/traceability.md` — if an archived doc held a trace pointer, re-point it; flag
     orphans honestly (`ORPHAN`/`PARTIAL`). Never invent IDs.
6. **Stamp + deferred-items note.** Update the "Last prune run" line at the top of this
   file (SHA + date + one-line summary). Summarize: counts archived/deleted/distilled per
   release, every link repaired, anything flagged. **Append a `## Deferred / revisit` section
   at the END of `dev/experiments-ledger.md`** listing decisions intentionally NOT made this
   pass — at minimum the **R-2 `dev/research/` disposition** (distilled but not deleted;
   delete-vs-archive-subdir still to be decided; confirm delete is the correct approach before
   any future removal). Each deferred item: what was held, why, and what must be checked
   before resolving it.

---

## 5. Verify (gate before declaring done)

- **No data loss (exhaustive):** **every** DELETED result-bearing file has a complete
  ledger row (original path + recoverable commit/blob SHA + the mandatory fields), and the
  numbers/CI/verdict are accurate against source. Reconcile the ledger rows against the
  DELETE set in `dev/CLEANUP-MAP.md` 1:1 — zero unaccounted deletions. (Spot-checking
  supplements this reconciliation; it does not replace it.)
- **No code/tests touched:** `git status --porcelain src/` clean; no test file modified.
- **Indexes true:** `dev/DOC-INDEX.md`, every `README.md`, and the archive manifest
  reference only existing paths; no row points at a moved/deleted file.
- **Links resolve:** no CURRENT doc links into a path you archived/deleted without a
  redirect. (`./scripts/agent-lint.sh` / lychee if available.)
- **IDs intact:** every live REQ/AC/SR still has a home; no accepted ADR edited or deleted.
- **Reversible:** every removal is a `git mv`/`git rm` (recoverable), never an
  out-of-git delete.

---

## 6. Execution model (large scope)

The map (Phase 1) is best built by fanning out **one read-only subagent per directory**
(`dev/plans/runs/`, `dev/plans/prompts/`, `dev/design/`, `dev/adr/`, `dev/notes/`,
`dev/deps/`, …), each returning its directory's classification table + fidelity-extraction
rows against the §2 taxonomy; the main thread merges them into `dev/CLEANUP-MAP.md` and
resolves cross-directory citations (a file cited from another directory is ≥ REFERENCE).

**Phase 2 is single-writer for every shared mutable doc.** One writer (the main thread)
owns *all* of: `dev/experiments-ledger.md`, `dev/DOC-INDEX.md`, `dev/archive/README.md`
(manifest), every `README.md`, `dev/traceability.md`, and all inbound-link repairs in
CURRENT docs — these race if touched in parallel. Subagents may run only **read-only**
work: planning, or producing a **per-directory move-manifest** (the list of `git mv`/`git
rm` + banner edits for files *inside that one directory*) that the single writer then
applies and commits. Do not let two agents edit shared docs or repair the same
link/index concurrently. Keep the ledger-build (step 1) strictly before any deletion (step 4).
