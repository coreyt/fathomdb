#!/usr/bin/env bash
# Verify that agent-test registrations form a total, disjoint fast/heavy set.
set -euo pipefail

usage() {
  printf 'Usage: check-agent-test-tier-totality.sh [--script PATH]\n' >&2
}

SCRIPT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --script)
      shift
      if [ "$#" -eq 0 ] || [ -z "$1" ]; then
        usage
        exit 2
      fi
      SCRIPT="$1"
      ;;
    *)
      usage
      exit 2
      ;;
  esac
  shift
done

if [ -z "$SCRIPT" ]; then
  SCRIPT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/agent-test.sh"
fi
if [ ! -f "$SCRIPT" ]; then
  printf 'FAIL agent-test tier totality: script not found: %s\n' "$SCRIPT" >&2
  exit 2
fi

awk '
  function fail(message) { print "FAIL agent-test tier totality: " message > "/dev/stderr"; failed = 1 }
  /^[[:space:]]*(run_tier_suite|skip_tier_suite)[[:space:]]+/ {
    tier = $2
    label = $3
    if (tier == "" || label == "") {
      fail("malformed tier registration at line " NR)
      next
    }
    if (tier != "fast" && tier != "heavy") {
      fail("unassigned registration " label " at line " NR " has tier " tier)
      next
    }
    if (!(label in seen)) {
      seen[label] = 1
      labels++
    }
    if (assignment[label] != "" && assignment[label] != tier) {
      fail("duplicate tier assignment for " label ": " assignment[label] " and " tier)
    }
    assignment[label] = tier
    count[tier]++
    next
  }
  /^[[:space:]]*(run_suite|skip_suite)[[:space:]]+[[:alnum:]_.-]+([[:space:]]|$)/ {
    fail("raw run_suite registration at line " NR " bypasses fast/heavy assignment")
  }
  END {
    if (count["fast"] == 0) fail("fast tier has no registrations")
    if (count["heavy"] == 0) fail("heavy tier has no registrations")
    if (labels == 0) fail("no tiered registrations found")
    if (!failed) {
      printf "ok agent-test tier totality: %d labels (fast registrations=%d heavy registrations=%d)\n", labels, count["fast"], count["heavy"]
    }
    exit failed ? 1 : 0
  }
' "$SCRIPT"
