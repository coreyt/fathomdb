#!/usr/bin/env python3
"""Locate the FOUR known-bad episodes in the staged corpus by textual signature.

HARD RULE: never emit more than a ~200-char snippet per hit. We scan raw JSONL
via parse.iter_file (streaming) and print only compact match records + aggregates.
Output -> coverage/out/episode_hits.jsonl  (+ printed session rollup).
"""
import sys, os, re, json, collections
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import parse

MANIFEST = "/home/coreyt/transcript-data/manifest.tsv"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "out")
os.makedirs(OUT, exist_ok=True)

# Named signatures per episode. Case-insensitive.
SIGS = {
    # Episode 1: CR-047 premise-substitution (wrong DELETE)
    "E1_no_live_consumers": r"no live consumers?",
    "E1_already_superseded": r"already superseded",
    "E1_no_consumers": r"no consumers? (?:found|remain|left|exist)",
    "E1_cr047": r"\bCR-?047\b",
    # Episode 2: 30-N wrong-unit-of-work
    "E2_98_call_sites": r"~?\s*98\s+call\s*sites?",
    "E2_call_sites_N": r"~\s*\d{2,4}\s+call\s*sites?",
    "E2_scheduledtask_dup": r"ScheduledTask\s+is\s+a\s+duplicate",
    "E2_is_a_duplicate": r"\bis\s+a\s+duplicate\b",
    "E2_30N": r"\b30-?N\b",
    # Episode 3: 36-hour silent background-agent stall
    "E3_36_hour": r"36[-\s]?hour",
    "E3_stall": r"\bstall(?:ed|ing)?\b",
    "E3_auto_resume": r"auto[-\s]?resume",
    "E3_commissioned_orch": r"commissioned .{0,40}orchestrat",
    "E3_silent_death": r"silent(?:ly)? (?:death|died|die|stall)",
    "E3_no_notification": r"no (?:notification|notice) ",
    # Episode 4: OPP-12 pre-audit design drift
    "E4_net_new": r"\bnet[-\s]?new\b",
    "E4_exists_vs_netnew": r"exists?\s+vs\.?\s+net[-\s]?new",
    "E4_contradicts_shipped": r"contradict(?:s|ing|ed)? .{0,30}(?:shipped|signed|mechanism)",
    "E4_opp12": r"\bOPP-?12\b",
    "E4_code_grounded": r"code[-\s]?grounded",
    "E4_adversarial_rounds": r"adversarial .{0,20}rounds?",
}
COMPILED = {k: re.compile(v, re.I) for k, v in SIGS.items()}

def snippet(text, m, width=120):
    s = max(0, m.start() - width // 2)
    e = min(len(text), m.end() + width // 2)
    return text[s:e].replace("\n", " ")[:200]

def main():
    paths = []
    with open(MANIFEST) as fh:
        for ln in fh:
            p = ln.split("\t")[0].strip()
            if p:
                paths.append(p)
    hits = []
    per_sig = collections.Counter()
    per_session = collections.defaultdict(lambda: collections.Counter())
    per_file = collections.defaultdict(lambda: collections.Counter())
    for p in paths:
        sess = parse.parent_session_id(p)
        for rec in parse.iter_file(p):
            txt = rec["text"] or ""
            if not txt:
                continue
            for name, rx in COMPILED.items():
                m = rx.search(txt)
                if m:
                    per_sig[name] += 1
                    per_session[sess][name] += 1
                    per_file[p][name] += 1
                    hits.append({
                        "sig": name, "file": p, "session": sess,
                        "line_no": rec["line_no"], "ts": rec["ts"],
                        "type": rec["type"], "is_hitl": rec["is_hitl"],
                        "is_subfile": rec["is_subfile"],
                        "tools": rec["tool_names"],
                        "snip": snippet(txt, m),
                    })
    with open(os.path.join(OUT, "episode_hits.jsonl"), "w") as fh:
        for h in hits:
            fh.write(json.dumps(h, ensure_ascii=False) + "\n")
    print("=== per-signature totals ===")
    for k in SIGS:
        print(f"{per_sig[k]:6d}  {k}")
    print("\n=== sessions with >=2 distinct sigs of ANY single episode ===")
    # group sigs by episode prefix
    def epi(name): return name.split("_")[0]
    for sess, c in sorted(per_session.items()):
        by_epi = collections.defaultdict(set)
        for name, n in c.items():
            by_epi[epi(name)].add(name)
        for ep, sigs in by_epi.items():
            if len(sigs) >= 2:
                print(f"{ep}  {sess}  sigs={sorted(sigs)}")
    print(f"\ntotal hits: {len(hits)}  files_with_hits: {len(per_file)}")

if __name__ == "__main__":
    main()
