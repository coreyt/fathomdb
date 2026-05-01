#!/usr/bin/env bash
# agent-permission-canary.sh
#
# Exercises the Bash allowlist. Deny verification is NOT done by this script —
# subprocess calls bypass Claude Bash-tool permission enforcement. Deny
# verification is the orchestrator's responsibility: the canary agent must
# attempt each denied command as a separate Bash tool call (see docs).
#
# Paired with .claude/settings.json. See scripts/preflight.sh.

set -u

pass=0
fail=0
failed=()

check() {
    local label="$1"; shift
    if "$@" >/dev/null 2>&1; then
        echo "  PASS: $label"
        pass=$((pass + 1))
    else
        echo "  FAIL: $label  (cmd: $*)"
        fail=$((fail + 1))
        failed+=("$label")
    fi
}

echo "── Allowlist checks ──"
check "git-status"      git status --short
check "git-log"         git log --oneline -1
check "git-rev-parse"   git rev-parse --show-toplevel
check "git-diff"        git diff --stat HEAD
check "git-show"        git show --stat HEAD
check "pwd"             pwd
check "ls"              ls -1
check "cat-readme"      cat README.md
check "head"            head -1 README.md
check "tail"            tail -1 README.md
check "wc"              wc -l README.md
check "grep"            grep -r "fn " --include="*.rs" -l crates
check "find"            find crates -maxdepth 2 -name "Cargo.toml"
check "df"              df -h /
check "date"            date
check "cargo-version"   cargo --version
check "rustc-version"   rustc --version
check "cargo-metadata"  cargo metadata --format-version 1 --no-deps

echo ""
echo "── Result ──"
echo "$pass pass, $fail fail"
if [ "$fail" -ne 0 ]; then
    printf 'failing buckets:\n'
    printf '  - %s\n' "${failed[@]}"
    echo ""
    echo "Fix .claude/settings.json permissions.allow / permissions.deny and re-run."
    exit 1
fi
echo "READY. Allowlist is complete."

echo ""
echo "── Next step ──"
echo "Deny verification: orchestrator must run each of these commands as a SEPARATE Bash tool call via the canary agent and verify each is blocked:"
echo "  - rm --help"
echo "  - curl --help"
echo "  - git push --dry-run origin HEAD"
echo "  - cargo publish --dry-run --help"
exit 0
