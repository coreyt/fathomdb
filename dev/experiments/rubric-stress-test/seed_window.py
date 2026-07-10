#!/usr/bin/env python3
"""Print ONLY a +/-3 normalized-line window around one candidate.
Usage: python3 seed_window.py <file> <line_no> [radius]
Never prints raw multi-KB blobs: every line truncated to 220 chars.
"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import parse

f = sys.argv[1]
ln = int(sys.argv[2])
rad = int(sys.argv[3]) if len(sys.argv) > 3 else 3
lo, hi = ln - rad, ln + rad
for r in parse.iter_file(f):
    if lo <= r["line_no"] <= hi:
        tag = r["type"][:4]
        if r["is_tool_result"]:
            tag = "tres"
        elif r["is_hitl"]:
            tag = "HITL"
        t = " ".join(r["text"].split())[:220]
        mark = ">>" if r["line_no"] == ln else "  "
        print(f"{mark}{r['line_no']:>6} {tag:>4} {t}")
    if r["line_no"] > hi:
        break
