#!/usr/bin/env bash
# check-design-refs.sh — TC-92 (DOC-HYGIENE-3): A REQUIREMENT ID MUST HAVE
# DESIGN COVERAGE AT THE MOMENT IT IS MINTED.
#
# ---------------------------------------------------------------------------
# THE MEASURED FAILURE
# ---------------------------------------------------------------------------
# At the Slice 22 commission, `scripts/commission-manifest.sh 0.8.20 22` exited 1:
#     ZERO design docs matched 0.8.20 Slice 22 (tokens: TC-67, TC-68, R-20-VC)
# — its TC-37 vacuous-pass guard, working exactly as designed, refusing to emit a
# brief whose required-reading list would be empty.
#
# THE GUARD WAS RIGHT AND THE ALARM WAS REAL, but the diagnosis was NOT "the
# design does not exist". It existed for every leg and was found in minutes:
# TC-67 in `record-lifecycle-protocol/projection-registry-and-async-embed.md`,
# TC-68 in `0.8.18-slice-5-vector-equivalence-probe.md`, decision #18 in
# `errors.md`, sqlite-vec #99 in `vector.md`. None of them MENTIONED TC-67,
# TC-68 or R-20-VC, because those ids were minted 2026-07-27, long after the
# documents were written. A LITERAL TOKEN SCAN CANNOT SEE A DEPENDENCY THAT RUNS
# BACKWARD IN TIME, which is the normal direction for a reserved-gap slice
# consuming older design.
#
# THE CONSEQUENCE IS BIMODAL, and the quiet half is the dangerous one. Zero
# matches HARD-FAILS and cannot be commissioned at all — loud, recoverable, and
# what happened to Slice 22. ONE weak incidental match emits a brief whose
# required reading is thin but LOOKS complete. Slice 21 was that shape: it
# matched exactly one document, on TC-71, and the manifest honestly printed
# `NO design doc mentions: ac_002, TC-57, R-20-CR`. The line was correct and was
# easy to read past.
#
# ---------------------------------------------------------------------------
# WHAT IS ALREADY FIXED, AND WHY THIS IS STILL NEEDED
# ---------------------------------------------------------------------------
# Slice 22 was unblocked by hand-annotating five documents with a "Requirement
# traceability" blockquote. That approach is SUPERSEDED by `design_refs` — a
# curated per-ladder-entry citation list on the state file (d30ef52f) — and is
# not the remedy here.
#
# THE UNFIXED HALF IS THE DISCIPLINE GAP: nothing requires a back-link at the
# moment a requirement id is minted. TC-92 records that as remedy (a), "a
# checklist item at exactly the moment the knowledge exists". A checklist is a
# convention; this file is the mechanical version, and `scripts/hooks/pre-commit`
# fires it exactly when the state file or the design tier is staged — which is
# the commit in which a new id gets minted.
#
# ---------------------------------------------------------------------------
# THE PREDICATE
# ---------------------------------------------------------------------------
# For every ladder entry of every `dev/plans/release-state-*.json`, every token
# the entry declares must have DESIGN COVERAGE, by one of:
#   (i)  a design document whole-token matches it under the manifest's own
#        selectors — i.e. it was back-linked; or
#   (ii) one of the entry's curated `design_refs` documents whole-token matches
#        it — the route for a document the manifest's walker cannot reach; or
#   (iii) it is listed in the FROZEN BASELINE EXEMPTION table below.
# Anything else is a hard failure naming the slice, the token and both remedies.
#
# WHY PER-TOKEN AND NOT PER-ENTRY. "The entry has SOME `design_refs`, therefore
# every token it declares is covered" would be a blanket pass: mint a new
# requirement id on a slice that already carries one curated citation and the
# check would never fire. That is precisely the quiet mode TC-92 is about, and
# it would make this a snapshot rather than a ratchet. Coverage is asked
# token-by-token, which is also the granularity the manifest itself reports at.
#
# WHY NOT "SKIP LANDED SLICES". That is the cheap way to make the baseline green
# and it is wrong: it would let a NEW gap through on a landed slice, and landed
# slices are re-commissioned (a historical regeneration is an explicitly
# supported manifest mode). The two known-uncovered tokens are enumerated
# individually instead, so the set can only shrink by accident and can only grow
# by a deliberate edit here.
#
# NO TOKEN DERIVATION OF ITS OWN. This check DRIVES `commission-manifest.sh` and
# reads its coverage report; it never re-derives what a slice's tokens are.
# TC-100 is the standing proof that the matching rule is subtle (`"C-1" in
# "TC-15"` is True, which once padded Slice 15's required reading to eighteen
# documents), and two tools disagreeing about a slice's design coverage would be
# worse than either alone. The one thing this file must reproduce — the
# whole-token boundary, for curated documents the manifest's walker never reads —
# is asserted to be byte-identical to the manifest's at every run, and the check
# REFUSES TO RUN if it has changed (TC-37: a gate that cannot run must not report
# a pass).
#
# ---------------------------------------------------------------------------
# `--staged-only` JUDGES THE INDEX, NOT THE WORKING TREE
# ---------------------------------------------------------------------------
# A commit-time gate must reason about THE CONTENT THAT WILL BE COMMITTED. This
# is the same lesson `check-staged-ledger-sidecars.sh` records for TC-88: reading
# the worktree goes green for the author and red for everyone downstream. Under a
# PARTIAL COMMIT a worktree read is wrong in both directions:
#   * FALSE PASS — a staged ladder entry mints an id, and the back-link exists
#     only as an UNSTAGED edit. The commit lands with no design of record and the
#     gate said nothing. That is exactly the quiet mode TC-92 is about, arriving
#     through the gate built to stop it.
#   * FALSE BLOCK — an unrelated unstaged edit removes coverage, and a clean
#     staged commit is refused. On the ACTIVE commit path that is the TC-121
#     hazard: `seat-path-guard.sh` false-positived and two independent agents
#     learned to route around it.
#
# SO THE SWEEP RUNS OVER A SNAPSHOT OF THE INDEX. `git checkout-index --all
# --prefix` materialises exactly the bytes `git commit` is about to write into a
# throwaway tree, and the manifest is driven THERE. Measured before it was built,
# because the whole approach depends on it: `commission-manifest.sh` needs git
# for one thing only — `cd "$(git rev-parse --show-toplevel)"` — so an empty
# `git init` in the snapshot is enough, and its output for a real slice is
# BYTE-IDENTICAL to the worktree run. The manifest is NOT modified; byte-identity
# is its safety argument and it stays intact. Cost: ~0.26s for 2121 files.
#
# NO GIT WRITE, EVER — AND THAT IS A STRUCTURAL PROPERTY, NOT A PROMISE (TC-128)
# This script invokes exactly four git commands, and every one of them is
# READ-ONLY:
#     git rev-parse --show-toplevel        (resolve the repo root)
#     git --no-optional-locks diff --cached --name-only   (the trigger scan)
#     git ls-files --unmerged              (the cannot-certify probe)
#     git checkout-index --all --prefix=…  (writes ONLY inside $SNAPSHOT)
# There is NO `git init`, no `git config`, no `git add`, no `git stash`, nothing
# that can alter a repository. The snapshot is made a git repository BY HAND with
# `mkdir` and `printf`; no git binary is involved in creating it.
#
# WHY IT IS BUILT THAT WAY, measured twice on 2026-07-29 (18:14:51 and 18:26:28).
# An earlier version called `git init "$SNAPSHOT"`. A real linked-worktree
# pre-commit hook — how every orchestrated commit in this repo is made — exports
# `GIT_DIR` as an ABSOLUTE path, and with GIT_DIR set `git init` DOES NOT create
# `$SNAPSHOT/.git`: GIT_DIR wins and git reinitialises the repository GIT_DIR
# names. Both times that was the PRIMARY CHECKOUT, whose `.git/config` gained
# `bare = true`, breaking every work-tree operation there while `git rev-parse
# HEAD` still resolved — half-alive, not obviously broken. (A plain checkout
# exports no `GIT_DIR` and a RELATIVE `GIT_INDEX_FILE`, which is why testing only
# that shape reproduces the harmless case and misses this one entirely.)
#
# Ordering the environment scrub correctly WOULD also have fixed it. That was
# rejected: it leaves a gate on the commit path whose worst-case failure is
# corrupting the repository it guards, and which any later edit moving one line
# can re-arm. With no write anywhere in the path, the worst case of a leaked
# GIT_* is a WRONG ANSWER FROM A DOCS-HYGIENE GATE. Take the route where damage
# is impossible, not the one where it is unlikely.
#
# THE ENVIRONMENT BOUNDARY STILL MATTERS FOR CORRECTNESS (not for safety):
#   REAL-REPO SIDE, environment INTACT: the trigger scan, the unmerged probe and
#     `checkout-index` MUST see the hook's `GIT_INDEX_FILE`, because a PARTIAL
#     commit (`git commit -- <paths>`) puts the content that will be committed
#     into a TEMPORARY index and names it there (measured:
#     `.git/next-index-*.lock`). Scrubbing before them would snapshot the wrong
#     tree and silently reintroduce the worktree-vs-index defect.
#   SNAPSHOT SIDE, environment SCRUBBED: the manifest, and the self-check below.
# And the snapshot is required to RESOLVE TO ITSELF before the sweep runs, so a
# git call can never quietly answer about a different tree.
#
# THE RESIDUAL, NAMED RATHER THAN IMPLIED. Two states cannot be certified and are
# reported LOUDLY and then let through (exit 0), never failed: an UNMERGED index,
# and a snapshot that cannot be built. This deliberately diverges from
# `check-staged-ledger-sidecars.sh`, which exits 2 on an unmerged path — that
# gate protects an invariant which conflict resolution actively breaks, whereas
# this one is a coverage discipline where a refusal nobody can act on costs more
# than the miss. A merge commit touching `dev/design/**` must not be blocked by a
# gate that simply could not look.
#
# READ-ONLY, and LOCAL-ONLY. It writes nothing inside the repository (the
# snapshot lives under `mktemp -d` and is removed on exit). It is wired into the
# pre-commit hook and deliberately NOT into `preflight.sh --landing` (every check
# there is paired with a mirrored CI job, and this one has no mirror) nor into
# `agent-test.sh` nor `.github/**`.
#
# Usage:
#   scripts/check-design-refs.sh                      # sweep every release
#   scripts/check-design-refs.sh --release 0.8.20     # one release
#   scripts/check-design-refs.sh --release 0.8.20 --slice 22
#   scripts/check-design-refs.sh --staged-only        # pre-commit: self-gates
#                                                     # and judges the INDEX
# Exit: 0 = every declared token has coverage (or a frozen exemption), or
#           --staged-only found nothing relevant staged, or it announced that it
#           could not certify this commit (unmerged index / no snapshot);
#       1 = an uncovered token; 2 = the gate could not run.
set -euo pipefail

