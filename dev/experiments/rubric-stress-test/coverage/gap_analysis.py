#!/usr/bin/env python3
"""Structural stall detector (Episode #3 class): large intra-file wall-clock gaps
and orphaned/stalled subagents. NO text enters context beyond a tiny label.

Two structural signals:
  (S1) intra-file gap: within ONE transcript file, two consecutive timestamped
       lines >GAP_H hours apart => the process stalled mid-file (nothing happened).
  (S2) orphaned subagent: a subagent file whose LAST ts is >GAP_H h before its
       parent-session's LAST ts (parent kept going; the child went silent).
Both are deterministic-structural, computable with 0 LLM.
"""
import sys, os, json, collections, datetime
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import parse

MANIFEST = "/home/coreyt/transcript-data/manifest.tsv"
GAP_H = 6.0

def parse_ts(s):
    if not s:
        return None
    try:
        return datetime.datetime.fromisoformat(s.replace("Z", "+00:00"))
    except Exception:
        return None

def main():
    paths = [ln.split("\t")[0].strip() for ln in open(MANIFEST) if ln.strip()]
    # per file: consecutive gaps + first/last ts
    file_last = {}
    file_first = {}
    file_lines = collections.Counter()
    s1_hits = []  # intra-file gaps
    for p in paths:
        prev = None
        prev_ln = None
        first = None
        last = None
        n = 0
        for rec in parse.iter_file(p):
            t = parse_ts(rec["ts"])
            n += 1
            if t is None:
                continue
            if first is None:
                first = t
            last = t
            if prev is not None:
                dh = (t - prev).total_seconds() / 3600.0
                if dh >= GAP_H:
                    s1_hits.append((dh, p, prev_ln, rec["line_no"],
                                    prev.isoformat()[:16], t.isoformat()[:16]))
            prev = t
            prev_ln = rec["line_no"]
        file_first[p] = first
        file_last[p] = last
        file_lines[p] = n
    # S2: orphaned subagents vs parent-session last ts
    sess_last = collections.defaultdict(lambda: None)
    for p, t in file_last.items():
        if t is None:
            continue
        s = parse.parent_session_id(p)
        if sess_last[s] is None or t > sess_last[s]:
            sess_last[s] = t
    s2_hits = []
    for p, t in file_last.items():
        base = os.path.basename(p)
        is_sub = base.startswith("agent-") or "/subagents/" in p
        if not is_sub or t is None:
            continue
        s = parse.parent_session_id(p)
        st = sess_last[s]
        if st is None:
            continue
        dh = (st - t).total_seconds() / 3600.0
        if dh >= GAP_H:
            s2_hits.append((dh, p, file_lines[p], t.isoformat()[:16], st.isoformat()[:16]))

    s1_hits.sort(reverse=True)
    s2_hits.sort(reverse=True)
    print("=== S1: top intra-file wall-clock gaps (process stalled mid-file) ===")
    for dh, p, l0, l1, t0, t1 in s1_hits[:20]:
        print(f"{dh:7.1f}h  {os.path.basename(p)[:26]:26s} L{l0}->L{l1}  {t0} -> {t1}")
    print(f"\ntotal S1 gaps>= {GAP_H}h: {len(s1_hits)}  in {len(set(h[1] for h in s1_hits))} files")
    print("\n=== S2: top orphaned/stalled subagents (child silent, parent kept going) ===")
    for dh, p, n, t0, t1 in s2_hits[:20]:
        print(f"{dh:7.1f}h  {os.path.basename(p)[:26]:26s} nlines={n:<4d} childlast={t0}  parentlast={t1}")
    print(f"\ntotal S2 orphans>= {GAP_H}h: {len(s2_hits)}")

if __name__ == "__main__":
    main()
