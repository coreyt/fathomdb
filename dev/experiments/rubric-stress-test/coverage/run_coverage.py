#!/usr/bin/env python3
"""Run coverage detectors over the FULL corpus (manifest.tsv). Writes
coverage/out/coverage_candidates.jsonl + a summary JSON. Streams file-by-file
(bounded memory); prints ONLY aggregates.

Usage: python3 run_coverage.py [manifest_or_split]  (default: full manifest)
"""
import sys, os, json, time, collections
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import parse
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import detectors_coverage as dc

MANIFEST = "/home/coreyt/transcript-data/manifest.tsv"
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out")
TEXT_CAP = 24000


def load_paths(arg):
    if arg and arg.endswith(".txt"):
        with open(arg) as fh:
            return [l.strip() for l in fh if l.strip()]
    src = arg or MANIFEST
    paths = []
    with open(src) as fh:
        for ln in fh:
            p = ln.split("\t", 1)[0].strip()
            if p and p.endswith((".jsonl", ".output")):
                paths.append(p)
    return paths


def main():
    arg = sys.argv[1] if len(sys.argv) > 1 else None
    paths = load_paths(arg)
    os.makedirs(OUT, exist_ok=True)
    out_path = os.path.join(OUT, "coverage_candidates.jsonl")

    t0 = time.time()
    per_det = collections.Counter()
    per_fam = collections.Counter()
    per_class = collections.Counter()
    per_conf = collections.Counter()
    needs_adj = collections.Counter()  # detector -> #needs_adjudication rows
    det_sessions = collections.defaultdict(set)
    fam_sessions = collections.defaultdict(set)
    n_files = n_records = n_cand = 0

    with open(out_path, "w") as ofh:
        for p in paths:
            n_files += 1
            recs = []
            for r in parse.iter_file(p):
                if len(r["text"]) > TEXT_CAP:
                    r["text"] = r["text"][:TEXT_CAP]
                recs.append(r)
            n_records += len(recs)
            if not recs:
                continue
            for cand in dc.detect_file(recs, p):
                n_cand += 1
                cand["parent_session"] = parse.parent_session_id(cand["file"])
                per_det[cand["detector"]] += 1
                per_fam[cand["family"]] += 1
                per_class[cand.get("detectability_class", "?")] += 1
                per_conf[cand.get("confidence_heuristic", "?")] += 1
                if cand.get("needs_adjudication"):
                    needs_adj[cand["detector"]] += 1
                det_sessions[cand["detector"]].add(cand["parent_session"])
                fam_sessions[cand["family"]].add(cand["parent_session"])
                ofh.write(json.dumps(cand, ensure_ascii=False) + "\n")

    dt = time.time() - t0
    summary = {
        "source": arg or MANIFEST,
        "files": n_files, "records": n_records, "candidates": n_cand,
        "runtime_sec": round(dt, 1),
        "per_detector": {k: {"hits": v, "sessions": len(det_sessions[k]),
                             "needs_adjudication": needs_adj[k]}
                         for k, v in per_det.most_common()},
        "per_family": {k: {"hits": v, "sessions": len(fam_sessions[k])}
                       for k, v in per_fam.most_common()},
        "per_detectability_class": dict(per_class.most_common()),
        "per_confidence": dict(per_conf.most_common()),
    }
    print(json.dumps(summary, indent=2))
    with open(out_path + ".summary.json", "w") as sfh:
        json.dump(summary, sfh, indent=2)


if __name__ == "__main__":
    main()
