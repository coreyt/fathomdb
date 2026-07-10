#!/usr/bin/env python3
"""Probe 3: pin down the VERDICT vocabulary of real review-catch events.
Two reviewer output channels:
  (A) codex review stdout: Bash result whose command has 'codex exec' OR whose
      content is cat-ing a codex-review*.log. Search these for verdict tokens.
  (B) Skill code-review output.
  (C) assistant-text that REPORTS a review verdict (durable summary).
Emit hit counts for a *disambiguated* signature set + samples.
"""
import json, re, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"
files = [l.strip() for l in open(SPLIT) if l.strip()]

# real review-invocation command markers
CODEX_INVOKE = re.compile(r"codex\s+exec\b|codex-nostdin|codex\s+.*\breview\b", re.I)
CODEXLOG_READ = re.compile(r"codex-review[-\w]*\.log|REVLOG=|REV_LOG=", re.I)

# verdict tokens that mark a CATCH (a problem flagged) vs clean
VERDICT = {
    "BLOCK":       re.compile(r"\bBLOCK(?:ER|ING)?\b"),
    "CONCERN":     re.compile(r"\bCONCERN(?:S)?\b"),
    "CLEAN":       re.compile(r"\bCLEAN\b"),
    "verdict:":    re.compile(r"\bverdict\b\s*[:=]", re.I),
    "P0P1":        re.compile(r"\bP[01]\b"),
    "severity":    re.compile(r"\b(?:CRITICAL|HIGH|MEDIUM|LOW)\b"),
    "CONFIRMED":   re.compile(r"\bCONFIRMED\b"),
    "PLAUSIBLE":   re.compile(r"\bPLAUSIBLE\b"),
    "R_id":        re.compile(r"\bR-[A-Z0-9]{1,6}-\d+\b"),
    "CR_id":       re.compile(r"\bCR-\d+\b"),
    "RC_id":       re.compile(r"\bRC-\d+\b"),
    "Fn_id":       re.compile(r"\bF\d{1,2}\b"),
    "must":        re.compile(r"\bmust\b", re.I),
    "will_fail":   re.compile(r"\bwill fail\b|\bwould fail\b", re.I),
    "contradicts": re.compile(r"\bcontradict", re.I),
    "round_n":     re.compile(r"\bround\s*\d\b|\d\s*rounds?\b", re.I),
}

# assistant-text review-verdict reporting phrases
ATEXT = {
    "codex_verdict": re.compile(r"codex[^.]{0,40}\b(BLOCK|CONCERN|CLEAN|verdict|round)", re.I),
    "review_found":  re.compile(r"\b(?:codex|review(?:er)?|§9)\b[^.]{0,60}\b(found|flagged|caught|raised|BLOCK|CONCERN)", re.I),
    "blocks_resolved": re.compile(r"BLOCKs?\s+resolved|resolved.*BLOCK", re.I),
    "n_rounds":      re.compile(r"\b\d\s*(?:codex\s*)?rounds?\b", re.I),
}

def blocks(content):
    if isinstance(content, list):
        for b in content:
            if isinstance(b, dict):
                yield b

codex_out_counts = collections.Counter()
skill_out_counts = collections.Counter()
atext_counts = collections.Counter()
samples = collections.defaultdict(list)

def snip(bucket, tag, txt):
    if len(samples[(bucket,tag)]) < 4:
        samples[(bucket,tag)].append(re.sub(r"\s+"," ",txt)[:280])

n_codex_results = 0
n_skill_results = 0
for f in files:
    try:
        fh = open(f, errors="replace")
    except Exception:
        continue
    codex_ids = {}   # tool_use_id -> 'codex' | 'skill'
    for raw in fh:
        raw = raw.strip()
        if not raw or raw[0] != "{":
            continue
        try:
            obj = json.loads(raw)
        except Exception:
            continue
        msg = obj.get("message") or {}
        role = msg.get("role") or obj.get("type")
        for b in blocks(msg.get("content")):
            t = b.get("type")
            if t == "tool_use":
                nm = b.get("name","")
                inp = b.get("input",{}) or {}
                if nm == "Bash":
                    cmd = inp.get("command","")
                    if CODEX_INVOKE.search(cmd) or CODEXLOG_READ.search(cmd):
                        codex_ids[b.get("id")] = "codex"
                elif nm == "Skill":
                    if re.search(r"code-review|security-review", json.dumps(inp), re.I):
                        codex_ids[b.get("id")] = "skill"
            elif t == "tool_result":
                tuid = b.get("tool_use_id")
                if tuid in codex_ids:
                    which = codex_ids[tuid]
                    c = b.get("content","")
                    txt = c if isinstance(c,str) else json.dumps(c)
                    if which == "codex":
                        n_codex_results += 1
                        for tag, rx in VERDICT.items():
                            if rx.search(txt):
                                codex_out_counts[tag]+=1; snip("codex",tag,txt)
                    else:
                        n_skill_results += 1
                        for tag, rx in VERDICT.items():
                            if rx.search(txt):
                                skill_out_counts[tag]+=1; snip("skill",tag,txt)
            elif t == "text" and role == "assistant":
                txt = b.get("text","")
                for tag, rx in ATEXT.items():
                    if rx.search(txt):
                        atext_counts[tag]+=1; snip("atext",tag,txt)
    fh.close()

print(f"## codex-review results correlated: {n_codex_results}")
for tag,c in codex_out_counts.most_common():
    print(f"{c:6d}  {tag}")
print(f"\n## skill code-review results correlated: {n_skill_results}")
for tag,c in skill_out_counts.most_common():
    print(f"{c:6d}  {tag}")
print("\n## assistant-text verdict-reporting phrases")
for tag,c in atext_counts.most_common():
    print(f"{c:6d}  {tag}")

print("\n## SAMPLES")
for (bucket,tag) in [("codex","BLOCK"),("codex","R_id"),("codex","CONCERN"),
                     ("codex","CLEAN"),("skill","severity"),("skill","CONFIRMED"),
                     ("atext","codex_verdict"),("atext","review_found"),
                     ("atext","blocks_resolved")]:
    ss = samples.get((bucket,tag),[])
    if ss:
        print(f"\n### [{bucket}] {tag}")
        for s in ss:
            print("  -", s)
