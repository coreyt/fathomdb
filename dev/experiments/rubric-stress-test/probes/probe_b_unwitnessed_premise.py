#!/usr/bin/env python3
"""Probe for family B: PREMISE-SUBSTITUTION / UNWITNESSED-PREMISE.

Scans EXPERIMENT-split transcripts for load-bearing factual assertions made in
assistant text with NO cited witness (no adjacent tool_use / file evidence), esp.
the RCA-flagged phrases: "no live consumers", "already superseded", "is a duplicate",
"is the cost center", "owned by X now", "~N sites/call sites".

Also flags candidate REVERSALS later in the same session (the high-value subset).

Reads ONLY script output. Never prints full lines; snippets truncated to <=300 chars.
"""
import json, re, sys, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

# ---- candidate premise signatures (the load-bearing claims) ----
PREMISE_PATS = {
    "no_consumers": re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+|real\s+|active\s+)?consumers?\b", re.I),
    "no_callers":   re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+)?(?:callers?|call\s?sites?|references?|usages?|users)\b", re.I),
    "already_superseded": re.compile(r"\b(?:already\s+)?supersed(?:ed|es)\b", re.I),
    "is_a_duplicate": re.compile(r"\bis\s+(?:a\s+)?duplicate\b|\bare\s+duplicates?\b|\bis\s+redundant\b", re.I),
    "is_the_cost_center": re.compile(r"\bis\s+the\s+cost\s+center\b|\bhot\s?path\b.{0,20}\bcost\b", re.I),
    "owned_by_now": re.compile(r"\bowned\s+by\s+\w+\s+now\b|\bnow\s+owned\s+by\b|\bhandled\s+by\s+\w+\s+now\b", re.I),
    "n_sites": re.compile(r"[~≈]\s?\d{1,4}\s+(?:call\s?)?sites?\b|\b\d{1,4}\s+call\s?sites?\b|\b\d{1,4}\s+usages?\b", re.I),
    "nothing_uses": re.compile(r"\bnothing\s+(?:else\s+)?(?:uses|references|calls|depends\s+on|reads)\b", re.I),
    "safe_to_delete": re.compile(r"\bsafe\s+to\s+(?:delete|remove|drop)\b|\bcan\s+(?:be\s+)?(?:safely\s+)?(?:delete|remove|drop)", re.I),
    "not_used_anywhere": re.compile(r"\bnot\s+used\s+anywhere\b|\bunused\b|\bdead\s+code\b|\bnever\s+(?:called|used|referenced)\b", re.I),
    "is_unreachable": re.compile(r"\bunreachable\b|\bnever\s+reached\b", re.I),
}

# hedges that indicate the assertion IS witnessed / uncertain (reduce FP)
WITNESS_HEDGE = re.compile(
    r"\b(?:grep|rg|ripgrep|search(?:ed|ing)?|confirm(?:ed)?|verif(?:y|ied)|checked|"
    r"per\s+the|according\s+to|as\s+shown|the\s+(?:grep|search|output|results?)\s+show|"
    r"i\s+(?:ran|searched|grepped|checked))\b", re.I)
UNCERTAIN = re.compile(r"\b(?:I\s+think|likely|probably|might|maybe|appears?\s+to|seems?\s+to|assume|guess|not\s+sure|unsure)\b", re.I)

# ---- reversal signatures (self-correction / HITL bounce) ----
REVERSAL_PATS = re.compile(
    r"\b(?:actually|turns?\s+out|i\s+was\s+wrong|correction|scope\s+corrected|"
    r"i\s+stand\s+corrected|on\s+closer\s+look|in\s+fact\s+there|"
    r"there\s+(?:are|is)\s+(?:still\s+)?(?:live\s+)?consumers?|"
    r"is\s+(?:actually\s+)?(?:still\s+)?(?:used|referenced|called)|"
    r"not\s+(?:a\s+)?duplicate|wasn'?t\s+superseded|still\s+in\s+use|"
    r"wrong\s+about|my\s+earlier\s+claim|revert|retract)\b", re.I)


