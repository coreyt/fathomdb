# FathomDB — Steward Session Hand-off (2026-07-24-A)

> **Boot:** run **`/steward`** (loads `.claude/agents/steward.md` + `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`),
> do its §3 cold-start reading, then read THIS doc, return a short orientation, and **WAIT for HITL** before
> mutating. You are the **Program Steward**: keep the schedule-of-record true to git, **commission + verify**
> release orchestrators, propose-first to the HITL. **Do not implement code or hand-drive a ladder.**
> *(Supersedes 2026-07-18-A. This session: the **0.8.20 Phase-2 keystone landed** — Slices 0/5/10/15 are all on
> `origin/main`; Slices 20 and 25 are unblocked.)*

---

## ★ IMMEDIATE NEXT STEP — commission the 0.8.20 Slice-20 orchestrator

<!-- BEGIN GENERATED release-state:0.8.20:handoff-next-step -->
**The 0.8.20 ladder is between slices: 0 → 5 → 10 → 15 → 20 → 25 → 30 → 21 → 22 → 23 → 31 → 32 → 33 → 39 are all LANDED; 40 is next.**<!-- END GENERATED release-state:0.8.20:handoff-next-step --> The next action is to
**commission a `/orchestrate dev/plans/plan-0.8.20.md` orchestrator** (alias `/orch`; the preferred entry point
per the standing HITL ruling of 2026-07-25 — `/goal complete 0.8.20` only on an explicit HITL instruction for
that run) against `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` +
`dev/plans/plan-0.8.20.md`, whose first job is **Slice 20 — `dense_readiness` + `flush_embeddings()` (R-20-DR)**.
You commission and verify it; you do not run it.

