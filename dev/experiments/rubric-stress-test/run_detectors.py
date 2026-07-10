#!/usr/bin/env python3
"""Run all detectors over a split; write candidate JSONL + a summary.

Usage: python3 run_detectors.py <split-file> <out.jsonl>

Processes file-by-file (bounded memory: one file's records at a time, each
record's text capped) so no whole transcript ever enters an LLM context.
Prints ONLY aggregates.
"""
import sys, json, os, time, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import parse
import detectors

TEXT_CAP = 24000  # per-record text cap (bounds RAM on multi-MB tool_result lines)


def main():
    split = sys.argv[1]
    out_path = sys.argv[2]
    paths = parse.load_paths(split)
    t0 = time.time()
    per_det = collections.Counter()
    per_fam = collections.Counter()
    per_conf = collections.Counter()
    sessions_hit = set()
    n_files = 0
    n_records = 0
    n_cand = 0
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
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
            for cand in detectors.detect_file(recs):
                n_cand += 1
                # F1/F4: stamp the session-coherent group id + a stable content
                # fingerprint so labels pin to content, not to stride position.
                cand["parent_session"] = parse.parent_session_id(cand["file"])
                cand["fingerprint"] = parse.candidate_fingerprint(cand)
                per_det[cand["detector"]] += 1
                per_fam[cand["family"]] += 1
                per_conf[cand.get("confidence_heuristic", "?")] += 1
                sessions_hit.add((cand["family"], cand["parent_session"]))
                ofh.write(json.dumps(cand, ensure_ascii=False) + "\n")
    dt = time.time() - t0
    fam_sessions = collections.Counter(fam for fam, _ in sessions_hit)
    summary = {
        "split": split, "files": n_files, "records": n_records,
        "candidates": n_cand, "runtime_sec": round(dt, 1),
        "per_detector": dict(per_det.most_common()),
        "per_family": dict(per_fam.most_common()),
        "per_confidence": dict(per_conf.most_common()),
        "unique_sessions_per_family": dict(fam_sessions.most_common()),
    }
    print(json.dumps(summary, indent=2))
    with open(out_path + ".summary.json", "w") as sfh:
        json.dump(summary, sfh, indent=2)


if __name__ == "__main__":
    main()
