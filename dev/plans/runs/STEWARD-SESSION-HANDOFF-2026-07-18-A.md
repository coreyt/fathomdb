# FathomDB — Steward Session Hand-off (2026-07-18-A)

> **Boot:** run **`/steward`** (loads `.claude/agents/steward.md` + `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`),
> do its §3 cold-start reading, then read THIS doc, return a short orientation, and **WAIT for HITL** before
> mutating. You are the **Program Steward**: keep the schedule-of-record true to git, **commission + verify**
> release orchestrators, propose-first to the HITL. **Do not implement code or hand-drive a ladder.**
> *(Supersedes 2026-07-10-A. This session: 0.8.19 COMPLETE closed; **0.8.20 (OPP-12 Phase-2 + erasure) plan
> authored, design ratified, landed**; the repo consolidated onto one `main`.)*

---

## ★ IMMEDIATE NEXT STEP — commission the 0.8.20 Slice-0 X0 design gate

**0.8.20 is PLAN-COMPLETE + design-ratified + landed.** The next action is to **commission a `/goal complete
0.8.20` orchestrator** against `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` + `dev/plans/plan-0.8.20.md`, whose first
job is **Slice 0 — the X0 design gate.** You commission and verify it; you do not run it.

**What Slice 0 must produce** (from `plan-0.8.20.md` §4 Slice 0 + §9):

1. Stand up `dev/plans/runs/STATUS-0.8.20.md`.
2. **Freeze §3 reqs + RED-testable ACs** (R-20-RV/NV/PR/EAV/DR/SUR · R-20-E1…E8 · R-20-H7/X1/EU7/PUB/AC).
3. **Author the erasure Slice-0 design** on top of `dev/design/0.8.20-erasure-and-h-end-state-v4.md` (the design
   of record): the **one row-owned projection registry** + **total projector (node + edge)** + `SourceId`
   newtype + **`erase_source()` SDK verb** + WAL `truncate_wal()`/`ErasureIncomplete` + telemetry **selective
   redaction** + `excise_collection_record` + the **`_legacy:pre-0.8.20` migration (WHERE `logical_id IS NULL`
   only)**. RED tests **assert on RAW TABLE CONTENTS**, not search results (both FTS read paths gate on
   `canonical_nodes`, so a text-search test passes on the broken code).
4. Record the **eu7-basis** and **`embed_batch_cls`-TS-parity** decisions (F-22).
5. **Fold TC-RUBRIC-5** (dedicated-worktree + `scripts/preflight.sh --landing`) **and TC-RUBRIC-7** (persist
   every codex §9 transcript to a durable release-namespaced path) **into the X0 process gate.**
6. **Run codex §9** on the package (via `dev/agent-tools/codex-nostdin.sh`), persist the transcript, take X0 to
   **HITL sign-off**.

**TC-11 is CLOSED — do NOT re-open it.** The `h:` end-state pin (A / §2(ii) overrule) and the REQ-037 carve-out
are HITL-ratified (2026-07-12) and landed; Slice 0 builds on them, it does not re-litigate them.

**Prereqs (plan §7) to confirm before commissioning:** a dedicated worktree cut off a verified `origin/main`
tip (TC-RUBRIC-5 — **never the primary/shared checkout**); baseline captured (eu7 + FTS/vector + X1) so R-20-EU7
has a reference; `0.8.18 #11-full` machinery ✓ (proven, never fired); Memex `0.5.x-successor` co-land readiness.

---

## ⚠ OPERATIONAL — the repo is now consolidated (verify, don't assume)

- **`origin/main` = `cb9e9268`.** Everything is on one line. **Primary checkout `/home/coreyt/projects/fathomdb`
  is re-attached to `main`** (was a detached, contended, 18-behind checkout — the anti-pattern TC-RUBRIC-5
  exists to end). Worktrees now: **primary `[main]`**, `0.5.1-memex-build` (memex sandbox, leave alone), and
  whatever steward/orchestrator worktrees you cut. *(This hand-off was written from a dedicated `steward-0718`
  worktree; prune it when done.)*
- **TC-RUBRIC-5 is ADOPTED but NOT YET ENFORCED.** `scripts/preflight.sh --landing` (HARD-fail on the primary
  checkout; verified mechanism: `git --git-dir` == `--git-common-dir` ⇔ primary) is **authorized, not built** —
  **fold the tooling into Slice 0.** Until then it's discipline: **release/landing git-writes in a dedicated
  linked worktree; verify the branch before every commit.**
