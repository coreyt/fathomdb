# fathomdb release checklist

Reference for cutting a versioned release of fathomdb. Works for the Rust crate
(`crates.io`), Python wheel (`PyPI`), and TypeScript package (`npm`) — they ship
as a synchronized triple under a single version number.

Current version: see `scripts/set-version.sh` or `Cargo.toml` workspace version.

## 0. Preconditions

Before starting the checklist, confirm:

- [ ] You're on `main` in the primary checkout, clean working tree
  (`git status --porcelain` empty).
- [ ] `main` is up to date with `origin/main` (`git fetch && git status`).
- [ ] No long-running orchestrated worktrees are open
  (`git worktree list` — expect only `main`).
- [ ] You know what version you're cutting and why (patch for bugfixes,
  minor for features, major for breaking changes). fathomdb is pre-1.0, so
  breaking changes land as minor bumps (0.x.y → 0.(x+1).0).

## 0.5. Registry state check

**This precedes version selection.** A past release cycle may have
partial-published a version to some registries but not others. Check
that the version you're about to cut is strictly greater than the
maximum published version across **all three** registries, not just
the local `Cargo.toml`:

```bash
curl -s https://crates.io/api/v1/crates/fathomdb \
  | python3 -c "import json,sys; print('crates.io:', json.load(sys.stdin)['crate']['max_version'])"
curl -s https://pypi.org/pypi/fathomdb/json \
  | python3 -c "import json,sys; print('pypi:', json.load(sys.stdin)['info']['version'])"
curl -s https://registry.npmjs.org/fathomdb \
  | python3 -c "import json,sys; print('npm:', json.load(sys.stdin)['dist-tags']['latest'])"
```

- [ ] All three registries report a maximum version you recognize.
- [ ] Your new version is strictly greater than the max across all three.
- [ ] If any registry is ahead of `Cargo.toml` (i.e. a past release
  published there but the local bump was never committed), skip the
  ahead version and choose the next one. Never reuse a published version
  number.
- [ ] If any registry has gaps (e.g. crates.io has 0.2.6 and 0.2.8 but
  not 0.2.7), that's historical evidence of a partial-publish incident.
  Review the checklist §10 rollback section before proceeding.

## 1. Code-quality gates

All gates must be green on the commit you're about to tag.

**Primary gate**: run `./scripts/preflight-CI.sh` from the repo root.
This is the full CI-equivalent gate script and runs every clippy / test
/ lint invocation that CI runs, including the feature-gated variants
that catch release-blocking bugs (`--features tracing`,
`--features python`, `cargo nextest run --workspace --features tracing`,
Python tests, TypeScript typecheck + tests, Go vet).

**Do not substitute `./scripts/preflight.sh`** — that script is
intentionally lightweight for agent-harness launches and skips
feature-gated clippy / test runs. Running `preflight.sh` for release
prep will miss `used_underscore_binding` on tracing-span bindings,
conditional-feature test failures, and other CI-visible defects.

- [ ] `./scripts/preflight-CI.sh` — green end to end

Additional gates not in `preflight-CI.sh` that must also pass:

- [ ] `cargo clippy --workspace --all-targets --features default-embedder -- -D warnings -A missing-docs`
  (Candle dep is heavy; preflight-CI.sh skips this. If this fails the
  `default-embedder` feature regressed.)
- [ ] `cargo nextest run -p fathomdb --features default-embedder --test builtin_embedder`
  (feature-gated Candle embedder tests)
- [ ] `cargo nextest run -p fathomdb --test scale`
  (concurrency + determinism stress suite, ~30s)
- [ ] `bash docs/build.sh` — mkdocs build --strict must be clean
- [ ] TypeScript native binding is up-to-date. The TS test suite loads
  `typescript/packages/fathomdb/test/.native/fathomdb.node` — this file
  is a debug-build artifact copied from `target/debug/libfathomdb.so`
  and is easy to forget to regenerate after Rust-side changes. Rebuild
  and copy before the TS test run:
  ```bash
  cargo build -p fathomdb --features node
  cp target/debug/libfathomdb.so typescript/packages/fathomdb/test/.native/fathomdb.node
  cd typescript && npm test --workspace=packages/fathomdb
  ```
- [ ] `tests/cross-language/run.sh` — cross-language parity fixtures
  (if the runner exists at the path — check `ls tests/cross-language/`)

## 2. Version sync and bump

fathomdb ships Rust + Python + TypeScript under a single version. `scripts/set-version.sh`
updates all four surfaces (`Cargo.toml`, `python/pyproject.toml`, `typescript/packages/fathomdb/package.json`,
and cascades through `Cargo.lock` + `package-lock.json`).

- [ ] Pick the new version: `NEW_VERSION=0.x.y`
- [ ] `./scripts/set-version.sh $NEW_VERSION`
- [ ] `cargo update --workspace` to refresh `Cargo.lock`
- [ ] `cd typescript && npm install --package-lock-only` to refresh the TS
  package-lock (or run `npm install` in full; the lockfile must match the
  package.json version)
- [ ] `./scripts/check-version-consistency.py --tag v$NEW_VERSION` — must
  pass. This is also enforced in CI.
