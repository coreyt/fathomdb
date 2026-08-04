# Branch protection — intended state and how to apply it

> **STATUS 2026-08-04: STAGED, NOT APPLIED.** The `main` ruleset still carries
> **only** the `pull_request` rule. `required_status_checks` is **not** in force,
> so **a red PR can still be merged today.** Do not read the checked-in JSON as a
> description of live configuration — it is the *target*, not the *state*.

## Why it is not applied

Applying it needs repository **Administration: Read and write**. The available
credential is a fine-grained PAT without that permission, so the `PUT` returns:

```text
HTTP 403: Resource not accessible by personal access token
```

The failed call changed nothing; the ruleset was re-read afterwards and still
lists exactly `["pull_request"]`.

## How to apply

Either grant the PAT **Administration: Read and write** and run:

```bash
gh api -X PUT repos/coreyt/fathomdb/rulesets/20166133 \
  --input dev/steward/branch-protection-ruleset.json
```

…or set the same 16 checks through the web UI at
**Settings → Rules → `default_ruleset` → Require status checks to pass**.

Afterwards, verify it took:

```bash
gh api repos/coreyt/fathomdb/rulesets/20166133 -q '[.rules[].type]'
# expected: ["pull_request","required_status_checks"]
```

## What the payload contains

The `PUT` **replaces** the whole ruleset, so `branch-protection-ruleset.json`
was built from the live object rather than hand-written. It preserves `name`,
`target`, `enforcement`, `conditions`, empty `bypass_actors`, and — importantly
— the existing `pull_request` parameters including
`allowed_merge_methods: [merge, squash, rebase]` and `required_reviewers: []`.
An earlier hand-drafted payload omitted those two; a `PUT` with it would have
silently reset the repository's permitted merge methods.

### The 16 required checks

`verify` · `security` · `default-embedder-tests` · `rust-workspace-race-report` ·
`board-currency` · `c1-contract-conformance` · `commission-manifest` ·
`design-status` · `docs` · `governed-surface-pin` · `ledger-integrity` ·
`plan-anchors` · `release-state-views` · `steward-orient` ·
`transcript-hygiene` · `CodeQL`

### What is deliberately EXCLUDED, and why

**`wheel-size-gate` and the four `Analyze (…)` jobs are matrix jobs whose check
NAME CHANGES depending on whether the matrix expands.** On the docs-only PR #180
the gate reported as bare `wheel-size-gate`; on PR #178 and #181 it reported as
`wheel-size-gate (ubuntu-latest, x86_64-unknown-linux-gnu, linux-x64, 2_28,
7400000)`. A required check is matched by exact name, so requiring either form
deadlocks every PR that produces the other. `CodeQL` is the stable aggregate
that covers the `Analyze` jobs, and it is required instead.

`strict_required_status_checks_policy` is **false** on purpose: `true` forces
every PR to be rebased onto the newest `main` before merging, which on a
~35-minute `verify` would serialise all merges behind full re-runs.

### Why requiring conditionally-skipped jobs is safe

`verify`, `security`, `default-embedder-tests` and `rust-workspace-race-report`
are skipped by the `changes` path filter on docs-only PRs. GitHub treats a
**skipped** required check as satisfied, so those PRs still merge. PR #180
demonstrated the behaviour: 18 pass, 5 skipping, merged cleanly.

## Origin

Recommendation R3.5 of `dev/design/ci-verify-robustness-review.md`, from the
finding that the ruleset had no `required_status_checks` rule at all. Three PRs
(#178, #179, #180) were merged during that session on manual check-reading
alone, which is exactly the gap this closes.
