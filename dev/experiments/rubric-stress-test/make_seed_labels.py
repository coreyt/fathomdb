#!/usr/bin/env python3
"""Recompute seed precision by pinning hand-labels to a stable per-candidate
FINGERPRINT (F4), not to a fragile (detector, idx_in_stride) position.

Durable label store: seed_labels_fp.jsonl — one row per hand-labeled candidate:
  {fingerprint, detector, family, confidence, parent_session, file, line_no,
   matched_signal, label(TP|FP), reasoning, snippet_head}
The fingerprint = sha1(parent_session | file | line_no | matched_signal)
(see parse.candidate_fingerprint). N1: snippet is deliberately EXCLUDED from the
key (verified 0 mismatch) so a cosmetic ±window/regex tweak keeps a still-valid
detection pinned instead of re-hashing and orphaning every label wholesale.

On each run we build a fingerprint index over out/experiment_candidates.jsonl and
re-attach every label by fingerprint. NOTHING is re-mapped by position, so a
detector/regex/split change can never silently bind a label to a different
candidate:
  * a label whose fingerprinted candidate is still present  -> SURVIVOR (scored)
  * a label whose candidate is gone/changed                 -> ORPHAN (needs relabel;
    reported loudly, NOT scored)
Every label row is accounted for; unmatched keys never vanish silently.

IMPORTANT framing (do not overstate):
  * The reported precision is MACRO SEED precision over a capped per-detector
    distinct-session stride (N<=10/detector) — it is NOT a population estimate.
    review-catch is ~88% of real candidate volume but, after the C1/C2/N1/N2 round,
    contributes only N=3 survivor labels, so this macro number under-weights the
    dominant, lowest-precision class (and the candidate-weighted approximation is
    now dominated by that single N=3 cell — even less defensible, do not cite it as
    population precision). A defensible population number requires the stratified
    random sample with Horvitz-Thompson weighting (see formalization.md N1/F2 and
    Known-Gap #1/#2 for current volumes and orphan accounting — the source of truth).
  * Confidence-tier precision is reported with Wilson 95% CIs; tier Ns are far
    too small to CONFIRM the monotonic trend.
  * Per-detector cells with N<5 are asterisked as insufficient sample.
"""
import sys, json, collections, os, math

HERE = os.path.dirname(os.path.abspath(__file__))
CAND = os.path.join(HERE, "out/experiment_candidates.jsonl")
FP_STORE = os.path.join(HERE, "seed_labels_fp.jsonl")
MIN_N = 5  # per-detector cells below this are flagged insufficient


def wilson(k, n, z=1.96):
    """Wilson score interval for a binomial proportion."""
    if n == 0:
        return (0.0, 0.0)
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = (z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))) / d
    return (max(0.0, c - h), min(1.0, c + h))


def load_labels():
    with open(FP_STORE) as fh:
        return [json.loads(l) for l in fh if l.strip()]


def index_candidates():
    """fingerprint -> candidate, plus real per-detector candidate volume."""
    idx = {}
    vol = collections.Counter()
    with open(CAND) as fh:
        for line in fh:
            c = json.loads(line)
            fp = c.get("fingerprint")
            if fp:
                idx[fp] = c
            vol[c["detector"]] += 1
    return idx, vol


def prec_str(tp, n, flag_small=False):
    if n == 0:
        return "n/a"
    s = f"{tp}/{n} = {tp/n:.2f}"
    if flag_small and n < MIN_N:
        s += " *"
    return s


def main():
    labels = load_labels()
    idx, vol = index_candidates()

    survivors, orphans = [], []
    for lab in labels:
        c = idx.get(lab["fingerprint"])
        if c is None:
            orphans.append(lab)
        else:
            survivors.append((lab, c))

    # accountability: every label is either a survivor or a reported orphan
    assert len(survivors) + len(orphans) == len(labels)

    det_tp = collections.Counter(); det_n = collections.Counter()
    fam_tp = collections.Counter(); fam_n = collections.Counter()
    conf_tp = collections.Counter(); conf_n = collections.Counter()

    out_tsv = os.path.join(HERE, "seed_labels.tsv")
    with open(out_tsv, "w") as fh:
        fh.write("detector\tfamily\tconf\tsig\tparent_session\tline\tlabel\treasoning\tsnippet\n")
        for lab, c in sorted(survivors, key=lambda x: (x[0]["detector"], x[0]["line_no"])):
            d = lab["detector"]; l = lab["label"]
            det_n[d] += 1; fam_n[c["family"]] += 1; conf_n[c["confidence_heuristic"]] += 1
            if l == "TP":
                det_tp[d] += 1; fam_tp[c["family"]] += 1; conf_tp[c["confidence_heuristic"]] += 1
            fh.write("\t".join([
                d, c["family"], c["confidence_heuristic"], c["matched_signal"],
                lab["parent_session"], str(c["line_no"]), l,
                lab["reasoning"], c["snippet"][:160].replace("\t", " ")]) + "\n")

    print(f"== label pinning (F4) ==")
    print(f"  labels in store: {len(labels)}   survivors (scored): {len(survivors)}"
          f"   orphans (NEED RELABEL): {len(orphans)}")
    for o in orphans:
        print(f"    ORPHAN {o['detector']:30s} sess={o['parent_session'][:20]:20s} "
              f"line={o['line_no']} was={o['label']} :: {o['reasoning'][:70]}")

    print("\n== per-detector seed precision (macro stride, N<=10/detector; * = N<5 insufficient) ==")
    for d in sorted(det_n):
        lo, hi = wilson(det_tp[d], det_n[d])
        print(f"  {d:32s} {prec_str(det_tp[d], det_n[d], True):16s} Wilson95[{lo:.2f},{hi:.2f}]")

    print("== per-family seed precision ==")
    for f in sorted(fam_n):
        lo, hi = wilson(fam_tp[f], fam_n[f])
        print(f"  {f:26s} {prec_str(fam_tp[f], fam_n[f]):16s} Wilson95[{lo:.2f},{hi:.2f}]")

    print("== per-confidence seed precision (monotonic-in-seed only; Ns too small to confirm) ==")
    for cf in ("high", "med", "low", "info", "error"):
        if conf_n[cf] == 0:
            continue
        lo, hi = wilson(conf_tp[cf], conf_n[cf])
        print(f"  {cf:6s} {prec_str(conf_tp[cf], conf_n[cf]):16s} Wilson95[{lo:.2f},{hi:.2f}]")

    T = sum(det_tp.values()); Nn = sum(det_n.values())
    lo, hi = wilson(T, Nn)
    print(f"\n== overall MACRO seed precision: {prec_str(T, Nn)}  Wilson95[{lo:.2f},{hi:.2f}] ==")
    print("   (macro stride precision — NOT a population estimate)")

    # F2: candidate-weighted approximation, applying each detector's SEED rate to
    # its REAL candidate volume. Illustrative caution only (seed rates are noisy);
    # a defensible population figure needs a stratified H-T sample.
    tot_vol = sum(vol[d] for d in det_n)
    if tot_vol:
        w = sum((det_tp[d] / det_n[d]) * vol[d] for d in det_n) / tot_vol
        print(f"== candidate-weighted precision (seed rate x real volume, illustrative): {w:.2f} ==")
        print("   per-detector real volume:")
        for d in sorted(det_n, key=lambda x: -vol[x]):
            share = vol[d] / tot_vol
            print(f"     {d:32s} vol={vol[d]:6d} ({share:4.0%})  seed_rate={det_tp[d]/det_n[d]:.2f} (N={det_n[d]})")


if __name__ == "__main__":
    main()
