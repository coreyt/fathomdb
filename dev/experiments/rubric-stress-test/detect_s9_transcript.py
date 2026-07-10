#!/usr/bin/env python3
"""[D] detector for the v3.1 rubric amendment (§13.3) — the B1/B2 evidence-persistence
companion sub-check: "a §9 review transcript exists on disk for every landed slice."

Deterministic, 0-LLM, stdlib only. This is the mechanical half of B1 (verifier-
independence) and B2 (BLOCK-never-overridden): an absent transcript makes those HARD
invariants UNWITNESSED, because a recorded verdict-trail entry ("§9 PASS") is narration,
not the independent artifact (rubric B3/D5 applied to the gate's own evidence). The
0.8.19 pilot (TC-RUBRIC-7) found the transcripts were NOT on disk — this detector makes
that a mechanical check instead of a hand observation.

Verdict semantics (per landed slice):
  MET   — a §9 transcript file exists at a durable, release-namespaced repo-tracked path.
  UNMET — no transcript found (independence/no-override UNWITNESSED for that slice).
The subject-level B1/B2 [D] sub-check is UNMET if ANY landed slice is missing its transcript.

Usage:
  python3 detect_s9_transcript.py --release 0.8.19 [--repo <root>] [--slices 0,5,15] [--json]
Discovers landed slices from the STATUS board when --slices is omitted.
"""
import argparse, json, os, re, sys, glob, subprocess

# Canonical durable path (rubric v3.1 exemplar) + tolerated fallbacks. Ordered:
# the first is the one the amendment prescribes (repo-tracked, release-namespaced).
PATH_PATTERNS = [
    "dev/plans/runs/codex-s9-{release}-slice-{slice}.txt",
    "dev/plans/runs/codex-s9-{release}-slice-{slice}.md",
    "scratchpad/codex/{release}/slice-{slice}-review.txt",   # non-git-tracked (weaker; flagged)
    "scratchpad/codex/{release}/slice-{slice}-review.md",
]
REPO_TRACKED_PREFIX = "dev/"   # a transcript under scratchpad/ is not durably repo-tracked


def repo_root(explicit=None):
    if explicit:
        return os.path.abspath(explicit)
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"], text=True).strip()
    except Exception:
        return os.getcwd()


def discover_landed_slices(repo, release):
    """Deterministic best-effort: read the STATUS board for landed/PASS slice markers.
    Returns a sorted list of slice ids (strings). Empty if the board is absent."""
    board = os.path.join(repo, "dev/plans/runs", f"STATUS-{release}.md")
    slices = set()
    if os.path.exists(board):
        with open(board, encoding="utf-8", errors="replace") as f:
            text = f.read()
        # lines that mention a slice AND a landing/§9 signal
        for m in re.finditer(r"[Ss]lice[ -]?(\d+)", text):
            ctx = text[max(0, m.start() - 80): m.end() + 120]
            if re.search(r"§9|PASS|LANDED|landed|merged|GREEN|CLEAN", ctx):
                slices.add(m.group(1))
    return sorted(slices, key=lambda s: int(s))


def find_transcript(repo, release, sl):
    for pat in PATH_PATTERNS:
        rel = pat.format(release=release, slice=sl)
        # tolerate a hex-suffixed or globbed variant
        matches = glob.glob(os.path.join(repo, rel)) or (
            [os.path.join(repo, rel)] if os.path.exists(os.path.join(repo, rel)) else [])
        if matches:
            p = os.path.relpath(matches[0], repo)
            return p, p.startswith(REPO_TRACKED_PREFIX)
    return None, False


def check_s9_transcripts(release, repo=None, slices=None):
    repo = repo_root(repo)
    if not slices:
        slices = discover_landed_slices(repo, release)
    rows = []
    for sl in slices:
        path, repo_tracked = find_transcript(repo, release, sl)
        rows.append(dict(
            slice=sl,
            transcript_path=path,
            exists=path is not None,
            repo_tracked=repo_tracked,
            verdict="MET" if path is not None else "UNMET",
            note=None if repo_tracked or path is None
            else "transcript found only under non-git-tracked scratchpad/ (weak persistence)",
        ))
    missing = [r["slice"] for r in rows if not r["exists"]]
    weak = [r["slice"] for r in rows if r["exists"] and not r["repo_tracked"]]
    subcheck = "UNMET" if (not rows or missing) else "MET"
    return dict(
        criterion="B1/B2 [D] sub-check (v3.1 §13.3 — §9-transcript-exists-per-landed-slice)",
        release=release, repo=repo,
        slices_checked=[r["slice"] for r in rows],
        rows=rows, missing=missing, weak_persistence=weak,
        subcheck_verdict=subcheck,
        evidence=(f"{len(rows) - len(missing)}/{len(rows)} landed slices have a §9 transcript on disk"
                  + (f"; MISSING slices {missing}" if missing else "")
                  + (f"; WEAK (scratchpad-only) {weak}" if weak else "")
                  if rows else "no landed slices discovered (pass --slices to check explicitly)"),
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--release", required=True)
    ap.add_argument("--repo", default=None)
    ap.add_argument("--slices", default=None, help="comma-separated slice ids; else discover from STATUS board")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    slices = [s.strip() for s in a.slices.split(",")] if a.slices else None
    result = check_s9_transcripts(a.release, a.repo, slices)
    if a.json:
        print(json.dumps(result, indent=2))
    else:
        print(f"B1/B2 [D] sub-check ({result['release']}): {result['subcheck_verdict']}")
        print(f"  {result['evidence']}")
        for r in result["rows"]:
            mark = "OK " if r["verdict"] == "MET" else "!! "
            print(f"  {mark}slice {r['slice']}: {r['transcript_path'] or '(absent)'}"
                  + (f"  [{r['note']}]" if r.get("note") else ""))
    # exit 1 on UNMET so it can gate in CI / the harness [D] layer
    sys.exit(0 if result["subcheck_verdict"] == "MET" else 1)


if __name__ == "__main__":
    main()
