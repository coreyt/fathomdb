#!/usr/bin/env python3
"""F1/F6 turn-LOCALIZED recall acid test.

The prior recall_combined.py scored a detector as a "catch" if it fired ANYWHERE in
a 72-87 file session-group. Firing inside a large episode group is a base-rate hit,
not recall: these detectors already fire in 55-77 of the corpus's session groups. This
script instead defines, per episode, the ACTUAL FAILING TURN (file + anchor line, with
a bounded line window = its immediate causal cluster) and scores ONLY detector rows that
overlap that window as recall. Group-level fires are reported SEPARATELY as
"fired-in-group (base-rate context)", never as a catch.

Anchors (grounded in coverage/out/episode_hits.jsonl, verified turn locations):
  E1 CR-047 premise-substitution : memex/2fa060bc L1610-1617  ("no live consumers" /
                                    "already superseded" premise -> wrong DELETE)
  E2 30-N wrong-unit-of-work     : memex/2fa060bc L1456-1495  ("~98 call sites" /
                                    "ScheduledTask is a duplicate" premise)
  E3 36-hour silent stall (ACID) : fathomdb/ebec94c7 L1226    (the ~35.7h wall-clock gap;
                                    also a 23.8h gap at L853; f57b5dee: none)
  E4 OPP-12 design drift  (ACID) : fathomdb/60b48af5 L2204 + L1745  and
                                    fathomdb/ebec94c7 L83        (exists-vs-net-new /
                                    contradicts-shipped audit turns)

HARD RULE honored: streams via parse.iter_file; prints only aggregates + short snippets.
"""
import sys, os, json, collections, re
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import parse
import detectors as AE
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import detectors_coverage as DC
from recall_combined import EPISODES, COV_BUCKET, files_for, load_recs, clip  # reuse group logic

HERE = os.path.dirname(os.path.abspath(__file__))

# Per-episode failing-turn anchors: (file_substring, anchor_line, window_lines)
ANCHORS = {
    "E1_CR047_premise_delete": [("memex/2fa060bc", 1613, 40)],
    "E2_30N_wrong_unit":       [("memex/2fa060bc", 1475, 40)],
    "E3_36h_silent_stall":     [("fathomdb/ebec94c7", 1226, 10),
                                ("fathomdb/ebec94c7", 853, 10)],
    "E4_OPP12_design_drift":   [("fathomdb/60b48af5", 2204, 40),
                                ("fathomdb/60b48af5", 1758, 40),
                                ("fathomdb/ebec94c7", 83, 40)],
}


def anchor_file(sub):
    """Resolve an anchor file-substring to a concrete .jsonl path in the manifest."""
    with open("/home/coreyt/transcript-data/manifest.tsv") as fh:
        for ln in fh:
            p = ln.split("\t", 1)[0].strip()
            if p.endswith(".jsonl") and sub in p:
                return p
    return None


def localized_fires(ep):
    """Return (localized_catches, anchor_desc). A catch = detector row whose line_no is
    within [anchor-W, anchor+W] of an anchor in the anchor file."""
    catches = []       # (source, detector/family, file, line, sig, snip, gap_hours)
    seen_files = {}
    for sub, aline, W in ANCHORS[ep]:
        p = anchor_file(sub)
        if not p:
            continue
        recs = seen_files.get(p)
        if recs is None:
            recs = load_recs(p)
            seen_files[p] = recs
        lo, hi = aline - W, aline + W
        for c in AE.detect_file(recs):
            ln = c.get("line_no") or 0
            if lo <= ln <= hi:
                catches.append(("A-E", c.get("family", "?"), os.path.basename(p)[:16],
                                ln, clip(c.get("matched_signal", ""), 30),
                                clip(c.get("snippet") or "", 90), None))
        for c in DC.detect_file(recs, p):
            if c.get("family") == "ERROR":
                continue
            ln = c.get("line_no") or 0
            if lo <= ln <= hi:
                catches.append(("COV:" + COV_BUCKET.get(c["detector"], "structural"),
                                c.get("detector", "?"), os.path.basename(p)[:16],
                                ln, clip(c.get("matched_signal", ""), 30),
                                clip(c.get("snippet", ""), 90), c.get("gap_hours")))
    return catches


def group_baserate(subs):
    """Total fires per detector across the WHOLE session-group (base-rate context)."""
    fs = files_for(subs)
    cov = collections.Counter()
    ae = collections.Counter()
    for p in fs:
        recs = load_recs(p)
        if not recs:
            continue
        for c in AE.detect_file(recs):
            ae[c.get("family", "?")] += 1
        for c in DC.detect_file(recs, p):
            if c.get("family") != "ERROR":
                cov[c.get("detector", "?")] += 1
    return len(fs), ae, cov


def main():
    # corpus-wide base rate: #session-groups each detector fires in (from summary.json)
    summ_path = os.path.join(HERE, "out", "coverage_candidates.jsonl.summary.json")
    corpus_sessions = {}
    if os.path.exists(summ_path):
        s = json.load(open(summ_path))
        corpus_sessions = {k: v["sessions"] for k, v in s.get("per_detector", {}).items()}

    result = {}
    print("=" * 96)
    print("TURN-LOCALIZED RECALL  (catches = fires inside the failing-turn window only)")
    print("=" * 96)
    for ep in ANCHORS:
        catches = localized_fires(ep)
        nfiles, ae_grp, cov_grp = group_baserate(EPISODES[ep])
        # summarize localized catches per detector
        loc_by_det = collections.Counter(c[1] for c in catches)
        print(f"\n### {ep}")
        print(f"  anchors: {[(a[0], a[1], '±%d' % a[2]) for a in ANCHORS[ep]]}")
        print(f"  --- LOCALIZED on-target catches ({len(catches)}) ---")
        if not catches:
            print("    (none at the failing turn)")
        for src, det, f, ln, sig, snip, gap in catches[:24]:
            g = f" gap={gap}h" if gap else ""
            print(f"    [{src}] {f} L{ln} {det}{g} sig={sig!r} :: {snip}")
        # base-rate context: same detectors across the whole group
        print(f"  --- fired-in-group (BASE RATE across {nfiles} files; NOT a catch) ---")
        merged = {}
        for det, n in cov_grp.most_common():
            merged[det] = {"in_group": n, "localized": loc_by_det.get(det, 0),
                           "corpus_session_groups": corpus_sessions.get(det)}
        for det, d in list(merged.items())[:14]:
            print(f"    {det:<34} in_group={d['in_group']:<4} localized={d['localized']:<3} "
                  f"corpus_groups={d['corpus_session_groups']}")
        result[ep] = {
            "anchors": ANCHORS[ep],
            "localized_catches": [
                {"source": c[0], "detector": c[1], "file": c[2], "line": c[3],
                 "signal": c[4], "gap_hours": c[6]} for c in catches],
            "localized_by_detector": dict(loc_by_det),
            "group_files": nfiles,
            "group_by_detector": dict(cov_grp),
            "ae_group_by_family": dict(ae_grp),
        }
    with open(os.path.join(HERE, "out", "recall_localized.json"), "w") as fh:
        json.dump(result, fh, indent=2)
    print("\nwrote out/recall_localized.json")


if __name__ == "__main__":
    main()
