#!/usr/bin/env python3
"""Miner 1 — in-code marker inventory + dangling resolution (0 LLM).

Scans the SHIPPED source tree (src/) for marker-like cross-references to
governance artifacts, classifies each, and RESOLVES it against the artifact
side to decide dangling vs resolvable. Pure stdlib + regex + filesystem.

Marker classes and their artifact-side registries:
  ADR-PATH   dev/adr/<file>.md path ref        -> filesystem existence
  DESIGN-PATH dev/design/<file>.md path ref     -> filesystem existence
  ADR-ID     ADR-<ver>-<slug> id ref            -> dev/adr/ filename prefix match
  AC         AC-<nnn>[a-z]                       -> ids defined in dev/acceptance.md
  REQ        REQ-<nnn>                           -> ids defined in dev/acceptance.md
  TC         TC-<n>                              -> ids in todos ledger (+ status)
  F          F-<nn>                              -> ids mentioned in dev/plans/*.md

Output: out/incode_markers.jsonl  (one row per marker occurrence)
        out/incode_summary.json   (per-class counts + dangling rates)
"""
import json, os, re, subprocess, sys, collections

REPO = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
os.chdir(REPO)
OUT = "dev/experiments/code-markers-eval/out"
os.makedirs(OUT, exist_ok=True)

SRC_EXT = (".rs", ".ts", ".js", ".py")
SRC_ROOTS = ["src"]
SKIP_DIRS = {"target", "node_modules", "dist", "build", "__pycache__", ".git"}

# ---- artifact-side registries -------------------------------------------
def load_registries():
    reg = {}
    # ADR files (basename without .md, and full path)
    adr_files = {}
    for f in os.listdir("dev/adr") if os.path.isdir("dev/adr") else []:
        if f.endswith(".md"):
            adr_files[f[:-3]] = "dev/adr/" + f
    reg["adr_files"] = adr_files
    # AC / REQ ids defined in acceptance.md
    ac_ids, req_ids = set(), set()
    if os.path.exists("dev/acceptance.md"):
        txt = open("dev/acceptance.md", encoding="utf-8", errors="replace").read()
        ac_ids = set(re.findall(r"AC-\d+[a-z]?", txt))
        req_ids = set(re.findall(r"REQ-\d+", txt))
    reg["ac_ids"], reg["req_ids"] = ac_ids, req_ids
    # TC ids + latest status from todos ledger
    tc_status = {}
    lp = "dev/todos-and-considerations-ledger.jsonl"
    if os.path.exists(lp):
        for line in open(lp, encoding="utf-8", errors="replace"):
            line = line.strip()
            if not line:
                continue
            try:
                r = json.loads(line)
            except Exception:
                continue
            tid = str(r.get("id", ""))
            if tid.startswith("TC-"):
                tc_status[tid] = r.get("status")  # last write wins = current
    reg["tc_status"] = tc_status
    # F ids mentioned anywhere in dev/plans/*.md
    f_ids = set()
    for root, _dirs, files in os.walk("dev/plans"):
        for f in files:
            if f.endswith(".md"):
                try:
                    t = open(os.path.join(root, f), encoding="utf-8", errors="replace").read()
                except Exception:
                    continue
                f_ids |= set(re.findall(r"F-\d+", t))
    reg["f_ids"] = f_ids
    return reg


REG = load_registries()

# ---- marker extractors ---------------------------------------------------
# Order matters: path refs consumed before bare ADR-id so we don't double count.
PATH_RE = re.compile(r"dev/(adr|design)/[A-Za-z0-9._/\-]+\.md")
ADRID_RE = re.compile(r"\bADR-[0-9]+(?:\.[0-9]+)*-[A-Za-z0-9\-]+")
AC_RE = re.compile(r"\bAC-\d+[a-z]?\b")
REQ_RE = re.compile(r"\bREQ-\d+\b")
TC_RE = re.compile(r"\bTC-\d+\b")
F_RE = re.compile(r"\bF-\d+\b")


