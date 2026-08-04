---
title: CI verify-job robustness review
date: 2026-08-04
status: PROPOSED
desc: >
  Evidence-based review of the CI `verify` job as a failure surface — why a 186ms
  shell bug costs 33 minutes, what the real time distribution is, and a prioritized
  set of proposals for local confidence, earlier failure, and CI hygiene. Analysis
  and recommendations ONLY; no CI config, script, or workflow is changed by this doc.
---

# CI verify-job robustness review

**Status: PROPOSED.** Nothing here is implemented. This document changes no
workflow, script, or configuration. Every recommendation is a proposal for the
HITL to accept, defer, or reject.

## 0. The one-paragraph version

The `verify` job is not slow because its checks are badly ordered — the
lint/typecheck/security phases finish in **45 seconds**. It is slow because
`agent-test.sh` runs 54 suites serially in one opaque job step, and three of
them (`test-python` 14m40s, `test-rust` 10m25s, `test-ts` 59s) account for
**26 minutes of the 30**. The remaining 51 suites total **3m45s**. On
2026-08-04 the failing suite printed `FAIL test-agent-test-collect-all
(exit=2, 186ms)` into the log at `00:45:39`, and the job kept running until
`01:15:26` — **29 minutes and 47 seconds after the answer was already on
screen**. The single highest-value change is to run the cheap tier as its own
job so that answer becomes the run's verdict in ~7 minutes instead of ~33.
The single highest-value *prevention* is that **shellcheck is not run anywhere
in this repository** — a fact that is the root cause of the bug class, not
just of this instance.

---

## 1. What `verify` actually is

### 1.1 Job graph (`.github/workflows/ci.yml`, 641 lines, 16 jobs)

| Job | `needs` | `if` | Timeout | Measured duration (run 30766927908) |
|---|---|---|---|---|
| `changes` | — | — | 5m | 9s |
| `verify` | `changes` | `docs_only != true` | **60m** | **28m36s** |
| `default-embedder-tests` | `changes` | `docs_only != true` | 60m | 12m18s |
| `rust-workspace-race-report` | `changes` | `docs_only != true` | 30m | 9m31s |
| `wheel-size-gate` (matrix ×1) | `changes` | `docs_only != true` | 30m | 2m25s |
| `security` | `changes` | `docs_only != true` | 20m | 1m59s |
| `markdownlint` | `changes` | `docs_only == true` | 10m | skipped |
| `c1-contract-conformance` | none | none | 5m | 1m05s |
| `commission-manifest` | none | none | 5m | 22s |
| `transcript-hygiene` | none | none | 5m | 19s |
| `steward-orient` | none | none | 5m | 11s |
| `board-currency` | none | none | 5m | 10s |
| `docs` | none | none | 15m | 11s |
| `release-state-views` | none | none | 5m | 9s |
| `plan-anchors` | none | none | 5m | 8s |
| `design-status` | none | none | 5m | 8s |
| `ledger-integrity` | none | none | 5m | 7s |
| `governed-surface-pin` | none | none | 5m | 6s |

Everything except `verify` finishes inside ~12 minutes. `verify` is the sole
long pole, by a factor of ~2.3× over the next-slowest job.

The always-on doc/governance jobs (`board-currency` … `commission-manifest`)
are an existing, working instance of exactly the pattern this review
recommends extending: cheap, independent, no `needs:`, finishing in seconds.

### 1.2 Inside `verify` — per-step timing (run 30766927908, `verify` job 91547240565)

```text
Set up job                            3s
actions/checkout (fetch-depth: 0)     4s
Configure per-session TMPDIR          0s
actions/setup-python                  0s
actions/setup-node                    7s
dtolnay/rust-toolchain 1.95.0        10s
Swatinem/rust-cache                  25s
Bootstrap dev tooling                60s   (135s on run 30866386557)
agent-verify (lint -> typecheck -> test)  26m42s   <-- ONE opaque step
```

The whole gate is a single `run: bash scripts/agent-verify.sh` step
(`.github/workflows/ci.yml:79-80`). GitHub's UI therefore shows one step that
either takes half an hour and passes, or takes half an hour and fails. There is
no per-phase timing, no annotation, and no artifact.

### 1.3 Inside `agent-verify.sh` (42 lines)

`scripts/agent-verify.sh` runs, strictly serially, short-circuiting on the
first failing phase:

1. `agent-lint.sh` — ruff-version preflight, actionlint-version preflight,
   `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
   migration lint, platform-capabilities, public-doc-truth, `ruff check`,
   actionlint, then `agent-lint-md.sh`.
2. `agent-typecheck.sh` — `cargo check --workspace`, `pyright -p src/python`,
   `tsc --noEmit` via `npm run typecheck`.
3. `agent-security.sh` under `STRICT=1 AC037_LIVE_OPTIONAL=1`.
4. `agent-test.sh` — 54 registered suites.

**Measured split on run 30866386557** (the failing run; `agent-verify` step
started `00:44:52`, security battery printed its summary at `00:45:37`):

- phases 1+2+3 combined: **~45 seconds** (warm `rust-cache`)
- phase 4 (`agent-test.sh`): **~29m49s** (sum of the printed per-suite ms)
- total: `FAIL verify at step=test (1834s elapsed)`

The 45-second figure is from a PR whose diff did not force a Rust rebuild. On a
PR that does touch Rust, `clippy --workspace --all-targets` plus
`cargo check --workspace` will be materially slower. **I did not measure that
case** — the happy path prints nothing (`run_capped` is silent on success and
`AGENT_VERBOSE` is unset in CI), so no successful run in the retention window
carries per-phase timing. That is itself finding O-1 below.

### 1.4 The real time distribution inside `agent-test.sh`

Extracted verbatim from the collect-all summary of run 30866386557 (which is
printed *only because the run failed*):

| Suite | ms | share |
|---|---|---|
| `test-python` | 879,909 | 49.2% |
| `test-rust` | 625,248 | 35.0% |
| `test-ts` | 58,937 | 3.3% |
| `test-check-c1-conformance` | 79,942 | 4.5% |
| `test-check-board-currency` | 42,350 | 2.4% |
| `test-commission-manifest` | 14,637 | 0.8% |
| `test-preflight-landing` | 12,388 | 0.7% |
| `test-check-transcript-hygiene` | 10,982 | 0.6% |
| all 46 others | 64,399 | 3.6% |
| **total** | **1,788,792** (29m49s) | |

Two facts follow directly:

- **The cheap tier is 3m45s.** Everything except `test-rust`, `test-python`,
  and `test-ts` sums to 224,698ms. That tier plus lint+typecheck+security is
  **~4m30s of work**, or ~7m15s wall including checkout/toolchain/bootstrap.
- **`test-python` is the single largest cost in CI**, larger than the entire
  Rust workspace suite. I could not break it down further: `run_capped`
  discards the output of a passing suite, so the CI log for a 33-minute job is
  **913 lines total**. Investigating it needs `AGENT_VERBOSE=1` or `pytest
  --durations`.

### 1.5 Ordering: cheap-after-expensive, quantified

`agent-verify.sh`'s comment says "run lint -> typecheck -> test in latency
order", and at the *phase* level it is right. The inversion is one level down,
inside `agent-test.sh`:

- The failing suite, `test-agent-test-collect-all`, is the **5th of 54**
  registrations (`scripts/agent-test.sh:83`) and completed in **186ms**,
  about 7 seconds into the test phase.
- `test-rust` is registered at line ~342 and `test-python` after it — i.e. the
  26 expensive minutes are last, which is correct.
- So ordering is *not* the problem. **Collect-all is.**

### 1.6 The collect-all trade-off is the actual mechanism

`scripts/lib/agent-suite-run.sh` (0.8.20 R-20-HARNESS) converted
`agent-test.sh` from fail-fast to collect-all: `run_suite` records the outcome
and **always returns 0** so `set -e` cannot abort the run, and
`suite_summary_and_exit` exits 1 iff any suite failed. That change was made for
a good reason, documented at `scripts/agent-test.sh:120-140`: under the old
fail-fast shape, a failing `test-check-governed-surface-pin` made every suite
registered after it **unreachable and vacuously green**.

The cost of that fix is precisely the 30 minutes:

```text
2026-08-04T00:45:39.345  FAIL test-agent-test-collect-all (exit=2, 186ms)
2026-08-04T00:45:39.348  grep: write error: Broken pipe
      ... 29m47s of test-rust + test-python + test-ts ...
