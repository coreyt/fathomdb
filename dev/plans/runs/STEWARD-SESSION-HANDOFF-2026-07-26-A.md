---
status: SUPERSEDED
---

> **⚠ SUPERSEDED by `STEWARD-SESSION-HANDOFF-2026-07-27-A.md` (2026-07-27).** Every task listed below has
> been actioned or ruled: Slice 20c was verified, the codex round-5 `[P2]` became **TC-71** (folded into
> Slice 21), **OOS-12** resolved to a pre-existing `slice15e` flake of the TC-72 family,
> `t_s34_dump_mutations_lock_held_exits_71` likewise (TC-29/TC-72), the ledgerwrite id-collision repair was
> **declined by ruling** in favour of the tooling fix, and Slices 21/30 plus DOC-HYGIENE-3 were placed in the
> re-sequenced tail (master **F-35**). Retained as the decision record; **do not act on it as a task list.**

# Steward session hand-off — 2026-07-26-A

> **HITL-authored task list for the next Steward session.** Read after the §3 cold-start.
> Repo state at hand-off: `origin/main` = `f1f50243`, tree clean, everything pushed.
> 0.8.20 ladder: slices **0 · 5 · 10 · 15 · 20 · 20c · 25 LANDED**; remaining **30 → 40**.

1. Initial items:
   - check repo state regarding 0.8.20 Slice 20c+
   - Fix ledgerwrite has no id-collision rejection (and do a one-time surgical repair, avoiding the append-only rule)

2. There seem to be open issues:
   - codex round 5 returns one [P2] — narrower than everything before it: `vector_projection_declared` keys off
     `vector_declared` alone, so `{roles:[filterable], vector:true}` would wrongly activate the dense arm
     instead of staying inert.
   - OOS-12
   - `t_s34_dump_mutations_lock_held_exits_71`

3. Still pending work, regardless:
   - Slice 21
   - DOC-HYGIENE-3
   - Slice 30

---

Determine if there are other items that need to be considered as part of 0.8.20 scope.

---

Propose a sequence to run this work.