def iter_assistant_text(content):
    """Yield text from an assistant message content (string or block list)."""
    if isinstance(content, str):
        yield content, False  # (text, has_tool_use_sibling)
        return
    if isinstance(content, list):
        has_tool = any(isinstance(b, dict) and b.get("type") == "tool_use" for b in content)
        for b in content:
            if isinstance(b, dict) and b.get("type") == "text":
                yield b.get("text", ""), has_tool


def user_realturn_text(content):
    """Return HITL text if this user line is a REAL turn (not a tool_result), else None."""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        if any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content):
            return None
        parts = [b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text"]
        return " ".join(parts) if parts else None
    return None


def main():
    files = [l.strip() for l in open(SPLIT) if l.strip()]
    premise_counts = collections.Counter()
    witnessed = collections.Counter()      # premise hit but witness hedge nearby
    unwitnessed = collections.Counter()    # premise hit, no witness, no tool_use sibling
    reversal_sessions = set()
    premise_sessions = collections.defaultdict(list)  # session -> [(pat,file,line,snip)]
    samples = []
    files_scanned = 0

    for fp in files:
        try:
            fh = open(fp, encoding="utf-8")
        except OSError:
            continue
        files_scanned += 1
        session = fp.split("/")[-1]
        for lineno, raw in enumerate(fh, 1):
            raw = raw.strip()
            if not raw or '"' not in raw:
                continue
            # cheap prefilter
            try:
                obj = json.loads(raw)
            except Exception:
                continue
            if not isinstance(obj, dict):
                continue
            t = obj.get("type")
            msg = obj.get("message") or {}
            role = msg.get("role")
            content = msg.get("content")
            if content is None:
                continue

            if t == "assistant" or role == "assistant":
                for text, has_tool in iter_assistant_text(content):
                    if not text:
                        continue
                    for name, pat in PREMISE_PATS.items():
                        m = pat.search(text)
                        if not m:
                            continue
                        premise_counts[name] += 1
                        # window around the match for witness/uncertain check
                        s = max(0, m.start() - 160); e = min(len(text), m.end() + 160)
                        win = text[s:e]
                        wit = bool(WITNESS_HEDGE.search(win)) or has_tool
                        if wit:
                            witnessed[name] += 1
                        else:
                            unwitnessed[name] += 1
                            premise_sessions[session].append((name, fp, lineno))
                            if len(samples) < 15:
                                snip = re.sub(r"\s+", " ", win)[:280]
                                samples.append(f"[{name}] {session}:{lineno} :: {snip}")
                    # reversal detection (any assistant text)
                    if REVERSAL_PATS.search(text):
                        reversal_sessions.add(session)

            elif t == "user" or role == "user":
                ut = user_realturn_text(content)
                if ut and REVERSAL_PATS.search(ut):
                    reversal_sessions.add(session)  # HITL bounce reversal

    # high-value: unwitnessed premise in a session that ALSO has a later reversal
    hv_sessions = [s for s in premise_sessions if s in reversal_sessions]

    print(f"FILES_SCANNED={files_scanned}/{len(files)}")
    print("\n== PREMISE HITS (all assistant text) ==")
    for k in sorted(premise_counts, key=lambda x: -premise_counts[x]):
        print(f"  {k:22} total={premise_counts[k]:5} witnessed={witnessed[k]:5} UNWITNESSED={unwitnessed[k]:5}")
    print(f"\nTOTAL_UNWITNESSED_ASSERTIONS={sum(unwitnessed.values())}")
    print(f"SESSIONS_WITH_UNWITNESSED_PREMISE={len(premise_sessions)}")
    print(f"SESSIONS_WITH_ANY_REVERSAL={len(reversal_sessions)}")
    print(f"HIGH_VALUE_SESSIONS (unwitnessed premise + later reversal)={len(hv_sessions)}")
    print("\n== SAMPLE UNWITNESSED SNIPPETS (<=280 chars) ==")
    for sm in samples:
        print("  " + sm)
    print("\n== SAMPLE HIGH-VALUE SESSIONS (first 10) ==")
    for s in hv_sessions[:10]:
        pats = collections.Counter(p[0] for p in premise_sessions[s])
        print(f"  {s}  premises={dict(pats)}")


if __name__ == "__main__":
    main()