2026-08-04T01:15:26.207  FAIL test-agent-test-collect-all rc=2 193ms
2026-08-04T01:15:26.210  registered=54 ran=54 passed=53 failed=1 ...
2026-08-04T01:15:26.210  FAIL verify at step=test (1834s elapsed)
```

The information existed at `00:45:39`. Only the *job exit* was deferred.

**This review does not propose reverting collect-all.** Reverting it restores
the vacuous-green hazard the repo deliberately closed. The proposals in §4.2
keep every suite running while making the verdict arrive early — by
*parallelising the tiers into separate jobs*, not by aborting a tier.

### 1.7 Structural constraints on any restructuring

Two repo-owned tests pin the current shape and must be updated deliberately,
not incidentally:

- `scripts/tests/test_verify_ci_timeout_budget.sh` asserts that a top-level
  job literally named `verify:` exists in `ci.yml` and carries
  `timeout-minutes: 60`. Renaming or splitting `verify` fails this suite.
- `scripts/tests/test_ci_rust_workspace_gate.sh:129` asserts the exact serial
  Cargo invocation reaches CI.

Both are healthy guards. They are named here so a proposal is not mistaken for
a free edit.

---

## 2. Evidence: history of verify failures

### 2.1 Sample and method

`gh run list --workflow=CI --limit 100` returns 100 runs spanning **2026-07-30
→ 2026-08-04**: **77 failure, 11 success, 9 cancelled, 3 in flight** (the three
in flight are PR #178/#179 and were not touched). Job lists were pulled for all
77 failures; raw logs for 20 jobs via
`gh api repos/coreyt/fathomdb/actions/jobs/<id>/logs`, which worked every time —
**no retention gaps in this window**. `gh run view --log-failed` was not relied
on.

**The sample is not representative of steady state.** This window is dominated
by the 0.8.20 Slice-40 CI-readiness campaign and the v0.8.20 registry-recovery
incident; most of 2026-07-30 → 2026-08-01 is one continuous red streak on
`main`. A 77% failure rate is a property of that campaign, not of the repo.
Older runs are outside Actions' log retention and were not reconstructed.

There is also a documented hazard in reading this data:
`dev/steward/steward-ledger.jsonl` seq-205 records a **false green claim** made
from `gh run list` **without `--workflow CI`** — another workflow's success
reported as CI's. All run-level claims here specify the workflow.

### 2.2 `verify` duration: green vs red

| population | n | median | range |
|---|---|---|---|
| `verify` success | 12 | **1900 s (31.7 min)** | 1526–2819 s |
| `verify` failure | 58 | **251 s (4.2 min)** | 192–2226 s |

The failure distribution is **trimodal**, and this is the single most important
operational fact in the review:

| mode | n | where it fails | cost |
|---|---|---|---|
| ~192–277 s (≈4 min) | **46** | `typecheck` (pyright) | cheap |
| ~465–517 s (≈8 min) | **4** | `lint` (ruff, after pyright passed) | cheap |
| **~1521–2226 s (25–37 min)** | **8** | **`step=test`** | **~90% of a full green run, paid before learning anything** |

No run has hit the 60-minute job timeout.

So the honest framing of the problem is narrower than "verify takes 33 minutes
to fail": **`agent-verify.sh`'s phase ordering already works** — 50 of 58
failures surfaced in under 9 minutes. The expensive tail is the 8 runs that
reached the test stage. §2.4 shows that **5 of those 8 failed in a suite that
costs under 6 seconds**.

### 2.3 Failing-job histogram across all 77 failing runs

| job | failing runs |
|---|---|
| `commission-manifest` | 64 |
| **`verify`** | **58** |
| `security` | 46 |
| `rust-windows` (since retired to 0.8.22) | 44 |
| `transcript-hygiene` | 30 |
| `default-embedder-tests` | 6 |
| `steward-orient` | 5 |
| `design-status` | 5 |
| `board-currency` | 3 |
| `rust-macos` | 2 |
| `c1-contract-conformance` | 1 |

Note that `commission-manifest`, `transcript-hygiene`, `board-currency`,
`design-status` and `steward-orient` are **not** `docs_only`-gated (by explicit
design — `ci.yml:439-451`, `:462-473`, `:490-502`), so a steward documentation
commit reds `main` through them.

### 2.4 Root-cause classification

Runs fail multiple jobs at once, so these sum above 77.

| class | runs | evidence |
|---|---|---|
| **ordering** (self-referential gate) | ~62 | `commission-manifest` arm 11d |
| **toolchain-drift** | ~47 | pyright stub drift (46), actionlint missing (1) |
| **flake-race** | ~46 | `rust-windows` tc57 (44), CLI `DatabaseLocked` (1), collect-all SIGPIPE (1) |
| **genuine-defect** | ~38 | transcript-hygiene name leak (30), ruff debt (3), steward-orient arm 7a (6) |
| **config-error** | ~10 | design-status frontmatter (5), board-currency STALE (3), `next_slice` missing (2) |
| **timeout / resource** | 6 | `default-embedder-tests` `slice20c-flush-barrier` — one test ran **589 s** |
| **environment** | 4 | `rg` missing (3), actionlint missing (1) |
| **unknown / masking** | 5 | `test-ts` ×2 (failing assertion truncated out of the log), `c1-contract-conformance` ×1, `rust-macos` ×2 |

#### A. pyright stub drift — 46 runs, ~2 days of red `main` — `toolchain-drift`

Hits `verify` **and** `security` in the same run (both bootstrap, both
typecheck). Sampled: job 91017315514 (run 30585965779, main), 91315965177 (run
30680340130, main), 91316420161 (run 30680493993, main), 91017315528
(`security`, run 30585965779).

```text
src/python/fathomdb/engine.py:548:29 - error: Cannot access attribute "dense_disabled" for class "Engine"
src/python/fathomdb/graph.py:153:16 - error: Argument of type "IdSpace" cannot be assigned to parameter "id" of type "IdSpace"
    "fathomdb._fathomdb.IdSpace" is not assignable to "fathomdb.types.IdSpace" (reportArgumentType)
7 errors, 0 warnings, 0 informations
```

Native `.pyi` stub versus implementation drift, visible only because CI builds
a **fresh** editable wheel — a stale locally-built module masks it exactly.
Introduced at `f2f843ed` (2026-07-09) and unnoticed because the greens in
between were `docs_only` fast-path runs where `verify` and `security` were
**skipped**. `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md:1041`: *"The jobs
were red the whole time and the greens were vacuous."*

**This is the single most consequential finding in the history**, and it is
directly relevant to R3.5's skipped-check caveat.

#### B. `rg: command not found` — 3 runs × 25–37 min, producing a FALSE verdict — `environment`

Runs **30693361449** (job 91351966435, 1709 s), **30699416963** (2184 s),
**30699415820** (2204 s).

```text
scripts/tests/test_ts_cache_coverage_split.sh: line 34: rg: command not found
FAIL  network-gated TypeScript files drifted: expected [src/ts/tests/embedder-event-narrowing.test.ts …], got []
FAILED SUITES: test-ts-cache-coverage-split test-ts-cache-coverage-no-rg
FAIL verify at step=test (1610s elapsed)
```

A missing tool produced an **empty result set** that the guard read as real
drift. The clearest "27 minutes to learn something untrue" case in the sample.
Fixed at `c6e16949 test: remove ripgrep dependency from cache guard`, which
also added `scripts/tests/test_ts_cache_coverage_split_no_rg.sh` as a
recurrence arm — a real fix. **But the same dependency remains at
`scripts/tests/test_check_release_state_views.sh:882`, where it fails in the
opposite, worse direction (§3.1.3).**

#### C. `test-agent-test-collect-all` SIGPIPE — the 2026-08-04 failure — `flake-race`

Run **30866386557** (job 91859096405), `verify` 2004 s. Full trace at §1.6.
The same job's cache-restore step also logged:

```text
##[error]ENOENT: no such file or directory, opendir '/home/runner/work/fathomdb/fathomdb/target/tests/target'
```

**This confirms the rust-cache ENOENT.** It is non-fatal cache-restore noise
from `Swatinem/rust-cache`, and — per a full sweep of git history, `dev/`,
`scripts/` and `.github/` — it has **no record anywhere in the repo**. See
R3.2.

#### D. Two `test-ts` failures whose cause is unrecoverable from CI — `masking`

Runs **30706491146** (job 91386428226, 1521 s) and **30699770006** (2226 s):

```text
FAIL test-ts (exit=1, 61867ms)
output truncated (1593 lines total); full log: /tmp/fathomdb-agent-test-ts-7192.log
FAILED SUITES: test-ts
```

There is **no `not ok` / TAP failure line anywhere in the CI log**.
`run_capped` printed its first 200 lines and wrote the rest to a `/tmp` file
that dies with the runner. Two 25–37 minute runs produced **no diagnosable
information**. Same shape at runs 30681036908 / 30683450976 / 30684118883,
where a ruff failure reported `output truncated (9638 lines total)`.

This is not a hypothetical observability gripe — it is two failures that
**cannot be root-caused from the record**. It moves R3.7 to P1.

#### E. Other verify-stage failures

- **`flake-race`, Rust CLI lock holder** — run 30721829988 (job 91433908579,
  1853 s): `t_s34_dump_mutations_lock_held_exits_71` panicked with
  `DatabaseLocked { holder_pid: Some(47535) }`. This is TC-29, recorded as a
  known cross-binary flake, still open.
- **`genuine-defect`, `steward-orient` arm 7a** — run 30749628122 (job
  91501245492, 1802 s).
- **`environment`, actionlint absent after bootstrap** — run 30713660173:
  `FAIL lint-actions: actionlint 1.7.12 is required but not installed` →
  `FAIL verify at step=lint (0s elapsed)`. Bootstrap's `go install` had
  succeeded in sibling runs. Bootstrap does not guarantee the tools its own
  gates require.
- **`genuine-defect`, ruff debt in `eval/`** — 3 runs, `FAIL verify at
  step=lint (44s elapsed)`.

#### F. `commission-manifest` arm 11d — 62 runs, the largest single source of red — `ordering`

> **✅ ALREADY FIXED — verified by the Steward 2026-08-04. This item is
> HISTORICAL, not open.** The analysis below is accurate for the sample window
> but the sample straddles the repair. Sampling `commission-manifest` job
> conclusions per CI run in creation order:
>
> | window | result |
> |---|---|
> | up to 2026-07-31T17:23Z | 20 sampled runs **failure** |
> | 2026-08-01T02:57Z – 12:28Z | mixed (repair in progress) |
> | since 2026-08-01T12:32Z | **25 consecutive runs pass** |
>
> It is green on every one of the last 25 runs, including PR #178 and #179
> merged today. **The R2.8 recommendation and its #1 shortlist rank are
> therefore superseded** — see the correction at R2.8. The structural lesson
> stands and is worth keeping: a permanently-red required gate is itself a
> masking mechanism, because it trains readers to discount red.

```text
FAIL arm 11d (pre-change generator): no revision of scripts/commission-manifest.sh
predates design_refs — cannot prove additivity; do NOT skip this
```

The guard asks git history a question that history can no longer answer: once
`design_refs` exists in every reachable revision of the generator, the arm is
**self-referentially unsatisfiable**. It is not `docs_only`-gated, so it reds
`main` on every documentation commit. It accounts for more red than any real
defect in the sample. Separately, 2 runs failed on
``FAIL commission-manifest: `dev/plans/release-state-0.8.20.json` names no
`next_slice`.`` A related depth-1-checkout failure in this job was already
fixed by `e9851332 ci: fetch history for manifest recurrence guard`.

### 2.5 What the repo's own records add

The repo's incident register is richer than the CI logs. Its canonical home is
**`dev/todos-and-considerations-ledger.jsonl`** (227 `TC-nn` entries) and
`dev/steward/steward-ledger.jsonl` (239 entries) — not
`dev/plans/ci-deferred.md`, which is `status: SUPERSEDED`, scoped to restoring
pre-0.6.0 workflows, and of whose three items only
`benchmark-and-robustness.yml` remains genuinely open.
`dev/plans/runs/STEWARD-SESSION-HANDOFF-2026-07-30-A.md` §10 carries a 22-item
*"Traps — read before trusting any green"* list.

**The masked-gate lineage, with fixes and their honesty:**

| Incident | Root cause | Fix | Real or mask? |
|---|---|---|---|
| **TC-16** — `agent-test.sh` aggregate exit meaningless | `set -euo pipefail` aborted at the first failing suite; **23 of 31 registered suites had never run in a full pass** (`dev/design/0.8.20-slice-39.5-collect-all-test-harness.md:27`) | collect-all harness, `scripts/lib/agent-suite-run.sh`, landed `b6cc8fa6` | **Real.** Non-vacuity pinned by registering the guard suite *after* others |
| **TC-37** — `agent-lint-md.sh` exits 0 having linted nothing | In a linked worktree both `skip_notice` branches taken | skip → hard fail (`597738d9`) + recurrence guard | **Real.** Consequence measured: TC-49, the md gate was red on main for **3 weeks** unseen |
| **~3-week red `main`** | 7 pyright errors; every green in between was a `docs_only` skip | Publish gate (i) redefined as *"every `ci.yml` job that EXECUTED"*; HITL ruled FIX EVERYTHING, none waived (seq-219) | **Real**, and the gate definition is the durable part |
| **TC-74 / serial Rust gate** | TC-72: 1-in-3 failure on a slice branch **and on its `main` control**, a different test each time | serial gates + `rust-workspace-race-report` reports-only | **Acknowledged partial mask, designed honestly** — the ruling itself says *"Serializing buys DETERMINISM, not correctness: the races stop being exercised rather than being fixed, which is why the non-blocking arm is load-bearing and not decoration"* |
| **0.8.9 macOS de-flake** | Cross-platform tests unmasked by a link fix | First attempt relaxed AC-029/AC-032b tolerances 1.5×→3×; **codex §9 caught it as "the exact lying-gate trap 0.8.9 exists to end — reverted"**; accepted fix keeps the tolerances and hardens the measurement | **A mask proposed, rejected, replaced by a real fix.** The best single datum on this repo's culture |

**"Background exit masks real exit" — still live, and the weakest control
found.** Doctrine at `dev/design/orchestration.md:891`. It fired **twice inside
one slice** despite an explicit guard
(`dev/plans/runs/0.8.20-slice-15b-fix-3-output.json:62`), and the Steward hit
it **in its own verification command** (steward-ledger seq-108→109) when an
exit-status variable followed a `$(basename …)` substitution, wrongly reporting
`test_actionlint_fixture.sh` green. seq-109 proposed the tooling fix — that
`ledgerwrite` refuse or warn on unexpanded shell metacharacters. **It is not
implemented.** The only control today is process discipline: closure receipts
attest the method (e.g. `dev/plans/runs/0.8.20-slice-5-fix-1-output.json:113`,
*"Exit codes read from PIPESTATUS/$?. No trailing echo was used to synthesize a
status."*). That is exactly the "write a be-careful note" posture the repo's own
`guardrail-failures-fix-tooling-not-people` rule forbids.

**Serialization work, verified in git** (all landed via PR #176, merge
`f94275e1`): `c722527b` (RED-first spec) → `bf417e79` *ci: serialize Rust
workspace release gate* → `38347c7b` *ci: harden serial workspace reporter* →
`149f4b9e` *test(ci): detect direct Cargo in every Rust gate* (bans bare
`cargo test` in any gate) → `a5f51b05`, `74af9686`. Plan at
`dev/plans/tc91-serial-ci-implementation-plan.md`. Separately `84e5c9db`
*ci: serialize default embedder test files* (TypeScript, Node 25 IPC-handle
retention) and its predecessors `63eb3a5a`, `08f9e3c7`.

**Timeout history — TDD'd, not hand-edited.** `d24430a2` moved `verify`
30→45 min, then `c1162f4e` 45→60 when 45 proved insufficient; both carry
receipts under `dev/plans/runs/receipts/0.8.20/` recording RED-before /
GREEN-after on `test_verify_ci_timeout_budget.sh` and
`"other_job_timeouts_changed": false`. The recorded tension
(`STEWARD-SESSION-HANDOFF-2026-07-31-A.md:80`): *"a 60–75 minute serial floor —
and trap 13 requires re-running every gate after a rebase."*

**Two prior SIGPIPE incidents.** `scripts/steward-orient.sh:259` carries the
fix and its reasoning: *"Here-string, not a pipe: under pipefail an early
`grep -q` exit would SIGPIPE the producer and misreport a REGISTERED worktree
as an orphan."* And `308f7922`, the 2026-08-04 fix, whose message ends: *"The
sibling line two below it already guarded the same idiom with `|| true`; this
one did not."*

**shellcheck: untracked debt.** Independently confirmed from the run records —
`dev/plans/runs/0.8.20-slice-39-leg1a-output.json:187`: *"shellcheck is not
installed here and is not wired into CI or `scripts/agent-lint.sh`."* Agents
have handled it honestly (`agent-seat-hardening-ASH-B-output.json:64`: *"NO
SHELLCHECK CLAIM IS MADE"*), substituting `bash -n`. **There is no ledger
entry, no deferral record, and no plan item for it** — the one gap in an
otherwise exhaustive record.

**Adjacent, and a live supply-chain hazard on the gate:** `b25e80c4` pinned
Ruff and `c17b14ae` pinned actionlint, each with a version-drift test. **pyright
is still unpinned** (`>=1.1.380`) — TC-142, deferred to 0.8.21, described in
the ledger as *"CI can redden overnight with no repository change. A
supply-chain hazard on the gate that currently blocks publish."* Given §2.4A,
this is not theoretical.

---

## 3. The three themes, confirmed or refuted

### 3.1 Shell fragility under `set -euo pipefail` — CONFIRMED, and systemic

A full sweep of 145 in-scope `.sh` files (excluding `.venv`, `node_modules`,
`target`, vendored corpus downloads), all 5 workflows, and both live hooks:

| Signal | Count |
|---|---|
| shell files in scope | 145 |
| files with `set -euo pipefail` | 123 (134 with any `pipefail`) |
| `\| head` pipeline sites | 38 (23 with no `\|\| true` guard) |
| `\| grep -q` sites | 220 (216 with a bounded `printf`/`echo` producer) |
| `$?`-after-pipeline masking | 0 found |
| **`shellcheck` invocations, anywhere in the repo** | **0** |

**The headline finding is the last row.** There is no `.shellcheckrc`. All 43
`shellcheck` string hits in the repo are `# shellcheck source=` /
`# shellcheck disable=` *directives in comments* — the tool that would read
them is never executed. Specifically it is absent from:

