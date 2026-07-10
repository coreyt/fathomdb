#!/usr/bin/env python3
"""Probe 4: measure disambiguation power of a REVIEW-CONTEXT proximity anchor.
Hypothesis: real review-catch events have a verdict/defect token within ~120
chars of a review-context anchor (codex|§9|code-review|reviewer|review round|
ReportFindings|verdict). File-content echoes (plan docs, ledgers) do NOT.
Measure: for BLOCK/CONCERN/CR_id/R_id, split hits into anchored vs bare, by
line-class. Also test the exclusion of Read/Edit tool_result echoes.
"""
import json, re, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"
files = [l.strip() for l in open(SPLIT) if l.strip()]

ANCHOR = re.compile(r"codex|§9|section 9|code[- ]review|reviewer|review round|"
                    r"ReportFindings|\bverdict\b|round[- ]\d|design review|"
                    r"security[- ]review|§9 gate", re.I)

TOKENS = {
    "BLOCK":   re.compile(r"\bBLOCK(?:ER|ING|S)?\b"),
    "CONCERN": re.compile(r"\bCONCERN(?:S)?\b"),
    "CR_id":   re.compile(r"\bCR-\d+\b"),
    "R_id":    re.compile(r"\bR-[A-Z0-9]{1,6}-\d+\b"),
    "flagged": re.compile(r"\b(flagged|caught|surfaced|raised)\b", re.I),
    "verdict_flip": re.compile(r"verdict[^.]{0,30}(flip|flag|BLOCK|CONCERN)", re.I),
}

def blocks(content):
    if isinstance(content, list):
        for b in content:
            if isinstance(b, dict):
                yield b

# is this tool_result a Read/Edit echo (embeds file content)?
FILE_ECHO = re.compile(r'"filePath"|"file":\s*\{|"oldString"|"newString"|"originalFile"')

anchored = collections.Counter()
bare = collections.Counter()
by_class_anchored = collections.Counter()
echo_hits = collections.Counter()
samples_anchored = collections.defaultdict(list)

def near(text, m):
    a=max(0,m.start()-120); b=min(len(text),m.end()+120)
    return ANCHOR.search(text[a:b]) is not None

for f in files:
    try:
        fh=open(f,errors="replace")
    except Exception:
        continue
    for raw in fh:
        raw=raw.strip()
        if not raw or raw[0]!="{":
            continue
        try:
            obj=json.loads(raw)
        except Exception:
            continue
        msg=obj.get("message") or {}
        role=msg.get("role") or obj.get("type")
        # gather (cls, text, is_echo) units
        units=[]
        tur=obj.get("toolUseResult")
        if tur is not None:
            txt=tur if isinstance(tur,str) else json.dumps(tur)
            units.append(("toolUseResult", txt, bool(FILE_ECHO.search(txt))))
        for b in blocks(msg.get("content")):
            t=b.get("type")
            if t=="text" and role=="assistant":
                units.append(("assistant-text", b.get("text",""), False))
            elif t=="text" and role=="user":
                units.append(("user-nontoolresult", b.get("text",""), False))
            elif t=="tool_result":
                c=b.get("content","")
                txt=c if isinstance(c,str) else json.dumps(c)
                units.append(("tool_result", txt, bool(FILE_ECHO.search(txt))))
            elif t=="tool_use":
                units.append(("assistant-tooluse", json.dumps(b.get("input",{})), True))
        if isinstance(msg.get("content"),str) and role=="user":
            units.append(("user-nontoolresult", msg["content"], False))
        for cls,txt,is_echo in units:
            if not txt: continue
            for tag,rx in TOKENS.items():
                for m in rx.finditer(txt):
                    if near(txt,m):
                        anchored[tag]+=1
                        by_class_anchored[(tag,cls)]+=1
                        if is_echo: echo_hits[tag]+=1
                        if cls=="assistant-text" and len(samples_anchored[tag])<4:
                            a=max(0,m.start()-90); b2=min(len(txt),m.end()+90)
                            samples_anchored[tag].append(re.sub(r"\s+"," ",txt[a:b2]))
                    else:
                        bare[tag]+=1
                    break  # count each token/unit once
    fh.close()

print("## anchored vs bare (per token) — anchored = review-context within 120c")
print(f"{'token':14s} {'anchored':>9s} {'bare':>9s} {'echo(of anch)':>13s}")
for tag in TOKENS:
    print(f"{tag:14s} {anchored[tag]:9d} {bare[tag]:9d} {echo_hits[tag]:13d}")

print("\n## anchored hits by line-class")
for (tag,cls),c in sorted(by_class_anchored.items(),key=lambda x:-x[1])[:24]:
    print(f"{c:7d}  {tag:8s} {cls}")

print("\n## anchored assistant-text samples (±90c window)")
for tag in ["BLOCK","CONCERN","CR_id","R_id","flagged","verdict_flip"]:
    ss=samples_anchored.get(tag,[])
    if ss:
        print(f"\n### {tag}")
        for s in ss: print("  -", s[:260])
