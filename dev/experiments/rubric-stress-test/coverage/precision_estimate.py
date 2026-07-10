#!/usr/bin/env python3
"""C2 fix — HONEST precision estimation for the `needs_adjudication=False` auto-flag
detectors.

The prior version scored precision ONLY via a narrow hand-written negation/hypothetical
regex per detector. Those regexes have near-zero recall against real false positives, so
every DQ detector scored FP=0 and precision came out exactly 1.000 with an illusory
"tight" Wilson CI. A tight CI at k=n there means only "the negation lexicon matched
nothing", NOT "the firing rows were verified correct" — no true-positive row was ever
independently confirmed.

This script does TWO independent things and keeps them clearly separate:

  (A) AUTO-ADJUDICATOR ESTIMATE (weak, disclosed): the per-detector negation lexicon is
      run over the FULL auto-flag population. It is reported as a LOWER-SENSITIVITY
      heuristic with UNMEASURED FP sensitivity — an upper bound on precision, not a
      verified figure. Its Wilson CI is explicitly labelled non-trustworthy.

  (B) HAND-ADJUDICATED SAMPLE (the real figure): a seeded RANDOM sample of the auto-flag
      POSITIVES (not just negation-hits) is drawn per detector and its bounded ±2-line
      windows are written to out/precision_windows.txt for manual labelling. A human
      (the agent, under the read-budget hard rule) records TP/FP verdicts in
      out/handlabels.tsv (detector<TAB>line_no<TAB>TP|FP). Precision is then computed
      from that LABELLED SAMPLE with a Wilson CI that reflects the true (small) sample
      size and real adjudication uncertainty.

Only (B) is a defensible precision claim, and only for the detectors whose sample was
actually labelled; everything else is reported as "not independently confirmed".

HARD RULE honored: only bounded ±2-line windows, capped total, no whole-transcript read.
"""
import sys, os, json, math, collections, re, random
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import parse
HERE = os.path.dirname(os.path.abspath(__file__))
CAND = os.path.join(HERE, "out", "coverage_candidates.jsonl")
WIN_OUT = os.path.join(HERE, "out", "precision_windows.txt")
HANDLABELS = os.path.join(HERE, "out", "handlabels.tsv")

SEED = 20260709
SAMPLE_PER_DET = 40      # random positives drawn per detector for the labelling pool
WINDOWS_TOTAL_CAP = 24   # bounded windows actually emitted (honors the read budget)

# Detectors that ship as needs_adjudication=False (auto-flag). DQ-STALE-DOC is NO LONGER
# here (C1: re-scoped to a candidate-extractor), so it is not scored as an auto-flag.
DETERMINISTIC = [
    "DQ-SHORTKNOWLEDGE", "DQ-ASSUME-STRUCTURAL", "DQ-UNVERIFIED-METRIC",
    "DQ-STALE-VERSION", "DQ-DEPBLIND-RETRO", "DQ-IGNOREDESIGN-STRUCTURAL",
    "ROLE-BLEED-SOURCE-EDIT", "BRANCH-UNVERIFIED-BEFORE-COMMIT",
    "IRREVERSIBLE-ACTION-UNGATED", "WORKTREE-DISCIPLINE-BREACH",
]

# (A) weak negation lexicons — a match => the auto-adjudicator SUSPECTS a false positive.
# Near-zero recall by construction; used only for the disclosed weak estimate.
NEG = {
    "DQ-SHORTKNOWLEDGE": re.compile(
        r"\b(?:means?|refers? to|is defined|describes?|what .{0,20}means|protected main|"
        r"(?:is|was|been|already|pending|awaiting|status|marked|model)\s+(?:approv|ratif|sign)|"
        r"approval status|sign-?off status|for example|e\.g\.)\b", re.I),
    "DQ-ASSUME-STRUCTURAL": re.compile(
        r"\b(?:rather than assum|instead of assum|don'?t (?:need to |have to )?assum|"
        r"not assum|no assumption|without assuming|avoid assuming)\b", re.I),
    "DQ-UNVERIFIED-METRIC": re.compile(r"\b(?:if|would|hypothetical|plan to|will (?:add|create))\b", re.I),
    "DQ-STALE-VERSION": re.compile(r"\b(?:if|would|to avoid|prevent|keep .{0,15}fresh)\b", re.I),
    "DQ-DEPBLIND-RETRO": re.compile(
        r"\b(?:won'?t (?:break|fail)|to avoid (?:break|fail)|so (?:it|that) (?:doesn'?t|won'?t)|"
        r"would (?:break|fail) if)\b", re.I),
    "DQ-IGNOREDESIGN-STRUCTURAL": re.compile(r"(dev/design/|dev/adr/|/adr[-/]|record-lifecycle|\.md\b)", re.I),
    "ROLE-BLEED-SOURCE-EDIT": None,
    "BRANCH-UNVERIFIED-BEFORE-COMMIT": re.compile(r"\b(?:rev-parse|--show-current|branch --show)\b", re.I),
    "IRREVERSIBLE-ACTION-UNGATED": re.compile(r"\b(?:tag -l|tag --list|--dry-run|describe --tags|-n\b)\b", re.I),
    "WORKTREE-DISCIPLINE-BREACH": re.compile(r"\bisolated\b|not (?:the )?shared", re.I),
}


