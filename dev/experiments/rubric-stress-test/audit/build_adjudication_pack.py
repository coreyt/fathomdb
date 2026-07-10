#!/usr/bin/env python3
"""Build a bounded adjudication pack for the v3 revision loop.

Selects a reproducible sample of candidate examples and attaches a SAFE +/-3 line
window (each line truncated to 220c by seed_window/parse) so two independent judges
can label them WITHOUT ever loading a raw transcript into context. Serves four
measurements at once:
  - C6 grounding      : are any of the 9 DQ-LIMITED-SAMPLE rows a real TP?
  - Q-DISC specificity: does the judge correctly REJECT good-behavior candidates?
  - Q-IRR             : inter-rater agreement across two independent judge runs.
  - positive controls : known-bad episode TP rows anchor the scale.

Emits audit/adjudication_pack.jsonl and audit/adjudication_pack.md.
Works only from detector output + windows. No git commits.
"""
import json, os, sys, io, contextlib
BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, BASE)
import parse  # noqa

EX = os.path.join(BASE, "audit", "rubric_audit_examples.jsonl")
CORPUS = "/home/coreyt/transcript-data"
RADIUS = 3

examples = [json.loads(l) for l in open(EX)]
by_det = {}
for e in examples:
    by_det.setdefault(e["detector"], []).append(e)

def pick(detector, n=None):
    rows = sorted(by_det.get(detector, []), key=lambda e: e["example_id"])
    return rows if n is None else rows[:n]

# --- sample -----------------------------------------------------------------
sample = []
# C6 grounding: ALL 9 limited-sample rows
for e in pick("DQ-LIMITED-SAMPLE"):
    sample.append(("c6-grounding", e))
# Specificity (good-behavior 0-precision detectors)
for e in pick("DQ-SHORTKNOWLEDGE", 8):
    sample.append(("specificity", e))
for e in pick("DQ-ASSUME-STRUCTURAL", 3):
    sample.append(("specificity", e))
for e in pick("DQ-STALE-VERSION", 2):
    sample.append(("specificity", e))
for e in pick("DQ-IGNOREDESIGN-STRUCTURAL", 1):
    sample.append(("specificity", e))
# Positive controls: confirmed-TP episode rows (known-bad anchors)
controls = sorted([e for e in examples if e.get("episode")], key=lambda e: e["example_id"])[:4]
for e in controls:
    sample.append(("positive-control", e))

def window(session, line):
    path = os.path.join(CORPUS, session + ".jsonl")
    if not os.path.exists(path):
        return None
    lo, hi = line - RADIUS, line + RADIUS
    out = []
    try:
        for r in parse.iter_file(path):
            if lo <= r["line_no"] <= hi:
                tag = "tres" if r["is_tool_result"] else ("HITL" if r["is_hitl"] else r["type"][:4])
                t = " ".join(r["text"].split())[:220]
                mark = ">>" if r["line_no"] == line else "  "
                out.append(f"{mark}{r['line_no']:>6} {tag:>4} {t}")
            if r["line_no"] > hi:
                break
    except Exception as ex:
        return f"(window error: {ex})"
    return "\n".join(out)

pack = []
for role, e in sample:
    win = window(e["source"]["session"], e["source"]["line"])
    pack.append(dict(
        example_id=e["example_id"], role=role, detector=e["detector"],
        mapped_criteria=e["mapped_criteria"], coverage_verdict=e["coverage_verdict"],
        failure_class=e["failure_class"], session=e["source"]["session"],
        line=e["source"]["line"], window=win,
    ))

with open(os.path.join(BASE, "audit", "adjudication_pack.jsonl"), "w") as f:
    for p in pack:
        f.write(json.dumps(p) + "\n")

# human-readable (windows hidden roles so judges are not primed)
with open(os.path.join(BASE, "audit", "adjudication_pack.md"), "w") as f:
    f.write("# Adjudication pack (v3 revision loop)\n\n")
    f.write(f"{len(pack)} candidates. Each carries a +/-3 line window (>> marks the hit line).\n\n")
    for i, p in enumerate(pack, 1):
        f.write(f"## [{i}] {p['example_id']}\n")
        f.write(f"- detector: `{p['detector']}`  mapped_criteria: {p['mapped_criteria']}  class: {p['failure_class']}\n\n")
        f.write("```\n" + (p["window"] or "(no window)") + "\n```\n\n")

missing = sum(1 for p in pack if not p["window"])
print(f"pack: {len(pack)} candidates  (c6={sum(1 for _,r in [(x['role'],x) for x in pack] if r['role']=='c6-grounding')} "
      f"spec={sum(1 for x in pack if x['role']=='specificity')} ctrl={sum(1 for x in pack if x['role']=='positive-control')})  "
      f"windows_missing={missing}")
