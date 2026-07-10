#!/usr/bin/env python3
"""Probe for family A-hitl-bounce: HITL user turns that reject/redirect the agent's prior proposal.
Scans ONLY user lines whose content is NOT a tool_result (real HITL turns).
Reports hit counts per candidate signal + short snippets. Non-token-burning: only aggregates printed."""
import json, re, sys, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

# ---- candidate signals (regex, case-insensitive, word-ish boundaries) ----
PATTERNS = {
    # explicit negation/rejection directed at assistant
    "no_comma":        r"^\s*no[,.]",
    "thats_wrong":     r"\b(that'?s|this is|you'?re)\s+(not\s+right|wrong|incorrect|not\s+correct)\b",
    "not_what":        r"\bthat'?s not what\b|\bnot what i (asked|meant|wanted|said)\b",
    "you_missed":      r"\byou (missed|forgot|didn'?t|failed to|overlooked|skipped|ignored)\b",
    "you_misund":      r"\byou (misunderstood|misread|got .* wrong|are wrong|were wrong)\b",
    "actually":        r"\bactually,?\s",
    "reconsider":      r"\b(reconsider|rethink|think again|re-?think)\b",
    "reread":          r"\bre-?read\b|\bre-?check\b|\breview (the )?(docs?|ledgers?|code)\b",
    "instead":         r"\binstead\b",
    "stop_halt":       r"\b(stop|halt|wait)[,.! ]|^\s*(stop|halt|wait)\b",
    "dont_should_not": r"\b(don'?t|do not|should not|shouldn'?t|never)\b",
    "why_did_you":     r"\bwhy (did|are|would|do) you\b",
    "revert_undo":     r"\b(revert|undo|roll ?back|back out)\b",
    "correction_dir":  r"\b(that'?s not|it'?s not|not the|wrong (way|approach|file|direction))\b",
    # CR-047 / 30N ground-truth phrasings
    "gt_which_way":    r"\bwhich (way|one) is (intended|correct|right)\b|\bdetermine which\b",
    "gt_scope":        r"\bscope (corrected|is wrong|creep)\b|\bwrong about\b|\bduplicate\b",
}
COMPILED = {k: re.compile(v, re.I) for k, v in PATTERNS.items()}

# lines to skip: injected command/system content masquerading as user turns
SKIP = re.compile(r"<command-name>|<local-command|<system-reminder>|Caveat: The messages below|"
                  r"stdout>|<command-message>|\[Request interrupted", re.I)

def text_of(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for b in content:
            if isinstance(b, dict) and b.get("type") == "text":
                parts.append(b.get("text", ""))
        return "\n".join(parts)
    return ""

def is_toolresult(content):
    if isinstance(content, list):
        return any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content)
    return False

def main():
    files = [l.strip() for l in open(SPLIT) if l.strip()]
    counts = collections.Counter()
    snippets = collections.defaultdict(list)
    n_user_hitl = 0
    n_files = 0
    for path in files:
        try:
            fh = open(path)
        except OSError:
            continue
        n_files += 1
        for lineno, line in enumerate(fh, 1):
            if '"type":"user"' not in line and '"type": "user"' not in line:
                continue
            try:
                o = json.loads(line)
            except Exception:
                continue
            if o.get("type") != "user":
                continue
            msg = o.get("message") or {}
            c = msg.get("content")
            if is_toolresult(c):
                continue
            txt = text_of(c)
            if not txt or SKIP.search(txt):
                continue
            n_user_hitl += 1
            head = txt[:600]  # only test near the start where correction cues live
            for k, rx in COMPILED.items():
                if rx.search(head):
                    counts[k] += 1
                    if len(snippets[k]) < 3:
                        m = rx.search(head)
                        snip = txt[max(0, m.start()-40):m.start()+120].replace("\n", " ")
                        snippets[k].append((path.split("/")[-1], lineno, snip[:300]))
    print(f"files_scanned={n_files}  user_hitl_turns(non-toolresult)={n_user_hitl}\n")
    for k, _ in sorted(counts.items(), key=lambda x: -x[1]):
        print(f"{counts[k]:6d}  {k}")
    print("\n---- SAMPLE SNIPPETS (<=3 per signal) ----")
    for k in counts:
        for fn, ln, s in snippets[k]:
            print(f"[{k}] {fn}:{ln}  {s}")

if __name__ == "__main__":
    main()
