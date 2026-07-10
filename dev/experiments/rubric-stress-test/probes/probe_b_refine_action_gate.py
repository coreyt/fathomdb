#!/usr/bin/env python3
"""Refinement probe for family B: measure precision gates.

For each UNWITNESSED premise hit, test two disambiguators from the RCA root cause:
  (A) ACTION-PROXIMITY: the claim sits near a destructive/scope action verb
      (delete/remove/drop/finish/consolidate/skip/dedup) -> it's LOAD-BEARING.
  (B) CODE-SUBJECT: the claim is about a code entity (fn/class/table/field/
      module/struct/endpoint) rather than a doc/ledger/note/claim/section.
Separates the true premise-substitution risk from benign doc-supersession vocab.
"""
import json, re, collections

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

PREMISE_PATS = {
    "no_consumers": re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+|real\s+|active\s+)?consumers?\b", re.I),
    "no_callers":   re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+)?(?:callers?|call\s?sites?|references?|usages?)\b", re.I),
    "already_superseded": re.compile(r"\b(?:already\s+)?supersed(?:ed|es)\b", re.I),
    "is_a_duplicate": re.compile(r"\bis\s+(?:a\s+)?duplicate\b|\bare\s+duplicates?\b|\bis\s+redundant\b", re.I),
    "owned_by_now": re.compile(r"\bowned\s+by\s+\w+\s+now\b|\bnow\s+owned\s+by\b", re.I),
    "n_sites": re.compile(r"[~≈]\s?\d{1,4}\s+(?:call\s?)?sites?\b|\b\d{1,4}\s+call\s?sites?\b", re.I),
    "nothing_uses": re.compile(r"\bnothing\s+(?:else\s+)?(?:uses|references|calls|depends\s+on|reads)\b", re.I),
    "safe_to_delete": re.compile(r"\bsafe\s+to\s+(?:delete|remove|drop)\b|\bcan\s+(?:be\s+)?(?:safely\s+)?(?:delete|remove|drop)", re.I),
    "not_used_anywhere": re.compile(r"\bnot\s+used\s+anywhere\b|\bdead\s+code\b|\bnever\s+(?:called|used|referenced)\b", re.I),
}

WITNESS_HEDGE = re.compile(
    r"\b(?:grep|rg|ripgrep|search(?:ed|ing)?|confirm(?:ed)?|verif(?:y|ied)|checked|"
    r"per\s+the|according\s+to|the\s+(?:grep|search|output|results?)\s+show|"
    r"i\s+(?:ran|searched|grepped|checked))\b", re.I)

ACTION = re.compile(
    r"\b(?:delete|deletes|deleting|deleted|remove|removes|removing|removed|drop|dropping|"
    r"finish|finishing|consolidat|dedup|de-?duplicat|prune|purge|kill|"
    r"skip|skipping|rip\s?out|tear\s?out|collapse|merge\s+(?:it|them|these)|"
    r"scope|out\s+of\s+scope|don'?t\s+(?:need|touch)|no\s+need\s+to)\b", re.I)

CODE_SUBJ = re.compile(
    r"\b(?:function|fn|method|class|struct|enum|table|column|field|module|crate|"
    r"endpoint|route|handler|migration|schema|type|trait|interface|component|"
    r"ScheduledTask|\w+\(\)|`\w+`|\w+::\w+|\w+\.\w+\()\b")
DOC_SUBJ = re.compile(
    r"\b(?:doc|note|ledger|seq|hand-?off|section|§\d|line|claim|caveat|clause|"
    r"paragraph|entry|ROADMAP|ADR|memory|guardrail|update|reconcil)\b", re.I)


def iter_assistant_text(content):
    if isinstance(content, str):
        yield content, False; return
    if isinstance(content, list):
        has_tool = any(isinstance(b, dict) and b.get("type") == "tool_use" for b in content)
        for b in content:
            if isinstance(b, dict) and b.get("type") == "text":
                yield b.get("text", ""), has_tool


def main():
    files = [l.strip() for l in open(SPLIT) if l.strip()]
    total = collections.Counter()
    unwit = collections.Counter()
    unwit_action = collections.Counter()          # unwitnessed + action-proximity
    unwit_action_code = collections.Counter()     # + code subject (tightest)
    unwit_action_doconly = collections.Counter()  # action but doc-subject only (FP-ish)
    gold_samples = []

    for fp in files:
        try:
            fh = open(fp, encoding="utf-8")
        except OSError:
            continue
        session = fp.split("/")[-1]
        for lineno, raw in enumerate(fh, 1):
            raw = raw.strip()
            if not raw or "supersed" not in raw and not any(
                w in raw for w in ("consumer", "duplicate", "call site", "callsite",
                                   "dead code", "safe to", "nothing", "owned by",
                                   "call sites", "not used", "never called", "never used")):
                continue
            try:
                obj = json.loads(raw)
            except Exception:
                continue
            if not isinstance(obj, dict):
                continue
            if obj.get("type") != "assistant" and (obj.get("message") or {}).get("role") != "assistant":
                continue
            content = (obj.get("message") or {}).get("content")
            if content is None:
                continue
            for text, has_tool in iter_assistant_text(content):
                if not text:
                    continue
                for name, pat in PREMISE_PATS.items():
                    m = pat.search(text)
                    if not m:
                        continue
                    total[name] += 1
                    s = max(0, m.start() - 200); e = min(len(text), m.end() + 200)
                    win = text[s:e]
                    if WITNESS_HEDGE.search(win) or has_tool:
                        continue
                    unwit[name] += 1
                    has_act = bool(ACTION.search(win))
                    has_code = bool(CODE_SUBJ.search(win))
                    has_doc = bool(DOC_SUBJ.search(win))
                    if has_act:
                        unwit_action[name] += 1
                        if has_code:
                            unwit_action_code[name] += 1
                            if len(gold_samples) < 15:
                                snip = re.sub(r"\s+", " ", win)[:280]
                                gold_samples.append(f"[{name}] {session}:{lineno} :: {snip}")
                        elif has_doc:
                            unwit_action_doconly[name] += 1

    print("name / total / unwitnessed / +action / +action+CODE(gold) / +action+DOConly")
    for k in sorted(total, key=lambda x: -total[x]):
        print(f"  {k:20} {total[k]:5} {unwit[k]:5} {unwit_action[k]:5} "
              f"{unwit_action_code[k]:5} {unwit_action_doconly[k]:5}")
    tot_gold = sum(unwit_action_code.values())
    print(f"\nGOLD (unwitnessed + action-proximity + code-subject) = {tot_gold}")
    print(f"unwitnessed total = {sum(unwit.values())}  -> gold retains "
          f"{100*tot_gold/max(1,sum(unwit.values())):.0f}%")
    print("\n== GOLD SAMPLES (<=280) ==")
    for g in gold_samples:
        print("  " + g)


if __name__ == "__main__":
    main()
