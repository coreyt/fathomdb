#!/usr/bin/env python3
"""Final probe for family B: bare-assertion detection + reversal linkage.

Witness signals EXPANDED to include a `path:line` citation or code fence in-window
(a code-review finding that cites src/...:123 IS witnessed -> not this family).
Subject gate: exclude pure doc/ledger-supersession vocab (no code entity anywhere).
Then link each surviving BARE premise to a later REVERSAL in the same session (HV).
"""
import json, re, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

PREMISE_PATS = {
    "no_consumers": re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+|real\s+|active\s+)?consumers?\b", re.I),
    "no_callers":   re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+)?(?:callers?|call\s?sites?|references?|usages?)\b", re.I),
    "already_superseded": re.compile(r"\b(?:already|now)\s+supersed(?:ed|es)\b", re.I),  # tightened: require already/now
    "is_a_duplicate": re.compile(r"\bis\s+(?:a\s+)?duplicate\b|\bare\s+duplicates?\b|\bis\s+redundant\b", re.I),
    "owned_by_now": re.compile(r"\bowned\s+by\s+\w+\s+now\b|\bnow\s+owned\s+by\b|\bhandled\s+by\s+\w+\s+now\b", re.I),
    "n_sites": re.compile(r"[~≈]\s?\d{1,4}\s+(?:call\s?)?sites?\b|\b\d{1,4}\s+call\s?sites?\b", re.I),
    "safe_to_delete": re.compile(r"\bsafe\s+to\s+(?:delete|remove|drop)\b", re.I),
    "not_used_anywhere": re.compile(r"\bnot\s+used\s+anywhere\b|\bdead\s+code\b|\bnever\s+(?:called|used|referenced)\b", re.I),
    "nothing_uses": re.compile(r"\bnothing\s+(?:else\s+)?(?:uses|references|calls|depends\s+on|reads)\b", re.I),
}

# Witness = evidence the claim is grounded (any of these in-window OR tool_use sibling)
WITNESS = re.compile(
    r"\b(?:grep|rg|ripgrep|search(?:ed|ing)?|confirm(?:ed)?|verif(?:y|ied)|checked)\b"
    r"|[\w./-]+\.(?:rs|py|ts|js|md|toml):\d+"          # path:line citation
    r"|```|`[\w./:-]+:\d+`", re.I)

CODE_SUBJ = re.compile(
    r"[\w./-]+\.(?:rs|py|ts|js)\b|\w+::\w+|\w+\(\)|`\w+`|"
    r"\b(?:function|fn|method|class|struct|enum|table|column|field|module|crate|"
    r"endpoint|route|handler|migration|schema|trait|ScheduledTask)\b", re.I)

REVERSAL = re.compile(
    r"\b(?:actually,?\s|turns?\s+out|i\s+was\s+wrong|scope\s+corrected|"
    r"stand\s+corrected|there\s+(?:are|is)\s+(?:still\s+)?(?:live\s+)?consumers?|"
    r"is\s+(?:actually\s+)?(?:still\s+)?(?:used|referenced|called)|"
    r"not\s+(?:a\s+)?duplicate|wasn'?t\s+superseded|still\s+in\s+use|"
    r"wrong\s+about|my\s+earlier\s+claim|retract)\b", re.I)


def iter_assistant_text(content):
    if isinstance(content, str):
        yield content, False; return
    if isinstance(content, list):
        has_tool = any(isinstance(b, dict) and b.get("type") == "tool_use" for b in content)
        for b in content:
            if isinstance(b, dict) and b.get("type") == "text":
                yield b.get("text", ""), has_tool


def real_user_text(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        if any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content):
            return None
        return " ".join(b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text")
    return None


def main():
    files = [l.strip() for l in open(SPLIT) if l.strip()]
    bare = collections.Counter()          # unwitnessed + code-subject present
    reversal_sessions = set()
    bare_by_session = collections.defaultdict(list)
    samples = []

    for fp in files:
        try:
            fh = open(fp, encoding="utf-8")
        except OSError:
            continue
        session = fp.split("/")[-1]
        for lineno, raw in enumerate(fh, 1):
            raw = raw.strip()
            if not raw:
                continue
            try:
                obj = json.loads(raw)
            except Exception:
                continue
            if not isinstance(obj, dict):
                continue
            msg = obj.get("message") or {}
            role = msg.get("role") or obj.get("type")
            content = msg.get("content")
            if content is None:
                continue
            if role == "assistant":
                for text, has_tool in iter_assistant_text(content):
                    if not text:
                        continue
                    if REVERSAL.search(text):
                        reversal_sessions.add(session)
                    for name, pat in PREMISE_PATS.items():
                        m = pat.search(text)
                        if not m:
                            continue
                        s = max(0, m.start() - 220); e = min(len(text), m.end() + 220)
                        win = text[s:e]
                        if has_tool or WITNESS.search(win):
                            continue           # witnessed -> not this family
                        if not CODE_SUBJ.search(win):
                            continue           # pure doc/plan supersession -> drop
                        bare[name] += 1
                        bare_by_session[session].append(name)
                        if len(samples) < 15:
                            snip = re.sub(r"\s+", " ", win)[:280]
                            samples.append(f"[{name}] {session}:{lineno} :: {snip}")
            elif role == "user":
                ut = real_user_text(content)
                if ut and REVERSAL.search(ut):
                    reversal_sessions.add(session)

    hv = [s for s in bare_by_session if s in reversal_sessions]
    print("== BARE (unwitnessed + code-subject) premise assertions ==")
    for k in sorted(bare, key=lambda x: -bare[x]):
        print(f"  {k:20} {bare[k]}")
    print(f"\nTOTAL_BARE={sum(bare.values())}  SESSIONS={len(bare_by_session)}")
    print(f"REVERSAL_SESSIONS={len(reversal_sessions)}")
    print(f"HIGH_VALUE (bare premise + later reversal in session) sessions={len(hv)}")
    print("\n== BARE SAMPLES (<=280) ==")
    for s in samples:
        print("  " + s)


if __name__ == "__main__":
    main()