if ! TOPLEVEL="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  printf 'check-design-refs: not inside a git repository\n' >&2
  exit 2
fi
cd "$TOPLEVEL"

usage() {
  cat <<'USAGE'
usage:
  scripts/check-design-refs.sh [--release <rel>] [--slice <n>] [--staged-only]

  --release <rel>  restrict to dev/plans/release-state-<rel>.json
  --slice <n>      restrict to one ladder entry (requires --release)
  --staged-only    exit 0 unless this commit stages (or DELETES) a release-state
                   file or a path under dev/design/ — the pre-commit shape. The
                   sweep then runs over a snapshot of the INDEX, not the worktree.
USAGE
}

ONLY_RELEASE=""
ONLY_SLICE=""
STAGED_ONLY=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --release) ONLY_RELEASE="${2:-}"; shift 2 ;;
    --slice) ONLY_SLICE="${2:-}"; shift 2 ;;
    --staged-only) STAGED_ONLY=1; shift ;;
    *) printf 'check-design-refs: unknown option %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
done

# ---------------------------------------------------------------------------
# SCOPE GATE — get out of the way of unrelated commits.
# ---------------------------------------------------------------------------
# A gate that taxed every commit in the repo would be turned off, and a gate that
# gets turned off is worse than no gate (the standing lesson from TC-88's own
# wiring, and from TC-121, where a guard that false-positived was routed around
# by two different agents). The trigger set is exactly the two places a
# requirement id can be minted or its design of record can move.
SNAPSHOT=""
cleanup() {
  [ -n "$SNAPSHOT" ] || return 0
  case "$SNAPSHOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$SNAPSHOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$SNAPSHOT" >&2 ;;
  esac
}
trap cleanup EXIT