- all 5 files in `.github/workflows/`;
- `scripts/agent-lint.sh`, whose legs are `cargo clippy`, `cargo fmt`,
  `agent-lint-migrations.sh`, `check-platform-capabilities.sh`,
  `check-public-doc-truth.py`, `ruff check src/python`, `actionlint`, and
  `agent-lint-md.sh` — **no shell linter of any kind**;
- `scripts/hooks/pre-commit` (fmt / ruff / markdownlint) and
  `scripts/hooks/pre-push` (clippy + actionlint).

There is also no `bash -n`, no `shfmt`, and no `checkbashisms`. `actionlint`
does embed shellcheck for workflow `run:` blocks — but it never sees
`scripts/**`, which is where every incident in this class has occurred.

So SC2312 (`check-extra-masked-returns`, the check that flags exactly this
bug), SC2086, and SC2181 are all off **by absence**, not by decision.

#### 3.1.1 This is the third occurrence of the same bug class

`scripts/tests/test_commission_manifest.sh:938` and `:962` carry comments
documenting two *prior* incidents of the identical class — a `diff … | head -20
| tr …` aborting under pipefail, and a `! (git show …) | grep -q design_refs`
reading the wrong revision. Together with `scripts/tests/
test_agent_test_collect_all.sh:461` (fixed 2026-08-04 in `308f7922`,
"fix(tests): remove SIGPIPE race in collect-all arm F"), that is **three
separate occurrences, each fixed by hand at the site, with no mechanical guard
added**. That is precisely the pattern the repo's own standing rule forbids:
fix the tooling so it cannot recur for anyone.

#### 3.1.2 Live sites, prioritized

**P0 — fail-open guards.** `set -e` is suspended inside `if`, so a
SIGPIPE-poisoned rc does not abort; it flips the condition to false and the
guard is silently skipped. These fail open *exactly when they should fire*:

- `scripts/check-design-refs.sh:243` — `if git ls-files --unmerged | grep -q .;
  then` … the "index has UNMERGED paths, coverage UNVERIFIED" warning.
- `scripts/check-staged-ledger-sidecars.sh:162` — same idiom, and this one
  guards a hard `exit 2`. It runs on **every commit** via
  `scripts/hooks/pre-commit:24`. The TC-88 sidecar gate would fail open
  mid-conflict — the exact scenario its own header says it exists for.
- `scripts/release/publish-rc1-bootstrap.sh:37` — `if curl -fsS "$url" … |
  grep -qF …` streaming a crates.io sparse-index page into an early-exiting
  `grep -qF`. On SIGPIPE the idempotency `SKIP` is bypassed and `cargo publish`
  is re-attempted against an already-published crate. Real-money path.

  *Caveat on probability:* for the two `git ls-files` sites the producer's
  output is normally far smaller than the 64KB pipe buffer, so the producer
  usually finishes before `grep -q` exits and never receives SIGPIPE. The risk
  is real but low-frequency — and the failure mode is silent, which is what
  makes it worse than an abort, not better.

**P1 — byte-for-byte twins of the 2026-08-04 failure** (`grep … | head … |
cut`, unguarded, under `pipefail`):

- `scripts/set-version.sh:278`, `:363`, `:390`, `:394` — four sites. Reached by
  `set-version.sh --check-files`, which runs in the release path *and* under
  `FATHOMDB_PREPUSH_FULL=1 git push`.
- `scripts/tests/test_check_design_refs.sh:767`, `:768` — in a CI test,
  grepping a whole script file. Structurally the closest twin to the failure
  and the most likely next repeat.

**P2 — SIGPIPE only on the failure path**, converting a clear diagnostic into a
confusing abort: `scripts/sbom-survey/smoke-install-run.sh:333` (`diff … |
head -20 | sed …`), `scripts/tests/test_steward_orient.sh:191` (`find "$sandbox"
-mindepth 1 | head -5` — unbounded producer, non-empty precisely in the residue-
leak case being tested) and `:520`.

**P3 — `|| true` present, which converts SIGPIPE into a silently-wrong result**
rather than an abort: `scripts/tests/test_ts_cache_coverage_split.sh:123-127`
(5 sites), `scripts/lint-design-status.sh:109,127`,
`scripts/lint-plans-status.sh:49`. Low risk here (here-string producers), but
the idiom teaches the wrong reflex.

#### 3.1.3 Masking: what came back clean, and one new live vacuous-pass

Deliberately checked, because of this repo's history with "background exit
masks real exit":

- **`$?`-after-pipeline masking: none found.** All 129 `$?` sites read the rc
  immediately. The `set +e … cmd … rc=$? … set -e` idiom is used correctly and
  consistently, including at `scripts/tests/test_agent_test_collect_all.sh:446-452`
  and `.github/workflows/ci.yml:104-106`.
