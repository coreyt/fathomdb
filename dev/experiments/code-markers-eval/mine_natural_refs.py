#!/usr/bin/env python3
"""Miner 3 — non-code natural-experiment reference classes (0 LLM).

Measures how OTHER marker-like cross-references in this program have aged:
  A. Memory [[wikilink]] graph  -> dangling rate (target .md missing).
     CLAUDE.md declares a [[name]] MAY point at a not-yet-written memory
     (designed-in dangling tolerance). We measure the ACTUAL rate.
  B. Commit-message refs: `ledger seq NN`, `TC-N`, `F-NN`, `Claude-Session:`
     -> resolvability against the current registries.
  C. Doc `@<sha>` pins in dev/design + dev/adr -> git object existence +
     ancestor-reachability (a pin to a rebased-away sha dangles).

Output: out/natural_refs.json (+ out/wikilinks.jsonl, out/commit_refs.jsonl)
"""
import json, os, re, subprocess, collections

REPO = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
os.chdir(REPO)
OUT = "dev/experiments/code-markers-eval/out"
MEM = "/home/coreyt/.claude/projects/-home-coreyt-projects-fathomdb/memory"

result = {}

# ---- A. memory wikilink graph -------------------------------------------
wl_rows = []
if os.path.isdir(MEM):
    existing = {f[:-3] for f in os.listdir(MEM) if f.endswith(".md")}
    WL = re.compile(r"\[\[([^\]]+)\]\]")
    occ = 0
    dangling = 0
    per_target = collections.Counter()
    for f in sorted(existing):
        path = os.path.join(MEM, f + ".md")
        for i, line in enumerate(open(path, encoding="utf-8", errors="replace"), 1):
            for m in WL.finditer(line):
                tgt = m.group(1).strip()
                # strip optional |alias and #section
                tgt_file = tgt.split("|")[0].split("#")[0].strip()
                resolves = tgt_file in existing
                occ += 1
                if not resolves:
                    dangling += 1
                per_target[tgt_file] += 1
                wl_rows.append(dict(src=f, line=i, target=tgt_file,
                                    resolves=resolves))
    distinct_targets = set(per_target)
    distinct_dangling = {t for t in distinct_targets if t not in existing}
    result["A_memory_wikilinks"] = dict(
        memory_files=len(existing),
        occurrences=occ,
        dangling_occurrences=dangling,
        dangling_occurrence_rate=round(dangling / occ, 4) if occ else 0.0,
        distinct_targets=len(distinct_targets),
        distinct_dangling_targets=len(distinct_dangling),
        distinct_dangling_rate=round(len(distinct_dangling) / len(distinct_targets), 4)
            if distinct_targets else 0.0,
        dangling_target_examples=sorted(distinct_dangling)[:10],
        note="CLAUDE.md tolerates dangling [[links]] by design; this is the actual rate.",
    )
with open(os.path.join(OUT, "wikilinks.jsonl"), "w") as fh:
    for r in wl_rows:
        fh.write(json.dumps(r) + "\n")

# ---- B. commit-message refs ---------------------------------------------
log = subprocess.check_output(
    ["git", "log", "--all", "--pretty=format:%H%x01%s%x01%b%x02"],
    text=True, errors="replace")
commits = [c for c in log.split("\x02") if c.strip()]

# registries
tc_ids = set()
lp = "dev/todos-and-considerations-ledger.jsonl"
if os.path.exists(lp):
    for line in open(lp, encoding="utf-8", errors="replace"):
        line = line.strip()
        if line:
            try:
                r = json.loads(line)
                if str(r.get("id", "")).startswith("TC-"):
                    tc_ids.add(r["id"])
            except Exception:
                pass
def max_seq(path):
    mx = 0
    if os.path.exists(path):
        for line in open(path, encoding="utf-8", errors="replace"):
            line = line.strip()
            if line:
                try:
                    mx = max(mx, int(json.loads(line).get("seq", 0)))
                except Exception:
                    pass
    return mx
steward_max = max_seq("dev/steward/steward-ledger.jsonl")
todos_max = max_seq(lp)

