#!/usr/bin/env python3
"""Probe for E-halt-scout-falsify signatures over the EXPERIMENT split only.

Family: an agent scouts read-only, then HALTS because a premise proved false
(CR-047 / 30-N save mechanism). Highest-signal = ratification->reversal pairs
(confirmed-wrong-then-fixed).

Scans the right JSONL line types:
  - assistant-text  : {type:text} blocks from assistant lines  (the HALT/retraction lives here)
  - user-hitl       : user lines whose content is NOT a tool_result (real HITL bounce)
  - tool-result     : user lines carrying tool_result blocks / toolUseResult (grep/read scouting output)
Handles content as str OR list-of-blocks. Reads ONLY the experiment split.
NEVER prints full lines; snippets truncated to 300 chars.
"""
import json, re, sys, collections, os

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

# --- candidate signature groups (case-insensitive unless noted) ---
SIGNALS = {
    # explicit halt verbs
    "HALT": re.compile(r"\bHALT(?:ED|ING)?\b"),
    "STOP_caps": re.compile(r"\bSTOP\b"),
    # premise-falsification
    "premise_false": re.compile(r"\b(premise|assumption|claim)\b[^.\n]{0,60}\b(is|was|proved|turned out|being)\b[^.\n]{0,30}\b(false|wrong|incorrect|invalid|untrue)\b", re.I),
    "does_not_exist": re.compile(r"\b(does\s+not|doesn'?t|no\s+longer)\s+exist(s)?\b", re.I),
    "not_wired": re.compile(r"\bnot\s+(wired|hooked|connected|plumbed|called|used|referenced)\b", re.I),
    "no_live_consumers": re.compile(r"\bno\s+(live\s+)?(consumers?|callers?|users?|references?|dependents?)\b", re.I),
    "found_live_callers": re.compile(r"\b(found|there\s+are|has|have)\b[^.\n]{0,30}\b(live\s+)?(callers?|consumers?|references?)\b", re.I),
    "already_superseded": re.compile(r"\balready\s+(superseded|replaced|removed|deleted|gone|handled)\b", re.I),
    "is_a_stub": re.compile(r"\b(is|it'?s|just|only|merely)\s+a?\s*stub\b", re.I),
    "duplicate": re.compile(r"\bis\s+a\s+duplicate\b", re.I),
    # scope/retraction reversal
    "scope_corrected": re.compile(r"\bSCOPE\s+CORRECT(?:ED|ION)\b"),
    "retracted": re.compile(r"\bretract(?:ed|ing|ion)?\b", re.I),
    "was_wrong": re.compile(r"\b(I\s+was|you\s+were|this\s+was|that\s+was|we\s+were)\s+WRONG\b", re.I),
    "conflation": re.compile(r"\bconflat(?:ion|ed|ing|es)\b", re.I),
    "correction_marker": re.compile(r"\b(correction|scratch that|on second look|actually,|turns out)\b", re.I),
    # scouting language
    "scout": re.compile(r"\bscout(?:ing|ed)?\b", re.I),
    "read_only_check": re.compile(r"\b(read[- ]only|before\s+(touch|chang|edit|writ)|first\s+verify|let me verify)\b", re.I),
}

# lines counting per-file to detect ratification->reversal proximity
def iter_lines(path):
    try:
        with open(path, "r", errors="replace") as f:
            for ln, raw in enumerate(f, 1):
                raw = raw.strip()
                if not raw:
                    continue
                try:
                    yield ln, json.loads(raw)
                except Exception:
                    continue
    except FileNotFoundError:
        return

