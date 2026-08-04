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
  function brace_delta(text, copy) {
    copy = text
    opens = gsub(/\{/, "", copy)
    copy = text
    closes = gsub(/\}/, "", copy)
    return opens - closes
  }
  function canonical_raw_registration(text) {
    if (current_function == "run_tier_suite") {
      return text ~ /^[[:space:]]*run_suite[[:space:]]+"\$label"[[:space:]]+"\$@"[[:space:]]*$/
    }
    if (current_function == "run_tier_maybe_suite") {
      return text ~ /^[[:space:]]*skip_suite[[:space:]]+"\$label"[[:space:]]+"\$skip_reason"[[:space:]]*$/ \
        || text ~ /^[[:space:]]*run_suite[[:space:]]+"\$label"[[:space:]]+"\$@"[[:space:]]*$/
    }
    return 0
  }
  {
    line = $0
    began_function = 0
    if (!in_function && $1 ~ /\(\)$/ && $2 == "{") {
      current_function = $1
      sub(/\(\)$/, "", current_function)
      function_depth = brace_delta(line)
      in_function = 1
      began_function = 1
    }

    if (line ~ /^[[:space:]]*(run_tier_suite|run_tier_maybe_suite)[[:space:]]+/) {
      tier = $2
      label = $3
      if (tier == "" || label == "") {
        fail("malformed tier registration at line " NR)
      } else if (tier != "fast" && tier != "heavy") {
        fail("unassigned registration " label " at line " NR " has tier " tier)
      } else if (label in assignment) {
        fail("duplicate suite label " label " at line " NR "; first assigned to " assignment[label] ", again assigned to " tier)
      } else {
        assignment[label] = tier
        labels++
        count[tier]++
      }
    }

    if (line !~ /^[[:space:]]*#/ \
      && line ~ /(^|[;[:space:]])(run_suite|skip_suite)[[:space:]]+/ \
      && !canonical_raw_registration(line)) {
      fail("raw run_suite registration at line " NR " bypasses fast/heavy assignment")
    }

    if (in_function) {
      if (!began_function) function_depth += brace_delta(line)
      if (function_depth <= 0) {
        in_function = 0
        current_function = ""
      }
    }
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
