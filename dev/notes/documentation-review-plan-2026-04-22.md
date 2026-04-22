# Documentation Review Plan - 2026-04-22

## Goal

Bring repository documentation back to a clear support contract:

- `docs/` is correct, buildable, and suitable for users/operators of the
  current release line.
- `dev/` contains only up-to-date developer information: architecture,
  relevant designs, support contracts, and current process docs.
- `dev/notes/` contains current planning, design changes, release notes, and
  active investigation material.
- Out-of-date or superseded developer documents are moved under
  `dev/archive/`, preserving history without letting stale plans look
  authoritative.

The current workspace version is `0.5.3` in `Cargo.toml`,
`python/pyproject.toml`, and `typescript/packages/fathomdb/package.json`.

## Current Baseline

Validation already run:

- `python3 scripts/check-doc-hygiene.py` passes.
- `bash docs/build.sh` passes.

## Execution Summary

Executed in this pass:

- Added all previously orphaned `docs/` pages to MkDocs nav.
- Updated vector/profile docs for the current 0.5.3 per-kind vector table
  contract.
- Updated `dev/doc-governance.md` so active governance no longer points at
  archived files.
- Moved clearly historical release scopes, completed implementation plans,
  dated readiness/research snapshots, stale vector-table designs, and old
  partner analysis into `dev/archive/`.
- Reduced active top-level developer docs from 91 to 68 Markdown files.
- Reduced active `dev/notes/` docs from 54 to 19 Markdown files.

Final validation:

- `python3 scripts/check-doc-hygiene.py` passes.
- `bash docs/build.sh` passes without orphan-page warnings.

Known baseline issues found during planning and resolved in this pass:

- `docs/build.sh` reported pages that existed but were not in MkDocs nav:
  - `docs/supported-tokenizer-configs.md`
  - `docs/supported-vector-configs.md`
  - `docs/tokenization-and-embedding-choices.md`
  - `docs/operations/projection-profiles.md`
  These pages are now included in `docs/mkdocs.yml`.
- `dev/doc-governance.md` named archived files as active governance docs. It now
  lists current normative docs and records that there are no active tracker
  docs.
- `dev/` started with 91 top-level Markdown files and `dev/notes/` started with
  54 Markdown files. After the archive pass, `dev/` has 68 top-level Markdown
  files and `dev/notes/` has 19 Markdown files.
- The working tree has an unrelated untracked `sessions.txt`; leave it alone
  during this review.

## Classification Rules

Use these rules for every Markdown file reviewed:

- Keep in `docs/` only if it describes supported behavior, user workflows,
  operator workflows, or public API reference for the current release line.
- Keep in top-level `dev/` only if it is normative or actively relevant to
  engineering decisions: architecture, current design, support policy, release
  policy, testing policy, security, production acceptance, or live developer
  setup.
- Keep in `dev/notes/` only if it is active planning, a current release scope,
  a current design-change note, an unresolved investigation, or a handoff that
  still informs near-term work.
- Move to `dev/archive/` if the file is a completed release scope, an old TODO,
  a superseded design, a historical readiness assessment, or a partner/project
  analysis that no longer describes current work.
- Update in place instead of archiving when the document is meant to be
  normative but contains stale links, stale file names, or old version language.
- Do not delete historical docs as part of this review unless a later task
  explicitly approves deletion.

## Review Phases

### 1. Reconcile Governance

- Update `dev/doc-governance.md` so its normative and tracker lists match the
  current repo.
- Decide whether the old implementation plan and response-cycle TODO remain
  historical archive entries or need current replacements.
- Expand `scripts/check-doc-hygiene.py` only if there is a small reliable
  invariant worth automating, such as governance entries must exist or archived
  tracker paths must not be listed as active.

Exit criteria:

- Governance lists reference existing current files.
- The check script still passes.

### 2. Validate `docs/`

- Build with `bash docs/build.sh`.
- Review all pages in `docs/mkdocs.yml` nav for current `0.5.3` behavior.
- Decide whether the four orphaned pages should be added to nav, folded into
  existing pages, or moved out of `docs/`.
- Audit release-version prose in `docs/`:
  - 0.4.x language should remain only when it explains migration history.
  - 0.5.1/0.5.2 migration notes should remain only where they help users
    upgrade to the current 0.5.3 surface.
- Verify examples against the current public surfaces for Python,
  TypeScript, Rust, and operator CLI.

Exit criteria:

- `bash docs/build.sh` passes.
- No unexpected orphan pages remain.
- Public examples and API reference match current exported behavior.

### 3. Tidy Top-Level `dev/`

- Review every top-level `dev/*.md` file against the classification rules.
- Keep a small, explicit set of normative/current docs at top level.
- Move completed plans, old readiness reports, one-off partner analyses, and
  superseded proposals into `dev/archive/`.
- Prefer moving whole files over editing many historical claims in place.

First-pass archive candidates verified and moved:

- `dev/archive/0.4.x-todo.md`
- `dev/archive/additiona-stress-tests-plan-2026-04-10.md`
- `dev/archive/memex-fathomdb-readiness-2026-03-28.md`
- `dev/archive/path-to-production-2026-03-29.md`
- `dev/archive/pathway-to-basic-cypher-2026-03-28.md`

Deferred for explicit feature confirmation:

- remaining top-level `dev/plan-*.md` files
- older detailed design files that may still be relevant architecture

Exit criteria:

- Top-level `dev/` only contains current architecture, current design, policy,
  and live developer references.
- Historical context is preserved under `dev/archive/`.

### 4. Tidy `dev/notes/`

- Review release scope and design notes by version.
- Archive completed 0.4.x, 0.5.0, 0.5.1, 0.5.2, and completed 0.5.3 planning
  docs unless they remain active references for current 0.5.3 support.
- Keep active 0.5.4 candidates and 0.6.0 roadmap/design notes if they still
  describe future work.
- Resolve or archive notes with "decision pending" if the decision has already
  been made.

First-pass archive candidates to verify:

- completed `dev/notes/scope-0.4.*.md`
- completed `dev/notes/0.5.0-scope.md`
- completed `dev/notes/0.5.1-scope.md`
- completed `dev/notes/0.5.2-scope.md`
- completed 0.4.x and 0.5.1/0.5.2 design notes
- pre-tag 0.5.3 hotfix notes once current 0.5.3 behavior is confirmed

Exit criteria:

- `dev/notes/` reads as current planning and unresolved/current design work.
- Completed historical release planning is archived.

### 5. Cross-Link Sweep

- Run Markdown/link search after moves.
- Update links that should point to new archive paths.
- Remove or revise references that still present archived docs as current.
- Confirm `README.md`, `docs/index.md`, and `dev/doc-governance.md` describe
  the final layout.

Exit criteria:

- No broken relative links introduced by file moves.
- Current entry points direct readers to `docs/`, current `dev/`, or archived
  history intentionally.

### 6. Final Verification

Run:

```bash
python3 scripts/check-doc-hygiene.py
bash docs/build.sh
rg -n "TODO-response-cycle-feedback|0\.1_IMPLEMENTATION_PLAN" dev docs README.md
git status --short
```

Optional if doc edits touch API examples:

```bash
cargo test -p fathomdb
pytest --rootdir python python/tests/
cd typescript/packages/fathomdb && npm test
```

## Deliverables

- Updated current docs in `docs/` and `dev/`.
- Archived stale developer docs under `dev/archive/`.
- Updated governance and links.
- A short review summary listing:
  - files updated in place
  - files moved to archive
  - docs build/check results
  - any remaining docs that need product/API confirmation
