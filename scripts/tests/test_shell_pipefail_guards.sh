#!/usr/bin/env bash
# scripts/tests/test_shell_pipefail_guards.sh — 0.8.21 Slice 25 (SHELL-FIX).
#
# ---------------------------------------------------------------------------
# THE BUG CLASS THIS PINS
# ---------------------------------------------------------------------------
# Under `set -euo pipefail`, `producer | consumer` where the CONSUMER exits
# early (`grep -q`, `head -n1`) closes the read end while the producer is still
# writing. The producer dies on SIGPIPE (rc 141) and `pipefail` makes that the
# rc of the whole pipeline. Whether it happens depends on how much the producer
# had left to write when the consumer left — i.e. on output volume — which is
# why these sites pass locally and fail in CI. `dev/design/
# ci-verify-robustness-review.md` §3.1.1 records three separate occurrences of
# exactly this, each fixed by hand at the site.
#
# The dangerous sub-case, and the one every arm below covers, is a POISONED
# GUARD: `if producer | grep -q .; then <refuse>; fi`. `set -e` is suspended
# inside an `if` condition, so the SIGPIPE rc does not abort — it silently
# evaluates the condition to FALSE and the guard is skipped. The gate then
# fails OPEN, and it does so *exactly when it should have fired*, because the
# producer only has enough output to lose the race when there is something to
# report. An abort is loud and safe; a fail-open is silent and wrong.
#
# ---------------------------------------------------------------------------
# WHY THESE ARMS ARE DETERMINISTIC AND NOT RACES
# ---------------------------------------------------------------------------
# The race is only a race because the producers at the real sites are normally
# smaller than the 64KB pipe buffer, so they usually finish before the consumer
# leaves. Each arm therefore substitutes a PATH shim for the producer command
# only (`git ls-files --unmerged`, `curl`) that writes MEGABYTES. A producer
# with 6MB still to write cannot possibly finish inside a 64KB buffer, so
# SIGPIPE is certain rather than likely. Everything else in the script under
# test is the real thing, running the real code path.
#
# So each arm is red on the pre-fix code (the guard fails open) and green after
# (the guard fires), and neither direction depends on timing.
#
# Arm 4 covers the adjacent finding in §3.1.3: an undeclared `rg` dependency
# inside `scripts/tests/test_check_release_state_views.sh`, where `! rg -q …`
# turns rg's 127 into `true` and reports `pass` without ever reading the file —
# a TC-37 vacuous pass. The shim there records that rg was consulted at all.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

WORK="$(mktemp -d)"
cleanup() {
  case "$WORK" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$WORK" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$WORK" >&2 ;;
  esac
}
trap cleanup EXIT

REAL_GIT="$(command -v git)"
REAL_AWK="$(command -v awk)"
if [ -z "$REAL_GIT" ] || [ -z "$REAL_AWK" ]; then
  printf 'FAIL  git and awk are required to run this test\n' >&2
  exit 1
fi

# A `git` shim that answers `ls-files --unmerged` with ~6MB of plausible
# unmerged-index lines and delegates every other subcommand to the real git.
# 6MB against a 64KB pipe buffer makes the SIGPIPE deterministic.
make_git_shim() {
  local dir="$1"
  mkdir -p "$dir"
  cat >"$dir/git" <<SHIM
#!/usr/bin/env bash
_unmerged=0
_lsfiles=0
for _a in "\$@"; do
  case "\$_a" in
    ls-files) _lsfiles=1 ;;
    --unmerged) _unmerged=1 ;;
  esac
done
if [ "\$_lsfiles" -eq 1 ] && [ "\$_unmerged" -eq 1 ]; then
  exec "$REAL_AWK" 'BEGIN { for (i = 0; i < 120000; i++) printf "100644 %040d 1\tdev/ledgers/conflicted-%d.jsonl\n", i, i }'
fi
exec "$REAL_GIT" "\$@"
SHIM
  chmod +x "$dir/git"
}

