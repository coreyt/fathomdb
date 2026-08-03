---
status: PROPOSED
---

# Temporary serial Rust-workspace release gate

## 1. Purpose and boundary

`cargo test --workspace --no-fail-fast` is currently an unreliable release
signal when Cargo runs its test binaries concurrently. TC-29 records a
cross-binary lock-holder test that is reliable in isolation but has failed in
a full suite. TC-72 measured a different concurrency-sensitive failure in one
of three full-workspace runs on both a merged candidate and its `main`
control. The evidence therefore does not identify a candidate regression, but
it also does not make a parallel green sufficient evidence.

The HITL ruling for TC-74 is a temporary two-part control:

1. a **serial Rust-workspace run** is the release-gating result; and
2. an equivalent **parallel run** still executes and reports, but cannot fail
   the gate.

This design implements that ruling only. It neither classifies nor fixes the
underlying races, and it does not convert a parallel failure into a pass. The
parallel reporter preserves that diagnostic distinction until race-hunting is
completed in its separately scheduled work.

The scope is the Rust `cargo test --workspace` result gated by Linux `verify`.
B4 cancels the native macOS and Windows release legs for 0.8.20; they remain
deferred to 0.8.22. Python, TypeScript, lint, typecheck, security, and the
other CI jobs retain their present behaviour.

## 2. Current code and required invariant

Today `scripts/agent-test.sh` registers `test-rust` directly as:

```text
cargo test --workspace --quiet --no-fail-fast
```

`scripts/agent-verify.sh` runs `agent-test.sh` after lint, typecheck, and
security. Linux CI `verify` invokes `bash scripts/agent-verify.sh`. B4 leaves
no native macOS or Windows gate active for 0.8.20, so changing the local script
must remain sufficient to keep local and active CI evidence identical.

The invariant is:

> The command used for the gating Rust-workspace result is one canonical,
> directly executable script. `agent-test.sh`, local release evidence, and
> the active Linux gating Rust-workspace CI leg reaches that same `--serial`
> invocation.
> GitHub Actions is confirmation of already-observed local behaviour, never
> its first proof.

The serial control is an evidence control, not a claim that the database is
safe under concurrent application use. It removes Cargo/test-harness
concurrency from this particular release gate; it must not be described as a
fix for TC-29, TC-72, or any engine concurrency defect.

## 3. Canonical interface

Add `scripts/test-rust-workspace.sh` as the sole owner of the workspace Rust
test invocation. It accepts exactly one required mode:

```text
bash scripts/test-rust-workspace.sh --serial
bash scripts/test-rust-workspace.sh --parallel-report
```

No environment variable selects a mode, and no mode defaults silently. A
missing, repeated, or unknown argument prints usage and exits `2`. Explicit
arguments make a copied log or CI step self-describing and prevent an
accidental return to Cargo defaults.

Both modes run from the repository root and preserve the present test scope:

```text
cargo test --workspace --quiet --no-fail-fast
```

`--serial` additionally supplies both required serialization controls:

```text
cargo test --workspace --quiet --no-fail-fast --jobs 1 -- --test-threads=1
```

`--jobs 1` prevents Cargo from scheduling workspace test targets concurrently.
`-- --test-threads=1` prevents the Rust test harness from running tests in a
single test binary concurrently. Both are necessary; either one alone leaves
one concurrency layer active.

`--parallel-report` runs the current command without either serialization
flag. It has the normal underlying `cargo` exit code. The script itself never
suppresses a failure, retries, or re-labels it; the CI reporter is the only
consumer allowed to make that result non-gating.

`scripts/agent-test.sh` changes its `test-rust` registration to:

```text
run_suite test-rust bash scripts/test-rust-workspace.sh --serial
```

Consequently, the existing `bash scripts/agent-verify.sh` remains the single
canonical full gate command locally and in Linux CI. Its collect-all summary
records the immediate exit code of the serial Rust suite through the existing
`run_suite` wrapper.

