#!/usr/bin/env bash
# scripts/lint-plan-anchors.sh — T1d enforcement (DOC-HYGIENE-2): the LINE-ANCHOR BAN.
#
# ---------------------------------------------------------------------------
# WHY (measured, not hypothetical)
# ---------------------------------------------------------------------------
# `dev/plans/plan-0.8.20.md` carried 18 backticked `<name>:<line>` pointers. The
# TC-45 pair (`engine:14867` / `engine:14890`) was ~2,100 lines off — the real
# call sites were at 16963 / 16986 — and `git log` shows the anchors were
# authored AFTER the merge they claimed to describe, so they were never correct,
# not merely stale. A line number is a pointer with no referential integrity: it
# rots on the very next commit that touches the file, silently, and nothing in
# the repo notices. A symbol name does not rot, and — crucially — it can be
# mechanically checked.
#
# Same class the master's own §6 findings keep re-discovering, and the same
# remedy the standing rule prescribes: fix the tooling so it cannot recur for
# anyone, rather than writing a "be careful with line numbers" note.
#
# ---------------------------------------------------------------------------
# WHAT IT ENFORCES (two rules + one guard)
# ---------------------------------------------------------------------------
# RULE 1 — BARE LINE-ANCHOR BAN.
#   A backticked token whose ENTIRE content is `<name>:<3-or-more digits>`
#   (optionally a range, `lib.rs:876-887`, or an open end, `engine:12166+`) is a
#   bare line anchor and is BANNED inside an ACTIVE plan.
#
#   The rule is a SHAPE, not a whitelist of known crate prefixes. An enumerated
#   prefix list (engine|py|napi|schema|cli|…) is defeated by the very next name
#   anyone invents — `fathomdb-cli:389` already escapes every enumeration that
#   would plausibly have been written — so `<name>` here is any identifier-ish
#   token: letters, digits, `_`, `.`, `-`, `+`.
#
#   Anchoring on the WHOLE backtick group is deliberate. It means the rule fires
#   only on a token that is NOTHING BUT a name and a line number — i.e. a pure
#   pointer — and never on prose or code that merely happens to contain a colon
#   (`derive_logical_id = SHA256("{kind}:{name}")`, `bm25(idx, 1.0, 3.0)`).
#
#   Three digits is the floor because two-digit `:NN` suffixes are overwhelmingly
#   section/clause references in this corpus (`§2:98`), not line numbers.
#   KNOWN GAP, deliberately left open for a follow-on tranche: this shape does
#   NOT catch (a) prefixless anchors `` `:6229` ``, (b) slashed paths
#   `` `dev/acceptance.md:1147` ``, (c) spaced forms `` `engine lib.rs:6479` ``,
#   or (d) sub-100 line numbers. Widening to any of those is a one-line change
#   to ANCHOR_RE below, but it turns ~90 further sites red across the ACTIVE
#   set and so needs its own conversion budget.
#
# RULE 2 — MANDATORY SYMBOL-EXISTENCE CHECK.
#   This is the crux. A gate that merely swaps an unverified NUMBER for an
#   unverified SYMBOL reproduces the exact failure mode above with better
#   cosmetics — it launders a bad pointer as a good one — and is therefore worse
#   than no gate at all. So every citation an ACTIVE plan makes is verified
#   against the file it names.
#
#   THE CITATION FORM (precise, and the only form this lint vouches for):
#
#       `<needle>` in `<path>`
#       `<needle>` / `<needle>` / … in `<path>`     (a `/`-separated chain)
#
#   * `<path>` must be file-path-shaped: `name.ext`, optionally slashed. It is
#     resolved against the worktree — exactly, or by UNIQUE suffix match (plans
#     write `fathomdb-engine/src/lib.rs`; the file lives at
#     `src/rust/crates/fathomdb-engine/src/lib.rs`). Unresolvable => FAIL.
#     Resolving to MORE THAN ONE file => FAIL as ambiguous: a citation nobody
#     can resolve deterministically is not a citation. (This is why a bare
#     `README.md` is rejected — cite the full path.)
#   * `<needle>` is verified by LITERAL substring match (`grep -F`) in the
#     resolved file. It is not required to be a Rust identifier: a quoted DDL
#     fragment, a YAML job key or a heading phrase all work, which is what makes
#     the form usable for the non-Rust targets ACTIVE plans cite
#     (`.github/workflows/release.yml`, design `README.md`s).
#   * A needle containing an ELISION PLACEHOLDER (`…` or `...`) cannot be
#     matched literally. Rather than SKIP it — which would silently reintroduce
#     the unverified-pointer class this gate exists to close — the lint FAILS
#     and says the citation is unverifiable. Rewrite it to something greppable.
#
#   Prose is deliberately unconstrained: only the `` `x` in `y.ext` `` shape is
#   read as a citation. `the … arm in `foo.rs`` or `Detail in `plan-0.8.8.md``
#   are prose and are not checked — the lint is permissive about surrounding
#   text and strict about the parts it verifies.
#
#   The extractor is WRAP-AWARE: a citation may straddle ONE line break, with an
#   optional `>` blockquote marker on the continuation line. This is not a
#   nicety. A first, line-based implementation of this lint silently missed four
#   real citations in the ACTIVE set that markdown soft-wrap had split across
#   lines (plan-0.8.20.md 150/151, 466/467, 704/705, and the `search_index_v2`
#   DDL citation in the master). A checker that a line break can defeat is a
#   checker whose green means nothing — the same vacuous-pass class as TC-37,
#   just at a different granularity. Two line breaks or a blank line ends a
#   citation: that is a paragraph boundary, not a wrap.
#
# VACUOUS-PASS GUARD (TC-37 — this repo's named failure class: a gate that
# reports green without having run). If the scan discovers ZERO ACTIVE plans,
# or verifies ZERO citations, the checking loops never execute and this script
# would exit 0 having vouched for nothing. Both are HARD failures.
#
# ---------------------------------------------------------------------------
# SCOPE
# ---------------------------------------------------------------------------
# `dev/plans/*.md` (TOP LEVEL) whose YAML frontmatter says `status: ACTIVE`.
# Derived from frontmatter, never from a filename list.
#
# Everything else is out of scope BY CONSTRUCTION, not by exception:
#   - a plan that is COMPLETE/SUPERSEDED/PROPOSED/UNKNOWN is a historical record;
#     rewriting its anchors would be falsifying the record;
#   - `dev/plans/runs/**` (slice logs, status boards, closure JSON) and
#     `dev/plans/prompts/**` (one-shot execution prompts) are immutable run
#     artifacts — the top-level glob excludes them structurally;
#   - `dev/archive/**` is frozen and is not under `dev/plans/` at all.
# Same scope split as scripts/lint-plans-status.sh; see dev/plans/README.md.
#
# Usage:  scripts/lint-plan-anchors.sh [--quiet]
# Exit:   0 = every ACTIVE plan is anchor-free AND every citation it makes
#             resolves and exists (and >=1 plan and >=1 citation were seen);
#         1 = a violation, or a vacuous scan.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