if [ "$STAGED_ONLY" -eq 1 ]; then
  # NO `--diff-filter=ACMR` HERE. Deletions are exactly the case that matters:
  # a commit that REMOVES the only design document naming a token deletes that
  # token's coverage, and filtering deletions out of the trigger scan let it
  # through the early exit untouched. Deleted paths decide only WHETHER to run —
  # the sweep reads the index snapshot and so never tries to open one.
  # `--no-optional-locks` so even this read cannot write: a plain `git diff` may
  # refresh the index's stat cache, which is a write into the repository being
  # gated. Harmless, but the claim in the header is that this gate writes
  # NOTHING, and a claim with an exception is a claim nobody can rely on.
  STAGED="$(git --no-optional-locks diff --cached --name-only)"
  if ! grep -qE '^dev/plans/release-state-.*\.json$|^dev/design/' <<<"$STAGED"; then
    exit 0
  fi

  # CANNOT-CERTIFY #1 — an unmerged index has no stage 0, so there is no single
  # "content that will be committed" to snapshot. Loud, then out of the way: see
  # THE RESIDUAL in the header for why this is exit 0 and not a refusal.
  if git ls-files --unmerged | grep -q .; then
    printf 'check-design-refs: NOT CHECKED — the index has UNMERGED paths, so the content\n' >&2
    printf '  that will be committed cannot be snapshotted. Design coverage for this commit\n' >&2
    printf '  is UNVERIFIED (TC-92). Resolve the conflict and re-run:\n' >&2
    printf '    scripts/check-design-refs.sh\n' >&2
    exit 0
  fi

  SNAPSHOT="$(mktemp -d)"
  # ---- REAL-REPO SIDE OF THE BOUNDARY — the git environment stays INTACT ----
  # `:0:` semantics for the whole tree: exactly the bytes `git commit` writes.
  #
  # `GIT_INDEX_FILE` MUST STILL BE SET HERE, and this is load-bearing rather than
  # incidental. A PARTIAL commit (`git commit -- <paths>`) builds a TEMPORARY
  # index holding exactly the content that will be committed and names it in
  # `GIT_INDEX_FILE` (measured: `.git/next-index-<pid>.lock`). Scrubbing the
  # environment before this line would snapshot the repository's ordinary index
  # instead — the wrong tree — and silently reintroduce the worktree-vs-index
  # defect in a subtler form. The same applies to the `git diff --cached` and
  # `git ls-files --unmerged` calls above.
  if ! git checkout-index --all --prefix="$SNAPSHOT/" 2>/dev/null; then
    printf 'check-design-refs: NOT CHECKED — could not materialise the index into a\n' >&2
    printf '  snapshot; git checkout-index --all failed. Design coverage for this commit\n' >&2
    printf '  is UNVERIFIED (TC-92). Re-run against the worktree after committing:\n' >&2
    printf '    scripts/check-design-refs.sh\n' >&2
    exit 0
  fi

  # ================= THE BOUNDARY — everything below is SNAPSHOT-SIDE ========
  # Defence in depth, NOT the safety argument. Everything below is read-only by
  # construction (see NO GIT WRITE, EVER, in the header), so a leaked GIT_* can
  # at worst produce a WRONG ANSWER from a docs-hygiene gate — never a damaged
  # repository. The scrub is here anyway because a wrong answer is still bad.
  unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
        GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR GIT_PREFIX

  # ---- THE SNAPSHOT IS MADE A REPOSITORY BY HAND: mkdir + printf, NO GIT -----
  # `git init` USED TO BE HERE AND IT CAUSED TWO LIVE INCIDENTS (TC-128). With
  # GIT_DIR exported — which a real linked-worktree pre-commit hook always does,
  # as an ABSOLUTE path — `git init "$SNAPSHOT"` does not create
  # `$SNAPSHOT/.git` at all. GIT_DIR wins, and git REINITIALISES THE REPOSITORY
  # GIT_DIR NAMES: on 2026-07-29 at 18:14:51 and again at 18:26:28 that was the
  # primary checkout, whose `.git/config` was rewritten with `bare = true`,
  # breaking every work-tree operation there (`git add`, `git commit`,
  # `git status` all refusing with "must be run in a work tree") while
  # `git rev-parse HEAD` still resolved, so it read as half-alive rather than
  # broken. Repaired both times with `git config core.bare false`; blast radius
  # assessed as exactly that one line, with refs, remote, identity and
  # `core.hooksPath` intact.
  #
  # THE FIX IS NOT A BETTER-ORDERED SCRUB, IT IS REMOVING THE ONLY WRITE. A guard
  # on the commit path must not be ABLE to damage the thing it guards. `git init`
  # was the single git call in this whole path that writes anything; with it gone
  # the write-safety is structural rather than dependent on getting an
  # environment scrub exactly right forever.
  #
  # MEASURED: this is all `git rev-parse --show-toplevel` needs, from the root and
  # from any subdirectory. No git binary is invoked to build it.
  mkdir -p "$SNAPSHOT/.git/objects" "$SNAPSHOT/.git/refs/heads"
  printf 'ref: refs/heads/main\n' >"$SNAPSHOT/.git/HEAD"
  printf '[core]\n\trepositoryformatversion = 0\n\tbare = false\n' >"$SNAPSHOT/.git/config"

  cd "$SNAPSHOT"

  # ---- BELT AND BRACES: prove the snapshot is self-contained before using it --
  # Without a resolvable toplevel the manifest's `cd "$(git rev-parse
  # --show-toplevel)"` degrades to `cd ""` — a bash no-op returning 0 — and the
  # sweep would limp along on behaviour POSIX leaves unspecified. Worse, a git
  # call that resolved OUTSIDE the snapshot would silently answer about the wrong
  # tree. So require the snapshot to resolve to ITSELF, and refuse to proceed on
  # anything else. `rev-parse` is read-only, and `env -u` scrubs on the same
  # invocation so this check cannot be broken by a later edit moving a line.
  SNAP_TOP="$(env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
                  -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
                  -u GIT_COMMON_DIR -u GIT_PREFIX \
                  git rev-parse --show-toplevel 2>/dev/null || true)"
  if [ ! -d "$SNAPSHOT/.git" ] \
     || [ "$(readlink -f "${SNAP_TOP:-/nonexistent}")" != "$(readlink -f "$SNAPSHOT")" ]; then
    printf 'check-design-refs: NOT CHECKED — the index snapshot does not resolve to itself\n' >&2
    printf '  (toplevel came back as "%s"). Refusing to sweep, because a git call that\n' "$SNAP_TOP" >&2
    printf '  resolves outside the snapshot would answer about the wrong tree. Design\n' >&2
    printf '  coverage for this commit is UNVERIFIED (TC-92). Re-run after committing:\n' >&2
    printf '    scripts/check-design-refs.sh\n' >&2
    exit 0
  fi
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf 'check-design-refs: python3 not found; the state-file parser cannot run.\n' >&2
  printf '  A gate that cannot run must not report a pass (TC-37).\n' >&2
  exit 2