The CI legs use the runner as follows:

| Gating leg | Required invocation |
| --- | --- |
| Linux `verify` | `agent-verify.sh` → `agent-test.sh` → `test-rust-workspace.sh --serial` |

No native macOS or Windows release leg is active in 0.8.20. The Linux-first
platform-scope test protects that B4 deferral; native release work resumes in
0.8.22.

## 4. Local-before-CI evidence protocol

Before opening or updating a pull request that changes this control or relies
on it for a release gate, run the CI command in a clean **standalone clone**
at the exact candidate commit. Do not use a linked worktree for the bootstrap:
the repository policy forbids editable Python installation from a worktree,
where it could rebind a shared virtual environment.

1. Create a fresh ordinary clone, check out the candidate commit, and run
   `bash scripts/bootstrap.sh` there.
2. Run `AGENT_VERBOSE=1 bash scripts/agent-verify.sh` three consecutive times
   from that clone. Do not substitute a direct Cargo command: the point is to
   exercise the same script CI will execute, including the serial runner.
3. For each attempt, redirect the command itself to a uniquely named log,
   immediately save its exit code in a small evidence record, and stop at the
   first non-zero exit. The recorder must not use a trailing `echo`, a pipeline
   whose final status masks Cargo, or a retry that overwrites the failed
   result.
4. Retain the three log paths, start/end timestamps, candidate SHA, and each
   immediate exit code in the implementation receipt/PR description. A zero
   only after all three attempts is local preflight evidence; it does not
   replace CI evidence.

The recording wrapper is deliberately simple in shape:

```bash
set +e
AGENT_VERBOSE=1 bash scripts/agent-verify.sh >"$log" 2>&1
rc=$?
set -e
printf 'attempt=%s rc=%s log=%s\n' "$attempt" "$rc" "$log"
test "$rc" -eq 0
```

The `rc=$?` assignment immediately follows the command under test. This is
required by the existing exit-masking incident: a succeeding diagnostic command
must never replace a failed test result.

## 5. CI wiring and diagnostics

The Linux `verify` job continues to run exactly:

```text
bash scripts/agent-verify.sh
```

Because `agent-test.sh` invokes the canonical runner with `--serial`, a failure
of the Rust-workspace suite remains a normal gate failure. No active 0.8.20
gating workflow job may retain or add a direct `cargo test --workspace`
command. No CI-only serial command is permitted.

Add a separate Linux job named `rust-workspace-race-report`. It installs the
same Rust toolchain/cache as the existing Rust test job, then runs:

```text
bash scripts/test-rust-workspace.sh --parallel-report
```

Its shell step must redirect the complete combined Cargo stdout/stderr to a
named log in `$RUNNER_TEMP`, then capture that command's exit code immediately.
It writes the mode, SHA, run attempt, duration, log name, and exit code to
`$GITHUB_STEP_SUMMARY`; on a non-zero reporter result it emits an Actions
warning that names the artifact; only then does it exit `0`. A later step
uploads that complete log under `if: always()` with the repository's pinned
action:

```text
actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a
```

The upload's path must be the exact reporter log, not a metadata file that
merely repeats its exit code. This is an explicit non-gating reporting
contract, not `continue-on-error` ambiguity and not a hidden retry.

The reporter job must have a finite timeout and use the same `--no-fail-fast`
scope as the gate. It must not share a test database, temporary directory, or
Cargo target directory with `verify`: GitHub jobs provide isolation by default,
and any later sharing optimization requires a separate concurrency review.

For a serial gate failure, the existing `run_suite`/`run_capped` path already
prints the suite label, immediate exit code, and its full spill-log location.
The CI log is the primary artifact; upload a serial-gate failure log only if
the implementation can do so without changing the command's exit semantics.

## 6. Red-first implementation tests

Write these tests before adding the runner or workflow wiring. The tests must
drive the actual scripts, using a disposable fake `cargo` earlier on `PATH`
where command arguments and exit propagation need observation; they must not
reimplement the runner in a helper.

