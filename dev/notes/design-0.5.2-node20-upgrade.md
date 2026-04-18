# Design: Node.js 20 action deprecation upgrade (0.5.2 Item 7)

**Release:** 0.5.2
**Scope item:** Item 7 from `dev/notes/0.5.2-scope.md`
**Breaking:** No (CI-only; no runtime API change)

---

## Problem

GitHub Actions emits a deprecation advisory on every workflow run that
uses an action still bundled with Node.js 20:

> Node.js 20 actions are deprecated. The following actions are running
> on Node.js 20 and may not work as expected:
> `softprops/action-gh-release@153bb8e04406b158c6c84fc1615b65b24149a1fe`
> (v2.6.1). Actions will be forced to run with Node.js 24 by default
> starting June 2nd, 2026. Node.js 20 will be removed from the runner
> on September 16th, 2026.

Timeline:

- **2026-06-02** — default runtime flips to Node.js 24. Actions still
  shipped with Node.js 20 may run but behavior is no longer guaranteed.
- **2026-09-16** — Node.js 20 removed from runners entirely. Actions
  still on Node.js 20 stop working.

The v0.5.1 release workflow (`24615114603`) succeeded but surfaced the
advisory on the `github-release` job. Fixing in 0.5.2 buys headroom
before the 0.6.0 release window and prevents a broken release workflow
from surprising us mid-tag.

---

## Current state (anchored to 0.5.1 HEAD)

Confirmed Node.js 20 action: `softprops/action-gh-release`
at `.github/workflows/release.yml:205`.

Known-safe actions in use (verified Node.js 24-compatible by inspection
at time of writing):

- `actions/checkout@de0fac2e...` (v6.0.2)
- `actions/setup-python@a309ff8b...` (v6.2.0)
- `actions/setup-node@53b83947...` (v6.3.0)
- `actions/download-artifact@3e5f45b2...` (v8.0.1)
- `actions/upload-artifact@bbbca2dd...` (v7.0.0)
- `dtolnay/rust-toolchain@3c5f7ea2...` (stable)
- `Swatinem/rust-cache@c1937114...` (v2.9.1)
- `PyO3/maturin-action@e83996d1...` (v1.51.0)
- `pypa/gh-action-pypi-publish@cef22109...` (v1.14.0)
- `mozilla-actions/sccache-action@d651010b...`

Audit plan: confirm each of the above is genuinely Node.js 24-ready at
the pinned SHA (not just assumed). Actions that have released a newer
tag on Node.js 24 but where we are SHA-pinned to an older Node.js 20
build will silently keep running on 20 until the flip.

---

## Goal

- Replace `softprops/action-gh-release` with its latest Node.js
  24-compatible release, SHA-pinned.
- Audit every other action across the five workflow files
  (`.github/workflows/{ci,python,typescript,release,benchmark-and-robustness}.yml`);
  upgrade any that is still on Node.js 20.
- Verify the fix end-to-end before the release tag.

---

## Design

### Step 1: Audit

List every action reference across all workflow files:

```bash
grep -hE 'uses: [a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+@' .github/workflows/*.yml \
  | sort -u
```

For each entry, resolve the pinned SHA to a release tag (via `gh api
repos/<owner>/<repo>/git/ref/tags/<tag>` or checking the action's
releases page). Note the Node.js version bundled with that release.

Outputs a table of `action@sha → tag → node_version`. Any row with
`node_version == 20` is a fix target.

### Step 2: Upgrade `softprops/action-gh-release`

Replace:

```yaml
- uses: softprops/action-gh-release@153bb8e04406b158c6c84fc1615b65b24149a1fe # v2.6.1
```

With the latest Node.js 24-bundled release. At the time of scope
cutting (2026-04-18), check the action's GitHub Releases page for
v2.7.x or later and confirm the release notes call out Node.js 24
support. Pin by SHA and keep the version comment up to date.

### Step 3: Upgrade any other Node.js 20 action found in Step 1

Same SHA-pin pattern. Preserve the inline comment format
(`<sha> # <tag>`).

### Step 4: Verify

Two independent signals:

1. **Advisory-free workflow run on main.** Push a trivial docs commit
   (or re-push a no-op) and confirm the next run of all five workflows
   emits no "Node.js 20 actions are deprecated" annotation.
2. **Forced Node.js 24 opt-in passes.** Temporarily set the job env
   `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24=true` on one workflow, push,
   confirm the workflow passes. Revert the env change (it's only
   needed as a bridge before the platform flip).

### Step 5: Document

Add a brief entry to `dev/notes/release-checklist.md` under "CI
runner compatibility":

> Audit `.github/workflows/*.yml` for deprecated-runtime actions once
> per release cycle. GitHub emits a deprecation annotation on affected
> runs; verify the most recent main push is free of annotations before
> cutting a release tag.

---

## TDD approach

CI workflow changes don't fit classic red-green TDD. Validation is
observational:

1. Before: one of the release workflow runs on `v0.5.1`
   (`24615114603`) shows the deprecation annotation.
2. After: the next push emits no annotation.

Capture the before/after annotation state in the commit message.

---

## Out of scope

- Migrating to reusable workflows (`workflow_call`). Separate cleanup.
- Consolidating the five workflow files. Separate cleanup.
- Upgrading actions whose *functionality* we want changed (vs. just
  the Node.js runtime). This item is purely a deprecation deadline
  response.

---

## Acceptance

- `softprops/action-gh-release` upgraded to a Node.js 24 release.
- Any other Node.js 20 action identified in Step 1 upgraded.
- A clean workflow run on main shows no Node.js 20 deprecation
  annotation.
- Release workflow still functions end-to-end (verify by re-triggering
  the Release workflow on a throwaway tag, or wait for 0.5.2's real
  tag run).

---

## Cypher enablement note

N/A. CI-only change.
