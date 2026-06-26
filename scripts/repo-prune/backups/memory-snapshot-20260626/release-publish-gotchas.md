---
name: release-publish-gotchas
description: FathomDB release mechanics — pushing a v* tag auto-fires a real multi-registry publish; dry-run first; tier order, local-hook gaps, post-publish-smoke propagation flakes.
metadata: 
  node_type: memory
  type: project
  originSessionId: a66b3760-9c16-4c26-a7ec-2142c28a2a01
---

Operational truths about cutting a FathomDB release, learned the hard way during 0.7.2 PR-4 (shipping v0.7.1) and PR-8 (shipping v0.7.2).

- **Pushing a `v*` tag auto-fires the REAL publish.** `.github/workflows/release.yml` triggers on `push: tags: v*` with `DRY_RUN='false'` and publishes to crates.io (7 tiers) + PyPI + npm + a GitHub release. `git push origin <tag>` is NOT a benign git op — it's the irreversible release. `git push origin main` alone fires nothing.
- **Always rehearse first** with `gh workflow run release.yml -f dry_run=true --ref main` (DRY_RUN=true → cargo/npm `--dry-run`, PyPI/smoke/github-release skipped). It runs the full `verify-release` preflight + build matrix and catches blockers before any irreversible publish.
- **The release workflow checks out the TAG's tree, not main.** A fix on `main` does not help a tag pointing at an older commit — re-cut/force-move the tag onto the fix commit (`git tag -f` + `git push --force origin <tag>`) to re-fire. Recovery from a partial publish relies on `scripts/release/cargo-publish-if-new.sh` (skip-if-version-exists), so re-firing is safe and does not republish.
- **The local pre-push hook skips actionlint** ("not on PATH" — it's at `/home/coreyt/go/bin/actionlint`). So workflow lint errors (shellcheck SC*, GitHub's 10-input `workflow_dispatch` cap, etc.) only surface in CI's `verify-release`. Run `/home/coreyt/go/bin/actionlint .github/workflows/*.yml` locally before a release.
- **Version bumps must use `scripts/set-version.sh --workspace <v>`**, not a hand-edit of `Cargo.toml:version`. The script also moves the `[workspace.dependencies]` Axis-W sibling pins (publish-time version reqs) + `src/python/pyproject.toml` + `src/ts/package.json`. Missing the sibling pins makes a publish declare stale sibling deps. Axis E (`fathomdb-embedder-api`) stays independent.
- **cargo-publish tier order is dependency-topological**, defined in `dev/design/release.md` § Tiered publish order and `release.yml`. Since 0.7.1, `fathomdb-engine` depends on `fathomdb-embedder`, so the correct order is embedder-api → schema → query → **embedder → engine** → facade → cli; PyPI/npm after engine. (A stale order with engine before embedder partial-published schema+query then failed at engine.)

- **`post-publish-smoke` flakes on registry propagation — it is NOT a publish failure.** On v0.7.2 the `npm-package` smoke failed in ~1.6s (`npm install fathomdb@<v>` couldn't resolve the just-published package), which cascade-**skipped** `github-release` via `needs:`. The publish itself was fine (npm/PyPI/crates.io all `latest`). Recovery: confirm the package is actually live (`npm view`, run `scripts/release/smoke/smoke-npm-package.sh <v>` locally), then `gh run rerun <run> --failed` — it reruns only the failed smoke + dependent `github-release`; the idempotent publish jobs do NOT re-run. Don't re-cut the tag for a smoke flake.
- **The annotated tag object SHA ≠ the commit SHA.** `git rev-parse v0.7.2` and `git ls-remote --tags origin v0.7.2` return the tag *object* (e.g. `9702465`); the commit is `git rev-parse v0.7.2^{commit}` (e.g. `51a3f94`). Both can be true simultaneously — don't mistake the tag-object SHA for a divergence.
- **Post-release closure commits land AFTER the tag and are fine.** STATUS flip + closure JSON go on `main` past the tagged commit; the tag tree (the published artifact) is unaffected. Push them as a normal docs follow-up.

Related: [[pr3-tiered-latency-budget]], [[pr2a-go-recompute-split]].
