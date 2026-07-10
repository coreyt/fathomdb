#!/usr/bin/env python3
"""Recall test: do coverage detectors fire in the FOUR known-bad episode sessions?
Reads only coverage_candidates.jsonl (our own output) + prints compact rollups."""
import os, json, collections
HERE = os.path.dirname(os.path.abspath(__file__))
CAND = os.path.join(HERE, "out", "coverage_candidates.jsonl")

# episode -> (session/file substrings that localize it, detectors we'd HOPE fire)
EPISODES = {
    "E1_CR047_premise_delete": (["2fa060bc"], ["DQ-UNVERIFIED-METRIC", "DQ-SNAP-DISPOSITION", "DQ-INCORRECT", "DQ-SHORTKNOWLEDGE"]),
    "E2_30N_wrong_unit":       (["2fa060bc", "f57b5dee"], ["DQ-UNVERIFIED-METRIC", "DQ-DEPBLIND-RETRO", "DQ-SNAP-DISPOSITION"]),
    "E3_36h_silent_stall":     (["tmp-task", "task/"], ["SILENT-STALL"]),
    "E4_OPP12_design_drift":   ([], ["DQ-NETNEW-DRIFT", "DQ-IGNOREDESIGN-STRUCTURAL", "DQ-IGNOREDESIGN-TEXTUAL"]),
}

rows = [json.loads(l) for l in open(CAND)]
print(f"total candidate rows: {len(rows)}\n")

# E1/E2: by session substring
for ep, (subs, dets) in EPISODES.items():
    print(f"=== {ep} ===")
    if subs:
        sel = [r for r in rows if any(s in (r.get("file", "") + r.get("parent_session", "")) for s in subs)]
        by_det = collections.Counter(r["detector"] for r in sel)
        print(f"  rows in localizing sessions ({subs}): {len(sel)}")
        for d in dets:
            hits = [r for r in sel if r["detector"] == d]
            mark = "HIT" if hits else "MISS"
            print(f"  [{mark}] {d}: {len(hits)}")
            for r in hits[:2]:
                print(f"        L{r['line_no']} sig={r['matched_signal']!r} :: {r['snippet'][:130]}")
    else:
        # E4: OPP-12 markers appear by content; show netnew-drift rows globally
        for d in dets:
            hits = [r for r in rows if r["detector"] == d]
            print(f"  {d}: {len(hits)} rows across {len(set(r['parent_session'] for r in hits))} sessions")
            for r in hits[:2]:
                print(f"        {os.path.basename(r['file'])[:16]} L{r['line_no']} sig={r['matched_signal']!r} :: {r['snippet'][:120]}")
    print()

# SILENT-STALL global: biggest gaps
ss = sorted((r for r in rows if r["detector"] == "SILENT-STALL"),
            key=lambda r: r.get("gap_hours", 0), reverse=True)
print("=== SILENT-STALL: top gaps (episode-3 shape = ~27.8h) ===")
for r in ss[:8]:
    print(f"  gap={r.get('gap_hours')}h spawn={r.get('spawn_anchored')} {os.path.basename(r['file'])[:22]} L{r['line_no']} :: {r['snippet'][:90]}")
