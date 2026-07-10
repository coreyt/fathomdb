#!/usr/bin/env python3
"""Deterministic per-detector seed sample: first-N candidates (file order) per
detector. Prints detector, file, line, signal, conf, and the stored snippet.
Bounded output for labeling. Usage: python3 sample_seed.py <candidates.jsonl> [N]
"""
import sys, json, collections

path = sys.argv[1]
N = int(sys.argv[2]) if len(sys.argv) > 2 else 10
per = collections.defaultdict(list)
with open(path) as fh:
    for line in fh:
        c = json.loads(line)
        d = c["detector"]
        if len(per[d]) < N:
            per[d].append(c)

for d in sorted(per):
    print(f"\n########## {d}  (showing {len(per[d])}) ##########")
    for i, c in enumerate(per[d]):
        b = c["file"].split("/")[-1][:18]
        print(f"[{d}#{i}] conf={c['confidence_heuristic']} sig={c['matched_signal']} {b}:{c['line_no']} struct={c.get('structural_ok','-')} sub={c.get('is_subfile','-')}")
        print("    " + c["snippet"][:280])