# ---------------------------------------------------------------------------
# Arm 1 — check-design-refs.sh --staged-only must WARN on an unmerged index.
# ---------------------------------------------------------------------------
# §3.1.2 P0: `if git ls-files --unmerged | grep -q .` guards the
# "NOT CHECKED — the index has UNMERGED paths" warning. Fail-open here reports
# nothing and lets an UNVERIFIED commit look checked.
arm1() {
  local sb="$WORK/a1" bin="$WORK/a1-bin" out rc
  mkdir -p "$sb/dev/design"
  "$REAL_GIT" -C "$sb" init -q
  "$REAL_GIT" -C "$sb" config user.email t@example.com
  "$REAL_GIT" -C "$sb" config user.name t
  # The real scripts dir, so a fail-open would carry on into the real sweep
  # rather than tripping over a missing tool — the arm must be red for the
  # reason it names.
  ln -s "$REPO_ROOT/scripts" "$sb/scripts"
  printf '# fixture design doc\n' >"$sb/dev/design/fixture.md"
  "$REAL_GIT" -C "$sb" add dev/design/fixture.md
  make_git_shim "$bin"

  set +e
  out="$(cd "$sb" && PATH="$bin:$PATH" bash "$REPO_ROOT/scripts/check-design-refs.sh" --staged-only 2>&1)"
  rc=$?
  set -e

  if grep -qF 'the index has UNMERGED paths' <<<"$out"; then
    pass "arm 1: check-design-refs.sh reports the unmerged index even when the producer is SIGPIPEd"
  else
    fail "arm 1 (FAIL-OPEN): check-design-refs.sh skipped its unmerged-index warning (rc=$rc). Output: $out"
  fi
}

# ---------------------------------------------------------------------------
# Arm 2 — check-staged-ledger-sidecars.sh must REFUSE on an unmerged pair.
# ---------------------------------------------------------------------------
# §3.1.2 P0, and the worst of the three: this guard fronts a hard `exit 2` and
# runs on EVERY commit via scripts/hooks/pre-commit:24. Failing open clears a
# commit whose ledger content the gate could not read — the precise scenario
# the gate's own header says it exists for.
arm2() {
  local sb="$WORK/a2" bin="$WORK/a2-bin" out rc
  mkdir -p "$sb/dev/ledgers"
  "$REAL_GIT" -C "$sb" init -q
  "$REAL_GIT" -C "$sb" config user.email t@example.com
  "$REAL_GIT" -C "$sb" config user.name t
  # The gate resolves its shared predicate as "$TOPLEVEL/scripts/check-ledgers.sh".
  ln -s "$REPO_ROOT/scripts" "$sb/scripts"
  printf '{"seq":1}\n' >"$sb/dev/ledgers/fixture.jsonl"
  printf '1\n' >"$sb/dev/ledgers/fixture.jsonl.seq"
  "$REAL_GIT" -C "$sb" add dev/ledgers/fixture.jsonl dev/ledgers/fixture.jsonl.seq
  make_git_shim "$bin"

  set +e
  out="$(cd "$sb" && PATH="$bin:$PATH" bash "$REPO_ROOT/scripts/check-staged-ledger-sidecars.sh" 2>&1)"
  rc=$?
  set -e

  if grep -qF 'are UNMERGED' <<<"$out" && [ "$rc" -eq 2 ]; then
    pass "arm 2: check-staged-ledger-sidecars.sh refuses (exit 2) on an unmerged pair even when the producer is SIGPIPEd"
  else
    fail "arm 2 (FAIL-OPEN): the TC-88 sidecar gate did not refuse an unmerged pair (rc=$rc). Output: $out"
  fi
}

