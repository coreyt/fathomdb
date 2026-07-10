#!/usr/bin/env python3
"""Probe 2: identify the STRUCTURE of real reviewer events, so detectors can
exclude file-read/edit echoes. Three questions:
  (a) How is codex invoked?  (Bash command shapes containing 'codex')
  (b) What does a codex RESULT look like (stdout shape)?
  (c) ReportFindings tool_use frequency + input keys.
  (d) code-review / Skill invocations.
Correlate tool_use.id -> tool_result.tool_use_id to tag reviewer outputs.
"""
import json, re, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"
files = [l.strip() for l in open(SPLIT) if l.strip()]

codex_cmds = collections.Counter()
codex_cmd_samples = []
reportfindings = 0
rf_input_keys = collections.Counter()
skill_review = collections.Counter()
tool_use_names = collections.Counter()
# reviewer tool_use ids -> label
reviewer_result_samples = []
# how are Read/Edit results shaped (to build exclusion)
read_result_shape = collections.Counter()

def blocks(content):
    if isinstance(content, list):
        for b in content:
            if isinstance(b, dict):
                yield b

for f in files:
    try:
        fh = open(f, errors="replace")
    except Exception:
        continue
    reviewer_ids = {}
    for raw in fh:
        raw = raw.strip()
        if not raw or raw[0] != "{":
            continue
        try:
            obj = json.loads(raw)
        except Exception:
            continue
        msg = obj.get("message") or {}
        for b in blocks(msg.get("content")):
            t = b.get("type")
            if t == "tool_use":
                nm = b.get("name", "")
                tool_use_names[nm] += 1
                inp = b.get("input", {}) or {}
                if nm == "Bash":
                    cmd = inp.get("command", "")
                    if re.search(r"\bcodex\b", cmd):
                        # capture the leading token pattern
                        key = re.sub(r"\s+", " ", cmd)[:60]
                        codex_cmds[key] += 1
                        if len(codex_cmd_samples) < 8:
                            codex_cmd_samples.append(re.sub(r"\s+"," ",cmd)[:200])
                        reviewer_ids[b.get("id")] = "codex"
                if nm == "ReportFindings":
                    reportfindings += 1
                    for k in inp.keys():
                        rf_input_keys[k] += 1
                    reviewer_ids[b.get("id")] = "ReportFindings"
                if nm in ("Skill", "Task"):
                    s = json.dumps(inp)[:200]
                    if re.search(r"code.?review|codex|security-review", s, re.I):
                        skill_review[nm] += 1
                        reviewer_ids[b.get("id")] = "skill-review"
            elif t == "tool_result":
                tuid = b.get("tool_use_id")
                if tuid in reviewer_ids:
                    c = b.get("content", "")
                    txt = c if isinstance(c, str) else json.dumps(c)[:1500]
                    if len(reviewer_result_samples) < 8:
                        reviewer_result_samples.append(
                            (reviewer_ids[tuid], re.sub(r"\s+"," ",txt)[:260]))
    fh.close()

print("## tool_use name frequency (top 25)")
for nm, c in tool_use_names.most_common(25):
    print(f"{c:7d}  {nm}")

print(f"\n## ReportFindings tool_use count: {reportfindings}")
print("   input keys:", dict(rf_input_keys))

print(f"\n## Skill/Task review-ish invocations: {dict(skill_review)}")

print(f"\n## codex Bash command shapes (top 15)")
for k, c in codex_cmds.most_common(15):
    print(f"{c:6d}  {k}")
print("\n## codex command samples:")
for s in codex_cmd_samples:
    print("  -", s)

print("\n## reviewer RESULT samples (correlated by id):")
for label, s in reviewer_result_samples:
    print(f"  [{label}] {s}")