- [ ] `git diff --stat` — expect exactly: `Cargo.toml`, `Cargo.lock`,
  `python/pyproject.toml`, `typescript/packages/fathomdb/package.json`,
  `typescript/package-lock.json` (plus anything transitive in `Cargo.lock`).
  Any other file in the diff means something unrelated is staged.

## 3. Changelog

- [ ] Open `CHANGELOG.md`. The top section is `## [Unreleased]` which should
  contain all entries accumulated since the last release.
- [ ] Rename `## [Unreleased]` to `## [$NEW_VERSION] - $YYYY-MM-DD` and insert
  a fresh empty `## [Unreleased]` section above it.
- [ ] Review every bullet. Entries should:
  - describe user-observable change, not internal refactors
  - cite the user-facing API surface (e.g. `search()`, `EmbedderChoice::Builtin`,
    `filterContentRefEq()`)
  - group under the keepachangelog categories: `Added`, `Changed`, `Fixed`,
    `Deprecated`, `Removed`, `Security`
- [ ] Check that major phases from the rollout are represented. For the
  Phase 10-15 + 12.5 adaptive+unified+embedder rollout, expect entries for:
  - unified `search()` surface (Phase 12)
  - `SearchBuilder` SDK bindings (Phase 13)
  - `RetrievalModality` / `vector_distance` / `vector_hit_count` (Phase 10)
  - tethered `VectorSearchBuilder` (Phase 11)
  - read-time embedder (`EmbedderChoice`, `QueryEmbedder`, `BuiltinBgeSmallEmbedder`,
    `default-embedder` feature flag) (Phase 12.5)