def wilson(k, n, z=1.96):
    if n == 0:
        return (0.0, 0.0, 0.0)
    p = k / n
    denom = 1 + z * z / n
    center = (p + z * z / (2 * n)) / denom
    half = (z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))) / denom
    return (round(p, 3), round(max(0, center - half), 3), round(min(1, center + half), 3))


def neg_suspects_fp(det, row):
    snip = row.get("snippet") or ""
    if det == "ROLE-BLEED-SOURCE-EDIT":
        return (row.get("role_source") != "command-mode") or (not row.get("fathomdb_session"))
    if det == "DQ-IGNOREDESIGN-STRUCTURAL":
        return bool(NEG[det].search(row.get("edited_path") or snip))
    rx = NEG.get(det)
    if rx is None:
        return False
    return bool(rx.search(snip))


def window(path, target, w=2):
    lines = {}
    for r in parse.iter_file(path):
        if abs(r["line_no"] - target) <= w:
            tag = "A" if r["type"] == "assistant" else ("U" if r["type"] == "user" else r["type"][:1])
            tools = ",".join(r["tool_names"])[:18]
            txt = re.sub(r"\s+", " ", (r["text"] or "")).strip()[:170]
            lines[r["line_no"]] = f"{tag}{'*' if r['line_no']==target else ' '}[{tools}] {txt}"
        if r["line_no"] > target + w:
            break
    return [lines[k] for k in sorted(lines)]


def row_key(r):
    """Stable identity for a candidate row: (file-basename[:26], line_no) — matches the
    token printed in the windows file, so same-line rows in different files don't collide."""
    return (os.path.basename(r["file"])[:26], r["line_no"])


def load_handlabels():
    """detector -> {(file_token, line_no): 'TP'|'FP'} from out/handlabels.tsv (optional).
    Row format: <detector>\\t<file_token>\\t<line_no>\\t<TP|FP>."""
    labels = collections.defaultdict(dict)
    if not os.path.exists(HANDLABELS):
        return labels
    for ln in open(HANDLABELS):
        ln = ln.strip()
        if not ln or ln.startswith("#"):
            continue
        parts = ln.split("\t")
        if len(parts) < 4:
            continue
        det, ftok, line_no, verdict = (parts[0].strip(), parts[1].strip(),
                                       parts[2].strip(), parts[3].strip().upper())
        if verdict in ("TP", "FP"):
            labels[det][(ftok, int(line_no))] = verdict
    return labels