QUIET=0
[ "${1:-}" = "--quiet" ] && QUIET=1

# RULE 1 shape. Widen HERE (single source) if the known gaps above get budget.
ANCHOR_RE='`[A-Za-z_][A-Za-z0-9_.+-]*:[0-9]{3,}(-[0-9]+)?\+?`'

# `perl` carries the wrap-aware citation extraction (it needs whole-file regex
# with match offsets, which line-oriented grep/awk cannot do). If it is absent
# the lint cannot run — and a gate that cannot run must fail loudly, never skip
# (TC-37).
if ! command -v perl >/dev/null 2>&1; then
  echo "FAIL lint-plan-anchors: perl not found; the wrap-aware citation extractor" >&2
  echo "  cannot run. A gate that cannot run must not report a pass (TC-37)." >&2
  exit 1
fi

# Emit one `<line-number><TAB><whitespace-normalised citation>` record per
# citation found in $1. `WS` permits at most ONE newline (plus an optional `>`
# blockquote continuation marker), so soft-wrapped citations are found while a
# blank line still terminates the match.
extract_citations() {
  perl -0777 -ne '
    my $WS   = qr/(?:[ \t]*\n[ \t]*>?[ \t]*|[ \t]+)/;
    my $TICK = qr/`[^`\n]+`/;
    my $CIT  = qr/(?:$TICK$WS?\/$WS?)*$TICK${WS}in${WS}`[A-Za-z0-9._\/-]+\.[A-Za-z0-9]+`/;
    while (/($CIT)/g) {
      my $m = $1;
      my $ln = (substr($_, 0, $-[0]) =~ tr/\n//) + 1;
      $m =~ s/[ \t]*\n[ \t]*>?[ \t]*/ /g;
      print "$ln\t$m\n";
    }
  ' "$1"
}

FAIL=0
PLANS=()
ANCHORS=0
CITATIONS=0

note() { [ "$QUIET" -eq 1 ] || printf '%s\n' "$*"; }

# --- discover ACTIVE plans from frontmatter -------------------------------
shopt -s nullglob
for f in dev/plans/*.md; do
  [ "$(head -n1 "$f")" = "---" ] || continue
  status="$(awk 'NR==1{next} /^---$/{exit} /^status:[[:space:]]*/{sub(/^status:[[:space:]]*/,""); sub(/[[:space:]]+$/,""); print; exit}' "$f")"
  [ "$status" = "ACTIVE" ] && PLANS+=("$f")
done
shopt -u nullglob

# --- worktree file index, for citation path resolution --------------------
INDEX="$(mktemp)"
trap 'rm -f "$INDEX"' EXIT
find . \( -name .git -o -name node_modules -o -name target -o -name .venv \) -prune \
  -o -type f -print 2>/dev/null | sed 's|^\./||' | sort >"$INDEX"

# Resolve a cited path to exactly one worktree file.
# Prints the resolved path on success; prints nothing and returns
# 1 = no match, 2 = ambiguous.
resolve_path() {
  local want="$1" hits
  if [ -f "$want" ]; then printf '%s\n' "$want"; return 0; fi
  hits="$(grep -F -x -e "$want" "$INDEX" || true)"
  [ -n "$hits" ] || hits="$(grep -E "(^|/)$(sed -E 's/[][\.^$*+?(){}|/]/\\&/g' <<<"$want")\$" "$INDEX" || true)"
  case "$(printf '%s' "$hits" | grep -c . || true)" in
    0) return 1 ;;
    1) printf '%s\n' "$hits"; return 0 ;;
    *) return 2 ;;
  esac
}

for plan in "${PLANS[@]}"; do
  # ---- RULE 1: bare line anchors ----------------------------------------
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    ln="${hit%%:*}"; tok="${hit#*:}"
    ANCHORS=$((ANCHORS + 1))
    FAIL=1
    {
      printf 'FAIL %s:%s: bare line anchor %s\n' "$plan" "$ln" "$tok"
      printf '  A `<name>:<line>` pointer has no referential integrity — it rots on the next\n'
      printf '  commit that touches the file, silently. Cite a greppable symbol instead:\n'
      printf '      `fn some_symbol` in `path/to/file.rs`\n'
      printf '  (this lint then verifies that symbol actually occurs in that file).\n'
    } >&2
  done < <(grep -noE "$ANCHOR_RE" "$plan" || true)

  # ---- RULE 2: every citation must resolve AND exist ---------------------
  while IFS=$'\t' read -r ln match; do
    [ -n "$match" ] || continue
    mapfile -t groups < <(tr '`' '\n' <<<"$match" | awk 'NR%2==0')
    (( ${#groups[@]} >= 2 )) || continue
    cited="${groups[-1]}"
    unset 'groups[-1]'

    resolved=""; rc=0
    resolved="$(resolve_path "$cited")" || rc=$?
    if [ "$rc" -eq 1 ]; then
      FAIL=1
      printf 'FAIL %s:%s: citation names `%s`, which matches no file in the worktree.\n' \
        "$plan" "$ln" "$cited" >&2
      continue
    elif [ "$rc" -eq 2 ]; then
      FAIL=1
      printf 'FAIL %s:%s: citation path `%s` is AMBIGUOUS (matches >1 file) — it cannot be\n' \
        "$plan" "$ln" "$cited" >&2
      printf '  verified mechanically. Cite the full repo-relative path.\n' >&2
      continue
    fi

    for sym in "${groups[@]}"; do
      case "$sym" in
        *…*|*...*)
          FAIL=1
          printf 'FAIL %s:%s: citation `%s` in `%s` contains an elision placeholder, so it\n' \
            "$plan" "$ln" "$sym" "$cited" >&2
          printf '  cannot be matched literally. An unverifiable citation is not skipped here —\n' >&2
          printf '  that is how an unverified pointer gets laundered as a verified one. Rewrite\n' >&2
          printf '  it as a literal symbol or quoted fragment that greps.\n' >&2
          continue ;;
      esac
      CITATIONS=$((CITATIONS + 1))
      if ! grep -qF -- "$sym" "$resolved"; then
        FAIL=1
        printf 'FAIL %s:%s: cited symbol `%s` does NOT occur in `%s` (resolved: %s).\n' \
          "$plan" "$ln" "$sym" "$cited" "$resolved" >&2
        printf '  The symbol was renamed, moved, or never existed. Re-verify with\n' >&2
        printf '      grep -n %s %s\n' "'$sym'" "$resolved" >&2
      fi
    done
  done < <(extract_citations "$plan")
done

# --- vacuous-pass guard (TC-37) -------------------------------------------
if [ "${#PLANS[@]}" -eq 0 ]; then
  {
    echo "FAIL lint-plan-anchors: ZERO ACTIVE plans discovered under dev/plans/*.md."
    echo "  The scan loops never ran, so exiting 0 would vouch for nothing (TC-37)."
    echo "  Either the frontmatter convention changed or the scan is pointed wrong."
  } >&2
  exit 1
fi
if [ "$CITATIONS" -eq 0 ]; then
  {
    echo "FAIL lint-plan-anchors: ZERO citations verified across ${#PLANS[@]} ACTIVE plan(s)."
    echo "  The existence check — the whole point of this gate — did not execute, so a"
    echo "  green here would be vacuous (TC-37). Either the citation form drifted away"
    echo "  from \`sym\` in \`path\`, or the extractor is broken."
  } >&2
  exit 1
fi

if [ "$FAIL" -ne 0 ]; then
  printf '\nlint-plan-anchors: %d bare line anchor(s); scanned %d ACTIVE plan(s), %d citation(s).\n' \
    "$ANCHORS" "${#PLANS[@]}" "$CITATIONS" >&2
  exit 1
fi

note "lint-plan-anchors: OK — ${#PLANS[@]} ACTIVE plan(s), 0 bare line anchors, ${CITATIONS} citation(s) verified to exist."
exit 0
