#!/usr/bin/env python3
"""Probe: D-self-correction family. Scans EXPERIMENT-split files for assistant-text
admissions of the assistant's OWN error. Reports hit counts per pattern + short snippets.
NEVER prints full lines; snippets truncated to 300 chars. Reads only script output."""
import json, re, sys, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

# Candidate signatures grouped. Word-boundaried, case-insensitive.
PATS = {
    "misunderstood":   re.compile(r"\bi\s+(?:mis-?understood|misunderstood)\b", re.I),
    "i_was_wrong":     re.compile(r"\bi\s+was\s+wrong\b", re.I),
    "that_was_incorrect": re.compile(r"\bthat\s+was\s+(?:incorrect|wrong)\b", re.I),
    "misread":         re.compile(r"\bi\s+(?:mis-?read|misread)\b", re.I),
    "let_me_correct":  re.compile(r"\blet\s+me\s+correct\b", re.I),
    "actually_correct":re.compile(r"\bactually(?:,)?\s+the\s+correct\b", re.I),
    "falsely_assumed": re.compile(r"\bi\s+(?:falsely|wrongly|incorrectly)\s+assumed\b", re.I),
    "closer_inspection":re.compile(r"\bon\s+closer\s+(?:inspection|look|reading)\b", re.I),
    "need_to_correct": re.compile(r"\bi\s+need\s+to\s+correct\b", re.I),
    "earlier_was_wrong":re.compile(r"\bmy\s+earlier\s+\w+(?:\s+\w+)?\s+was\s+wrong\b", re.I),
    "correcting_my":   re.compile(r"\bcorrecting\s+my\b", re.I),
    "i_apologize":     re.compile(r"\bi\s+apologi[sz]e\b", re.I),
    "you_re_right":    re.compile(r"\byou(?:'| a)re\s+(?:right|correct)\b", re.I),
    "my_mistake":      re.compile(r"\bmy\s+(?:mistake|error|bad)\b", re.I),
    "i_made_a_mistake":re.compile(r"\bi\s+made\s+(?:a|an)\s+(?:mistake|error)\b", re.I),
    "i_incorrectly":   re.compile(r"\bi\s+(?:incorrectly|mistakenly|erroneously)\s+\w+", re.I),
    "was_mistaken":    re.compile(r"\bi\s+was\s+mistaken\b", re.I),
    "let_me_reconsider":re.compile(r"\blet\s+me\s+recon(?:sider|check)\b", re.I),
    "i_overlooked":    re.compile(r"\bi\s+(?:overlooked|missed)\b", re.I),
    "to_correct_myself":re.compile(r"\bto\s+correct\s+myself\b", re.I),
    "scratch_that":    re.compile(r"\bscratch\s+that\b", re.I),
    "i_stand_corrected":re.compile(r"\bi\s+stand\s+corrected\b", re.I),
}

def iter_text_blocks(content):
    """Yield assistant text strings from a message.content (str or list)."""
    if isinstance(content, str):
        yield content
    elif isinstance(content, list):
        for b in content:
            if isinstance(b, dict) and b.get("type") == "text" and isinstance(b.get("text"), str):
                yield b["text"]

counts = collections.Counter()
per_pat_samples = collections.defaultdict(list)
files_scanned = 0
lines_scanned = 0
sidechain_hits = collections.Counter()

with open(SPLIT) as f:
    files = [l.strip() for l in f if l.strip()]

for path in files:
    files_scanned += 1
    try:
        fh = open(path, "r")
    except OSError:
        continue
    with fh:
        for lineno, raw in enumerate(fh, 1):
            if '"role"' not in raw and '"type"' not in raw:
                continue
            try:
                obj = json.loads(raw)
            except Exception:
                continue
            if obj.get("type") != "assistant":
                continue
            msg = obj.get("message") or {}
            if msg.get("role") != "assistant":
                continue
            lines_scanned += 1
            side = obj.get("isSidechain", False)
            for txt in iter_text_blocks(msg.get("content")):
                for name, pat in PATS.items():
                    m = pat.search(txt)
                    if m:
                        counts[name] += 1
                        if side:
                            sidechain_hits[name] += 1
                        if len(per_pat_samples[name]) < 3:
                            s = m.start()
                            snip = txt[max(0, s-80):s+120].replace("\n", " ")
                            per_pat_samples[name].append(
                                f"{path.split('/')[-1]}:{lineno} sc={side} | ...{snip[:300]}...")

print(f"files_scanned={files_scanned} assistant_lines={lines_scanned}")
print("=== HIT COUNTS (assistant-text only) ===")
for name, c in counts.most_common():
    print(f"{c:6d}  {name:20s}  (sidechain={sidechain_hits[name]})")
print("\n=== SAMPLE SNIPPETS (<=3 per pattern, top patterns) ===")
shown = 0
for name, _ in counts.most_common():
    for s in per_pat_samples[name]:
        print(f"[{name}] {s}")
        shown += 1
        if shown >= 15:
            break
    if shown >= 15:
        break
