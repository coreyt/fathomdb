#!/usr/bin/env bash
# Copy agent-runner spill logs into a runner-owned artifact directory.
#
# The source is deliberately restricted to regular files named
# fathomdb-agent-*.log. Before a log becomes an artifact, whole lines carrying
# process/environment transcript values are replaced. Compiler/test output and
# ordinary diagnostics remain intact, but a command that happened to dump PATH,
# a GitHub token, or runner configuration cannot turn the CI artifact into an
# environment transcript.
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: collect-agent-spill-logs.sh --destination <directory> [--source-dir <directory>]

Copies and redacts /tmp/fathomdb-agent-*.log into a runner-owned artifact
directory. --source-dir exists only to let the recurrence test use a disposable
fixture; CI uses the default /tmp source.
USAGE
}

source_dir="/tmp"
destination=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --source-dir)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      source_dir="$2"
      shift 2
      ;;
    --destination)
      [ "$#" -ge 2 ] || { usage; exit 2; }
      destination="$2"
      shift 2
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [ -z "$destination" ] || [ ! -d "$source_dir" ]; then
  usage
  exit 2
fi

if [ -e "$destination" ]; then
  printf 'collect-agent-spill-logs: destination already exists: %s\n' "$destination" >&2
  exit 2
fi

redact_log() {
  awk '
    /(^|[[:space:]"{,])"?(PATH|PYTHONPATH|NODE_PATH|LD_[[:alnum:]_]*|HOME|SSH_AUTH_SOCK|XDG_[[:alnum:]_]*|GITHUB_[[:alnum:]_]*|RUNNER_[[:alnum:]_]*|ACTIONS_[[:alnum:]_]*|[[:alnum:]_]*(TOKEN|SECRET|PASSWORD|KEY)[[:alnum:]_]*)"?[[:space:]]*[:=]/ {
      print "[REDACTED environment transcript line]"
      next
    }
    { print }
  ' "$1"
}

regular_candidates=()
skipped_nonregular=0
shopt -s nullglob
for candidate in "$source_dir"/fathomdb-agent-*.log; do
  if [ -L "$candidate" ] || [ ! -f "$candidate" ]; then
    skipped_nonregular=$((skipped_nonregular + 1))
    continue
  fi
  regular_candidates+=("$candidate")
done

if [ "${#regular_candidates[@]}" -eq 0 ]; then
  printf 'collect-agent-spill-logs: no regular agent spill logs found; refusing an empty artifact\n' >&2
  exit 1
fi

mkdir -p "$destination"
manifest="$destination/MANIFEST.txt"
printf 'format=fathomdb-agent-spill-logs-v1\n' >"$manifest"
printf 'redaction=whole environment-transcript lines\n' >>"$manifest"

copied=0
for candidate in "${regular_candidates[@]}"; do
  target="$destination/${candidate##*/}"
  redact_log "$candidate" >"$target"
  copied=$((copied + 1))
done

printf 'copied=%d\n' "$copied" >>"$manifest"
printf 'skipped_nonregular=%d\n' "$skipped_nonregular" >>"$manifest"
printf 'collected %d redacted agent spill log(s); skipped_nonregular=%d\n' \
  "$copied" "$skipped_nonregular"