- **3 untracked files live in the primary checkout — leave/decide, do not sweep into a commit:**
  `dev/design/opus-claude-failure-modes-2026-07-11.md` + `dev/plans/runs/failure-modes-rubric-hardening-*.log`
  (the **failure-mode/rubric workstream's** unique uncommitted work — its owner reconciles them);
  `dev/plans/runs/0.8.x-renumber-memex-handoff.md` (rescued from the pruned `0.8.11.2` worktree; **likely
  stale** — it cites the FathomDB `0.8.15`/Memex `0.5.3` sync, but `0.8.15` is PARKED per F-18; **HITL to decide
  commit-durable / update / discard**).

---

## Verified state (git @ `cb9e9268`)

- **0.8.19 ✅ COMPLETE** (label-only `578fe20b`) — OPP-12 record-lifecycle Phase-1 (existence axis + SCHEMA
  19→20 · `transition`/`purge` · C-2 typed `SearchHit.id` `IdSpace` swap · X1 26/26 · AC-074 signed).
  `[[0.8.19-complete-opp12-phase1-lifecycle-id]]`.
- **0.8.20 Phase-2 — PLAN AUTHORED + DESIGN RATIFIED + LANDED** (`a197da7c` deliverable + `cb9e9268` Memex
  notification):
  - **`plan-0.8.20.md`** de-staled from the Library-Sweep runbook (re-homed to **0.8.22**, F-19/F-20) → the
    **OPP-12 Phase-2 + erasure-completeness + breaking-pair-publish** plan. Mod-5 ladder **0 → 5 → 10 → 15 → 20
    → 25 → 30 → 40**. Base re-verified at `d526d15c` (code, not memory).
  - **TC-11 pin A RATIFIED (HITL 2026-07-12; master F-25, F-23 guardrail DISCHARGED):** anonymous/doc-seeded
    nodes stay **`h:<content-hash>` PERMANENTLY**; `structural-lifecycle-contract.md §2(ii)` is **OVERRULED**;
    the anonymous-surrogate leg is **CANCELLED, not deferred** — a surrogate serves ONLY registry-admitted
    governed entities, minted **at write time**, and a stored row's id-space is **NEVER re-derived** (guard:
    rows transitioning `logical_id NULL → NOT NULL` == 0). The predicate is **write-time/temporal**, NOT the
    semantic "declares a natural key" (which would backfill stored anonymous-but-typed rows = option C).
  - **REQ-037 lawful-erasure carve-out APPROVED (HITL 2026-07-12):** `erase_source(source_id)` ships as an **SDK
    lifecycle verb** (the provenance/`h:`-axis erasure counterpart to `purge` on the `l:` axis); `excise_source`
    stays CLI-only (recovery seam); `purge_logical_id` struck from REQ-037's forbidden list (shipped code already
    contradicted it — the SDK ships **no CLI**, and `purge` was already an SDK verb); **AC-041 unchanged, stays
    green** (`erase_source` is not a recovery-denylist name).
  - **Erasure root cause (plan §0.1):** `search_index_v2` is a **content-storing** FTS5 table maintained by only
    1 of 5 sites (`excise`/`rebuild`/`tokenizer-reproject` all miss it) → the body **survives verbatim** after
    `excise_source`. Fix the **mechanism** (one registry + total projector), not the missing DELETE.
  - **Design of record:** `dev/design/0.8.20-erasure-and-h-end-state-v4.md`.
  - **Memex notified:** OPP-12 sub-ledger **seq 10** (FATHOM→MEMEX §2(ii)-overrule; **impact on Memex = NONE** —
    `SearchHit.id` byte-unchanged, C-1 unaffected, no re-ratification). `[[opp12-record-lifecycle-protocol]]`.
- **Ledgers** (use `ledgerwrite`/`ledgerwatch`): steward `dev/steward/steward-ledger.jsonl` **@ seq 79**;
  todos `dev/todos-and-considerations-ledger.jsonl` **@ seq 24**; OPP-12 sub-ledger **@ seq 10**.

## The 0.8.20 build (what the orchestrator implements after Slice 0 — plan §4)

- **Slice 5 — erasure completeness (R-20-E1…E8):** registry + total projector + `erase_source` + mandatory
  `SourceId` provenance + WAL + telemetry redaction + op-store + `_legacy:` migration. **INDEPENDENT of Phase-2**
  (fixes shipped defects) → can run parallel to 10/15.
- **Slice 10** ReadView/read-modes + node-validity · **Slice 15** projection registry (C-1 co-land) +
  EAV/property-FTS *(Phase-2 keystone; 20 + 25 depend on it)* · **Slice 20** `dense_readiness` · **Slice 25**
  surrogate minting (registry-admitted **governed entities only**).
- **Slice 30 — RUBRIC-H7 `can-i-deploy` contract-conformance gate (R-20-H7):** a **publish precondition** —
  absent-or-failing **HOLDS the breaking pair** (HITL-directed, TC-RUBRIC-2).
