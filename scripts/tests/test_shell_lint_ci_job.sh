#!/usr/bin/env bash
# scripts/tests/test_shell_lint_ci_job.sh — 0.8.21 Slice 35 (SHELL-LINT-CI).
# The recurrence guard for the always-on `shell-lint` CI job, its minimal setup,
# its gating (as opposed to advisory) behaviour, and the workflow's concurrency
# group. Design of record: dev/design/ci-verify-robustness-review.md R2.1/R2.4.
#
# WHAT THE JOB IS FOR. On the 2026-08-04 run the shell defect's verdict sat in
# the `verify` log at 00:45:39 while the job ran on until 01:15:26 — 29m47s of
# runner time after the answer existed. Slice 30 made `agent-verify.sh` run the
# shell gate first, but `verify` still spends ~3 minutes on job setup before any
# lint runs and is skipped outright on a docs-only PR. This job reports in about
# a minute, on every PR.
#
# WHY THESE ARMS EXIST AS TESTS RATHER THAN AS A COMMENT. Every property below
# is one somebody can delete in a one-line YAML edit while the job keeps
# LOOKING present and keeps reporting green:
#   * add `needs: changes` + `if: docs_only != 'true'` and the gate vanishes on
#     the pushes it was added for (the precedent this repo already enforces for
#     `release-state-views`, test_check_release_state_views.sh arm 9);
#   * add `continue-on-error: true`, or capture the rc and `exit 0`, and the
#     gate becomes decoration — the `rust-workspace-race-report` shape, which is
#     deliberate THERE and forbidden here;
#   * add a rust/node/python setup and the "about a minute" claim quietly
#     becomes four;
#   * set `cancel-in-progress: true` unconditionally and a post-merge run on
#     `main` can be cancelled, which reports as *cancelled* — neither green nor
#     red — leaving `main`'s status ambiguous.
#
# RED-FIRST. Arms A/B/C were RED against the baseline ci.yml at 1976f374, which
# has no `shell-lint:` job and no `concurrency:` key at all ("ci.yml has no
# shell-lint job"). Arm G is the behavioural half: it plants the Slice 25 bug
# shape (`cmd | head` under pipefail) in a throwaway repo and requires the job's
# ACTUAL command to reject it, and arm H proves that red is load-bearing by
# re-running the same fixture against a stub gate that accepts it.
#
# Isolation: fixtures are throwaway git repos under mktemp -d. Nothing here
# writes into this checkout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CI_YML="$REPO_ROOT/.github/workflows/ci.yml"
# shellcheck source=../lib/shellcheck-version.sh
. "$REPO_ROOT/scripts/lib/shellcheck-version.sh"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/* | /tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

if [ ! -f "$CI_YML" ]; then
  fail "ci.yml not found at $CI_YML"
  exit 1
fi

# ---------------------------------------------------------------------------
# The job block, extracted the same way test_check_release_state_views.sh arm 9
# extracts `release-state-views` — top-level jobs are indented two spaces, so a
# sibling job's keys can never be mistaken for this one's.
# ---------------------------------------------------------------------------
JOB_BLOCK="$(awk '
  /^  shell-lint:/ { inblock = 1; print; next }
  inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"

# The assertions below read EXECUTABLE yaml, not the explanatory comments inside
# the block: a comment that happens to say `bootstrap.sh` or `|| true` must not
# be able to fail an arm, and — more importantly — a `# needs: changes` left
# behind must not be able to pass one either.
JOB_CODE="$(grep -v '^[[:space:]]*#' <<<"$JOB_BLOCK" || true)"

# --- Arm A: the job exists -------------------------------------------------
if [ -n "$JOB_BLOCK" ]; then
  pass "ci.yml defines a shell-lint job"
else
  fail "ci.yml has no shell-lint job"
fi

# Arms B-F read the block itself. If there is no block they must FAIL, never
# report a vacuous "no if: found" pass on an empty string (TC-37).
if [ -z "$JOB_BLOCK" ]; then
  fail "arms B-F cannot be evaluated: ci.yml has no shell-lint job to inspect"
else
  # --- Arm B: it is ALWAYS-ON ------------------------------------------------
  # The property the whole slice turns on. A `needs:` would also make the gate
  # contingent on another job SUCCEEDING, so a broken `changes` job would make the
  # shell gate absent rather than red.
  if grep -qE '^\s*(if|needs):' <<<"$JOB_CODE"; then
    fail "the shell-lint job must be ALWAYS-ON (no if:/needs: gate); block: $JOB_BLOCK"
  else
    pass "the shell-lint job is always-on (no if:, no needs:, not docs_only-gated)"
  fi

  # --- Arm C: it runs the SHARED gate, not a reimplementation ----------------
  # A hand-rolled `shellcheck ...` line in YAML would drift from what
  # agent-verify.sh runs locally, and would silently lose the SC2312 ratchet and
  # the early-exiting-consumer leg — the leg that covers the bug this repo
  # actually shipped four times.
  if grep -qF 'scripts/agent-lint-shell.sh' <<<"$JOB_CODE"; then
    pass "the shell-lint job runs the SHARED scripts/agent-lint-shell.sh"
  else
    fail "the shell-lint job must invoke scripts/agent-lint-shell.sh, not a reimplementation"
  fi

  # --- Arm D: it installs the PINNED shellcheck ------------------------------
  # `ubuntu-latest` ships its own shellcheck at whatever version GitHub picked.
  # Relying on it would either hard-fail the repo's version preflight or, worse,
  # quietly redefine "green". The job must install the pin.
  if grep -qF 'scripts/install-shellcheck.sh' <<<"$JOB_CODE"; then
    pass "the shell-lint job installs the pinned shellcheck via the shared installer"
  else
    fail "the shell-lint job must install the pinned shellcheck (scripts/install-shellcheck.sh)"
  fi

  # --- Arm E: MINIMAL setup (the speed claim, asserted) ----------------------
  heavy_setup=""
  for needle in dtolnay/rust-toolchain Swatinem/rust-cache actions/setup-node \
    actions/setup-python maturin-action 'npm ci' 'scripts/bootstrap.sh'; do
    if grep -qF -- "$needle" <<<"$JOB_CODE"; then
      heavy_setup="$heavy_setup $needle"
    fi
  done
  if [ -z "$heavy_setup" ]; then
    pass "the shell-lint job installs no rust/node/python toolchain (the ~1 minute claim)"
  else
    fail "the shell-lint job pulled in heavy setup, defeating its purpose:$heavy_setup"
  fi

  # --- Arm F: it GATES — nothing may downgrade a failure ---------------------
  # `rust-workspace-race-report` is deliberately report-only (`set +e` … `exit 0`
  # + a `::warning`). That is precisely the shape this job must NOT acquire.
  masking=""
  for needle in 'continue-on-error' 'exit 0' 'set +e' '|| true' '::warning'; do
    if grep -qF -- "$needle" <<<"$JOB_CODE"; then
      masking="$masking '$needle'"
    fi
  done
  if [ -z "$masking" ]; then
    pass "the shell-lint job is a GATE: no continue-on-error / rc-swallowing / advisory exit 0"
  else
    fail "the shell-lint job carries failure-masking constructs:$masking"
  fi

  # A timeout must exist (a hung job is not a gate either), and must stay small:
  # this job's whole premise is that it answers in about a minute.
  timeout_line="$(grep -E '^\s*timeout-minutes:' <<<"$JOB_CODE" || true)"
  timeout_value="${timeout_line##*: }"
  if [ -n "$timeout_value" ] && [ "$timeout_value" -le 15 ] 2>/dev/null; then
    pass "the shell-lint job caps itself at ${timeout_value} minutes (fast-tier budget)"
  else
    fail "the shell-lint job must carry a small timeout-minutes (<= 15); found '${timeout_line:-<none>}'"
  fi
fi

# ---------------------------------------------------------------------------
# Arm G/H — the BEHAVIOURAL half. The arms above only read YAML; on their own
# they would be satisfied by a job that invokes a gate which does not gate.
# ---------------------------------------------------------------------------
FIX="$TMPROOT/repo"
REAL_SHELLCHECK="$(find_shellcheck_bin || true)"
REAL_SHELLCHECK_VERSION=""
if [ -n "$REAL_SHELLCHECK" ]; then
  REAL_SHELLCHECK_VERSION="$(read_shellcheck_version "$REAL_SHELLCHECK")"
fi

setup_gate_fixture() {
  rm -rf "$FIX"
  mkdir -p "$FIX/scripts/lib" "$TMPROOT/home/.local/bin"
  cp "$REPO_ROOT/scripts/agent-lint-shell.sh" "$FIX/scripts/agent-lint-shell.sh"
  cp "$REPO_ROOT/scripts/lib/shellcheck-version.sh" "$FIX/scripts/lib/shellcheck-version.sh"
  cp "$REPO_ROOT/scripts/lib/shell-early-consumer.sh" "$FIX/scripts/lib/shell-early-consumer.sh"
  cp "$REPO_ROOT/.shellcheckrc" "$FIX/.shellcheckrc"
  chmod +x "$FIX/scripts/agent-lint-shell.sh"
  : >"$FIX/scripts/shellcheck-sc2312-ratchet.txt"
  : >"$FIX/scripts/shell-early-consumer-ratchet.txt"
  # A file that is clean under all three legs, so a red arm below cannot be red
  # merely because the fixture has nothing to lint.
  cat >"$FIX/scripts/clean.sh" <<'CLEAN'
#!/usr/bin/env bash
set -euo pipefail
first_hit="$(grep -m1 -n 'needle' haystack.txt || true)"
printf 'first: %s\n' "$first_hit"
CLEAN
  rm -f "$TMPROOT/home/.local/bin/shellcheck"
  ln -s "$REAL_SHELLCHECK" "$TMPROOT/home/.local/bin/shellcheck"
  (
    cd "$FIX"
    git init -q
    git config user.email test@example.com
    git config user.name test
  )
}

commit_fixture() {
  (
    cd "$FIX"
    git add -A
    git commit -q -m fixture
  ) >/dev/null 2>&1 || true
}

# THE SLICE 25 BUG SHAPE, verbatim in form: a producer piped into `head` under
# `set -o pipefail`. `head` closes the pipe, the producer dies of SIGPIPE, and
# pipefail makes 141 the pipeline's status.
write_slice25_defect() {
  cat >"$FIX/scripts/defect.sh" <<'DEFECT'
#!/usr/bin/env bash
set -euo pipefail
FIRST_SUITE_LINE="$(grep -nE '^run_suite ' "$AGENT_TEST" | head -n 1 | cut -d: -f1)"
printf 'first suite at line %s\n' "$FIRST_SUITE_LINE"
DEFECT
}

# The job's command, run against the fixture. $GATE_UNDER_TEST lets arm H point
# the SAME fixture at a stub, which is how a green here is shown to be load-
# bearing rather than a script that merely exits 0.
run_job_command() {
  local gate="${1:-$FIX/scripts/agent-lint-shell.sh}"
  set +e
  OUT="$(cd "$FIX" && HOME="$TMPROOT/home" bash "$gate" 2>&1)"
  RC=$?
  set -e
}

if [ "$REAL_SHELLCHECK_VERSION" != "$SHELLCHECK_VERSION" ]; then
  # No skip: the behavioural arms cannot run without the pinned linter, and a
  # "skipped" arm here would be exactly the vacuous green Slice 30 abolished.
  fail "shellcheck $SHELLCHECK_VERSION is required for arms G/H; found '${REAL_SHELLCHECK_VERSION:-<none>}'. Run scripts/install-shellcheck.sh."
else
  # --- Arm G0: the clean fixture PASSES (non-vacuity of arm G) -------------
  setup_gate_fixture
  commit_fixture
  run_job_command
  if [ "$RC" -eq 0 ]; then
    pass "arm G0: the job's command passes a clean tree (so arm G's red is about the defect)"
  else
    fail "arm G0 (clean tree): rc=$RC out=$OUT"
  fi

  # --- Arm G: the job's command REJECTS the Slice 25 shape ----------------
  write_slice25_defect
  commit_fixture
  run_job_command
  if [ "$RC" -ne 0 ] \
    && grep -qF 'early-exiting consumer' <<<"$OUT" \
    && grep -qF 'scripts/defect.sh' <<<"$OUT" \
    && grep -qF 'head -n 1' <<<"$OUT"; then
    pass "arm G: the job's command REJECTS \`cmd | head\` under pipefail and names the site"
  else
    fail "arm G (rejects the Slice 25 shape): rc=$RC out=$OUT"
  fi

  # --- Arm H: that red is load-bearing (mutant proof) ---------------------
  # Point the same fixture at a gate that does nothing. It accepts the defect —
  # which is what proves arm G measured the real gate and not the fixture.
  cat >"$TMPROOT/stub-gate.sh" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB
  run_job_command "$TMPROOT/stub-gate.sh"
  if [ "$RC" -eq 0 ]; then
    pass "arm H: a stub gate ACCEPTS the same defect — arm G's red comes from the real gate"
  else
    fail "arm H (mutant proof): the stub gate did not exit 0 (rc=$RC), so arm G proves nothing"
  fi

  # --- Arm I: the PIN wins over an image copy on PATH ---------------------
  # `ubuntu-latest` ships its own shellcheck. Put a wrong-version one FIRST on
  # PATH and require the resolver to still select the pinned binary in
  # ~/.local/bin. This is how the version mismatch is handled deliberately —
  # not by relaxing the pin.
  mkdir -p "$TMPROOT/image-bin"
  cat >"$TMPROOT/image-bin/shellcheck" <<'IMAGE'
#!/usr/bin/env bash
printf 'ShellCheck - shell script analysis tool\nversion: 0.9.0\n'
IMAGE
  chmod +x "$TMPROOT/image-bin/shellcheck"
  set +e
  RESOLVED="$(HOME="$TMPROOT/home" PATH="$TMPROOT/image-bin:$PATH" \
    bash -c '. "$1"/scripts/lib/shellcheck-version.sh; require_shellcheck_bin shell-lint' \
    _ "$REPO_ROOT" 2>&1)"
  RESOLVED_RC=$?
  set -e
  if [ "$RESOLVED_RC" -eq 0 ] && [ "$RESOLVED" = "$TMPROOT/home/.local/bin/shellcheck" ]; then
    pass "arm I: the pinned ~/.local/bin binary wins over a wrong-version shellcheck on PATH"
  else
    fail "arm I (pin beats the image copy): rc=$RESOLVED_RC resolved='$RESOLVED'"
  fi
fi

# ---------------------------------------------------------------------------
# Arm J — the installer the job depends on.
# ---------------------------------------------------------------------------
INSTALLER="$REPO_ROOT/scripts/install-shellcheck.sh"
if [ -f "$INSTALLER" ]; then
  pass "arm J: scripts/install-shellcheck.sh exists"
else
  fail "arm J: scripts/install-shellcheck.sh is missing — the job cannot install its linter"
fi

# ONE installer: bootstrap.sh must call it rather than keep a second copy, so a
# pin bump cannot leave CI and local dev on different linters.
if grep -qF 'install-shellcheck.sh' "$REPO_ROOT/scripts/bootstrap.sh"; then
  pass "arm J: scripts/bootstrap.sh delegates to the SAME installer (one pin, one installer)"
else
  fail "arm J: bootstrap.sh must call scripts/install-shellcheck.sh, not carry a second copy"
fi

# The installer must read the shared pin and verify the result — no
# "shellcheck already present, close enough" branch.
if grep -qF 'lib/shellcheck-version.sh' "$INSTALLER" \
  && grep -qF 'require_shellcheck_bin' "$INSTALLER"; then
  pass "arm J: the installer uses the shared pin and verifies its own post-condition"
else
  fail "arm J: the installer must source lib/shellcheck-version.sh and verify via require_shellcheck_bin"
fi

# The cache has to be keyed by the installer AND shared pin, otherwise a pin or
# checksum update could reuse an archive selected under an older trust record.
# The installer re-verifies the SHA on every hit; this arm proves CI actually
# persists that verified archive between otherwise-ephemeral runners.
if grep -qF 'actions/cache@' <<<"$JOB_CODE" \
  && grep -qF '/.cache/fathomdb/shellcheck' <<<"$JOB_CODE" \
  && grep -qF "hashFiles('scripts/lib/shellcheck-version.sh', 'scripts/install-shellcheck.sh')" <<<"$JOB_CODE"; then
  pass "arm J: shell-lint caches the version/checksum-keyed archive"
else
  fail "arm J: shell-lint must cache the ShellCheck archive with a key over the pin and installer checksums"
fi

# A cache miss must fail promptly instead of spending minutes retrying a linter
# download before the actual gate starts. The installer test covers hit/miss/
# corrupt behaviour; this arm pins the production bounded-transfer controls.
if grep -qF -- '--connect-timeout 10' "$INSTALLER" \
  && grep -qF -- '--max-time 60' "$INSTALLER" \
  && ! grep -qF -- '--retry' "$INSTALLER"; then
  pass "arm J: ShellCheck download has bounded connect/transfer time and no retry loop"
else
  fail "arm J: ShellCheck installer must use bounded curl timeouts without a retry loop"
fi

# ---------------------------------------------------------------------------
# Arm K — the workflow-level concurrency group (R2.4).
# ---------------------------------------------------------------------------
CONCURRENCY_BLOCK="$(awk '
  /^concurrency:/ { inblock = 1; print; next }
  inblock && /^[A-Za-z]/ { inblock = 0 }
  inblock { print }
' "$CI_YML")"

if [ -n "$CONCURRENCY_BLOCK" ]; then
  pass "arm K: ci.yml declares a workflow-level concurrency group"
else
  fail "arm K: ci.yml has no concurrency: block — a superseded push still burns a full ~35-minute run"
fi

if grep -qF 'github.workflow' <<<"$CONCURRENCY_BLOCK" \
  && grep -qF 'github.ref' <<<"$CONCURRENCY_BLOCK"; then
  pass "arm K: the group is scoped per workflow AND per ref (one branch cannot cancel another)"
else
  fail "arm K: the concurrency group must key on github.workflow and github.ref: $CONCURRENCY_BLOCK"
fi

# The `main` carve-out. A cancelled run reports as *cancelled* — neither green
# nor red — so cancelling a post-merge run would leave `main` ambiguous.
cancel_line="$(grep -E '^\s*cancel-in-progress:' <<<"$CONCURRENCY_BLOCK" || true)"
if grep -qF '${{' <<<"$cancel_line" && grep -qE 'refs/heads/main|pull_request' <<<"$cancel_line"; then
  pass "arm K: cancellation is conditional and carves out main (a cancelled run is not a pass)"
elif grep -qE 'cancel-in-progress:\s*true\s*$' <<<"$cancel_line"; then
  fail "arm K: cancel-in-progress is unconditionally true — a post-merge run on main could be cancelled, leaving main's status ambiguous"
else
  fail "arm K: cancel-in-progress must be a main-carving expression; found '${cancel_line:-<none>}'"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll shell-lint CI job tests passed\n'