- [ ] Do NOT list work tracked by open GitHub issues as shipped. In particular,
  write-time Builtin regeneration (GH issue #39) is not in this release.
- [ ] Add release links at the bottom of the file if the format uses
  reference-style link definitions.

## 4. Documentation currency

- [ ] `docs/guides/querying.md`, `docs/reference/query.md`, `docs/guides/property-fts.md`,
  `docs/guides/text-query-syntax.md` — no stale references to deprecated
  surfaces. Post-Phase-15 these promote `search()`.
- [ ] Code examples in `python/README.md` and `typescript/packages/fathomdb/README.md`
  are runnable against the current API.
- [ ] `dev/design-adaptive-text-search-surface.md` and
  `dev/design-adaptive-text-search-surface-addendum-1-vec.md` are the design
  of record and should accurately describe what shipped. The addendum has a
  "v1.5 update" section post-Phase 12.5.
- [ ] `mkdocs build --strict` passes (already in §1, re-confirm if docs
  touched during the release prep).

## 5. CI workflow health

Release verification depends on historical CI runs being green. Use
`scripts/verify-release-gates.py`.

- [ ] All four workflows passed on a commit within the last 10 days (this is
  the freshness threshold hard-coded in `verify-release-gates.py`):
  - `CI` (Rust workspace)
  - `Python`
  - `TypeScript`
  - `Benchmark And Robustness`
- [ ] The passing commit should be the commit you're about to tag, OR a
  direct ancestor with no subsequent substantive changes.
- [ ] `gh run list --workflow=CI --branch=main --limit=5` to confirm latest
  main runs are green. Repeat for the other three workflows.
- [ ] Check `.github/workflows/release.yml` for any env or action version
  changes you need to know about before pushing a tag.

## 6. Release gate script

- [ ] `./scripts/verify-release-gates.py --tag v$NEW_VERSION` — single-source
  gate check. Exits non-zero if any gate fails. This is the same script the
  release workflow runs, so green here means the tag push will get past
  gate verification.

## 7. Commit and tag

- [ ] `git add Cargo.toml Cargo.lock python/pyproject.toml typescript/packages/fathomdb/package.json typescript/package-lock.json CHANGELOG.md`
- [ ] `git commit -m "Release v$NEW_VERSION"` (match any existing
  release-commit convention; check `git log --oneline | grep -i release | head -5`)
- [ ] `git push origin main` (pre-push hooks will run clippy + tests again;
  expect ~30-60s)
- [ ] `git tag -a v$NEW_VERSION -m "Release v$NEW_VERSION"` (annotated tag)
- [ ] `git push origin v$NEW_VERSION` — this is the trigger for
  `.github/workflows/release.yml`

## 8. Release workflow monitoring

After pushing the tag, watch the release workflow:

- [ ] `gh run watch` or `gh run list --workflow=release --limit=3`
- [ ] `verify-release` job: checks gates, version consistency
- [ ] Publish jobs (crates.io / PyPI / npm): each must succeed
- [ ] If any publish step fails: the version number is now burned — you
  cannot reuse it. Cut `$NEW_VERSION` + 0.0.1 with the fix.

## 9. Post-release verification

- [ ] `cargo install fathomdb --version $NEW_VERSION` (dry-run in a tmp
  directory) — the crate pulled from crates.io compiles and its binary
  runs.
- [ ] `pip install fathomdb==$NEW_VERSION` in a fresh venv — imports, the
  version reported by `fathomdb.__version__` matches.
- [ ] `npm install fathomdb@$NEW_VERSION` in a scratch directory — imports,
  basic `Engine.open()` works.
- [ ] The GitHub release page has notes attached (usually auto-generated
  from the changelog by the release workflow — confirm they're not empty).
- [ ] `git log --oneline origin/main | head -3` — the release commit is on
  `main`, not a detached branch.

## 10. Rollback plan

**Partial-publish failures are possible.** The three publish jobs
(`publish-rust`, `publish-pypi`, `publish-npm`) in
`.github/workflows/release.yml` run in parallel and are not atomic.
If one succeeds and another fails, the successful side is burned —
crates.io / PyPI / npm will not let you re-publish the same version
number. The three registries must then reconverge at a new version.

The historical registry gaps (crates.io missing 0.2.7; npm missing
0.2.1-0.2.4 and 0.2.6) are evidence of past partial-publish incidents.
This is the documented tradeoff of parallel publish: speed in the
happy path, skipped version numbers in the unhappy path.

**If a publish job fails after a tag push**:

- **Do NOT delete the git tag.** Tags are immutable-by-convention for
  downstream tooling.
- **Do NOT yank the crate/wheel/package** unless it's actively unsafe.
  Yanking breaks anyone who installed it between release and rollback.
- **Do NOT re-tag the same version.** The successful side will reject
  it; the failed side will be confused by the tag re-push.
- **Do** check which registries successfully published via §0.5's
  registry state check. The successful side is now at the new version;
  the failed side is still at the old.
- **Do** cut a fresh version `$NEW_VERSION + 0.0.1` with whatever fix
  addresses the underlying failure. Restart the checklist from §0.5
  to confirm all three registries accept the new number.
- **Do** open a GitHub issue describing what broke so the fix is
  reviewed, not just hot-patched.

If a post-release smoke test fails **after** all three publishes
succeeded (e.g. the crate installs cleanly but a user-reported bug
surfaces), the same patch-forward strategy applies — ship a fix in
`$NEW_VERSION + 0.0.1`.

## Appendix: conditional-tracing binding pattern

When a binding is consumed only by a `trace_warn!` / `trace_info!` /
`trace_debug!` macro that expands to a no-op without the `tracing`
feature, naming the binding `_err` to suppress the no-feature
`unused_variables` lint trips `clippy::used_underscore_binding` under
`--features tracing` (because the binding IS used by the expanded
`tracing::warn!` span). The two lints are mutually exclusive on the
same binding.

The repo-wide pattern (see `crates/fathomdb-engine/src/runtime.rs:71`
and the `fill_vector_branch` arms in `coordinator.rs`) is to name the
binding without the underscore AND add an explicit discard:

```rust
Err(err) => {
    trace_warn!(error = %err, "operation failed");
    let _ = err; // Used by trace_warn! when tracing feature is active
    // ...
}
```

The `let _ = err;` satisfies `unused_variables` without the feature;
the `error = %err` inside the macro satisfies `used_underscore_binding`
with it. Both halves are required.

`./scripts/preflight-CI.sh` catches the `--features tracing` half via
line 96 of that script. `./scripts/preflight.sh` does NOT, because it's
an intentionally lightweight agent-harness preflight. Use
`preflight-CI.sh` for release prep.

## Appendix: known pre-existing issues

- `python/tests/examples/test_harness_baseline.py` and
  `test_harness_vector.py` pass when run independently but fail in
  bulk runs due to a sqlite-vec `vec_nodes_active` table that some
  baseline scenarios touch. The cleanup pack at `2c1ef1c` refreshed
  expected counts but the bulk-run interaction remains.
- `cargo clippy --features node` surfaces pre-existing clippy lints
  in `crates/fathomdb/src/node.rs` and `node_types.rs` (unused self,
  pass-by-value, never-constructed struct). Not in the default gate
  set but visible when reviewing node-feature builds.
- Python-macos `test_python_feedback_emits_slow_and_heartbeat_for_slow_operation`
  is a timing-sensitive test that intermittently misses the HEARTBEAT
  phase on macOS CI runners. Not a regression from the 0.3.0 rollout.
  If this is the only failure blocking a release, retry the workflow.
- GitHub issue #39: write-time vector regeneration via the Builtin
  embedder. Tracked but not in scope for this release.

## Appendix: version bump guidance

fathomdb is pre-1.0. Semantic versioning applies with pre-1.0 nuance:

- **Patch (0.x.y → 0.x.(y+1))**: bug fixes, doc updates, internal
  refactors with no API change.
- **Minor (0.x.y → 0.(x+1).0)**: new features, **including breaking
  changes**. Pre-1.0 consumers are expected to read the changelog.
- **Major (0.x.y → 1.0.0)**: API stabilization commitment. Requires a
  deliberate decision, not a drive-by release.

The Phase 10-15 + 12.5 rollout is a minor bump — it adds new APIs
(`search()`, `SearchBuilder`, `EmbedderChoice`, `QueryEmbedder` trait)
and changes some existing types (`match_mode: Option<SearchMatchMode>`,
new fields on `SearchHit`/`SearchRows`). Neither is a patch.