# ---------------------------------------------------------------------------
# Arm 3 — publish-rc1-bootstrap.sh must honour its idempotency SKIP.
# ---------------------------------------------------------------------------
# §3.1.2 P0, real-money path: `curl -fsS "$url" | grep -qF '"vers":"…"'`. On
# SIGPIPE the SKIP is bypassed and `cargo publish` re-runs against a version
# that is already on crates.io. The `cargo` shim records any invocation; the
# `sleep` shim keeps a red run from taking 60s per tier.
arm3() {
  local bin="$WORK/a3-bin" marker="$WORK/a3-cargo-invoked" out rc skips
  mkdir -p "$bin"
  cat >"$bin/curl" <<SHIM
#!/usr/bin/env bash
# ~7MB of sparse-index JSON whose FIRST line already carries the version the
# caller greps for, so the consumer leaves immediately and the rest is a write
# into a closed pipe.
exec "$REAL_AWK" 'BEGIN {
  printf "{\"name\":\"fathomdb\",\"vers\":\"0.6.0-rc.1\",\"deps\":[]}\n"
  for (i = 0; i < 120000; i++) printf "{\"name\":\"fathomdb\",\"vers\":\"0.0.%d\",\"deps\":[],\"cksum\":\"%040d\"}\n", i, i
}'
SHIM
  cat >"$bin/cargo" <<SHIM
#!/usr/bin/env bash
printf 'CARGO-INVOKED %s\n' "\$*" >>"$marker"
exit 0
SHIM
  printf '#!/usr/bin/env bash\nexit 0\n' >"$bin/sleep"
  chmod +x "$bin/curl" "$bin/cargo" "$bin/sleep"

  set +e
  out="$(PATH="$bin:$PATH" CARGO_REGISTRY_TOKEN=fixture-token \
    bash "$REPO_ROOT/scripts/release/publish-rc1-bootstrap.sh" 2>&1)"
  rc=$?
  set -e
  skips="$(grep -c '^SKIP ' <<<"$out" || true)"

  if [ -e "$marker" ]; then
    fail "arm 3 (FAIL-OPEN, real money): the idempotency SKIP was bypassed and cargo publish ran: $(cat "$marker")"
  elif [ "$rc" -eq 0 ] && [ "$skips" -eq 7 ]; then
    pass "arm 3: publish-rc1-bootstrap.sh skips all 7 already-published tiers even when the producer is SIGPIPEd"
  else
    fail "arm 3: publish-rc1-bootstrap.sh did not report 7 SKIPs (rc=$rc, skips=$skips). Output: $out"
  fi
}

# ---------------------------------------------------------------------------
# Arm 4 — test_check_release_state_views.sh must not consult rg at all.
# ---------------------------------------------------------------------------
# §3.1.3 NEW OPEN DEFECT. `! rg -q <marker> <plan>` reports `pass` on rg's 127
# without reading the file. rg-absence is a KNOWN hazard here (§2.4B: three CI
# runs lost to `rg: command not found`), and this direction is worse than that
# incident — that one failed loudly and wrongly, this one passes silently and
# wrongly. The shim records any consultation of rg and returns 127, exactly as
# an absent rg would.
arm4() {
  local bin="$WORK/a4-bin" marker="$WORK/a4-rg-consulted" out rc
  mkdir -p "$bin"
  cat >"$bin/rg" <<SHIM
#!/usr/bin/env bash
printf 'RG-CONSULTED %s\n' "\$*" >>"$marker"
exit 127
SHIM
  chmod +x "$bin/rg"

  set +e
  out="$(PATH="$bin:$PATH" bash "$REPO_ROOT/scripts/tests/test_check_release_state_views.sh" 2>&1)"
  rc=$?
  set -e

  if [ -e "$marker" ]; then
    fail "arm 4 (VACUOUS PASS): test_check_release_state_views.sh consulted an undeclared rg: $(cat "$marker")"
  elif [ "$rc" -eq 0 ]; then
    pass "arm 4: test_check_release_state_views.sh reaches its verdict without any rg dependency"
  else
    fail "arm 4: test_check_release_state_views.sh failed (rc=$rc) with rg unavailable. Output tail: $(tail -20 <<<"$out")"
  fi
}