1. Add `scripts/tests/test_rust_workspace_gate.sh`.
   - The `--serial` arm proves the actual fake-Cargo invocation contains
     `test --workspace --quiet --no-fail-fast --jobs 1 -- --test-threads=1`.
   - The `--parallel-report` arm proves neither serialization flag is present.
   - A fake Cargo non-zero exit is returned unchanged in both modes.
   - Missing, repeated, and unknown modes exit `2` before Cargo executes.
2. Extend the existing `test_agent_test_collect_all.sh` recurrence guard to
   prove `test-rust` delegates to `test-rust-workspace.sh --serial`, rather
   than retaining a direct `cargo test --workspace` registration.
3. Add a workflow recurrence test, for example
   `scripts/tests/test_ci_rust_workspace_gate.sh`, which parses the real
   `.github/workflows/ci.yml` as YAML rather than searching its text. It proves
   that `verify` still calls `agent-verify.sh`; that no active gating job
   contains a direct `cargo test --workspace` invocation; and that the deferred
   native jobs are absent. It also finds the distinct Linux reporter job,
   verifies its `--parallel-report` runner mode, and verifies an `if: always()`
   upload step uses
   `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` for the
   complete reporter log.

   The reporter-shell arm extracts the parsed `run:` body and executes that
   actual body under a controlled shell fixture: a fake runner returns a
   distinctive non-zero code and writes known stdout/stderr; `$RUNNER_TEMP` and
   `$GITHUB_STEP_SUMMARY` are disposable paths. The arm proves all of the
   following behaviours together: the body enters `set +e` before the runner;
   the summary receives the distinctive raw code and the log name; the complete
   stdout/stderr is in the named log; and the body returns `0` only after
   writing that summary. A misplaced `rc=$?`, an early `set -e`, a pipeline,
   or an early `exit 0` fails this executed control. `bash -n` remains a syntax
   check; it is not treated as proof of these semantics.
4. Register both new shell tests with `agent-test.sh` before the Rust suite so
   the collect-all harness reports their failures. Run actionlint against the
   edited workflow as part of the normal lint gate.

The initial state is red because neither the canonical runner nor the reporter
job exists. Green is the exact behavioural contract above, not merely a text
search for `--jobs 1`.

## 7. Acceptance evidence

The change is acceptable only when all of the following are retained:

- The red-first test commits/results and the focused script/workflow tests are
  green.
- The three consecutive clean-clone `agent-verify` attempts have immediate
  `0` results and readable logs on the exact candidate SHA.
- A pull-request CI run has every applicable 0.8.20 gating job green, including
  Linux `verify`; the serial Rust suite is visible in Linux's collect-all
  summary. B4 keeps native macOS and Windows release legs deferred to 0.8.22.
- The parallel reporter has executed, its artifact is retrievable, and its
  outcome is recorded separately from the gating result. A reporter red is a
  race observation to triage, not permission to retry the serial gate until it
  looks green.
- An independent review checks both the runner's actual process/exit behaviour
  and the workflow's failure-reporting semantics, rather than accepting a
  configuration-only reading.

## 8. Removal criteria

This control has no time-based expiry. Remove it only through an explicit
follow-up decision after race-hunting has classified and addressed the known
full-workspace failures, including TC-29, TC-72, and the still-unclassified
concurrent-drop observation noted with TC-74.

The removal change must first demonstrate, on the intended restored parallel
command, at least ten consecutive fresh-clone Linux passes and five consecutive
Linux CI `verify` passes, with immediate exit codes retained. It must also
demonstrate that removing the serial flags makes
the canonical runner's parallel mode the new gating mode, while deleting the
non-gating reporter rather than leaving two indistinguishable parallel jobs.

Until those conditions and the explicit decision exist, a green serial gate
means only that the candidate passed the controlled release test; it does not
close the underlying race work.
