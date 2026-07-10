#!/usr/bin/env python3
"""Fairer seed sample: up to N candidates per detector, each from a DISTINCT
session (first candidate seen per session), so precision isn't dominated by one
review-heavy file. Deterministic. Usage: python3 sample_seed_stride.py <cand.jsonl> [N]
"""
import sys, json, collections

path = sys.argv[1]
N = int(sys.argv[2]) if len(sys.argv) > 2 else 10
per = collections.defaultdict(list)
seen = collections.defaultdict(set)
with open(path) as fh:
    for line in fh:
        c = json.loads(line)
        d = c["detector"]
        s = c["session"]
        if s in seen[d]:
            continue
        if len(per[d]) < N:
            seen[d].add(s)
            per[d].append(c)

for d in sorted(per):
    print(f"\n########## {d}  ({len(per[d])} distinct-session) ##########")
    for i, c in enumerate(per[d]):
        b = c["file"].split("/")[-1][:18]
        print(f"[{d}#{i}] conf={c['confidence_heuristic']} sig={c['matched_signal']} {b}:{c['line_no']} struct={c.get('structural_ok','-')} sub={c.get('is_subfile','-')}")
        print("    " + c["snippet"][:280])