# ---------------------------------------------------------------------------
# Arm 5 — STATIC recurrence guard for the sites that cannot be raced on demand.
# ---------------------------------------------------------------------------
# The P1/P2/P3 sites in §3.1.2 are value-producing pipelines, not guards: their
# failure mode is an abort (P1/P2) or, where `|| true` is already present, a
# silently-wrong EMPTY value (P3). Neither can be turned into a deterministic
# behavioural assertion without rewriting the callers around a shim, so this
# arm asserts the STATIC property instead — the idiom is gone from the audited
# files — and says so plainly rather than pretending to a behavioural proof.
#
# `grep -m1` is the sanctioned shape (the 308f7922 fix): grep stops ITSELF, so
# no consumer ever closes the pipe early and there is nothing to race.

# Lines of $1 that pipe into an early-exiting consumer (`head`, or a quiet
# `grep` carrying no -m). `(^|[^|])\|` excludes `||`, which is an or-list and
# not a pipeline; comment lines are excluded so the explanatory comments added
# by this slice may keep naming the idiom they replaced. Empty output = clean.
detect_early_consumer() {
  grep -nE '(^|[^|])\|[[:space:]]*(head\b|grep([[:space:]]+-[a-zA-Z]+)*[[:space:]]+-[a-zA-Z]*q)' "$1" \
    | grep -vE '^[0-9]+:[[:space:]]*#' || true
}

arm5() {
  local f hits total=0
  # POSITIVE CONTROL FIRST. A static assertion whose detector matches nothing is
  # a vacuous pass dressed as a clean sweep (TC-37), so prove the detector fires
  # on the exact pre-fix lines before believing it about the fixed files.
  local ctl="$WORK/a5-control.sh"
  cat >"$ctl" <<'CTL'
if git ls-files --unmerged | grep -q .; then
line_no="$(grep -nE "^x" "$f" | head -1 | cut -d: -f1)"
warm="$(grep -nF 'x' <<<"$b" | head -n1 | cut -d: -f1 || true)"
if curl -fsS "$url" 2>/dev/null | grep -qF "vers"; then
SANDBOX_RESIDUE="$(find "$sandbox" -mindepth 1 | head -5)"
# if git ls-files --unmerged | grep -q . -- a comment must NOT be flagged
ok="$(grep -m1 x "$f" | cut -d: -f1)" || true
CTL
  hits="$(detect_early_consumer "$ctl")"
  if [ "$(grep -c . <<<"${hits:-}")" -eq 5 ]; then
    pass "arm 5 (positive control): the detector flags all 5 pre-fix idioms and neither the comment nor the grep -m1 form"
  else
    fail "arm 5 (positive control): detector must flag exactly the 5 bad lines, got:"$'\n'"$hits"
  fi
  local files=(
    scripts/set-version.sh
    scripts/tests/test_check_design_refs.sh
    scripts/sbom-survey/smoke-install-run.sh
    scripts/tests/test_steward_orient.sh
    scripts/tests/test_ts_cache_coverage_split.sh
    scripts/lint-design-status.sh
    scripts/lint-plans-status.sh
    scripts/check-design-refs.sh
    scripts/check-staged-ledger-sidecars.sh
    scripts/release/publish-rc1-bootstrap.sh
  )
  for f in "${files[@]}"; do
    if [ ! -f "$REPO_ROOT/$f" ]; then
      fail "arm 5: $f does not exist — the static assertion would be vacuous"
      continue
    fi
    hits="$(detect_early_consumer "$REPO_ROOT/$f")"
    if [ -n "$hits" ]; then
      fail "arm 5: $f still pipes into an early-exiting consumer under pipefail:"$'\n'"$hits"
      total=$((total + 1))
    fi
  done
  if [ "$total" -eq 0 ]; then
    pass "arm 5 (static): none of the ${#files[@]} audited files pipes a producer into an early-exiting consumer"
  fi
}

arm1
arm2
arm3
arm4
arm5

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nshell pipefail-guard tests passed\n'
