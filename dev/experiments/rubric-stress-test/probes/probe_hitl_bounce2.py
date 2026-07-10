#!/usr/bin/env python3
"""Probe v2 for A-hitl-bounce with DISAMBIGUATION:
 - require the user turn's parentUuid to resolve to an ASSISTANT turn (a bounce replies to a proposal)
 - gate on turn length (corrections are short; long user turns are role-contracts/spawn prompts)
 - split main-session (UUID.jsonl) vs subagent (agent-*.jsonl) files
Reports how each control shrinks the candidate set + samples of surviving hits."""
import json, re, collections, os

SPLIT = "/home/coreyt/transcript-data/split-experiment.txt"

CORRECTION = re.compile(
    r"^\s*no[,.\s]"                                  # leading "no,"
    r"|\bthat'?s (not right|wrong|incorrect|not what|not the)\b"
    r"|\bnot what i (asked|meant|wanted|said)\b"
    r"|\byou (missed|forgot|didn'?t|failed to|overlooked|skipped|misunderstood|misread|are wrong|were wrong)\b"
    r"|^\s*actually,?\s|\bactually,? (i|you|we|the|it)\b"
    r"|\b(reconsider|rethink|think again)\b"
    r"|\bre-?read\b|\bre-?check\b"
    r"|\binstead\b"
    r"|\bwhy (did|are|would) you\b"
    r"|\b(revert|undo|roll ?back|back that out)\b"
    r"|\bwrong (way|approach|file|direction|about)\b"
    r"|\bstep back\b"
    r"|\bthat'?s not what\b",
    re.I)

SKIP = re.compile(r"<command-name>|<local-command|<system-reminder>|Caveat: The messages below|"
                  r"<command-message>|\[Request interrupted|^Stop hook feedback:|"
                  r"^\s*This session is being continued|API Error|tool_use ids were found", re.I)

def text_of(c):
    if isinstance(c, str): return c
    if isinstance(c, list):
        return "\n".join(b.get("text","") for b in c if isinstance(b,dict) and b.get("type")=="text")
    return ""

def is_tr(c):
    return isinstance(c,list) and any(isinstance(b,dict) and b.get("type")=="tool_result" for b in c)

def main():
    files=[l.strip() for l in open(SPLIT) if l.strip()]
    buckets=collections.Counter()
    surv=[]   # surviving after all controls
    lenhist=collections.Counter()
    for path in files:
        is_sub = "/subagents/" in path or os.path.basename(path).startswith("agent-")
        try: lines=open(path).read().splitlines()
        except OSError: continue
        # build uuid->type map
        typ={}
        objs=[]
        for line in lines:
            try: o=json.loads(line)
            except Exception: o=None
            if not isinstance(o,dict): objs.append(None); continue
            objs.append(o)
            u=o.get("uuid")
            if u: typ[u]=o.get("type")
        for idx,o in enumerate(objs):
            if not o or o.get("type")!="user": continue
            msg=o.get("message") or {}; c=msg.get("content")
            if is_tr(c): continue
            txt=text_of(c)
            if not txt or SKIP.search(txt): continue
            buckets["all_hitl_turns"]+=1
            if not CORRECTION.search(txt[:600]): continue
            buckets["raw_pattern_hit"]+=1
            parent=o.get("parentUuid")
            parent_is_asst = typ.get(parent)=="assistant"
            if is_sub: buckets["hit_in_subagent"]+=1
            else: buckets["hit_in_main"]+=1
            if parent_is_asst: buckets["hit_parent_asst"]+=1
            if len(txt)<=400: buckets["hit_short<=400"]+=1
            # FULL disambiguated: replies to an assistant proposal (structural), any length
            if parent_is_asst:
                buckets["SURVIVING_parentasst"]+=1
                lenhist[min(len(txt)//200*200,2000)]+=1
                if len(surv)<15:
                    surv.append((os.path.basename(path), idx, len(txt), txt[:220].replace("\n"," ")))
    for k in ["all_hitl_turns","raw_pattern_hit","hit_in_main","hit_in_subagent",
              "hit_parent_asst","hit_short<=400","SURVIVING_parentasst"]:
        print(f"{buckets[k]:6d}  {k}")
    print("\nlen-bucket histogram of survivors (chars):", dict(sorted(lenhist.items())))
    print("\n---- SURVIVING SAMPLES (parent=assistant, any length) ----")
    for fn,idx,ln,s in surv:
        print(f"[{fn} #{idx} len={ln}] {s[:280]}")

if __name__=="__main__":
    main()