- **Trailing `exit 0` masking: present but deliberate and documented.**
  `.github/workflows/ci.yml:123` (`rust-workspace-race-report`) and `:230`
  (BGE warm-cache) both `exit 0` after capturing the real rc — and both emit
  the rc into `$GITHUB_STEP_SUMMARY` and/or a `::warning`. These are the
  *honest* form of "report, do not gate". They are not the "trailing echo fakes
  exit 0" pattern.
- **Unquoted expansions under `set -u`: essentially clean.** The two live sites
  (`scripts/md-safe-fix.sh:87`, `scripts/hooks/pre-commit:76`) are deliberate
  word-splits carrying matching `# shellcheck disable=SC2086` directives.

**NEW OPEN DEFECT — an undeclared `rg` dependency creates a vacuous pass.**
`scripts/tests/test_check_release_state_views.sh:881-885`:

```bash
if [ -z "$REAL_PLAN_PATH" ] \
   && ! rg -q 'BEGIN GENERATED release-state:0\.8\.20:plan-immediate-next' \
     "$REPO_ROOT/dev/plans/plan-0.8.20.md"; then
  pass "real repo — end-of-ladder state retires the generated next-slice pointer …"
```

If `rg` is not on `PATH`, it exits 127, `!` inverts that to true, and the arm
reports **`pass`** without ever having looked at the file. This is a TC-37-class
vacuous pass, and it is live: the repo already has a dedicated
`test-ts-cache-coverage-no-rg` suite (`scripts/agent-test.sh:401`) and comments
at `scripts/check-transcript-hygiene.sh:468` recording a prior
"rg: command not found" incident, so rg-absence is a known hazard here — this
site just was not covered. The same file uses `perl` at `:894`, another
undeclared tool.

**And rg-absence is not hypothetical: it has already cost three CI runs.**
§2.4B records runs 30693361449 / 30699416963 / 30699415820 failing after
25–37 minutes with `rg: command not found` in
`scripts/tests/test_ts_cache_coverage_split.sh:34`. That site was fixed
(`c6e16949`, with a `_no_rg` recurrence arm). The
`test_check_release_state_views.sh:882` site was not — and it fails in the
**opposite and worse direction**: the 2026-08-01 incident failed *loudly and
wrongly*; this one would pass *silently and wrongly*. Whether it passed
vacuously in the runs sampled cannot be determined from the log, which is
precisely the defect.

#### 3.1.4 One adjacent finding

`/home/coreyt/projects/fathomdb/.git/hooks/pre-push` exists and **diverges
substantially** from the tracked `scripts/hooks/pre-push` — it predates the
`FATHOMDB_PREPUSH_FULL` opt-in and the `set-version.sh --check-files` step. It
is currently inert because `core.hooksPath = scripts/hooks`, but it will
silently shadow the tracked hook for anyone who unsets that config. Candidate
for deletion.

### 3.2 Test isolation — CONFIRMED, and already paid for in wall-clock

The repo has a documented, HITL-ruled position on this, in
`dev/design/temporary-serial-rust-workspace-release-gate.md` (status:
PROPOSED). Its §1 states plainly: TC-29 records a cross-binary lock-holder test
reliable in isolation but failing in a full suite; TC-72 measured a different
concurrency-sensitive failure in **one of three** full-workspace runs on both a
merged candidate *and its `main` control*. The ruling (TC-74) is a two-part
control: the serial run gates, the parallel run reports but cannot fail.

That ruling is implemented exactly:

- `scripts/test-rust-workspace.sh` is the sole owner of the invocation, with
  no defaulting mode (`usage; exit 2` on a missing/unknown argument).
- `--serial` = `cargo test --workspace --quiet --no-fail-fast --jobs 1 --
  --test-threads=1`. Both layers are deliberate: `--jobs 1` stops Cargo
  scheduling test *targets* concurrently; `--test-threads=1` stops the harness
  running tests within one binary concurrently.
- `rust-workspace-race-report` (`ci.yml:84-129`) runs the parallel equivalent,
  writes rc + duration to `$GITHUB_STEP_SUMMARY`, emits a `::warning` on
  non-zero, uploads the log as an artifact, and `exit 0`s.

**This is the right shape for a known-flaky signal in an anti-masking repo**,
and §4.3 recommends it as the sanctioned template rather than a generic
quarantine tool. The ruling states the trade-off in its own words — worth
quoting because it is the model for every other recommendation here:

> Serializing buys DETERMINISM, not correctness: the races stop being
> exercised rather than being fixed, which is why the non-blocking arm is
> load-bearing and not decoration.

