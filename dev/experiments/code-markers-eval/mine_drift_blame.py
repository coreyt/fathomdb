#!/usr/bin/env python3
"""Miner 2 — marker/code DRIFT via git blame (0 LLM). Tests H2.

For every in-code marker occurrence (from Miner 1), compare the last-touch
commit of the MARKER line against the last-touch of the CODE lines around it.
If the surrounding code was modified strictly AFTER the marker line was last
touched, the code has "moved on" while the marker stayed put -> drift candidate.

Method (deterministic, from history):
  * `git blame --line-porcelain <file>` once per file -> line -> (sha, author-time).
  * marker_time  = author-time of the marker line.
  * code_time    = max author-time among the WINDOW lines (+/-W, excluding the
                   marker line itself and blank/pure-punctuation lines).
  * drift        = code_time > marker_time  (surrounding code newer).
  * drift_days   = (code_time - marker_time) / 86400.

Caveat recorded in the report: blame attributes a line to its LAST edit, so a
whitespace/rustfmt sweep touching the marker line resets marker_time and MASKS
drift (=> our drift rate is a LOWER bound). We also emit `marker_age_days` so
the report can separate genuinely-old markers.

Output: out/drift.jsonl, out/drift_summary.json
"""
import json, os, re, subprocess, collections, time

REPO = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
os.chdir(REPO)
OUT = "dev/experiments/code-markers-eval/out"
W = 3  # window half-width in lines
NOW = time.time()

markers = [json.loads(l) for l in open(os.path.join(OUT, "incode_markers.jsonl"))]
by_file = collections.defaultdict(list)
for m in markers:
    by_file[m["file"]].append(m)

BLANKISH = re.compile(r"^[\s{}()\[\];,]*$")


def blame_file(path):
    """Return dict line_no -> (sha, author_time_int, code_text)."""
    out = subprocess.check_output(
        ["git", "blame", "--line-porcelain", "HEAD", "--", path],
        text=True, errors="replace")
    res = {}
    sha = None
    atime = None
    ln = None
    for line in out.split("\n"):
        if not line:
            continue
        if line[0] != "\t":
            parts = line.split()
            # header line: <sha> <orig_line> <final_line> [<count>]
            if len(parts) >= 3 and re.fullmatch(r"[0-9a-f]{40}", parts[0]):
                sha = parts[0]
                ln = int(parts[2])
            elif line.startswith("author-time "):
                atime = int(line.split()[1])
        else:
            res[ln] = (sha, atime, line[1:])
    return res


rows = []
for path, ms in by_file.items():
    try:
        bl = blame_file(path)
    except Exception as e:
        for m in ms:
            m2 = dict(m, drift=None, error=str(e))
            rows.append(m2)
        continue
    maxln = max(bl) if bl else 0
    for m in ms:
        i = m["line"]
        if i not in bl:
            rows.append(dict(m, drift=None, error="line-not-blamed"))
            continue
        msha, mtime, _mtext = bl[i]
        code_times = []
        for j in range(max(1, i - W), min(maxln, i + W) + 1):
            if j == i or j not in bl:
                continue
            _s, t, txt = bl[j]
            if BLANKISH.match(txt):
                continue
            code_times.append(t)
        code_time = max(code_times) if code_times else None
        drift = (code_time is not None and code_time > mtime)
        rows.append(dict(
            cls=m["cls"], token=m["token"], status=m["status"],
            file=path, line=i, is_test=m["is_test"],
            marker_sha=msha, marker_time=mtime,
            marker_age_days=round((NOW - mtime) / 86400, 1),
            code_time=code_time,
            drift=drift,
            drift_days=round((code_time - mtime) / 86400, 1) if drift else 0.0,
        ))

with open(os.path.join(OUT, "drift.jsonl"), "w") as fh:
    for r in rows:
        fh.write(json.dumps(r) + "\n")

# summary
usable = [r for r in rows if r.get("drift") is not None]
drifted = [r for r in usable if r["drift"]]
by_cls = collections.defaultdict(lambda: [0, 0])  # cls -> [usable, drifted]
for r in usable:
    by_cls[r["cls"]][0] += 1
    if r["drift"]:
        by_cls[r["cls"]][1] += 1

ages = sorted(r["marker_age_days"] for r in usable)
def pct(p):
    if not ages:
        return None
    return ages[min(len(ages) - 1, int(p * len(ages)))]

summary = dict(
    total_markers=len(rows),
    usable=len(usable),
    unusable=len(rows) - len(usable),
    drifted=len(drifted),
    drift_rate=round(len(drifted) / len(usable), 4) if usable else None,
    window_halfwidth=W,
    marker_age_days_median=pct(0.5),
    marker_age_days_p90=pct(0.9),
    max_drift_days=max((r["drift_days"] for r in drifted), default=0.0),
    per_class={c: dict(usable=u, drifted=d,
                       drift_rate=round(d / u, 4) if u else None)
               for c, (u, d) in sorted(by_cls.items())},
    note="drift_rate is a LOWER bound: blame attributes a line to its last edit; "
         "a format/whitespace sweep touching the marker line resets marker_time "
         "and masks drift.",
)
with open(os.path.join(OUT, "drift_summary.json"), "w") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)
print(json.dumps(summary, indent=2, sort_keys=True))
