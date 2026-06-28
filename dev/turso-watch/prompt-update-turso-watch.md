# Prompt — refresh the Turso watch

Run this intermittently (suggest quarterly, or whenever a Turso release lands)
to re-assess **FathomDB's SQLite needs vs Turso's readiness** and append a fresh
dated snapshot to `dev/turso-watch/`.

Paste everything below the line as the task.

---

You are refreshing FathomDB's "Turso watch" — a recurring assessment of whether the
Turso database (the from-scratch Rust rewrite of SQLite, formerly **Limbo**, repo
`tursodatabase/turso`) has closed the gaps that block FathomDB from adopting it.

## Guardrails

- **Disambiguate.** Evaluate `tursodatabase/turso` (the Rust rewrite), NOT libSQL /
  Turso Cloud (the older C fork at `docs.turso.tech`). They have different
  capabilities. Confirm which product each source describes before trusting it.
- **Don't re-derive FathomDB's needs from scratch.** Read the latest dated gap
  analysis in `dev/turso-watch/` for the established list of FathomDB's six SQLite
  hard-dependencies. Re-verify each against the codebase ONLY if you suspect it
  changed since that snapshot (e.g. graph traversal moved out of SQL, FTS/vector
  path reworked, rusqlite bumped). Check `git log` on
  `src/rust/crates/fathomdb-engine/` and `fathomdb-schema/` since the last snapshot date.
- **Ground every Turso claim in a current source** — prefer the repo's `COMPAT.md`,
  release notes / CHANGELOG, and the official blog over secondhand articles.

## Steps

1. **Re-read the prior snapshot.** Open the most recent
   `dev/turso-watch/*-gap-analysis.md`. Note its date, the Turso version it
   evaluated, and the three "revisit triggers."

2. **Check FathomDB's side for drift.** `git log --oneline --since=<prior snapshot
   date> -- src/rust/crates/fathomdb-engine/src src/rust/crates/fathomdb-schema/src`.
   If any of the six pillars (sqlite-vec/vec0 ANN, FTS5+bm25, WITH RECURSIVE graph
   BFS, WAL+EXCLUSIVE single-writer, partial UNIQUE indexes, PRAGMA/introspection)
   was touched, re-verify how it's used now with file:line refs.

3. **Refresh Turso's readiness.** Fetch and read:
   - `https://github.com/tursodatabase/turso` (README — status label, latest version)
   - `https://raw.githubusercontent.com/tursodatabase/turso/main/COMPAT.md`
   - Turso releases / CHANGELOG and recent `turso.tech/blog` posts since the prior date.
   For each FathomDB pillar, record the current Turso status
   (supported / partial / unsupported / planned) with a quoted source.

   Pay special attention to the prior snapshot's blockers and revisit triggers:
   - **ANN vector indexing** — exact-search-only, or is approximate/indexed vector
     search now shipped? Is there a `vec0`-equivalent or only scalar vector funcs?
   - **FTS5** — still Tantivy-only (`fts_score()`), or is SQLite-compatible
     FTS5 + `bm25()` available? Loadable C extensions / `sqlite3_load_extension` yet?
   - **`WITH RECURSIVE`** — supported yet?
   - **rusqlite compatibility** — any rusqlite-compat layer / C-API completeness
     (UDFs, loadable extensions) that would shrink the binding rewrite?
   - **Partial UNIQUE indexes (`WHERE …`)** — now documented/tested? (FathomDB G0 keystone.)
   - Production-readiness: still BETA, or GA?

4. **Recompute the verdict.** For each pillar emit 🔴 blocker / 🟡 risk / 🟢 fine,
   exactly as the prior snapshot's gap table. Call out what CHANGED vs the prior
   snapshot (closed gaps, new gaps, version delta). State whether any revisit
   trigger has now fired.

5. **Write a new dated snapshot** `dev/turso-watch/YYYY-MM-DD-sqlite-vs-turso-gap-analysis.md`
   (today's date; do NOT overwrite prior snapshots — they form a timeline). Reuse
   the prior file's structure: Disambiguation, Part 1 (FathomDB needs), Part 2
   (Turso capabilities), Part 3 (gap table), Verdict & revisit triggers, Sources.
   Add a short **"Changes since <prior date>"** section near the top.

6. **Report back** a 5–10 line summary: Turso version + status, which gaps moved,
   whether any revisit trigger fired, and a one-line recommendation (hold / spike /
   re-evaluate). If a trigger fired, recommend the concrete next step (e.g. the
   sqlite-vec-vs-native-vector recall spike).

## Output contract

- One new dated file in `dev/turso-watch/`.
- Every Turso status claim carries a source URL.
- Verdict uses the 🔴/🟡/🟢 scheme and explicitly says whether to hold or act.