fi

ONLY_RELEASE="$ONLY_RELEASE" ONLY_SLICE="$ONLY_SLICE" python3 - <<'PY'
import glob
import json
import os
import re
import subprocess
import sys

ONLY_RELEASE = os.environ.get("ONLY_RELEASE") or ""
ONLY_SLICE = os.environ.get("ONLY_SLICE") or ""

MANIFEST = "scripts/commission-manifest.sh"
STATE_GLOB = "dev/plans/release-state-*.json"

# ---------------------------------------------------------------------------
# THE FROZEN BASELINE EXEMPTION TABLE
# ---------------------------------------------------------------------------
# Measured at 2671346d, the commit this check was written against: across all 14
# 0.8.20 ladder slices, EXACTLY TWO carry a token no design document mentions.
# Both slices are LANDED. Curating `design_refs` for them is the Steward's edit
# to the state file, not this check's, and back-linking them by hand is the
# superseded "Requirement traceability" blockquote. So they are enumerated here,
# individually, with a reason — and ANYTHING NOT IN THIS TABLE IS A HARD FAILURE.
#
# That asymmetry is the whole design: green today, and a new uncovered token goes
# red immediately, on a landed slice or a future one. The table is expected to
# SHRINK (when the Steward curates `design_refs` for either entry, its row goes
# stale and the check says so). Growing it is a deliberate, reviewable edit.
EXEMPTIONS = {
    ("0.8.20", 20, "TC-45"): (
        "LANDED. TC-45 is the supersession-terminal fix carried by R-20-DR; the "
        "dense_readiness design predates the id and the entry carries no "
        "design_refs. Curating it is the Steward's edit to the state file "
        "(TC-92 remedy (b)); hand-annotating the doc is the superseded remedy."
    ),
    ("0.8.20", 21, "ac_002"): (
        "LANDED. Slice 21's curated design_ref covers the TC-57 write-race leg "
        "and says nothing about the ac_002 oracle replacement, which is the "
        "second shape TC-92 records: one incidental match, honestly reported, "
        "easy to read past. Its design of record is the plan's requirements "
        "section, not a design memo."
    ),
}