def resolve(cls, tok):
    """Return (status, detail). status in {resolved, dangling, resolved-lifecycle}."""
    if cls in ("ADR-PATH", "DESIGN-PATH"):
        return ("resolved", "exists") if os.path.exists(tok) else ("dangling", "no-file")
    if cls == "ADR-ID":
        # ADR-0.8.14-exp-s... : match a file whose name starts with the token
        for name in REG["adr_files"]:
            if name == tok or name.startswith(tok):
                return ("resolved", REG["adr_files"][name])
        # relax: try version-prefix (ADR-0.8.14) if slug drifted
        m = re.match(r"ADR-[0-9]+(?:\.[0-9]+)*", tok)
        if m:
            pref = m.group(0)
            hits = [n for n in REG["adr_files"] if n.startswith(pref)]
            if hits:
                return ("dangling-slug", "ver-only:" + ";".join(sorted(hits)[:3]))
        return ("dangling", "no-adr")
    if cls == "AC":
        return ("resolved", "") if tok in REG["ac_ids"] else ("dangling", "not-in-acceptance")
    if cls == "REQ":
        return ("resolved", "") if tok in REG["req_ids"] else ("dangling", "not-in-acceptance")
    if cls == "TC":
        st = REG["tc_status"].get(tok)
        if st is None:
            return ("dangling", "no-tc")
        # resolvable AND carries a lifecycle state -> flag if terminal
        terminal = st in ("resolved", "closed", "superseded", "done", "wontfix")
        return ("resolved-terminal" if terminal else "resolved-open", "status=" + str(st))
    if cls == "F":
        return ("resolved", "") if tok in REG["f_ids"] else ("dangling", "no-f")
    return ("unknown", "")


def scan():
    rows = []
    for root_dir in SRC_ROOTS:
        for root, dirs, files in os.walk(root_dir):
            dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
            for fn in files:
                if not fn.endswith(SRC_EXT):
                    continue
                path = os.path.join(root, fn)
                # only comment/doc lines carry markers; but we scan all lines and
                # rely on the marker regexes being specific enough.
                try:
                    lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
                except Exception:
                    continue
                is_test = "/tests/" in path or fn.endswith(".test.ts") or fn.startswith("test_")
                for i, line in enumerate(lines, 1):
                    consumed = []  # (start,end) spans already claimed by path refs
                    for m in PATH_RE.finditer(line):
                        tok = m.group(0)
                        cls = "ADR-PATH" if "/adr/" in tok else "DESIGN-PATH"
                        st, det = resolve(cls, tok)
                        rows.append(dict(cls=cls, token=tok, status=st, detail=det,
                                         file=path, line=i, is_test=is_test))
                        consumed.append((m.start(), m.end()))
                    def outside(m):
                        return not any(s <= m.start() < e for s, e in consumed)
                    for rex, cls in ((ADRID_RE, "ADR-ID"), (AC_RE, "AC"),
                                     (REQ_RE, "REQ"), (TC_RE, "TC"), (F_RE, "F")):
                        for m in rex.finditer(line):
                            if not outside(m):
                                continue
                            tok = m.group(0)
                            st, det = resolve(cls, tok)
                            rows.append(dict(cls=cls, token=tok, status=st, detail=det,
                                             file=path, line=i, is_test=is_test))
    return rows


def main():
    rows = scan()
    with open(os.path.join(OUT, "incode_markers.jsonl"), "w") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")
    # summary
    by_cls = collections.defaultdict(lambda: collections.Counter())
    dist_tokens = collections.defaultdict(set)
    for r in rows:
        by_cls[r["cls"]][r["status"]] += 1
        dist_tokens[r["cls"]].add(r["token"])
    summary = {}
    for cls, ctr in by_cls.items():
        total = sum(ctr.values())
        dangling = sum(v for k, v in ctr.items() if k.startswith("dangling"))
        summary[cls] = dict(
            occurrences=total,
            distinct_tokens=len(dist_tokens[cls]),
            dangling=dangling,
            dangling_rate=round(dangling / total, 4) if total else 0.0,
            status_breakdown=dict(ctr),
        )
    summary["_registry_sizes"] = dict(
        adr_files=len(REG["adr_files"]), ac_ids=len(REG["ac_ids"]),
        req_ids=len(REG["req_ids"]), tc_ids=len(REG["tc_status"]),
        f_ids=len(REG["f_ids"]),
    )
    summary["_total_occurrences"] = len(rows)
    with open(os.path.join(OUT, "incode_summary.json"), "w") as fh:
        json.dump(summary, fh, indent=2, sort_keys=True)
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
