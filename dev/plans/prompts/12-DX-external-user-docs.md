# Phase 12-DX — External user docs (mkdocs quickstart + install + API ref)

Phase 12 Wave 3 second slice. Builds the external-user-facing
documentation site so 0.6.0 clients can install + run + reference
the SDK after GA tag fires. Per
`dev/plans/0.6.0-implementation.md` § Path to Client-Ready (0.6.0
GA), 12-DX row.

Out of scope:

- 12-D / 12-S / 12-P / 12-V-VERBS / 12-TX (closed); 12-B (deferred).
- Internal design docs in `dev/design/` + `dev/interfaces/` —
  those stay internal; this slice surfaces a CLIENT-FACING subset
  to `docs/`.
- New runtime features. Docs match the locked surface.

## Model + effort

Opus 4.7, intent: high. Per `dev/design/orchestration.md` § 2:

```bash
PHASE=12-DX-external-user-docs
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plans/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "phase-${PHASE}-${TS}" 0.6.0-rewrite
PREAMBLE=$(cat <<'EOF'
YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the work in this
worktree. Do NOT re-spawn yourself. Do NOT spawn other agents. The
"## Model + effort" section in this prompt describes how YOU were
just launched (claude -p with the listed model/effort). Do NOT
re-run that block. Use --disallowedTools Task Agent as a hard
guard. Write code, run tests, commit. Done.
EOF
)
( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plans/prompts/12-DX-external-user-docs.md ) \
  | claude -p --model claude-opus-4-7 --effort high \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Log destination

- stdout/stderr: `dev/plans/runs/12-DX-external-user-docs-<ts>.log`
- structured: `dev/plans/runs/12-DX-external-user-docs-output.json`
- reviewer verdict: `dev/plans/runs/12-DX-review-<ts>.md`

## Required reading

- `AGENTS.md` § 1, § 3, § 7.
- `MEMORY.md`, especially:
  - `feedback_reliability_principles` — net-negative-LoC bias; do
    NOT over-build docs.
  - `feedback_release_verification` — registry-installed wheel is
    the gate; docs that don't match installable surface = wrong.
- `dev/design/orchestration.md` § 2, § 3, § 8.
- `dev/plans/0.6.0-implementation.md` § Path to Client-Ready
  (0.6.0 GA) — 12-DX row + Deferred items disclosure section.
- `docs/release-notes/0.6.0.md` — the deferral disclosures that
  MUST be visible from client-facing docs (perf gates, logical-id
  verbs, TS milestone, no-0.5.x-shim, open-report).
- `dev/interfaces/python.md` + `dev/interfaces/typescript.md`
  (locked specs) — canonical surface to document.
- `dev/notes/12-TX-parity-matrix.md` — 41-row parity matrix; lift
  the public-facing rows.
- `mkdocs.yml` — current site config (mkdocs theme; minimal).
- `docs/` current state — most files are 4-5 line stubs; only
  `getting-started/index.md` (103 lines), `release-notes/0.6.0.md`
  (137 lines), and `index.md` (22 lines) have real content.
- Existing SDK READMEs: `src/python/README.md`, `src/ts/README.md`.

## Scope — five sub-scopes

**Net-negative-LoC bias.** Goal is "client can install + run end-
to-end + reference the public surface after GA." Stubs that say
"post-GA" stay as stubs. Do NOT build out guides/operations/
concepts beyond the minimum.

### Sub-1: API reference tooling decision + setup

Two paths:

- **(A) mkdocstrings**: Python API auto-generated from docstrings.
  Requires `pip install mkdocstrings[python]` + mkdocs.yml plugin
  config + properly-docstringed Python source. ~30 LoC mkdocs
  config; ~0 LoC docs/ if source is well-docstringed; possible
  docstring polish work in `src/python/fathomdb/*.py`.
- **(B) Hand-written API ref**: ~50-100 lines markdown per
  language section, no new mkdocs dependency.

**Recommendation**: try mkdocstrings first. The Python surface is
small (Engine + admin + 5 dataclasses + 18 error classes); mkdocstrings
gives autogeneration + cross-link future-proofing. Fall back to hand-
written if mkdocstrings dep conflicts with current bootstrap (we use
the basic mkdocs theme, not material; check compat).

For TS API ref: typedoc or hand-written markdown. typedoc adds an npm
dep; hand-written is fine for the small TS surface. **Recommend
hand-written for TS** (avoid expanding npm dep tree).

For Rust facade: link to docs.rs (planned post-publish). For pre-GA,
link to source on github.

### Sub-2: Install pages

Three pages under `docs/install/`:

- `docs/install/python.md` — Python install. Sections: requirements
  (Python 3.10+; sqlite-vec extension); `pip install fathomdb==0.6.0`
  (post-GA); current pre-GA editable install
  (`pip install -e src/python/` from `0.6.0-rewrite` branch); verify
  with hello-world snippet; troubleshooting (e.g. sqlite-vec missing).
- `docs/install/typescript.md` — TS install. Sections: requirements
  (Node 18+; platform-tagged napi .node binary auto-selected);
  `npm install fathomdb@0.6.0` (post-GA); current pre-GA build-from-
  source (`cd src/ts && npm install && npm run build`); verify with
  hello-world snippet; TS-not-yet-Python-parity caveat link.
- `docs/install/rust.md` — Rust install. Sections: requirements
  (Rust stable + sqlite-vec system lib); `cargo add fathomdb`
  (facade crate, post-GA); CLI install `cargo install fathomdb-cli`;
  pre-GA `cargo add fathomdb --git
https://github.com/coreyt/fathomdb --branch 0.6.0-rewrite`.

Each page is ~50-80 lines. Cite the locked interface spec for surface
details (don't duplicate).

### Sub-3: Quickstart walkthrough

Single comprehensive page: `docs/getting-started/quickstart.md`.
Should walk through:

1. Install (link to `docs/install/`).
2. Open a fresh DB.
3. Write a small batch of canonical rows.
4. Run a search query.
5. Inspect counters (instrumentation).
6. Close + exit cleanly.

Use **Python as primary language** (Python SDK is the most mature
per `project_typescript_sdk` memory; TS still pre-parity at 12-TX
close). Add TS snippets as secondary alongside Python (tabbed or
just adjacent). Each step shows expected output.

**The quickstart is the release gate per AC-056** (registry-installed
wheel + open/write/search/close/exit smoke). The quickstart text
should match `scripts/release/smoke/smoke-pypi-wheel.sh` step-by-
step so client experience and CI smoke are equivalent. If they
diverge, surface as blocker.

Update existing `docs/getting-started/index.md` to be a landing page
pointing at: `quickstart.md`, `install/python.md`, `install/typescript.md`,
`install/rust.md`. Move out the current preview content if it
conflicts.

### Sub-4: API reference pages

Under `docs/reference/`:

- `docs/reference/python-api.md` — Engine + admin.configure + types
  - error classes. If using mkdocstrings (Sub-1 path A), this is
    a thin shell with `:::fathomdb.Engine` directives. If hand-
    written, full signature + brief description + return type per
    callable + a "see also" pointer to the locked interface spec.
- `docs/reference/typescript-api.md` — Engine class (Promise-based)
  - admin.configure + interfaces + error classes. Hand-written
    (per Sub-1 decision).
- `docs/reference/cli.md` — `fathomdb doctor` + `fathomdb recover`
  verbs from Phase 10a. List flags + exit codes + JSON output
  shape per `dev/interfaces/cli.md`.
- `docs/reference/errors.md` — 18-leaf error taxonomy + base class
  (`EngineError`/`FathomDbError`). Table mapping error class to
  trigger + recovery hint code.
- `docs/reference/config.md` — `EngineConfig` knobs (Python
  snake_case + TS camelCase columns; tied to `design/engine.md`).

Update `docs/reference/index.md` (currently 8-line stub) to be a
landing page linking the five reference pages.

### Sub-5: Compatibility + concepts minimum

- `docs/compatibility/index.md` (currently 4 lines) — expand to
  cover:
  - Supported Python versions (3.10, 3.11, 3.12 per release.yml
    build-python matrix)
  - Supported Node versions (per release.yml build-napi matrix)
  - Supported Rust toolchain (stable)
  - Supported platforms (linux x86_64/aarch64, darwin x86_64/arm64,
    windows x86_64 per release.yml)
  - SQLite + sqlite-vec requirement
  - No-0.5.x-shim policy disclosure
  - Two-axis versioning explainer (Axis W workspace lockstep + Axis
    E embedder-api independent)
- `docs/concepts/index.md` (currently 5 lines) — expand to:
  - Engine lifecycle (open → write → search → close → process-exit)
  - Five-verb runtime surface explainer
  - Canonical rows + projections (1-paragraph each)
  - Embedder model (1 paragraph; pointer to docs.rs or rust crate
    for trait detail)
  - Recovery surface (1 paragraph; pointer to CLI ref)

Keep these tight: 60-100 lines each. Detailed treatment lives in
internal `dev/design/`; this is consumer overview.

Keep `docs/guides/index.md`, `docs/operations/index.md`,
`docs/positions/*.md` as-is (post-GA stubs).

### Optional: nav structure update

Update `mkdocs.yml` `nav:` to surface the new pages:

```yaml
nav:
  - Home: index.md
  - Getting Started:
      - Overview: getting-started/index.md
      - Quickstart: getting-started/quickstart.md
  - Install:
      - Python: install/python.md
      - TypeScript: install/typescript.md
      - Rust: install/rust.md
  - Reference:
      - Overview: reference/index.md
      - Python API: reference/python-api.md
      - TypeScript API: reference/typescript-api.md
      - CLI: reference/cli.md
      - Errors: reference/errors.md
      - Config: reference/config.md
  - Concepts: concepts/index.md
  - Compatibility: compatibility/index.md
  - Operations: operations/index.md
  - Guides: guides/index.md
  - Release Notes:
      - 0.6.0: release-notes/0.6.0.md
  - Positions:
      ...
```

## Required commands

```bash
cd /tmp/fdb-12-DX-external-user-docs-<ts>
# Verify mkdocs builds cleanly with strict mode (no broken links).
mkdocs build --strict
# If mkdocstrings was added: confirm Python docstrings parse without
# errors.
# Quickstart smoke: paste the quickstart Python snippet into a venv
# (or just dry-run validate it matches scripts/release/smoke/smoke-pypi-wheel.sh
# step-by-step).
diff -q <(grep -A100 'open\|write\|search\|close' docs/getting-started/quickstart.md | head -50) \
        <(grep -A100 'open\|write\|search\|close' scripts/release/smoke/smoke-pypi-wheel.sh | head -50) \
  || echo "(diff expected — manual review the equivalence)"
# Canonical local gate (regression guard).
bash scripts/agent-verify.sh
```

Known flakes (rerun once): `ac_029`, `ac_017`,
`t_safe_export_engine_error_exits_export_failure_66`. agent-verify
strace blocker (12-S) still applies on bare hosts; bootstrap installs
strace.

## Discipline

- Net-negative-LoC bias. The slice should add ~500-800 LoC of
  markdown + ~30 LoC of mkdocs config. NOT 2000+ LoC of detailed
  guides.
- Quickstart MUST match `scripts/release/smoke/smoke-pypi-wheel.sh`
  step-by-step. If they diverge, EITHER the script OR the
  quickstart is wrong — surface for HITL decision; don't silently
  pick one.
- Cite locked specs (`dev/interfaces/*.md`) rather than duplicating
  content. External-user docs are pointers + minimal examples;
  authoritative content stays in internal design docs.
- TS-not-yet-parity caveat must appear on TS install page +
  TS API ref page (linked to release-notes 0.6.0).
- Open-report deferral (per 12-TX closure) must appear in Python
  - TS API ref where `Engine.open` is documented.
- Deferred-perf disclosures must appear in compatibility or
  reference — clients evaluating fathomdb for perf-sensitive
  workloads need to see the deferred ACs.
- mkdocs build MUST be `--strict` (no broken links).
- Comment policy: docs CAN have explanatory prose (unlike code).
  But still terse + accurate.

## Blockers — surface before writing code

If any blocks, STOP and write blocker report at
`dev/plans/runs/12-DX-external-user-docs-output.json`:

1. **mkdocstrings incompatible with basic mkdocs theme.** mkdocstrings
   often assumes material theme. If it doesn't work cleanly with the
   current theme, fall back to hand-written API ref (Sub-1 path B).
2. **Python source docstrings sparse**: if mkdocstrings finds little
   to render because the `src/python/fathomdb/*.py` modules lack
   docstrings, adding docstrings expands scope. Surface decision:
   (a) add docstrings as part of this slice (in-scope), (b) skip
   mkdocstrings and go hand-written.
3. **Quickstart-vs-smoke divergence.** If `smoke-pypi-wheel.sh`
   semantics don't translate cleanly to "what a client would type"
   (e.g. the smoke uses a tempdir + cleanup that's noise for a
   quickstart), surface + propose one-of: (i) quickstart simplified
   form documented as "minimal" with smoke as "CI-grade", (ii)
   smoke reworked to match a clean quickstart.
4. **Pre-GA install paths break on this dev host.** If
   `pip install -e src/python/` fails locally (e.g. maturin
   environment), document the gap; recommend CI verification.

## Output

After all commands pass, write
`dev/plans/runs/12-DX-external-user-docs-output.json`:

```json
{
  "phase": "12-DX-external-user-docs",
  "baseline_sha": "<HEAD of 0.6.0-rewrite at spawn>",
  "branch": "phase-12-DX-external-user-docs-<ts>",
  "head_sha": "<HEAD after final commit>",
  "commits": ["<sha>: <subject>", "..."],
  "pages_added": [...],
  "pages_modified": [...],
  "api_ref_tooling": "mkdocstrings | hand-written | mixed",
  "mkdocs_strict_result": "pass | fail (+ tail)",
  "quickstart_smoke_equivalence": "matches | documented divergence | blocker",
  "deferral_disclosures_visible_from": [
    "compatibility/index.md",
    "reference/errors.md",
    "release-notes/0.6.0.md"
  ],
  "total_markdown_loc_added": <int>,
  "blockers_encountered": [{...}],
  "agent_verify_result": "pass | fail (+ tail)",
  "next_step_for_orchestrator": "promote to 0.6.0-rewrite; respawn codex reviewer for verdict"
}
```

Then stop. Do not advance to 12-RC1. Do not run the reviewer
yourself.