LEDGER_RE = re.compile(r"ledger seq (\d+)", re.I)
TC_RE = re.compile(r"\bTC-\d+\b")
F_RE = re.compile(r"\bF-\d+\b")
SESS_RE = re.compile(r"Claude-Session:\s*(\S+)")

cref_rows = []
c_counts = collections.Counter()
c_dangling = collections.Counter()
for c in commits:
    parts = c.split("\x01")
    if len(parts) < 3:
        continue
    sha, subj, body = parts[0].strip(), parts[1], parts[2]
    text = subj + "\n" + body
    for m in LEDGER_RE.finditer(text):
        seq = int(m.group(1))
        # a ledger-seq ref resolves if <= some ledger's max (we can't know which
        # ledger; accept if <= max(steward,todos))
        ok = seq <= max(steward_max, todos_max)
        c_counts["ledger_seq"] += 1
        if not ok:
            c_dangling["ledger_seq"] += 1
        cref_rows.append(dict(kind="ledger_seq", sha=sha[:12], val=seq, resolves=ok))
    for m in TC_RE.finditer(text):
        tok = m.group(0)
        ok = tok in tc_ids
        c_counts["TC"] += 1
        if not ok:
            c_dangling["TC"] += 1
        cref_rows.append(dict(kind="TC", sha=sha[:12], val=tok, resolves=ok))
    for m in SESS_RE.finditer(text):
        c_counts["Claude-Session"] += 1  # external URL; unresolvable offline
        cref_rows.append(dict(kind="Claude-Session", sha=sha[:12], val=m.group(1),
                              resolves=None))

result["B_commit_refs"] = dict(
    commits_scanned=len(commits),
    steward_ledger_max_seq=steward_max,
    todos_ledger_max_seq=todos_max,
    tc_ids_defined=len(tc_ids),
    counts=dict(c_counts),
    dangling=dict(c_dangling),
    ledger_seq_dangling_rate=round(c_dangling["ledger_seq"] / c_counts["ledger_seq"], 4)
        if c_counts["ledger_seq"] else None,
    tc_dangling_rate=round(c_dangling["TC"] / c_counts["TC"], 4)
        if c_counts["TC"] else None,
    note="Claude-Session refs are external URLs -> unresolvable offline (resolves=null).",
)
with open(os.path.join(OUT, "commit_refs.jsonl"), "w") as fh:
    for r in cref_rows:
        fh.write(json.dumps(r) + "\n")

# ---- C. doc @sha pins ----------------------------------------------------
SHA_RE = re.compile(r"@([0-9a-f]{7,40})\b")
pin_rows = []
for root_dir in ("dev/design", "dev/adr"):
    for root, _dirs, files in os.walk(root_dir):
        for f in files:
            if not f.endswith(".md"):
                continue
            path = os.path.join(root, f)
            for i, line in enumerate(open(path, encoding="utf-8", errors="replace"), 1):
                for m in SHA_RE.finditer(line):
                    sha = m.group(1)
                    exists = subprocess.run(
                        ["git", "cat-file", "-e", sha + "^{commit}"],
                        capture_output=True).returncode == 0
                    reachable = None
                    if exists:
                        reachable = subprocess.run(
                            ["git", "merge-base", "--is-ancestor", sha, "HEAD"],
                            capture_output=True).returncode == 0
                    pin_rows.append(dict(file=path, line=i, sha=sha,
                                         object_exists=exists, reachable_from_head=reachable))
n = len(pin_rows)
result["C_doc_sha_pins"] = dict(
    pins=n,
    missing_object=sum(1 for r in pin_rows if not r["object_exists"]),
    unreachable=sum(1 for r in pin_rows if r["object_exists"] and r["reachable_from_head"] is False),
    detail=pin_rows,
    note="In-repo @sha pins are rare; the frequently-cited @c3b3d631 lives in the "
         "auto-memory prose, not tracked docs.",
)

with open(os.path.join(OUT, "natural_refs.json"), "w") as fh:
    json.dump(result, fh, indent=2, sort_keys=True)
print(json.dumps({k: (v if k != "C_doc_sha_pins" else {kk: vv for kk, vv in v.items() if kk != "detail"})
                  for k, v in result.items()}, indent=2, sort_keys=True))
