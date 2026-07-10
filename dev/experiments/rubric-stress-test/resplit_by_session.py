#!/usr/bin/env python3
"""Re-split the transcript universe by PARENT SESSION, not by file (fixes F1).

Every file of one session — parent transcript, all subagents, workflow journals —
is assigned to the SAME fold, so a held-out (validation) file can never be a
near-duplicate sibling (same prompt / task / review cycle) of an experiment file
used to tune the detectors.

Deterministic: fold is chosen by a stable hash of the parent-session id, targeting
~31% validation (the original file-level ratio). The two ground-truth RCA sessions
are PINNED to experiment (they are the labeled positives used for tuning/smoke-test
and must never sit in the sealed held-out set).

Writes split-experiment.txt / split-validation.txt in DATA_DIR. Never commits.
"""
import os, sys, hashlib, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import parse

DATA = "/home/coreyt/transcript-data"
VAL_FRACTION = 0.31            # match the original file-level split ratio
PIN_EXPERIMENT = {
    "memex/2fa060bc-ca20-40dd-8b39-925793bf6ba9",     # CR-047 finish-vs-delete RCA
    "fathomdb/f57b5dee-caaa-4cdd-8546-a31809e0bd7c",   # 30-N plan-delta RCA
}


def all_files():
    """Universe = every file currently in either split (== the manifest)."""
    files = []
    for name in ("split-experiment.txt", "split-validation.txt"):
        p = os.path.join(DATA, name)
        with open(p) as fh:
            files.extend(l.strip() for l in fh if l.strip())
    return sorted(set(files))


def fold_for(session_id):
    if session_id in PIN_EXPERIMENT:
        return "E"
    h = int(hashlib.sha1(session_id.encode()).hexdigest(), 16) % 1000
    return "V" if h < int(VAL_FRACTION * 1000) else "E"


def main():
    files = all_files()
    groups = collections.defaultdict(list)
    for f in files:
        groups[parse.parent_session_id(f)].append(f)

    exp, val = [], []
    for sid in sorted(groups):
        (val if fold_for(sid) == "V" else exp).extend(sorted(groups[sid]))
    exp.sort(); val.sort()

    # invariant: no session id appears in both folds
    e_sids = {parse.parent_session_id(f) for f in exp}
    v_sids = {parse.parent_session_id(f) for f in val}
    straddle = e_sids & v_sids
    assert not straddle, f"leak: {len(straddle)} sessions straddle both folds"
    for pin in PIN_EXPERIMENT:
        assert pin not in v_sids, f"ground-truth {pin} leaked into validation"

    with open(os.path.join(DATA, "split-experiment.txt"), "w") as fh:
        fh.write("\n".join(exp) + "\n")
    with open(os.path.join(DATA, "split-validation.txt"), "w") as fh:
        fh.write("\n".join(val) + "\n")

    print(f"sessions: {len(groups)}  exp-sessions: {len(e_sids)}  val-sessions: {len(v_sids)}")
    print(f"files:    exp={len(exp)}  val={len(val)}  "
          f"val_frac={len(val)/(len(exp)+len(val)):.3f}")
    print(f"straddling sessions after re-split: {len(straddle)} (must be 0)")


if __name__ == "__main__":
    main()