def classify(obj):
    """Return (linetype, text) where linetype in assistant-text|user-hitl|tool-result|other."""
    if not isinstance(obj, dict):
        return "other", "", False
    t = obj.get("type")
    msg = obj.get("message") or {}
    role = msg.get("role")
    content = msg.get("content")
    tur = obj.get("toolUseResult")
    # extract text
    def blocks_text(blocks, want):
        out = []
        if isinstance(blocks, str):
            return blocks if want in ("any", "str") else ""
        if isinstance(blocks, list):
            for b in blocks:
                if not isinstance(b, dict):
                    continue
                bt = b.get("type")
                if bt == "text" and want in ("text", "any"):
                    out.append(b.get("text", ""))
                elif bt == "tool_result" and want in ("toolresult", "any"):
                    c = b.get("content")
                    if isinstance(c, str):
                        out.append(c)
                    elif isinstance(c, list):
                        for cc in c:
                            if isinstance(cc, dict) and cc.get("type") == "text":
                                out.append(cc.get("text", ""))
        return "\n".join(out)

    is_toolresult = False
    if isinstance(content, list):
        is_toolresult = any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content)

    if role == "assistant" or t == "assistant":
        return "assistant-text", blocks_text(content, "text"), obj.get("isSidechain", False)
    if role == "user" or t == "user":
        if is_toolresult or tur:
            txt = blocks_text(content, "toolresult")
            if tur and isinstance(tur, (str,)):
                txt += "\n" + tur
            elif tur:
                try:
                    txt += "\n" + json.dumps(tur)[:2000]
                except Exception:
                    pass
            return "tool-result", txt, obj.get("isSidechain", False)
        else:
            return "user-hitl", blocks_text(content, "str") if isinstance(content, str) else blocks_text(content, "text"), obj.get("isSidechain", False)
    return "other", "", obj.get("isSidechain", False)

def main():
    files = [l.strip() for l in open(SPLIT) if l.strip()]
    counts = collections.Counter()          # signal -> hits
    counts_by_linetype = collections.Counter()  # (signal,linetype)
    sidechain_hits = collections.Counter()  # signal -> hits on isSidechain lines
    files_with = collections.Counter()      # signal -> distinct files
    samples = collections.defaultdict(list)
    # per-file HALT presence for pairing
    files_with_halt = set()
    files_with_reversal = set()

    STRONG = {"HALT","scope_corrected","was_wrong","conflation","premise_false",
              "no_live_consumers","already_superseded","not_wired","is_a_stub","duplicate","retracted"}

    for path in files:
        seen_in_file = set()
        file_strong = False
        for ln, obj in iter_lines(path):
            lt, text, side = classify(obj)
            if not text or lt == "other":
                continue
            for name, rx in SIGNALS.items():
                m = rx.search(text)
                if m:
                    counts[name] += 1
                    counts_by_linetype[(name, lt)] += 1
                    if side:
                        sidechain_hits[name] += 1
                    if name not in seen_in_file:
                        seen_in_file.add(name)
                        files_with[name] += 1
                    if name in STRONG:
                        file_strong = True
                        if name == "HALT":
                            files_with_halt.add(path)
                        if name in ("scope_corrected","was_wrong","retracted","conflation","already_superseded"):
                            files_with_reversal.add(path)
                    if len(samples[name]) < 15:
                        s = m.start()
                        snip = text[max(0, s-60): s+120].replace("\n", " ")
                        snip = re.sub(r"\s+", " ", snip)[:300]
                        samples[name].append((os.path.basename(path)[:24], ln, lt, "SIDE" if side else "main", snip))

    print("=== HIT COUNTS (signal : total | distinct_files | sidechain_hits) ===")
    for name in SIGNALS:
        print(f"{name:22s}: {counts[name]:6d} | files={files_with[name]:4d} | side={sidechain_hits[name]:5d}")
    print("\n=== TOP signal x linetype ===")
    for (name, lt), c in counts_by_linetype.most_common(25):
        print(f"  {name:22s} {lt:14s} {c}")
    print(f"\n=== PAIRING: files with HALT={len(files_with_halt)}  files with reversal-marker={len(files_with_reversal)}  both={len(files_with_halt & files_with_reversal)}")
    print("\n=== SAMPLE SNIPPETS (<=300 char) ===")
    for name in ["HALT","scope_corrected","was_wrong","conflation","no_live_consumers",
                 "already_superseded","not_wired","is_a_stub","duplicate","premise_false","does_not_exist","retracted"]:
        for (fn, ln, lt, side, snip) in samples[name][:4]:
            print(f"[{name}] {fn}:{ln} {lt}/{side} :: {snip}")

if __name__ == "__main__":
    main()
