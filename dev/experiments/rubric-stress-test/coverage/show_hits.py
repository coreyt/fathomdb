#!/usr/bin/env python3
"""Show episode hits for a given session substring + optional sig filter.
Usage: show_hits.py <session_substr> [sig_prefix]"""
import sys, os, json
HITS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "out", "episode_hits.jsonl")
sub = sys.argv[1]
sigf = sys.argv[2] if len(sys.argv) > 2 else ""
rows = []
with open(HITS) as fh:
    for ln in fh:
        o = json.loads(ln)
        if sub in o["file"] and o["sig"].startswith(sigf):
            rows.append(o)
rows.sort(key=lambda o: (o["file"], o["line_no"]))
for o in rows:
    fb = os.path.basename(o["file"])
    hitl = "HITL" if o["is_hitl"] else ("SUB" if o["is_subfile"] else o["type"][:4])
    tools = ",".join(o["tools"])[:30]
    print(f'{o["sig"]:22s} {fb[:20]:20s} L{o["line_no"]:<6d} {o["ts"][:19]} {hitl:5s} [{tools}] {o["snip"][:150]}')
print(f"\n{len(rows)} hits")