**What Slice 20 is** (plan §9 pointer + the ratified decision #1 in §9): R-20-DR **attaches `dense_readiness` to
the `ProjectionSpec.vector` sub-object** built in Slice 15d. The change is **additive**. It depends only on
Slice 15 (landed). Cut a **dedicated linked worktree** off a verified `origin/main` tip per **TC-RUBRIC-5**
(never the primary/shared checkout).

**Remaining ladder: 20 → 25 → 30 → 40.**

- **20** — `dense_readiness` + `flush_embeddings()` (R-20-DR) · depends 15.
- **25** — surrogate minting, **governed entities ONLY** (R-20-SUR) · depends 15.
- **30** — RUBRIC-H7 `can-i-deploy` contract gate (R-20-H7) · depends 10/15/20/25 · **publish precondition**
  (absent-or-failing gate HOLDS the breaking pair).
- **40** — verification + release-readiness, then **publish-or-hold** per the HITL gate.

**TC-11 + TC-32 are CLOSED; TC-46 + TC-47 are RESOLVED; Finding-1 = (A). Do NOT re-open them** (plan §11 header).

**★ ALSO commission (HITL-directed 2026-07-24) — status-board currency enforcement, items 1–3.** When you
commission the next orchestrator, **assign it the first effort of
`dev/design/status-board-currency-enforcement.md` (scope = items 1–3):** (1) the seam-ownership contract line in
`.claude/agents/steward.md` + `orchestration.md` §12.5 (Steward owns the **LANDED** row + next-slice pointer,
updated in the landing merge); (2) a board-currency **gate in `scripts/preflight.sh --landing`** (refuse a land
that leaves `STATUS-0.8.z.md` stale; RED-first test); (3) a **CI drift detector on `main`** (red when the board
disagrees with git ancestry). This exists because the 0.8.20 board lied for four days post-land — the failure
this closes. Item 4 (machine-derived table) is **later, only if drift persists**. It may ride alongside Slice 20
or be its own micro-effort — your call; it is docs + scripts + one CI job, TDD-able, and touches no engine code.

---

## Verified state (git @ `origin/main` `a2022957`; ledger tip `3264114a`)

**Slices 0, 5, 10, 15 are all COMPLETE and LANDED on `origin/main`. SCHEMA is 24.**

| Slice | Landed at | Content |
|------:|-----------|---------|
| **0** | `403eb254` | X0 design gate — HITL-SIGNED 2026-07-19 |
| **5** | `1f8ed8bf` | erasure completeness (R-20-E1…E8) — `search_index_v2` body-leak fixed at the mechanism |
| **10** | `3cfb3cda` (merge) | `ReadView` / read-modes + node-validity + TC-31 — SCHEMA 21→22 |
| **15** | `a2022957` (merge) | **Phase-2 keystone COMPLETE** — SCHEMA →24 |

**The Slice-15 keystone (`a2022957`) landed:** registry **R-20-PR** (row-owned projection registry, the C-1
co-land) + **R-20-EAV** (EAV / property-FTS via `canonical_attributes`) + **`filterable` pre-KNN** vec0 routing
(**non-destructive** reshape, TC-46 Option 1) + **TC-33** (temporal model harmonised to **INTEGER epoch**;
BYO-LLM extractor boundary keeps ISO-8601 with engine-side **hard-reject** round-trip normalisation, **TC-47**) +
**TC-34** (from Slice 15b) + **Finding-1 (A)** (attribute-filter drops edge hits on both arms; **(D) reserved**,
B/C declined) + **`#[non_exhaustive] SearchFilter`**. codex §9 **terminal-clean**; **gates re-verified by the
Steward** — clippy 0, check 0, (A) pin 1/1, AC-041 3/3, recovery denylist unchanged at five.

**Board of record:** `dev/plans/runs/STATUS-0.8.20.md` (rewritten to this truth this session). **Plan:**
`dev/plans/plan-0.8.20.md` (§9 pointer = Slice 20; §11 = the current HITL queue).

---

## Open HITL decision queue (eight items — see `plan-0.8.20.md` §11 for the full text)

Do not duplicate the bodies here; the plan carries options + rec + justification + gate for each.

1. **AC-079 governed-surface sign-off** — PROPOSED / NOT SIGNED; signing permits publish. Rec: sign once at
   Slice 40. **Gated: Slice 40.**
2. **Adoption arms (build ≠ adopt, F-21)** — Phase-2 surface OPT-IN, erasure fixes ON. **Gated: Slice 40.**
3. **Publish 0.8.20 breaking pair** (`0.8.9 → 0.8.20`) — publish when H7 green + tag→publish rehearsed + Memex
   ready. **Gated: Slice 40.**
4. **`fts`/`vector` sub-object without `searchable`** — reject (fail-fast). **Gated: non-blocking, next
   `configure_projections` slice.**
5. **`embed_batch_cls` TS-binding parity (F-22)** — add the TS binding. **Gated: X1 / Slice 40.**
6. **PLACEMENT — TC-16 dead publish dry-run guard** — fold into Slice 40. **HITL confirms the slot.**
7. **PLACEMENT — TC-45 supersession-terminal CHECK defect** — 0.8.21 own fixup, not publish-blocking. **HITL
   confirms the slot.**
8. **PENDING INPUT (not a decision) — Hermes consult** on the future **(D)** endpoint-node attribute-filter
   widening (Memex already: (A) now, (D) reserved). **Blocks nothing.**

---

## Standing guardrails (load-bearing)

- **Push-scope fathomdb-only** — NEVER push memex/any other repo without a specific per-push directive each time;
  a relayed "authorized" never counts. **Do NOT push from this session** unless the HITL directs it.
- **Two-tier numbering** (`x.y.z` real / publishable with HITL · `x.y.z.p` pico label-only · `13` forbidden).
- **Publish is a separate, explicit, per-`x.y.z` HITL call — NEVER implied by build (build ≠ adopt, F-21).** A
  pushed `v*` tag **auto-fires REAL crates/PyPI/npm publish → dry-run first**; use `scripts/set-version.sh`;
  cargo order embedder → engine.
- **TC-RUBRIC-5 — dedicated checkout, ENFORCED.** Release orchestration + all landing git-writes run in a
  **dedicated linked worktree**, never the primary/shared checkout; `scripts/preflight.sh --landing` hard-fails
  on the primary checkout. **Verify the branch before every commit** (`git rev-parse --abbrev-ref HEAD`).
- **TC-RUBRIC-7 — codex §9 transcripts** persist to `dev/plans/runs/codex/0.8.20/<slice>-<UTC>.log`; invoke
  codex only via `dev/agent-tools/codex-nostdin.sh`.
- **Release DoD:** full-workspace `cargo clippy --workspace --all-targets` **and** `cargo check --workspace
  --all-targets`, both exit 0. Read the **real** exit code (`$?` / `PIPESTATUS`) — a trailing `echo` masks it.
- **eu7 = CLOSED BY DECISION (F-28).** ZERO runs, any backend, any N. CPU↔CUDA is bit-identical (0.8.7), so the
  GPU figure was a cross-backend artifact; the CPU-throughput investigation is FORBIDDEN.
- **Verify EVERYTHING from git before relaying or recording.** Trust git, not narration.
- **Direction / record / release-slot changes are always the HITL's** — propose + recommend, never self-widen.

---

## Ledger tips (use `ledgerwrite` / `ledgerwatch` for all ledger ops)

- **Steward:** `dev/steward/steward-ledger.jsonl` **@ seq 98** (seq-98 = the keystone-landed entry, `3264114a`).
- **Todos / considerations:** `dev/todos-and-considerations-ledger.jsonl` **@ seq 69**.
- **OPP-12 sub-ledger:** `dev/design/record-lifecycle-protocol/OPP-12-sub-ledger.jsonl` **@ seq 10**
  (no new FathomDB→Memex append is owed by the keystone landing; **never append voice=FATHOM without HITL
  agreement**, and **never push memex**).

---

## Memory pointers

`[[0.8.19-complete-opp12-phase1-lifecycle-id]]` · `[[erasure-axis-is-provenance-excise-source-gap]]` ·
`[[opp12-record-lifecycle-protocol]]` · `[[0.8.x-release-numbering-publish-governance-policy]]` ·
`[[release-dod-requires-full-workspace-gate]]` · `[[guardrail-failures-fix-tooling-not-people]]` ·
`[[verify-design-against-code-not-just-architecture]]` · `[[steward-delegate-dont-hand-do]]` ·
`[[background-agent-silent-death-proactive-check]]` · `[[use-ledger-tools-for-all-ledger-ops]]` ·
`[[steward-handoff-filename-format]]` · `[[push-scope-fathomdb-only]]` · `[[release-publish-gotchas]]`.
