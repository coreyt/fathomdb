#!/usr/bin/env python3
"""COMBINED recall acid test: run BOTH the A-E textual suite (detectors.py) AND
the coverage suite (detectors_coverage.py) scoped to each known-bad episode's
session file-group. Classify every fire as textual | structural | decision-quality.

HARD RULE honored: streams JSONL via parse.iter_file; prints only aggregates +
<=~20 short snippets. Never reads a whole transcript into an LLM context.
"""
import sys, os, json, collections, re
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import parse
import detectors as AE          # A-E textual families
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import detectors_coverage as DC

MANIFEST = "/home/coreyt/transcript-data/manifest.tsv"
TEXT_CAP = 24000

# Episode -> list of session-uuid substrings that localize its file-group.
# E1/E2 = the two RCA sessions (built-from). E3 = the actual stalled bg-orchestrator
# session(s) (silent, structural). E4 = the OPP-12 design-drift sessions.
EPISODES = {
    "E1_CR047_premise_delete":  ["2fa060bc"],                # memex RCA source
    "E2_30N_wrong_unit":        ["2fa060bc", "f57b5dee"],     # memex+fathomdb RCA
    "E3_36h_silent_stall":      ["ebec94c7", "f57b5dee"],     # sessions naming the 36h stall
    "E4_OPP12_design_drift":    ["60b48af5", "ebec94c7", "fed84f3a", "f57b5dee"],
}

# A-E families are all deterministic-TEXTUAL (verbalized catch).
AE_CLASS = "textual"

# Coverage detectors: map to our three-way bucket for the recall table.
COV_BUCKET = {
    # decision-quality (the broadened scope)
    "DQ-ASSUME-STRUCTURAL": "decision-quality", "DQ-ASSUME-TEXTUAL": "decision-quality",
    "DQ-SHORTKNOWLEDGE": "decision-quality", "DQ-UNVERIFIED-METRIC": "decision-quality",
    "DQ-STALE-DOC": "decision-quality", "DQ-STALE-VERSION": "decision-quality",
    "DQ-DEPBLIND-RETRO": "decision-quality", "DQ-DEPBLIND-CROSSREPO": "decision-quality",
    "DQ-IGNOREDESIGN-STRUCTURAL": "decision-quality", "DQ-IGNOREDESIGN-TEXTUAL": "decision-quality",
    "DQ-SNAP-DISPOSITION": "decision-quality", "DQ-NETNEW-DRIFT": "decision-quality",
    "DQ-INCORRECT": "decision-quality", "DQ-LIMITED-SAMPLE": "decision-quality",
    # structural (silent)
    "SILENT-STALL": "structural", "ROLE-BLEED-SOURCE-EDIT": "structural",
    "IRREVERSIBLE-ACTION-UNGATED": "structural", "WORKTREE-DISCIPLINE-BREACH": "structural",
    "BRANCH-UNVERIFIED-BEFORE-COMMIT": "structural",
    # textual (verbalized) from coverage
    "BLOCK-OVERRIDE": "textual", "PREMATURE-TERM": "textual",
}


def files_for(subs):
    fs = []
    with open(MANIFEST) as fh:
        for ln in fh:
            p = ln.split("\t", 1)[0].strip()
            if p and p.endswith((".jsonl", ".output")) and any(s in p for s in subs):
                fs.append(p)
    return fs


def load_recs(p):
    recs = []
    for r in parse.iter_file(p):
        if len(r["text"]) > TEXT_CAP:
            r["text"] = r["text"][:TEXT_CAP]
        recs.append(r)
    return recs


def clip(s, n=150):
    return re.sub(r"\s+", " ", s or "").strip()[:n]


def main():
    table = {}
    snippets = collections.defaultdict(list)
    for ep, subs in EPISODES.items():
        fs = files_for(subs)
        ae_fam = collections.Counter()
        cov_det = collections.Counter()
        bucket = collections.Counter()          # textual/structural/decision-quality
        ae_examples, cov_examples = [], []
        for p in fs:
            recs = load_recs(p)
            if not recs:
                continue
            # A-E textual suite
            for c in AE.detect_file(recs):
                fam = c.get("family", "?")
                ae_fam[fam] += 1
                bucket[AE_CLASS] += 1
                if len(ae_examples) < 3:
                    ae_examples.append((os.path.basename(p)[:20], c.get("line_no"), fam,
                                        clip(c.get("matched_signal", ""), 40),
                                        clip(c.get("snippet") or c.get("snip") or "", 110)))
            # Coverage suite
            for c in DC.detect_file(recs, p):
                det = c.get("detector", "?")
                if c.get("family") in ("ERROR",):
                    continue
                cov_det[det] += 1
                bucket[COV_BUCKET.get(det, "structural")] += 1
                if COV_BUCKET.get(det) in ("structural", "decision-quality") and len(cov_examples) < 6:
                    cov_examples.append((os.path.basename(p)[:20], c.get("line_no"), det,
                                         clip(c.get("matched_signal", ""), 40),
                                         clip(c.get("snippet", ""), 110),
                                         c.get("gap_hours")))
        table[ep] = {
            "files": len(fs),
            "textual_fires": bucket["textual"],
            "structural_fires": bucket["structural"],
            "decision_quality_fires": bucket["decision-quality"],
            "ae_families": dict(ae_fam),
            "cov_detectors": dict(cov_det),
        }
        snippets[ep] = {"ae": ae_examples, "cov": cov_examples}

    print("=== COMBINED RECALL TABLE (episode x textual x structural x decision-quality) ===\n")
    hdr = f"{'episode':<26} {'files':>5} {'A-E textual':>11} {'structural':>10} {'decision-Q':>10}"
    print(hdr); print("-" * len(hdr))
    for ep, t in table.items():
        print(f"{ep:<26} {t['files']:>5} {t['textual_fires']:>11} {t['structural_fires']:>10} {t['decision_quality_fires']:>10}")
    print()
    for ep, t in table.items():
        print(f"### {ep}")
        print(f"  A-E families: {t['ae_families'] or 'NONE (A-E MISS)'}")
        print(f"  coverage detectors: {t['cov_detectors']}")
        ex = snippets[ep]
        for e in ex["ae"]:
            print(f"    [A-E] {e[0]} L{e[1]} {e[2]} sig={e[3]!r} :: {e[4]}")
        for e in ex["cov"]:
            print(f"    [COV] {e[0]} L{e[1]} {e[2]} gap={e[5]} sig={e[3]!r} :: {e[4]}")
        print()

    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "out", "recall_combined.json"), "w") as fh:
        json.dump(table, fh, indent=2)


if __name__ == "__main__":
    main()