def main():
    rows = [json.loads(l) for l in open(CAND)]
    by_det = collections.defaultdict(list)
    for r in rows:
        if r.get("needs_adjudication"):
            continue
        by_det[r["detector"]].append(r)

    hand = load_handlabels()
    rng = random.Random(SEED)

    # ---- draw a seeded random sample of POSITIVES per detector (labelling pool) ----
    sample = {}
    for det in DETERMINISTIC:
        pop = by_det.get(det, [])
        k = min(SAMPLE_PER_DET, len(pop))
        sample[det] = rng.sample(pop, k) if k else []

    # ---- (A) weak auto-adjudicator estimate over full population ----
    print("=" * 100)
    print("(A) WEAK auto-adjudicator estimate — negation lexicon, UNMEASURED FP")
    print("    sensitivity. Treat as an UPPER BOUND, NOT a verified precision. CIs here")
    print("    are NOT trustworthy (they only reflect 'the negation lexicon matched X').")
    print("=" * 100)
    print(f"{'detector':<34}{'N':>5}{'neg?':>6}{'est_p<=':>9}  (non-trustworthy CI)")
    print("-" * 100)
    tbl = {}
    for det in DETERMINISTIC:
        pop = by_det.get(det, [])
        n = len(pop)
        neg = sum(1 for r in pop if neg_suspects_fp(det, r))
        p, lo, hi = wilson(n - neg, n)
        print(f"{det:<34}{n:>5}{neg:>6}{p:>9.3f}  [{lo:.3f}, {hi:.3f}]")
        tbl[det] = {
            "N_population": n,
            "auto_adjudicator": {
                "neg_lexicon_flagged": neg,
                "est_precision_upper_bound": p,
                "ci95_NON_TRUSTWORTHY": [lo, hi],
                "note": "negation-lexicon heuristic; FP sensitivity UNMEASURED; no "
                        "positive independently confirmed by this pass",
            },
        }

    # ---- (B) hand-adjudicated sample precision (the real figure, where labelled) ----
    print("\n" + "=" * 100)
    print("(B) HAND-ADJUDICATED SAMPLE precision — the only defensible figure. Computed")
    print("    from out/handlabels.tsv over the seeded random POSITIVE sample.")
    print("=" * 100)
    print(f"{'detector':<34}{'sample_n':>9}{'labelled':>9}{'TP':>4}{'FP':>4}{'precision':>11}  Wilson95")
    print("-" * 100)
    for det in DETERMINISTIC:
        samp = sample[det]
        labelled = [(r, hand[det][row_key(r)]) for r in samp if row_key(r) in hand.get(det, {})]
        ln = len(labelled)
        tp = sum(1 for _, v in labelled if v == "TP")
        fp = ln - tp
        if ln:
            p, lo, hi = wilson(tp, ln)
            cistr = f"[{lo:.3f}, {hi:.3f}]"
        else:
            p, cistr = None, "(no labels)"
        pstr = f"{p:.3f}" if p is not None else "  n/a"
        print(f"{det:<34}{len(samp):>9}{ln:>9}{tp:>4}{fp:>4}{pstr:>11}  {cistr}")
        tbl[det]["hand_sample"] = {
            "sample_pool": len(samp), "labelled": ln, "TP": tp, "FP": fp,
            "precision": p, "ci95": wilson(tp, ln)[1:] if ln else None,
            "confirmed": ln > 0,
        }

    # ---- emit bounded windows for the UNLABELLED sampled positives (labelling aid) ----
    unlabelled = []
    for det in DETERMINISTIC:
        for r in sample[det]:
            if row_key(r) not in hand.get(det, {}):
                unlabelled.append((det, r))
    # spread across detectors: round-robin so the window budget isn't eaten by one detector
    unlabelled.sort(key=lambda t: (t[0], t[1]["line_no"]))
    with open(WIN_OUT, "w") as wf:
        wf.write("# Bounded ±2-line windows for UNLABELLED sampled auto-flag positives.\n")
        wf.write("# Label each into out/handlabels.tsv:  <detector>\\t<file_token>\\t<line_no>\\tTP|FP\n")
        wf.write(f"# seed={SEED} sample_per_det={SAMPLE_PER_DET} cap={WINDOWS_TOTAL_CAP}\n\n")
        emitted = 0
        # round-robin across detectors
        pools = collections.defaultdict(list)
        for det, r in unlabelled:
            pools[det].append(r)
        order = [d for d in DETERMINISTIC if pools[d]]
        idx = collections.defaultdict(int)
        while emitted < WINDOWS_TOTAL_CAP and any(idx[d] < len(pools[d]) for d in order):
            for d in order:
                if idx[d] >= len(pools[d]) or emitted >= WINDOWS_TOTAL_CAP:
                    continue
                r = pools[d][idx[d]]
                idx[d] += 1
                emitted += 1
                wf.write(f"[{d}] {os.path.basename(r['file'])[:26]} L{r['line_no']} "
                         f"sig={r.get('matched_signal')!r}\n")
                for wl in window(r["file"], r["line_no"]):
                    wf.write("   " + wl + "\n")
                wf.write("\n")
    print(f"\nwrote {WIN_OUT} ({emitted if 'emitted' in dir() else 0} windows) — label into {os.path.basename(HANDLABELS)}")

    with open(os.path.join(HERE, "out", "precision_estimate.json"), "w") as fh:
        json.dump(tbl, fh, indent=2)
    print("wrote out/precision_estimate.json")


if __name__ == "__main__":
    main()
