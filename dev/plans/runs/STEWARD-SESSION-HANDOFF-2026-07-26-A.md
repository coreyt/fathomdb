---
status: ACTIVE
---

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
