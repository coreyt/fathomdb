#!/usr/bin/env python3
"""Probe: C-review-catch family. Scans EXPERIMENT split only.
Targets three line-classes:
  - toolUseResult (codex / code-review / ReportFindings output)
  - assistant-text (summaries of review verdicts / BLOCK / CONCERN)
  - assistant tool_use with name Codex/code-review/ReportFindings (invocations)
Reports HIT COUNTS per signature and up to N short snippets.
Never prints raw multi-KB blobs — every snippet truncated to 300 chars.
"""
import json, re, sys, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

# ---- candidate signatures (compiled) ----
SIGS = {
    # verdict tokens
    "BLOCK":            re.compile(r"\bBLOCK(?:ER|ING|S|ED)?\b"),
    "CONCERN":          re.compile(r"\bCONCERN(?:S)?\b"),
    "verdict_word":     re.compile(r"\bverdict\b", re.I),
    "must_fix":         re.compile(r"\bmust[- ]fix\b", re.I),
    # finding IDs
    "id_F":             re.compile(r"\bF\d{1,2}\b"),
    "id_RVEQ":          re.compile(r"\bR-[A-Z]{2,5}-\d+\b"),
    "id_RC":            re.compile(r"\bRC-\d+\b"),
    "id_CR":            re.compile(r"\bCR-\d+\b"),
    "id_P":             re.compile(r"\bP[012]\b"),   # severity P0/P1/P2
    "id_sev":           re.compile(r"\b(?:CRITICAL|HIGH|MEDIUM|LOW)\b"),
    # defect verbs
    "will_fail":        re.compile(r"\bwill fail\b", re.I),
    "contradicts":      re.compile(r"\bcontradict(?:s|ion|ed)?\b", re.I),
    "incorrect":        re.compile(r"\bincorrect(?:ly)?\b", re.I),
    "does_not":         re.compile(r"\bdoes(?:n't| not)\b", re.I),
    "would_break":      re.compile(r"\b(?:would|could|will) break\b", re.I),
    # review tooling names
    "codex":            re.compile(r"\bcodex\b", re.I),
    "code_review":      re.compile(r"\bcode[- ]review\b", re.I),
    "ReportFindings":   re.compile(r"ReportFindings"),
    "section9":         re.compile(r"\b§9\b|\bsection 9\b", re.I),
    # verdict tables / structured
    "finding_hdr":      re.compile(r"\b(?:Finding|Findings)\b\s*[:#]|\|\s*Severity\s*\|", re.I),
    "confirmed":        re.compile(r"\bCONFIRMED\b|\bPLAUSIBLE\b"),
    "clean_verdict":    re.compile(r"\bCLEAN\b"),
}

# tool names that are reviewers
REVIEW_TOOLS = re.compile(r"codex|code.?review|ReportFindings", re.I)

counts = collections.Counter()
# per-line-class counts
class_counts = collections.Counter()
snippets = collections.defaultdict(list)
MAX_SNIP = 6

def add_snip(sig, f, ln, cls, text):
    if len(snippets[sig]) < MAX_SNIP:
        t = re.sub(r"\s+", " ", text)[:280]
        snippets[sig].append((f.split("/")[-1][:24], ln, cls, t))

def scan_text(text, sigs_seen):
    for name, rx in SIGS.items():
        if rx.search(text):
            sigs_seen.add(name)

def content_to_texts(content):
    """Yield (subtype, text) tuples from a message.content (str or list)."""
    if content is None:
        return
    if isinstance(content, str):
        yield ("str", content)
        return
    if isinstance(content, list):
        for b in content:
            if not isinstance(b, dict):
                continue
            bt = b.get("type")
            if bt == "text":
                yield ("text", b.get("text", ""))
            elif bt == "tool_use":
                nm = b.get("name", "")
                inp = b.get("input", {})
                yield ("tool_use:" + str(nm), json.dumps(inp)[:4000])
            elif bt == "tool_result":
                c = b.get("content", "")
                if isinstance(c, list):
                    for cb in c:
                        if isinstance(cb, dict) and cb.get("type") == "text":
                            yield ("tool_result", cb.get("text", ""))
                elif isinstance(c, str):
                    yield ("tool_result", c)

def classify(role, subtype):
    if subtype.startswith("tool_result") or subtype == "toolUseResult":
        return "toolUseResult"
    if role == "assistant" and subtype in ("text",):
        return "assistant-text"
    if role == "assistant" and subtype.startswith("tool_use"):
        return "assistant-tooluse"
    if role == "user" and subtype in ("str", "text"):
        return "user-nontoolresult"
    return "other"

files = [l.strip() for l in open(SPLIT) if l.strip()]
nfiles = 0
nlines = 0
for f in files:
    try:
        fh = open(f, "r", errors="replace")
    except Exception:
        continue
    nfiles += 1
    for ln, raw in enumerate(fh, 1):
        nlines += 1
        raw = raw.strip()
        if not raw or raw[0] != "{":
            continue
        try:
            obj = json.loads(raw)
        except Exception:
            continue
        typ = obj.get("type")
        msg = obj.get("message") or {}
        role = msg.get("role") or typ
        # 1) toolUseResult top-level field (present on tool-result lines)
        tur = obj.get("toolUseResult")
        if tur is not None:
            txt = tur if isinstance(tur, str) else json.dumps(tur)[:8000]
            cls = "toolUseResult"
            seen = set(); scan_text(txt, seen)
            for s in seen:
                counts[s] += 1; class_counts[(s, cls)] += 1
                add_snip(s, f, ln, cls, txt)
        # 2) message.content blocks
        for subtype, text in content_to_texts(msg.get("content")):
            if not text:
                continue
            cls = classify(role, subtype)
            seen = set(); scan_text(text, seen)
            for s in seen:
                counts[s] += 1; class_counts[(s, cls)] += 1
                add_snip(s, f, ln, cls, text)
    fh.close()

print(f"# files scanned: {nfiles}  lines: {nlines}")
print("\n## SIGNATURE HIT COUNTS (line-occurrences)")
for s, c in counts.most_common():
    print(f"{c:8d}  {s}")

print("\n## HITS BY LINE-CLASS (top pairs)")
for (s, cls), c in sorted(class_counts.items(), key=lambda x: -x[1])[:40]:
    print(f"{c:8d}  {s:16s} {cls}")

print("\n## SAMPLE SNIPPETS (truncated 280c)")
for s in ["BLOCK", "CONCERN", "id_RVEQ", "id_RC", "id_CR", "will_fail",
          "contradicts", "confirmed", "section9", "clean_verdict", "finding_hdr"]:
    print(f"\n### {s}")
    for (fn, ln, cls, t) in snippets.get(s, []):
        print(f"  [{fn}:{ln} {cls}] {t}")
