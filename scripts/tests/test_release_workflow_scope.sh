#!/usr/bin/env bash
# scripts/tests/test_release_workflow_scope.sh — static structural assertions
# on .github/workflows/release.yml for the 0.8.18 GA capstone (Slice 20).
#
# Covers three signed acceptance criteria (dev/design/
# 0.8.18-slice-0-vector-equivalence-publish-design.md §U2):
#
#   R-REL-4e (tag-matrix gate): the 0.8.18 GA tag must build ONLY
#     x86_64-unknown-linux-gnu — the deferred macOS/Windows/aarch64/musl legs
#     are excluded from THIS tag (re-enabled by the follow-on orchestrator).
#     RED before gating (full 5-way python / 4-way napi matrix); GREEN after.
#
#   R-REL-4c (ordered commit points): every tiered cargo publish (t1..t7) is
#     transitively gated on `all-builds-passed`, and t(N) needs t(N-1) — the
#     cross-ecosystem gate fires before ANY publish, and tiers run in dep
#     order. Also asserts the fixed `sleep 60` index-propagation heuristic is
#     replaced by a poll-for-resolvability step (wait-for-crate-version.sh).
#
#   R-REL-4f (npm dist-tag): while platform coverage is partial (linux-x64-gnu
#     only), npm must publish under a NON-`latest` dist-tag so mac/win users are
#     not served an install-incompatible package as the default.
#
# Pure static parse (python3 + PyYAML); does not run the workflow.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WF="$REPO_ROOT/.github/workflows/release.yml"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

if [ ! -f "$WF" ]; then
  fail "release.yml not found at $WF"
  exit 1
fi

# --- R-REL-4e: matrix gated to x86_64-linux only ---------------------------
scope_out="$(python3 - "$WF" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
jobs = wf["jobs"]

def targets(job):
    inc = jobs[job].get("strategy", {}).get("matrix", {}).get("include", [])
    return [e.get("target") for e in inc]

py = targets("build-python")
napi = targets("build-napi")
ok_py = py == ["x86_64-unknown-linux-gnu"]
ok_napi = napi == ["x86_64-unknown-linux-gnu"]
print("PY", ok_py, py)
print("NAPI", ok_napi, napi)
PY
)"
if printf '%s\n' "$scope_out" | grep -q '^PY True'; then
  pass "build-python matrix gated to x86_64-unknown-linux-gnu only"
else
  fail "build-python matrix NOT gated to linux-x64 only: $(printf '%s' "$scope_out" | sed -n '1p')"
fi
if printf '%s\n' "$scope_out" | grep -q '^NAPI True'; then
  pass "build-napi matrix gated to x86_64-unknown-linux-gnu only"
else
  fail "build-napi matrix NOT gated to linux-x64 only: $(printf '%s' "$scope_out" | sed -n '2p')"
fi

# --- R-REL-4c: ordered commit points (all-builds-passed -> tiered chain) ----
order_out="$(python3 - "$WF" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
jobs = wf["jobs"]

def needs(job):
    n = jobs.get(job, {}).get("needs", [])
    return n if isinstance(n, list) else [n]

# all-builds-passed must gate on every build lane.
abp = set(needs("all-builds-passed"))
gate_ok = {"verify-release", "build-python", "build-napi", "build-rust"} <= abp
print("GATE", gate_ok, sorted(abp))

tiers = [f"publish-rust-t{i}-{s}" for i, s in enumerate(
    ["embedder-api", "schema", "query", "embedder", "engine", "facade", "cli"], start=1)]
# t1 gated on all-builds-passed; each subsequent tier gated on its predecessor.
chain_ok = "all-builds-passed" in needs(tiers[0])
for prev, cur in zip(tiers, tiers[1:]):
    if prev not in needs(cur):
        chain_ok = False
print("CHAIN", chain_ok)
PY
)"
if printf '%s\n' "$order_out" | grep -q '^GATE True'; then
  pass "all-builds-passed gates every build lane before any publish"
else
  fail "all-builds-passed missing a build-lane dependency: $(printf '%s' "$order_out" | grep '^GATE')"
fi
if printf '%s\n' "$order_out" | grep -q '^CHAIN True'; then
  pass "tiered publish chain t1<-...<-t7 rooted at all-builds-passed"
else
  fail "tiered publish chain broken: $(printf '%s' "$order_out" | grep '^CHAIN')"
fi

# poll-for-resolvability replaced the fixed 60s sleep.
if grep -qE '^\s*run:\s*sleep 60\s*$' "$WF"; then
  fail "fixed 'sleep 60' index-propagation heuristic still present (R-REL-4c: poll, do not sleep)"
else
  pass "no fixed 'sleep 60' — index propagation is poll-for-resolvability"
fi
if grep -q 'wait-for-crate-version.sh' "$WF"; then
  pass "wait-for-crate-version.sh poll step wired into tiers"
else
  fail "wait-for-crate-version.sh poll step missing"
fi

# --- R-REL-4f: npm dist-tag is non-latest while coverage is partial ---------
tag_out="$(python3 - "$WF" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
env = wf.get("env", {})
tag = env.get("NPM_DIST_TAG")
print("TAG", repr(tag))
print("NOTLATEST", tag is not None and tag != "latest")
PY
)"
if printf '%s\n' "$tag_out" | grep -q '^NOTLATEST True'; then
  pass "npm NPM_DIST_TAG is non-latest while platform coverage is partial ($(printf '%s' "$tag_out" | grep '^TAG'))"
else
  fail "npm dist-tag must be non-latest for the linux-x64-only 0.8.18 tag: $(printf '%s' "$tag_out" | grep '^TAG')"
fi

# R-REL-4f: the per-platform binary package publish job + the publish-time
# optionalDependencies injection are wired.
if python3 - "$WF" <<'PY'
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
sys.exit(0 if "publish-npm-platform-linux-x64-gnu" in wf["jobs"] else 1)
PY
then
  pass "per-platform npm publish job (linux-x64-gnu) present"
else
  fail "publish-npm-platform-linux-x64-gnu job missing"
fi
if grep -q 'npm-inject-optional-deps.sh' "$WF"; then
  pass "publish-time optionalDependencies injection wired into publish-npm"
else
  fail "npm-inject-optional-deps.sh not wired into the workflow"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll release-workflow-scope tests passed\n'