Race-hunting is deferred to 0.8.21 with that cost written down. The individual
races remain on the register: TC-29 (cross-binary lock holder — **observed
again in run 30721829988**, §2.4E), TC-90, TC-70 (where the ledger notes *"the
ladder is currently acting as an accidental backoff that masks this race"*),
TC-64. TC-57 has a real fix (`BEGIN IMMEDIATE`, `77be504b`).

One ledger item needs re-checking rather than acting on: **TC-141** claims
*"there is NO full-workspace cargo test on Linux anywhere in ci.yml"*. As of
the serialization landing (`bf417e79`, PR #176) `verify` → `agent-test.sh` →
`test-rust-workspace.sh --serial` **is** a full-workspace `cargo test
--no-fail-fast` on `ubuntu-latest`. TC-141 appears superseded by its own
release's work; **I have not confirmed this against the ledger entry's date** —
worth reconciling before it is carried into 0.8.21 as live debt.

The measured cost: `test-rust` = 10m25s. Note that `--jobs 1` serializes
**building** as well as running, so a large part of that is rustc time that the
isolation invariant does not actually require (see R2.5).

Other shared-state hazards found:

- **Shared `.venv` / maturin rebinding.** `scripts/agent-test.sh:340-360`
  documents TC-27: `tests/conftest.py` may run `maturin develop`, which
  **rebinds the active virtualenv to this source tree**. It is guarded — the
  rebuild is only authorised when the interpreter is `.venv/bin/python` inside
  *this* checkout, with conftest re-checking ownership. That guard is why the
  repo's memory records "forbid `maturin develop` / `pip install -e` from a
  worktree". Sound, but it means `test-python` is not safely parallelisable
  with anything else touching the venv.
- **Node test-runner IPC handle retention.** `ci.yml:246-263` runs the seven
  default-embedder TypeScript files in a `for` loop, one `node --test` process
  each, with the comment: "Node 25's multi-file runner can retain an exited
  child's IPC handles, and concurrent children contend for the same finite
  runner memory and CPU." Landed as `84e5c9db ci: serialize default embedder
  test files`.
- **TMPDIR: handled well.** Every heavy job sets
  `TMPDIR="$RUNNER_TEMP/fathomdb-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"` via
  `$GITHUB_ENV` (`ci.yml:58-65`, `:154-159`, `:185-190`). Per-run-attempt
  isolation, one `rm -rf` at job end. No shared-TMPDIR hazard found.
- **Port binds: one site**, `scripts/tests/test_verify_embedder_api_no_drift.sh:130`,
  with a proper PID/port-poll wait. Not a hazard as written.
- **GPU: not a CI concern.** GPU use is the local eval-harness policy; no CI
  job requests a GPU runner.

### 3.3 Local/CI divergence — CONFIRMED, but *not* the cause of the 2026-08-04 bug

What genuinely differs:

| Axis | Local | CI | Assessment |
|---|---|---|---|
| Rootless userns | AppArmor blocks on most dev hosts | blocked on `ubuntu-latest` | **Equivalent.** `agent-verify.sh:26-35` sets `AC037_LIVE_OPTIONAL=1` and the log shows `AC-037 ENVIRONMENTAL: unshare -rUn failed` → `1 downgrade(s)`. The authoritative AC-037-live gate is the separate `security` job pinned to `ubuntu-22.04` with `STRICT=1` and no opt-in. **Well-designed divergence, correctly bounded.** |
| Toolchain pinning | `bootstrap.sh` installs; `agent-lint.sh` hard-fails on Ruff ≠ 0.15.17 and actionlint ≠ 1.7.12 | `dtolnay/rust-toolchain@…1.95.0`, node 25.9.0, python 3.12 | **Good.** Version-preflight suites (`test-agent-lint-ruff-version`, `test-agent-lint-actionlint-version`, `test-runtime-release-alignment`) exist precisely to stop false local greens. |
| Locked installs | `bootstrap.sh` uses `npm install --silent` (**not** `npm ci`) for root and `src/ts` | `default-embedder-tests` uses `npm ci` (`ci.yml:241`) | **Real gap.** The `verify` job bootstraps with unlocked `npm install`, so a transitive dep can drift between a local run and CI, and between two CI runs. |
| Clean clone | incremental working tree, `target/` warm | fresh checkout + `Swatinem/rust-cache` | Divergent, but see below. |
| `TMPDIR` | system default | per-run-attempt dir | CI is stricter. Fine. |
| `rg` | present on dev host | **absent** — cost 3 runs × 25–37 min (§2.4B) | **Confirmed divergence.** One site fixed (`c6e16949`); `test_check_release_state_views.sh:882` still depends on it, and `:894` on `perl`. |
| `pyright` version | whatever the venv resolves | whatever the venv resolves | **Unpinned (`>=1.1.380`, TC-142).** Ruff and actionlint are both version-pinned with drift tests; pyright is not — and pyright drift is what red-lined `main` for ~2 days (§2.4A). |
| Container | none | GitHub-hosted VM | No Dockerfile, no `.devcontainer`, no `act`. **There is no way to run CI's environment locally today.** |

**Which local runs would NOT have caught the 2026-08-04 bug — and why this
matters.** Be blunt about this, because it disciplines the §4.1 recommendations:

- `bash scripts/agent-verify.sh` locally: **would not have caught it** — it is
  reported to have passed locally.
- A clean-clone local verify: **probably still would not have caught it.** The
  race is on pipe-buffer fill versus `head`'s exit. It depends on producer
  output volume and scheduler timing, not on checkout cleanliness. A clean
  clone changes neither.
- A container matching `ubuntu-latest`: **probably not**, for the same reason.
  A different CPU count would shift the odds, not close them.
- Running the suite in a loop: would find it eventually, at unbounded cost.
- **`shellcheck --enable=check-extra-masked-returns`: would have caught it
  deterministically, in under a second, before the code was ever committed.**

That asymmetry is the central conclusion of this review. For *this* bug class,
environment fidelity is nearly worthless and static analysis is nearly free.
Environment fidelity (§4.1) is still worth some investment — it addresses a
different class (toolchain drift, unlocked installs) — but it must not be sold
as the answer to what happened on 2026-08-04.

---

## 4. Recommendations

Every recommendation below states: **problem** (tied to evidence above),
**change**, **cost**, **risk**, **priority**. Priorities are P0 (do first),
P1, P2, P3 (defer / probably don't).

A standing constraint on all of them: this repo's culture forbids masked
failures, vacuous passes, and silently-skipped required suites. Any proposal
that could hide a real failure is flagged **⚠ MASKING RISK** and argued
explicitly. Two proposals below (R3.1, R3.4) are argued *against* on those
grounds.

### 4.1 Bucket 1 — more robust from failure: what to do differently locally

#### R1.1 — Run `shellcheck` in `agent-lint.sh`, with the masked-return check ON — **P0**

- **Problem.** §3.1: zero shellcheck invocations across 145 shell files and
  123 `set -euo pipefail` scripts; three occurrences of the same SIGPIPE class,
  each fixed by hand; 15+ live sites still open, three of which fail *open*.
- **Change.** Add a leg to `scripts/agent-lint.sh` alongside the existing
  ruff/actionlint version preflights (same hard-fail-on-version-drift posture,
  since a green from the wrong shellcheck version is a false green):

  ```bash
  readonly SHELLCHECK_VERSION="0.11.0"   # pin; preflight like RUFF_VERSION
  # shellcheck's SC2312 (check-extra-masked-returns) is OPTIONAL — it is off by
  # default and must be requested explicitly. It is the check that flags the
  # 2026-08-04 `grep … | head` failure.
  run_capped lint-shell "$shellcheck_bin" \
    --severity=style --enable=check-extra-masked-returns \
    $(git ls-files '*.sh')
  ```

  Add a `.shellcheckrc` at the repo root carrying `enable=check-extra-masked-returns`
  (and any others adopted) so editors and pre-push agree with CI by default.
- **Cost.** Small to add; the real cost is the **initial finding volume** across
  145 files. Budget one slice to triage. `shellcheck` runs the whole tree in
  ~1 second, so there is no ongoing latency cost.
- **Risk.** ⚠ The tempting shortcut — a blanket `# shellcheck disable=SC2312`
  header or a global `--exclude` — would be masking, and would reproduce the
  exact posture that let three incidents through. **If the finding volume is
  unmanageable, ratchet by directory (start with `scripts/hooks/**`,
  `scripts/check-*.sh`, `scripts/tests/**`) rather than by suppressing the
  check.** A per-directory ratchet is honest: it says "not yet covered". A
  global disable says "covered" while covering nothing.
- **Note.** SC2312 is genuinely noisy inside `if`/conditional contexts (upstream
  issues [#2809](https://github.com/koalaman/shellcheck/issues/2809),
  [#3042](https://github.com/koalaman/shellcheck/issues/3042)) and does not yet
  auto-suppress under `pipefail` ([#2368](https://github.com/koalaman/shellcheck/issues/2368)).
  Expect to write per-line justified `disable` comments; that is fine — a
  per-line disable with a reason is a decision, a global one is not.

##### CORRECTION (2026-08-04, recorded while implementing R1.1 in 0.8.21 Slice 30)

**The claim above the recommendation table — that
`shellcheck --enable=check-extra-masked-returns` "would have caught [the
2026-08-04 bug] deterministically" — is FALSE, and was measured false on the
version this repo now pins (shellcheck 0.11.0).**

Fed the verbatim pre-fix line from `308f7922`:

```bash
FIRST_SUITE_LINE="$(grep -nE '^[[:space:]]*run_suite[[:space:]]' "$AGENT_TEST" | head -n1 | cut -d: -f1)"
```

shellcheck 0.11.0 reports **nothing** — not under SC2312, and not under any of
its eleven optional checks (`add-default-case`, `avoid-negated-conditions`,
`avoid-nullary-conditions`, `check-extra-masked-returns`,
`check-set-e-suppressed`, `check-unassigned-uppercase`, `deprecate-which`,
`quote-safe-variables`, `require-double-brackets`, `require-variable-braces`,
`useless-use-of-cat`). The same holds for the P0 shape
`if git ls-files --unmerged | grep -q .; then`. The reason is structural, not a
bug: SC2312 fires where a command substitution's exit status is discarded by the
command it is an *argument to*. In an assignment the substitution's status *is*
the assignment's status, so by SC2312's own rule nothing is masked — even though
`pipefail` has already poisoned that status with the producer's SIGPIPE.

Adopting shellcheck is still right, and SC2312 covers a large and overlapping
family of genuine masked returns. But shellcheck **alone** would not have
stopped occurrences one through four, and a slice that shipped only shellcheck
while believing otherwise would have been a vacuous green about its own purpose.
Slice 30 therefore ships the SC2312 leg **and** a second enforced leg — the
early-exiting-consumer detector in `scripts/lib/shell-early-consumer.sh`,
promoted from the positive-controlled arm 5 of
`scripts/tests/test_shell_pipefail_guards.sh` — which does cover the shape.

**Ratchet form adopted.** Per-FILE, not per-directory (strictly tighter):
`scripts/shellcheck-sc2312-ratchet.txt` and
`scripts/shell-early-consumer-ratchet.txt`. Every tracked `*.sh` not listed is
enforced, new files are covered by default, and both lists may only shrink — a
listed file that has become clean fails the gate until its line is deleted.

**Named follow-ups from Slice 30** (deferred deliberately, not dropped):

- **FUP-SHELLCHECK-1** — clear SC2016 (91 sites) and SC2015 (40 sites) and
  remove them from `DEFERRED_CHECKS` in `scripts/agent-lint-shell.sh`.
- **FUP-SHELLCHECK-2** — empty `scripts/shellcheck-sc2312-ratchet.txt`
  (35 files / 344 findings at Slice 30 landing).
- **FUP-SHELLCHECK-3** — empty `scripts/shell-early-consumer-ratchet.txt`
  (37 files / 224 sites at Slice 30 landing, two of which are deliberate
  fixtures that will never leave the list).

#### R1.2 — Fix the 15 audited sites — **P0** (small, mechanical)

- **Problem.** §3.1.2.
- **Change.** `grep … | head -n1` → `grep -m1 …` (the fix already applied in
  `308f7922`) at `scripts/set-version.sh:278,363,390,394` and
  `scripts/tests/test_check_design_refs.sh:767,768`. For the three P0 fail-open
  guards, replace the pipeline entirely — `|| true` does not help because it
  masks the true branch too:

  ```bash
  # was: if git ls-files --unmerged | grep -q .; then
  if [ -n "$(git ls-files --unmerged)" ]; then
  ```

- **Cost.** Under an hour. **Risk.** Low; each is a behaviour-preserving
  rewrite. Should land *with* R1.1 so shellcheck proves the tree clean after.
- **Priority.** P0 — but note these are symptoms. R1.1 is the cure.

#### R1.3 — Add a shellcheck leg to `scripts/hooks/pre-push` — **P1**

- **Problem.** The fast pre-push path already runs `cargo clippy --workspace
  --all-targets` (tens of seconds) and `actionlint`. Shell is unlinted at every
  gate, including the one the developer actually feels.
- **Change.** One block in `scripts/hooks/pre-push`, mirroring the existing
  actionlint soft-skip posture:

  ```bash
  if command -v shellcheck >/dev/null 2>&1; then
    echo "[pre-push] shellcheck"
    # shellcheck disable=SC2046
    shellcheck --severity=style --enable=check-extra-masked-returns $(git ls-files '*.sh')
  else
    echo "[pre-push] shellcheck not on PATH — skipping. Run scripts/bootstrap.sh." >&2
  fi
  ```

- **Cost.** ~1 second added to a hook that already costs far more.
- **Risk.** The soft-skip is a small masking surface, but it matches the
  established actionlint precedent and the authoritative gate stays in
  `agent-lint.sh` (R1.1), which hard-fails. Acceptable.

#### R1.4 — Delete the orphan `.git/hooks/pre-push` — **P2**

- **Problem.** §3.1.4 — a stale, divergent, currently-inert hook that will
  shadow the tracked one if `core.hooksPath` is ever unset.
- **Change.** `rm .git/hooks/pre-push` (untracked, local to this clone). Better:
  have `scripts/install-hooks.sh` warn when a non-sample file exists in
  `.git/hooks/`, so it cannot recur in another clone — the standing
  "fix the tooling, not the actor" rule.
- **Cost/Risk.** Trivial / none.

#### R1.5 — `scripts/agent-verify.sh --like-ci`, honestly scoped — **P2**

- **Problem.** §3.3: there is no Dockerfile, no `.devcontainer`, no `act`, and
  the bootstrap uses unlocked `npm install` where CI's other job uses `npm ci`.
  A developer has no way to reproduce CI's environment.
- **Change.** A `--like-ci` flag that does the cheap, high-value 80%:
  `AGENT_VERBOSE=1`; `TMPDIR=$(mktemp -d)`; `npm ci` instead of `npm install`;
  a `git worktree`/clean-clone of `HEAD` into that TMPDIR so untracked files
  cannot green the run; and an explicit `PATH` scrub of tools the runner may not
  have (`rg`, `perl`) so §3.1.3-class dependencies surface locally.
- **Cost.** Moderate (~a slice). Full container parity is a much bigger lift and
  is **not** recommended at this stage.
- **Risk.** Low.
- **⚠ Honesty requirement.** §3.3 establishes that `--like-ci` **would not have
  caught the 2026-08-04 bug**. It addresses toolchain/untracked-file/unlocked-
  dependency drift, which is a real but different class. It must not be
  presented, in the commit message or in `AGENTS.md`, as the remedy for
  SIGPIPE-class flakes. R1.1 is that remedy.

#### R1.6 — Use `npm ci` in `scripts/bootstrap.sh` — **P2**

- **Problem.** §3.3 — `bootstrap.sh` runs `npm install --silent` for the root
  and `src/ts`; `ci.yml:241` runs `npm ci`. Two different dependency
  resolutions in the same CI run.
- **Change.** `npm ci` in both bootstrap sites, with `npm install` reserved for
  intentional dependency updates.
- **Cost.** Trivial. **Risk.** `npm ci` hard-fails on a lockfile/manifest
  mismatch — that is the point, but it will surface any existing drift as a red
  bootstrap on first landing. Land it deliberately, not in a batch.

### 4.2 Bucket 2 — fail EARLIER (highest value)

The governing number: **29m47s** elapsed between the failure being printed and
the job exiting (§1.6).

**But §2.2 requires a correction to the naive framing.** 50 of 58 `verify`
failures already surface in under 9 minutes, in `lint`/`typecheck`. The
expensive tail is the **8 runs that reached `step=test`**. Sizing the prize
honestly, from §2.4:

| test-stage failure | cost | which tier? |
|---|---|---|
| `rg: command not found` ×3 | 25–37 min each | **cheap** (`test-ts-cache-coverage-split`, 49 ms) |
| collect-all SIGPIPE ×1 | 33 min | **cheap** (186 ms) |
| `steward-orient` arm 7a ×1 | 30 min | **cheap** (5.4 s) |
| `test-ts` ×2 | 25–37 min | heavy |
| Rust CLI `DatabaseLocked` ×1 | 31 min | heavy |

**5 of 8 expensive failures — 5 of 58 total — were in a suite costing under
6 seconds, and cost 25–37 minutes each.** That is roughly **2.5 hours of
wall-clock in a 5-day window** spent waiting for an answer that existed within
the first 7 minutes. That is the actual size of the prize for R2.2, and it is
smaller than "every failure takes 33 minutes" would suggest — but the failure
mode is also the *most demoralising* one, because the log has already printed
the answer.

#### R2.1 — Add a standalone `shell-lint` job — **P0, do this first**

- **Problem.** The 2026-08-04 failure was statically detectable. Nothing in CI
  looks at shell statically.
- **Change.** A job with **no `needs:` and no `if:`** — the same always-on shape
  as `ledger-integrity` / `plan-anchors`, and for the same stated reason (a
  shell defect lands on code pushes, which `docs_only` excludes):

  ```yaml
    shell-lint:
      runs-on: ubuntu-latest
      timeout-minutes: 5
      steps:
        - uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0
        - name: shellcheck (incl. SC2312 check-extra-masked-returns)
          run: |
            shellcheck --version
            git ls-files '*.sh' -z | xargs -0 shellcheck \
              --severity=style --enable=check-extra-masked-returns
  ```

  (`shellcheck` is pre-installed on the `ubuntu-latest` image; pin it via
  `bootstrap.sh` for local parity and assert the version, per the ruff/actionlint
  precedent.)
- **Cost.** ~60 seconds of CI per run, one small YAML block. Depends on R1.1/R1.2
  landing first, or it starts red.
- **Risk.** None to gate integrity — it only adds a check.
- **Value.** Turns a 33-minute failure into a 60-second one for this entire bug
  class. Best ratio in the document.

#### R2.2 — Split `verify` into a fast tier and a heavy tier, running in parallel — **P0**

- **Problem.** §1.4/§1.6. The cheap tier is 3m45s of work; it is currently
  gated behind, and reported after, 26 minutes of Rust/Python/TS.
- **Change.** Two jobs, both **required**, both running every suite between
  them:

  ```text
  verify-fast   (~7m wall)  lint -> typecheck -> security -> the 51 cheap suites
  verify-heavy  (~30m wall) test-rust, test-python, test-ts
  ```

  This needs a tier selector in `agent-test.sh`. **It must not reuse
  `--exclude-suite`**: that flag's header
  (`scripts/agent-test.sh:19-27`) states it is a demonstration/debugging flag,
  never a default and never read from the environment — using it for routine CI
  would destroy that guarantee. Propose instead an explicit
  `--tier=fast|heavy|all` with `all` the default, plus a **totality guard**:
  a new suite in `scripts/tests/` asserting that

  1. every `run_suite`/`skip_suite` registration in `agent-test.sh` belongs to
     exactly one tier, and
  2. `fast ∪ heavy == all`, with a hard fail on any unassigned label.

  Without that guard this proposal *is* the vacuous-green hazard: a suite could
  silently belong to no tier and never run in CI. **With** it, the partition is
  mechanically total, which is strictly stronger than today's single job.
- **Cost.** Moderate — a slice. Also requires updating
  `scripts/tests/test_verify_ci_timeout_budget.sh` (§1.7), which currently
  asserts a job literally named `verify:` with `timeout-minutes: 60`. Keep
  `verify` as the *heavy* job's name to minimise churn, or update the suite to
  assert the budget on the renamed job — deliberately, with the reasoning
  written into the test.
- **Risk.** ⚠ MASKING RISK, fully mitigable. The risk is a suite falling
  between tiers. The totality guard above is the mitigation and is
  non-negotiable. A secondary risk: two jobs mean two `Swatinem/rust-cache`
  restores and two bootstraps (~+3 min of billed compute), traded against
  ~26 min of *developer wall-clock* saved on every fast-tier failure.
- **Value.** A shell/lint/typecheck/governance failure surfaces at ~7 minutes
  instead of ~33, and it runs concurrently with the heavy tier so a clean run
  is no slower. Sized against the sample: 5 of 58 failures, ~2.5 hours of
  wall-clock in 5 days. **Real, but note that R2.1 alone would have caught 1 of
  those 5 (the SIGPIPE) for a fraction of the effort** — do R2.1 first and
  measure before committing to R2.2.

#### R2.3 — Emit `::error` annotations from the harness — **P1**

- **Problem.** The 2026-08-04 run produced **zero** GitHub annotations. The
  failure is discoverable only by opening a 913-line raw log and reading to the
  bottom.
- **Change.** In `suite_summary_and_exit` (`scripts/lib/agent-suite-run.sh`),
  when `GITHUB_ACTIONS` is set, emit one annotation per failed suite before
  exiting:

  ```bash
  if [ -n "${GITHUB_ACTIONS:-}" ]; then
    for l in "${failed_labels[@]}"; do
      printf '::error title=agent-test suite failed::%s\n' "$l"
    done
    printf 'FAILED SUITES: %s\n' "${failed_labels[*]}" >> "$GITHUB_STEP_SUMMARY"
  fi
  ```

  Same treatment for `run_capped`'s `FAIL` branch in
  `scripts/lib/agent-output.sh`, so the annotation appears **at the moment of
  failure** (00:45:39) rather than only at the end.
- **Cost.** ~15 lines. **Risk.** None — additive output only; the `return "$rc"`
  contract that `test_agent_test_collect_all.sh` arm B pins is untouched.

#### R2.4 — Add a `concurrency` group — **P1**

- **Problem.** There is **no `concurrency:` key in any of the five workflows**.
  A force-push or a follow-up commit leaves the superseded 33-minute `verify`
  running to completion. The 2026-08-01 run list shows long chains of runs on
  the same branch (`slice-40-e1-base`: 8 runs in 6 hours), several of which
  were cancelled manually.
- **Change.** At the top of `ci.yml`:

  ```yaml
  concurrency:
    group: ci-${{ github.workflow }}-${{ github.ref }}
    cancel-in-progress: ${{ github.event_name == 'pull_request' }}
  ```

  Scoping cancellation to `pull_request` only is the documented convention:
  never cancel on the default branch or on release workflows, where every
  commit's result must exist.
- **Cost.** 3 lines. **Risk.** Low, given the `github.ref`-scoped group and the
  main-branch carve-out. **⚠ Note:** cancelled runs report as *cancelled*, not
  *failed* — under a required-status-checks ruleset (R3.5) a cancelled check is
  not a pass, so this cannot green a bad commit.

#### R2.5 — Split build from run in the serial Rust gate — **P1**

- **Problem.** `test-rust` = 625s. `--jobs 1` serializes **compilation** as well
  as test-target scheduling, but the TC-74 invariant
  (`dev/design/temporary-serial-rust-workspace-release-gate.md` §3) is about
  *execution* concurrency, not build concurrency.
- **Change.** Inside `scripts/test-rust-workspace.sh --serial` — the canonical
  owner, so the contract test at `test_ci_rust_workspace_gate.sh:129` stays
  meaningful — build in parallel, then run serially:

  ```bash
  --serial)
    cargo test --workspace --quiet --no-fail-fast --no-run   # default -j
    exec cargo test --workspace --quiet --no-fail-fast --jobs 1 -- --test-threads=1
    ;;
  ```

- **Cost.** Small change; **must be measured** before/after, and
  `scripts/tests/test_rust_workspace_gate.sh` + `test_ci_rust_workspace_gate.sh`
  must be updated to assert the two-phase shape rather than the single line.
- **Risk.** ⚠ Requires HITL sign-off: it edits an HITL-ruled evidence control.
  The execution invariant is preserved exactly (`--jobs 1 -- --test-threads=1`
  unchanged on the run), but the design doc's §3 prose ("`--jobs 1` prevents
  Cargo from scheduling workspace test targets concurrently") must be amended to
  record that the `--no-run` prebuild is a *build* step under which no test
  executes. **Do not land this as an incidental optimisation.**
- **Value.** Unmeasured, but plausibly several minutes off the heavy tier.

#### R2.6 — Investigate `test-python` (14m40s, 49% of the gate) — **P1**

- **Problem.** The largest single cost in CI, and completely opaque:
  `run_capped` discards a passing suite's output, so the CI log for a 33-minute
  job is 913 lines.
- **Change.** A one-off diagnostic run with `AGENT_VERBOSE=1` and
  `pytest --durations=25`, then decide. Likely candidates: the `maturin develop`
  rebuild triggered by `tests/conftest.py` (TC-27, `agent-test.sh:340-360`), and
  engine-backed tests that open real databases.
- **Cost.** One diagnostic run. **Risk.** None (investigation only).
- **Note.** Do not act on this before measuring. This review states the *size*
  of the cost, not its cause.

#### R2.7 — `AGENT_VERBOSE=1` in the CI `verify` step — **P2**

- **Problem.** Per-suite timing exists **only when the run fails**. There is no
  baseline for a healthy run, which is why §1.3 cannot state the lint/typecheck
  cost on a Rust-touching PR.
- **Change.** `run: AGENT_VERBOSE=1 bash scripts/agent-verify.sh` in `ci.yml:80`.
- **Cost.** Nil. **Risk.** Log volume grows from ~900 lines to a few thousand —
  acceptable, and it makes regressions in gate cost visible instead of
  invisible.

#### R2.8 — Repair or retire `commission-manifest` arm 11d — ~~P0~~ **SUPERSEDED: already fixed**

> **✅ CLOSED — verified by the Steward 2026-08-04.** `commission-manifest` has
> passed **25 consecutive CI runs** since 2026-08-01T12:32Z, including both PRs
> merged today. The repair landed roughly three days before this review was
> written; the sample window straddles it, so the arm read as live when it was
> already closed. **Do not action this item, and do not treat it as the
> top-priority shortlist entry** — the shortlist rank is corrected in §5.
> Retained for the record and for its structural lesson only.

- **Problem** *(historical)*. §2.4F: 62 of 77 failing runs. The arm asks git history whether a
  revision of `scripts/commission-manifest.sh` predates `design_refs`; once
  every reachable revision contains it, the question is **unanswerable by
  construction** and the arm hard-fails forever. Because the job is
  (correctly) not `docs_only`-gated, every steward documentation commit reds
  `main`.
- **Change.** Two honest options, both preferable to the status quo:
  1. **Pin the additivity proof to a fixed base revision** rather than
     "the newest revision predating a token" — i.e. assert against a recorded
     SHA, so the arm's subject stops moving.
  2. **Retire the arm** with a written record of what it stopped proving, per
     the repo's own convention for a ruled non-gate (steward seq-172:
     *"RULED, NOT DEFERRED … CONSEQUENCE WORTH NAMING"*).
- **⚠ MASKING RISK.** Option 2 removes a check. It is only acceptable *with*
  the written consequence statement, and only because the arm currently proves
  nothing — a gate that can never pass is not a gate, it is noise that trains
  everyone to ignore red. **A permanently-red gate is itself a masking
  mechanism**: it is why `main` being red stopped being informative for two
  days in §2.4A.
- **Cost.** Small (option 2) to moderate (option 1). **Risk.** Low either way
  provided the choice is recorded.

#### R2.9 — Pin `pyright`, with a version-drift test — **P1**

- **Problem.** §2.4A cost ~46 runs and ~2 days of red `main`. TC-142 records
  pyright as unpinned (`>=1.1.380`), *"a supply-chain hazard on the gate that
  currently blocks publish"*. Ruff (`b25e80c4`) and actionlint (`c17b14ae`)
  are both pinned **with drift tests** (`test_agent_lint_ruff_version.sh`,
  `test_agent_lint_actionlint_version.sh`). Pyright is the odd one out.
- **Change.** Pin the version in `src/python/pyproject.toml`'s dev extra and
  add `scripts/tests/test_agent_typecheck_pyright_version.sh` mirroring the
  two existing suites exactly.
- **Cost.** ~1 hour (the pattern already exists twice). **Risk.** Low. This is
  already carried as TC-142 for 0.8.21; the evidence here argues for
  promoting it.

#### R2.10 — Implement the `PIPESTATUS`-masking tooling guard — **P2**

- **Problem.** §2.5: "background exit masks real exit" fired **twice inside one
  slice despite an explicit guard**, and once in the Steward's *own*
  verification command, wrongly reporting a red suite green. The only control
  today is a per-receipt attestation. Steward seq-109 proposed the tooling fix
  and it was never implemented.
- **Change.** As proposed at seq-109 — have `ledgerwrite` refuse or warn on
  unexpanded shell metacharacters in a summary field. Additionally, R1.1's
  shellcheck adoption covers the *code* half of this class: SC2181 flags
  `$?`-after-an-intervening-command directly.
- **Cost.** Small. **Risk.** None.
- **Why it matters beyond CI.** This is the repo's own
  `guardrail-failures-fix-tooling-not-people` rule applied to itself: the
  current control is a be-careful note, and the note has already failed three
  times.

### 4.3 Bucket 3 — other options and best practices

#### R3.1 — Path-based selective execution: **recommend AGAINST beyond what exists** — **P3**

- **Current state.** The `changes` job computes exactly one boolean, `docs_only`
  (`ci.yml:24-40`), from a single `'!**/*.md'` filter. Its own comment states
  the bias correctly: "a misread can only make heavy jobs RUN on a docs change
  (slower), never SKIP on a code change." That is a **safe** use of path
  filtering, and it is used well.
- **The tempting extension** — per-language filters that skip `test-rust` when
  no `.rs` changed, skip `test-python` when no `.py` changed — would cut typical
  PR time dramatically.
- **⚠ MASKING RISK — why this review does not recommend it.** The repo's history
  is a catalogue of gates that were absent on exactly the push that broke them:
  `ci.yml:392-395` (the plan-anchors carve-out), `:439-451`, `:462-473`,
  `:490-502` all carry comments reasoning that a gate which cannot run on the
  push that invalidates it "is decorative". Cross-language coupling here is
  real and non-obvious — a Rust change breaks the Python binding; a
  `Cargo.lock` bump moves the wheel; a shell-script change alters a gate that a
  Rust test asserts. A path filter encodes a dependency graph that nobody
  maintains, and its failure mode is a **silent green**.
- **The narrow exception worth taking.** Use path filters only to *add* work,
  never to remove it — e.g. `8d936335 ci: run aarch64 preflight on native
  changes` is exactly this shape and is fine. If selective execution is ever
  pursued for the heavy tier, gate it on `github.ref != 'refs/heads/main'` so
  the full matrix always runs on the trunk, and pair it with an explicit
  "skipped because" line in `$GITHUB_STEP_SUMMARY` so a skip is never silent.

#### R3.2 — Cache correctness — **P2**

- **Problem A (verified).** On the failing run 30866386557 the step list shows
  `Post Run Swatinem/rust-cache@… skipped`. `Swatinem/rust-cache` saves only on
  success by default, so **a failing `verify` throws away the compile it just
  paid for**, and a retry after a one-line fix recompiles from the same stale
  baseline. With a 30-minute job and a 33-minute mean-time-to-failure, this
  compounds.
- **Change.** Set `save-if` explicitly rather than relying on the default, e.g.
  `save-if: ${{ github.ref == 'refs/heads/main' }}` for the canonical baseline,
  and consider allowing the fast tier to save unconditionally (it is cheap and
  its cache is small).
- **Problem B (VERIFIED in CI, UNRECORDED in the repo).** Run 30866386557, job
  91859096405, cache-restore step:

  ```text
  ##[error]ENOENT: no such file or directory, opendir
    '/home/runner/work/fathomdb/fathomdb/target/tests/target'
  ```

  It is annotated `##[error]` but is **non-fatal** — the job continued and
  failed later for an unrelated reason. A full sweep of git history, `dev/`,
  `scripts/` and `.github/` found **no record of it anywhere in the repo**, so
  it has never been triaged. The signature (a nested `target/` under
  `target/tests/`) points at a test fixture that builds a Cargo project inside
  `target/` — worth locating, because an `##[error]` that everyone has learned
  to ignore is exactly how a real cache corruption would go unnoticed.
- **Cost/Risk.** Small / low. Investigation first.

#### R3.3 — Retries: infrastructure only, never assertions — **P2**

- **Recommendation.** Retry **only** provably-idempotent environment steps, and
  only where a transient failure is externally caused:
  `npm ci`, `pip install`, `cargo install lychee`, the HF weight download in
  `default-embedder-tests`, and `actions/checkout`. Never `cargo test`, never
  `pytest`, never any `run_suite`.
- **Argument.** A retry around a test run converts a reproducible-1-in-3 signal
  into a reproducible-1-in-9 signal and calls it green. That is the definition
  of masking. The literature is consistent that retries mask instability while
  increasing pipeline cost, and that flakiness is usually driven by shared
  state, parallelism and environment — the very things this repo has been
  fixing directly (TC-74, `84e5c9db`). Retrying would have hidden TC-72's
  one-in-three failure instead of surfacing it. This repo's existing handling of
  that case is better than any retry policy.
- **Concrete shape.** Step-level only, and visible:

  ```yaml
        - name: Install locked TypeScript test dependencies
          uses: nick-fields/retry@<pinned-sha>
          with: { timeout_minutes: 5, max_attempts: 3, command: "cd src/ts && npm ci" }
  ```

  A retried step must still print each attempt's failure, so a *persistent*
  infra failure is not smoothed into an invisible slow success.
- **Cost/Risk.** Small / low, given the strict scope.

#### R3.4 — Flake quarantine: **use the existing pattern, do not adopt a generic tool** — **P3**

- **Problem.** Generic flake-quarantine tooling (auto-detect, auto-mute, track
  in a dashboard) is the standard industry answer and is a **poor fit here**.
  Its core move — a test that fails is demoted to non-blocking automatically —
  is exactly the vacuous-green this repo built TC-37 guards, the collect-all
  harness, and `test_lint_md_hard_fail_on_missing_linter.sh` to prevent.
- **Recommendation.** The repo already has the right pattern and should name it
  as the standard: **`rust-workspace-race-report`**. Run the suspect
  configuration, capture rc and duration into `$GITHUB_STEP_SUMMARY`, emit a
  `::warning`, upload the log as an artifact, `exit 0` — and require a written
  design doc (`dev/design/temporary-serial-rust-workspace-release-gate.md`)
  plus an HITL ruling saying *why* it does not gate and *when* it will. That is
  quarantine with a name, an owner, and an expiry. A tool that quarantines
  automatically has none of those.
- **Cost/Risk.** Nil / nil — it is a documentation of existing practice.

#### R3.5 — Required status checks under the ruleset — **P0, and currently a gap**

- **Problem (verified).** `gh api repos/coreyt/fathomdb/rulesets/20166133`
  returns a single active ruleset on `~DEFAULT_BRANCH` whose **only** rule is:

  ```json
  {"type": "pull_request", "parameters": {
     "required_approving_review_count": 0,
     "allowed_merge_methods": ["merge","squash","rebase"]}}
  ```

  There is **no `required_status_checks` rule at all**. Branch protection today
  enforces "go through a PR" and nothing else — **a PR with a red CI can be
  merged**. Given everything else in this repo is built to make failures
  un-hideable, this is the largest single governance gap found.
- **Change.** Add a `required_status_checks` rule naming the checks that must
  be green. Suggested required set (all currently exist and are reliable):
  `changes`, `verify`, `security`, `docs`, `board-currency`,
  `ledger-integrity`, `plan-anchors`, `governed-surface-pin`,
  `c1-contract-conformance`, `transcript-hygiene`, `release-state-views`,
  `commission-manifest`, `design-status`, `steward-orient`, plus `shell-lint`
  and the fast tier once R2.1/R2.2 land. **Explicitly not required:**
  `rust-workspace-race-report` (by design — TC-74 says it reports, never
  gates).
- **⚠ Skipped-job caveat.** `verify`, `security`, `default-embedder-tests` and
  `wheel-size-gate` are `if: docs_only != 'true'`, so on a docs-only PR they
  report **skipped**. GitHub treats a skipped required check as *passing* — which
  is the intended fast-path behaviour here, but it must be a conscious decision,
  written down, not a surprise. `markdownlint` is the complementary check on
  that path and should be required too.
- **Cost.** A ruleset edit. **Risk.** Low, and it removes a real hole. Requires
  a token with admin scope — the current PAT returned 403 on
  `/branches/main/protection`, so this is an HITL action.

#### R3.6 — Merge queue: **defer** — **P3**

- **Argument.** A merge queue solves "PR was green against a stale base and
  broke main on merge". That is a real risk here (`30699348159`,
  `30715857868`, `30680493993` are all failures on `main` after merges), but
  the fix costs: every workflow must add a `merge_group` trigger, job names must
  be unique across workflows, and each queued PR re-runs the full 33-minute
  matrix — turning today's serial merge cost into a queue-depth multiplier at a
  PR volume (single maintainer, a handful of open PRs) that does not need it.
- **Cheaper substitute.** Require branches to be up to date before merging (a
  ruleset toggle alongside R3.5), which forces a rebase-and-rerun only when the
  base has actually moved. Revisit the merge queue if concurrent PR volume grows.

#### R3.7 — Observability: artifacts and step summaries on failure — **P1** (raised on evidence)

- **Problem.** `run_capped` writes a full spill log to
  `/tmp/fathomdb-agent-<verb>-$$.log` and prints only the first 200 lines
  (`scripts/lib/agent-output.sh:52`). In CI that file dies with the runner —
  the footer prints a path that no longer exists by the time anyone reads it.
- **This has already destroyed evidence twice.** §2.4D: runs 30706491146 and
  30699770006 both failed `test-ts` after 25–37 minutes with
  `output truncated (1593 lines total); full log: /tmp/fathomdb-agent-test-ts-7192.log`
  and **no `not ok` / TAP failure line anywhere in the CI log**. Those two
  failures are not root-causable from the record at all. Three ruff failures
  (`output truncated (9638 lines total)`) are in the same state. This is not a
  convenience issue — it is a class of CI failure the repo **cannot
  investigate**, which is why this moves from P2 to P1.
- **Change.** In the `verify` job, add `if: failure()` steps that (a) copy
  `/tmp/fathomdb-agent-*.log` into `$RUNNER_TEMP` and upload them with
  `actions/upload-artifact` (the `rust-workspace-race-report` job already does
  exactly this, `ci.yml:124-129`), and (b) append the collect-all summary table
  to `$GITHUB_STEP_SUMMARY` so the failing-suite list is on the run's front page.
- **Cost.** ~10 lines of YAML. **Risk.** ⚠ Check the artifacts against
  `scripts/check-transcript-hygiene.sh`'s concern before enabling: spill logs
  are machine output and could carry absolute paths. Artifacts are not tracked
  files, so the gate does not apply, but the same care is warranted.

### 4.4 Prioritized, sequenced shortlist

Ordered by (wasted-CI-minutes removed) ÷ (effort × risk).

> **⚠ RANK 1 IS SUPERSEDED — corrected by the Steward 2026-08-04.** Arm 11d was
> already repaired on 2026-08-01 and has passed 25 consecutive runs. It is struck
> below and the list re-ranks accordingly: **the shell-fragility chain (old #2→#4)
> is now the top of the list**, which is also where the evidence points — that is
> the class that produced the 2026-08-04 failure this review was commissioned for,
> and the only open item the repo's TC register does not already track.

| # | Action | Ref | Effort | Buys | Evidence |
|---|---|---|---|---|---|
| ~~—~~ | ~~Repair or retire `commission-manifest` arm 11d~~ **CLOSED 2026-08-01; 25 runs green** | R2.8 | — | — | §2.4F |
| **1** | Fix the 15 audited SIGPIPE / fail-open sites | R1.2 | ~1 h | Removes 3 live fail-open guards; unblocks #2 | §3.1.2 |
| **2** | Add `shellcheck` to `agent-lint.sh` + `.shellcheckrc`, `check-extra-masked-returns` ON | R1.1 | 1 slice | Kills the bug class at authoring time. **The single most important item.** | §3.1, §2.5 |
| **3** | Add the always-on `shell-lint` CI job | R2.1 | ~30 min | 33-minute failure → 60-second failure for this class | §1.6 |
| **4** | Add `required_status_checks` to the ruleset | R3.5 | ~15 min (HITL) | Closes the "red PR can be merged" hole | §4.3 R3.5 |
| 6 | Upload spill logs + summary on failure | R3.7 | ~10 lines YAML | Two failures in the sample are **not root-causable at all** without this | §2.4D |
| 7 | Emit `::error` annotations from the harness | R2.3 | ~15 lines | Failure visible without log-diving | §1.6 |
| 8 | Pin `pyright` with a drift test | R2.9 | ~1 h | The class that red-lined `main` for ~2 days | §2.4A |
| 9 | Add the `concurrency` group | R2.4 | ~5 min | Stops superseded 33-minute runs | §4.2 R2.4 |
| 10 | `AGENT_VERBOSE=1` in CI | R2.7 | 1 line | A baseline for every future perf claim | §1.3 |
| 11 | Split `verify` into fast + heavy tiers, **with the totality guard** | R2.2 | 1 slice | ~2.5 h of wall-clock per 5-day window — measure after #4 | §4.2 R2.2 |
| 12 | Measure `test-python` (49% of the gate) | R2.6 | 1 run | Data, not a fix | §1.4 |
| 13 | Two-phase Rust build/run (**HITL sign-off required**) | R2.5 | 1 slice | Minutes off the heavy tier | §3.2 |
| 14 | `shellcheck` in `pre-push`; delete the orphan hook; `ledgerwrite` metachar guard | R1.3, R1.4, R2.10 | ~1 h | Shifts detection further left; closes a 3-time-repeated masking class | §2.5 |
| 15 | `npm ci` in bootstrap; `--like-ci` mode | R1.6, R1.5 | 1 slice | Toolchain-drift class (**not** the SIGPIPE class) | §3.3 |

**Explicitly not recommended:** per-language path-based skipping of heavy suites
(R3.1), generic flake-quarantine tooling (R3.4), test-level retries (R3.3), and
a merge queue at current PR volume (R3.6).

**If only four things are done: 1, 3, 4, 5.**

- **#1** is the biggest single lever on wasted attention, not wasted minutes: a
  gate that can never pass has red-lined `main` on 62 of 77 failing runs, and a
  permanently-red trunk is itself a masking mechanism — it is exactly how seven
  pyright errors survived two days unnoticed (§2.4A).
- **#3 and #4** together convert the entire SIGPIPE/masked-return class from a
  33-minute post-hoc discovery into a sub-second local one, in a repo where the
  class has now recurred **three times** and where the tooling gap is the one
  piece of debt the otherwise exhaustive incident register does not track.
- **#5** makes any CI result actually binding. Everything else in this
  document is optimisation of a signal that, today, nothing is required to obey.

---

## 5. Limits of this review

- **Sample.** 100 CI runs, **2026-07-30 → 2026-08-04 only** — a window dominated
  by the 0.8.20 Slice-40 CI-readiness campaign and the v0.8.20 registry-recovery
  incident. The 77% failure rate is a property of that campaign, **not** of
  steady state. Older runs are outside Actions' log retention. 20 of 77 failing
  runs had their raw logs read in full; the rest were classified by job name +
  duration signature, which is stated per group in §2.4.
- **Timing.** Per-suite timings come from **one** run (30866386557). Per-job
  durations: `verify` success n=12 (median 1900 s), failure n=58 (median 251 s).
- **Verified during this review, correcting the brief.** The rust-cache
  `ENOENT … opendir target/tests/target` error **is real** — run 30866386557,
  job 91859096405 — and has no record in the repo (R3.2). `rg: command not
  found` **is real** and cost three runs of 25–37 minutes each (§2.4B); one
  site was fixed at `c6e16949`, another remains at
  `test_check_release_state_views.sh:882`.
- **Not measured.** The cost of `agent-lint.sh` / `agent-typecheck.sh` on a PR
  that forces a full Rust rebuild — the happy path emits no timing at all
  (which is why R2.7 exists). The cost breakdown inside `test-python`
  (49% of the gate) — R2.6 is a request to measure, not a conclusion.
- **Stated but unconfirmed.** Whether TC-141 ("no full-workspace cargo test on
  Linux in ci.yml") is superseded by the serial gate landing in `verify` — see
  §3.2. Whether `test_check_release_state_views.sh:882` has *already* been
  passing vacuously in CI; the log cannot distinguish it from a real pass.
- **`dev/plans/ci-deferred.md` is not what its title suggests.** It is
  `status: SUPERSEDED`, dated 2026-05-12, and scoped to *restoring pre-0.6.0
  workflows*. Of its three items, `release.yml` and `set-version.sh` are DONE;
  only **`benchmark-and-robustness.yml`** remains genuinely open (weekly cron;
  jobs `rust-benchmarks`, `rust-scale-tests`, `rust-tracing-stress`,
  `python-stress-tests`, `typescript-observability-harness`; its own record
  notes *"Resurrection = net-new authorship, not restoration"*). The live CI-debt
  register is **`dev/todos-and-considerations-ledger.jsonl`** (227 `TC-nn`
  entries), not this file.
- **Known-open items adjacent to this review, from the register:** TC-142
  (pyright unpinned — R2.9), TC-130 (`dev/plans/runs/**` excluded from
  markdownlint: *"Green proves nothing there"*), TC-132
  (transcript-hygiene vacuous at pre-commit), TC-129
  (`test_check_design_refs.sh` — 30 arms, *"invoked by nothing"*), TC-25
  (`sdk_only_erasure.rs` compiles to zero tests under a feature-unified run),
  TC-83 (`pgrep -f` self-match; durable helper still unwritten), and the
  standing `AGENT_LONG` per-push vacuity of the perf/recall ACs. None are
  introduced by this review; all bear on "what a green actually proves".

## Sources

- [ShellCheck SC2312 — check-extra-masked-returns](https://www.shellcheck.net/wiki/SC2312)
- [koalaman/shellcheck#2368 — SC2312 and `set -o pipefail`](https://github.com/koalaman/shellcheck/issues/2368)
- [koalaman/shellcheck#2807 — SC2312 should be warning and default](https://github.com/koalaman/shellcheck/issues/2807)
- [GitHub Docs — Control the concurrency of workflows and jobs](https://docs.github.com/actions/writing-workflows/choosing-what-your-workflow-does/control-the-concurrency-of-workflows-and-jobs)
- [GitHub Actions concurrency: cancel-in-progress explained](https://starsling.dev/best-practices/github-actions/cancel-superseded-runs)
- [GitHub Docs — Managing a merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)
- [GitHub Docs — About protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches)
- [Evil Martians — Flaky tests, be gone](https://evilmartians.com/chronicles/flaky-tests-be-gone-long-lasting-relief-chronic-ci-retry-irritation)
- [Discerning Legitimate Failures From False Alerts: Chromium CI (arXiv 2111.03382)](https://arxiv.org/pdf/2111.03382)
