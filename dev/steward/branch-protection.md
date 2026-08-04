# Branch protection — live state

> **STATUS 2026-08-04: APPLIED AND VERIFIED.** The HITL applied the ruleset
> through the web UI. `required_status_checks` is in force on `main`, and the
> HITL additionally enabled `non_fast_forward` and `deletion`, which were not in
> the original proposal.

## Verification (the evidence, not an assertion)

```console
$ gh api repos/coreyt/fathomdb/rulesets/20166133 -q '[.rules[].type]'
["pull_request","required_status_checks","non_fast_forward","deletion"]
```

Re-read from the live ruleset and diffed against the intended set:

| property | live | intended |
|---|---|---|
| required checks | **16** | 16 |
| missing vs intended | **none** | — |
| extra vs intended | **none** | — |
| `strict_required_status_checks_policy` | `false` | `false` |
| `allowed_merge_methods` | `[merge, squash, rebase]` | unchanged |
| `required_approving_review_count` | `0` | unchanged |
| `bypass_actors` | `[]` | `[]` |
| `enforcement` | `active` | `active` |

`allowed_merge_methods` retaining **`merge`** is load-bearing: the 0.8.21 slice
SHAs cited by `release-state-0.8.21.json` and the steward ledger only survive if
merges are not squashed.

`branch-protection-ruleset.json` is now a **snapshot of the live object**,
refreshed after the change. It is the restore/audit reference.

## The two rules the HITL added beyond the proposal

- **`non_fast_forward`** — blocks force-pushes to `main`.
- **`deletion`** — blocks deleting `main`.

Both were flagged as off during the 2026-08-04 review. A force-pushable trunk
undercuts every claim the release record makes about what landed, so these close
a real hole rather than a theoretical one.

## Re-applying from the checked-in JSON

A `PUT` **replaces the whole ruleset**, so always build the payload from the live
object rather than hand-writing one:

```bash
gh api -X PUT repos/coreyt/fathomdb/rulesets/20166133 \
  --input dev/steward/branch-protection-ruleset.json
```

This needs repository **Administration: Read and write**. The steward's
fine-grained PAT does **not** have it — the attempt on 2026-08-04 returned
`HTTP 403: Resource not accessible by personal access token`, which is why the
change was made through the UI. Expect to do UI edits or grant that permission.

An earlier hand-drafted payload omitted `allowed_merge_methods` and
`required_reviewers`; applying it would have silently reset the repository's
permitted merge methods. Hence the build-from-live rule above.

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