- **Slice 40 — verification + release-readiness**, then **publish-or-hold** per the HITL gate.
- **Serialize the merges** (all touch `engine/src/lib.rs`); **one `maturin develop` at a time** (shared `.venv`).

## Later — the publish gate (F-21)

- **0.8.20 is the FIRST REAL PUBLISH in the line** — manifests **`0.8.9 → 0.8.20`** (first bump since 0.8.9;
  every release since was label-only). A **coordinated breaking pair** with a Memex `0.5.x-successor`.
- **Publish is a separate, explicit, per-`x.y.z` HITL call — NEVER implied by build (F-21).** Prereqs: the RUBRIC-H7
  gate GREEN, `0.8.18 #11-full` machinery (proven + exercised via staging, **never fired** — **rehearse the
  tag→publish path first**), Memex co-land readiness. Use `scripts/set-version.sh`; cargo order **embedder →
  engine**; a pushed `v*` tag **auto-fires REAL crates/PyPI/npm publish → dry-run first** (`[[release-publish-gotchas]]`).

## Open HITL queue / Slice-0 decisions (plan §11)

1. **Commission 0.8.20 Slice-0** (the immediate step) — confirm go.
2. **eu7 basis** (F-22): pin A keeps `SearchHit.id` byte-identical ⇒ **no-op expected**; prove at Slice 40, or authorize a bounded re-baseline.
3. **`embed_batch_cls` TS-binding parity** (F-22): add-TS or ratify Py-first? (folds into X1.)
4. **Adoption arms** (build ≠ adopt, F-21): which Phase-2 items change shipped default behavior — each needs its own adoption call. *(Default expectation: read-modes/registry/readiness opt-in; erasure fixes ship ON.)*
5. **Publish gate** (later) + **Memex `0.5.x-successor` co-land readiness**.
6. Disposition of the 3 untracked files in the primary checkout (above).

## Standing guardrails (load-bearing)

- **Push-scope fathomdb-only**; **two-tier numbering** (`x.y.z` real · `x.y.z.p` pico · `13` forbidden);
  **even=publishable, odd label-only**; **publish always a separate per-`x.y.z` HITL call**; **build ≠ adopt**.
- **TC-RUBRIC-5:** release orchestration + landing git-writes run in a **dedicated linked worktree**, never the
  primary/shared checkout; **verify the branch before every commit** (`git rev-parse --abbrev-ref HEAD`).
- **Footprint invariant:** library query path CPU-only, 1-bit Hamming, deterministic; only LLM seam = BYO-LLM in
  the caller; GPU/frontier = OFFLINE-BUILD/EVAL. **eu7 fidelity gate CPU same-backend.**
- **Release DoD:** full-workspace `cargo clippy` + `cargo check` both exit 0 (`[[release-dod-requires-full-workspace-gate]]`).
- **Direction / record / release-slot changes are always the HITL's** — propose + recommend, never self-widen.

## The workflow that worked (reuse it)

- **X0 design-review-before-code** — reqs+ACs → codex design review to terminal (accept+revise on BLOCK, never
  override) → HITL sign-off, before code. Caught real pre-code defects on 0.8.18/0.8.19/0.8.20.
- **Adversarial audit + independent verification of load-bearing rulings.** The 0.8.20 `h:`-pin/§2(ii) call took
  several rounds precisely because subagent groundings **over-generalized** (a fact true of the extractor path
  asserted of the write surface) and were briefly **presented as settled**. **Lesson: independently verify the
  load-bearing CODE claim before presenting a design conclusion as ratified** — `[[verify-design-against-code-not-just-architecture]]`.
- **codex §9 per slice is a real gate** (caught the GDPR-erasure leak on 0.8.19). Drive to terminal PASS.
- **Isolated dedicated worktree per orchestration; fresh clone for X1/wheel verification.** Verify EVERYTHING
  from git before relaying/recording. Full-workspace clippy+check on the COMBINED main after parallel slices land.

## Memory pointers

`[[0.8.19-complete-opp12-phase1-lifecycle-id]]` · `[[erasure-axis-is-provenance-excise-source-gap]]` ·
`[[opp12-record-lifecycle-protocol]]` · `[[0.8.x-release-numbering-publish-governance-policy]]` ·
`[[release-dod-requires-full-workspace-gate]]` · `[[guardrail-failures-fix-tooling-not-people]]` (TC-RUBRIC-5
adopted) · `[[verify-design-against-code-not-just-architecture]]` · `[[steward-delegate-dont-hand-do]]` ·
`[[use-ledger-tools-for-all-ledger-ops]]` · `[[steward-handoff-filename-format]]` · `[[push-scope-fathomdb-only]]` ·
`[[release-publish-gotchas]]`.