# ---------------------------------------------------------------------------
# ANTI-DRIFT: the manifest's whole-token boundary, asserted not assumed.
# ---------------------------------------------------------------------------
# These two literals must appear verbatim in commission-manifest.sh's token_re().
# They are the ONLY piece of the manifest's matching logic reproduced here, and
# they are needed for exactly one job: deciding whether a CURATED document — one
# the manifest's walker may never open — names a token. If the manifest's
# boundary construction ever changes, this check would silently be answering a
# different question from the tool whose report it consumes, so it stops.
LOOKBEHIND = r"(?<![0-9A-Za-z_])"
LOOKAHEAD = r"(?![0-9A-Za-z_])"


def die(lines, code=2):
    for ln in lines:
        print(ln, file=sys.stderr)
    sys.exit(code)


def read(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


if not os.path.isfile(MANIFEST):
    die(["FAIL check-design-refs: `%s` is missing. This check reads that tool's" % MANIFEST,
         "  coverage report; with no tool there is nothing to check, and exiting 0",
         "  would vouch for nothing (TC-37)."])

MANIFEST_SRC = read(MANIFEST)
for literal, role in ((LOOKBEHIND, "leading"), (LOOKAHEAD, "trailing")):
    if literal not in MANIFEST_SRC:
        die(["FAIL check-design-refs: the manifest's %s whole-token BOUNDARY literal" % role,
             "  %r no longer occurs in `%s`." % (literal, MANIFEST),
             "  This check reproduces that boundary to test CURATED documents the",
             "  manifest's walker never opens. If the two constructions DRIFT, the check",
             "  answers a different question from the report it consumes — so it refuses",
             "  to run rather than pass on a guess (TC-37, TC-100).",
             "  Fix: update LOOKBEHIND/LOOKAHEAD in scripts/check-design-refs.sh in the",
             "  same commit that changes commission-manifest.sh's token_re()."])

m = re.search(r"^DESIGN_ROOTS\s*=\s*\(([^)]*)\)", MANIFEST_SRC, re.M)
if not m:
    die(["FAIL check-design-refs: cannot find `DESIGN_ROOTS = (...)` in `%s`." % MANIFEST,
         "  The scanned-roots bound is what decides whether a curated document is",
         "  reachable by the scan at all; guessing it would misreport coverage."])
DESIGN_ROOTS = tuple(re.findall(r'"([^"]+)"', m.group(1)))
if not DESIGN_ROOTS:
    die(["FAIL check-design-refs: `DESIGN_ROOTS` in `%s` parsed as EMPTY." % MANIFEST])


def token_re(tok):
    """The manifest's whole-token matcher, boundaries asserted above.

    Spelled out rather than written `\\b` for the same reason token_re() is:
    a token may BEGIN with a non-word character (`#18`), where `\\b` asserts the
    opposite of what is wanted.
    """
    return re.compile(LOOKBEHIND + re.escape(tok) + LOOKAHEAD)


def run_manifest(release, slice_no):
    """Drive the manifest for one slice. Returns (rc, stdout, stderr)."""
    proc = subprocess.run(
        ["bash", MANIFEST, release, str(slice_no)],
        capture_output=True, text=True, check=False)
    return proc.returncode, proc.stdout, proc.stderr


TOKENS_RE = re.compile(
    r"^\s*Selected by the slice's OWN tokens from the state file:\s*(.*?)\s*$", re.M)
UNMATCHED_RE = re.compile(
    r"^\s*->\s*NO design doc mentions:\s*(.*?)\.\s*That part", re.M)
ZERO_RE = re.compile(r"ZERO design docs matched .*? \(tokens:\s*(.*?)\)\.")


def split_tokens(blob):
    return [t.strip() for t in blob.split(",") if t.strip()]


def coverage(release, slice_no):
    """(all_tokens, uncovered_by_scan) as the MANIFEST reports them.

    Two shapes. Exit 0: the manifest lists the slice's tokens and, per token,
    the ones nothing mentions. Exit 1 with the vacuous-pass message: the design
    tier is empty by both routes, so EVERY token is uncovered — and that is the
    Slice 22 case verbatim, so it must be read rather than treated as a crash.
    """
    rc, out, err = run_manifest(release, slice_no)
    if rc == 0:
        tm = TOKENS_RE.search(out)
        if tm is None:
            die(["FAIL check-design-refs: `%s %s %s` exited 0 but printed no token line."
                 % (MANIFEST, release, slice_no),
                 "  The report this check reads has changed shape; it will not guess at",
                 "  coverage from a report it no longer understands (TC-37)."])
        tokens = split_tokens(tm.group(1))
        um = UNMATCHED_RE.search(out)
        return tokens, (split_tokens(um.group(1)) if um else [])
    zm = ZERO_RE.search(err) or ZERO_RE.search(out)
    if zm:
        tokens = split_tokens(zm.group(1))
        # "none derived" is the manifest's wording for a slice whose short/title
        # yield no token at all. There is then no id to demand a back-link for.
        if tokens == ["none derived"]:
            tokens = []
        return tokens, list(tokens)
    die(["FAIL check-design-refs: `%s %s %s` failed for a reason that is NOT a design"
         % (MANIFEST, release, slice_no),
         "  coverage gap (rc=%d). The gate cannot judge coverage while the manifest" % rc,
         "  itself is broken, and a pass here would be vacuous (TC-37). Its output:",
         *("    " + ln for ln in (err or out).splitlines()[:20])])


def curated_refs(entry):
    raw = entry.get("design_refs")
    return [p for p in raw if isinstance(p, str) and p.strip()] if isinstance(raw, list) else []


def curated_hit(refs, tok):
    """The first curated document that whole-token names `tok`, or None.

    This is remedy (b) made real. `design_refs` reaches ANYTHING in the
    checkout, including tiers the manifest's walker does not cover, so a curated
    document can carry a token the scan could never have seen. Without this the
    check would false-positive on exactly the slices `design_refs` was built for.
    """
    rx = token_re(tok)
    for path in refs:
        if os.path.isfile(path) and rx.search(read(path)):
            return path
    return None


states = sorted(glob.glob(STATE_GLOB))
if not states:
    die(["FAIL check-design-refs: ZERO release-state files under %s." % STATE_GLOB,
         "  The sweep loop never ran, so exiting 0 would vouch for nothing (TC-37)."])

if ONLY_RELEASE:
    want = "dev/plans/release-state-%s.json" % ONLY_RELEASE
    states = [s for s in states if s == want]
    if not states:
        die(["FAIL check-design-refs: no release-state file at `%s`." % want])

problems = []
checked_slices = 0
checked_tokens = 0
used_exemptions = set()
lines = []

for state_path in states:
    release = re.sub(r"^dev/plans/release-state-(.*)\.json$", r"\1", state_path)
    try:
        state = json.loads(read(state_path))
    except (OSError, ValueError) as exc:
        die(["FAIL check-design-refs: `%s` is not parseable as JSON: %s" % (state_path, exc)])
    ladder = state.get("ladder") or []
    if not ladder:
        die(["FAIL check-design-refs: `%s` declares an EMPTY ladder, so the sweep"
             % state_path,
             "  checked nothing while looking green (TC-37)."])
    for entry in ladder:
        slice_no = entry.get("slice")
        if ONLY_SLICE and str(slice_no) != str(ONLY_SLICE):
            continue
        tokens, uncovered = coverage(release, slice_no)
        refs = curated_refs(entry)
        checked_slices += 1
        checked_tokens += len(tokens)
        short = entry.get("short") or "?"
        resolved = []
        for tok in uncovered:
            path = curated_hit(refs, tok)
            if path is not None:
                resolved.append("    %-12s CURATED — named by `%s`" % (tok, path))
                continue
            key = (release, slice_no, tok)
            if key in EXEMPTIONS:
                used_exemptions.add(key)
                resolved.append("    %-12s EXEMPT (frozen baseline, TC-92) — %s"
                                % (tok, EXEMPTIONS[key]))
                continue
            resolved.append("    %-12s ** NO DESIGN COVERAGE **" % tok)
            problems.append((release, slice_no, short, tok, refs))
        head = "check-design-refs: %s Slice %-3s (%s) — %d token(s), %d matched" % (
            release, slice_no, short, len(tokens), len(tokens) - len(uncovered))
        if uncovered:
            head += ", %d unmatched" % len(uncovered)
        lines.append(head)
        lines.extend(resolved)

# A stale exemption is not a failure — removing coverage debt must never be the
# thing that turns the tree red — but it must be LOUD, or the table silently
# accretes rows nobody can justify and stops meaning anything.
stale = sorted(set(EXEMPTIONS) - used_exemptions)
for rel, sl, tok in stale:
    lines.append(
        "check-design-refs: STALE EXEMPTION — %s Slice %s / %s now has design coverage."
        % (rel, sl, tok))
    lines.append("    Remove its row from EXEMPTIONS in scripts/check-design-refs.sh.")

print("\n".join(lines))
print("check-design-refs: %d slice(s), %d declared token(s), %d exemption(s) used, "
      "%d gap(s)." % (checked_slices, checked_tokens, len(used_exemptions), len(problems)))

if checked_slices == 0:
    die(["FAIL check-design-refs: ZERO slices checked. A sweep that examined nothing",
         "  has not verified anything, whatever its exit code says (TC-37)."])

if problems:
    out = ["",
           "FAIL check-design-refs: %d requirement/TC token(s) have NO DESIGN OF RECORD"
           % len(problems),
           "  and no frozen exemption (TC-92)."]
    for release, slice_no, short, tok, refs in problems:
        out.append("")
        out.append("  %s Slice %s (%s) declares `%s`, and:" % (release, slice_no, short, tok))
        out.append("    * no document under %s whole-token matches it, and"
                   % ", ".join("`%s/`" % r for r in DESIGN_ROOTS))
        if refs:
            out.append("    * none of its %d curated `design_refs` document(s) names it:"
                       % len(refs))
            for p in refs:
                out.append("        %s" % p)
        else:
            out.append("    * the ladder entry carries no `design_refs`.")
    out += [
        "",
        "  A requirement id minted with no design of record is how a slice gets",
        "  commissioned on a required-reading list that is empty (the manifest's TC-37",
        "  hard failure, e.g. Slice 22) or thin but plausible-looking (Slice 21). Close",
        "  it NOW, while the knowledge is in hand — that is the whole point of firing at",
        "  mint time.",
        "",
        "  TWO WAYS TO FIX IT, in preference order:",
        "    1. BACK-LINK the id: name it in the design document the leg actually",
        "       depends on, under one of %s, so the" % ", ".join("`%s/`" % r for r in DESIGN_ROOTS),
        "       manifest's own selector finds it. Prefer this when the document is a",
        "       live design of record you are editing anyway.",
        "    2. CURATE `design_refs` on the ladder entry in the release-state file,",
        "       naming the document(s) to read — the route for a reserved-gap id whose",
        "       design predates it, and the only route to a document outside the scanned",
        "       roots or to a byte-pinned contract that can never be back-linked. The",
        "       cited document must NAME the token; a citation that does not is a",
        "       reading list, not coverage.",
        "",
        "  If NEITHER is possible — a landed slice whose ids postdate its design and",
        "  whose state file is not yours to edit — add an explicitly justified row to",
        "  EXEMPTIONS in scripts/check-design-refs.sh. That is a deliberate, reviewable",
        "  edit, never a silent skip, and never a blanket 'this slice is LANDED'.",
    ]
    die(out, code=1)
PY
