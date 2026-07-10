# STATUS — 0.8.19 (OPP-12 record-lifecycle Phase-1 — lifecycle + id; odd micro, LABEL-ONLY)

> Live state board (source of truth = git witnesses per orchestration.md §1.5; this is a cache).
> Plan: `dev/plans/plan-0.8.19.md` · Slice-0 design package: `dev/design/0.8.19-slice-0-opp12-phase1-design.md` ·
> OPP-12 authority: `dev/design/record-lifecycle-protocol/` · Deps/decision record:
> `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` §4 (F-19/F-20/F-21/F-22).
>
> **NO standing landing mandate** (0.8.18's is SPENT). Every slice drives to terminal codex §9 PASS/BLOCK and
> **reports for landing — landing escalates to Steward/HITL.** ALWAYS HITL-gated regardless of any future
> mandate: **Slice 5 (SCHEMA 19→20 migration)**, **Slice 15 (breaking C-2 id swap)**, any adoption-default,
> any codex §9 override, any BLOCK. **No publish** (Phase-1 is label-only; manifests stay `0.8.9`).
>
> **X0 elevated process gate IN FORCE** (plan §5): every code-shipping unit = (A) reqs+RED-testable ACs →
> (B) independent codex design review → HITL sign-off → RED/GREEN TDD → codex §9. codex via
> `dev/agent-tools/codex-nostdin.sh` only (bare `codex exec` deadlocks on stdin).

## Current state — **Slice 5 §9 PASS (canary validated, awaiting HITL 19→20 landing) · Slice 15 in flight · Slice 0 CLOSED**

Base verified from `origin/main` @ `9db9d98b`: `SCHEMA_VERSION = 19` (`fathomdb-schema/src/lib.rs:6`),
`SearchHit.id: u64` (`fathomdb-engine/src/lib.rs:1173`), `stable_id`/`derive_stable_id` (`:1196`/`:10491`),
anonymous `logical_id: None` (`:7812`/`:11610`), `PreparedWrite::Node` (`:1859`). Prereqs all satisfied
(plan §7): EXP-S 0.8.14 ✓ · Cause-A `stable_id` ✓ · C-1 RATIFIED ✓ · F9 0.8.16 ✓ · #11-full 0.8.18 ✓.

## Slice ladder (mod-5): 0 → 5 → {10 ∥ 15} → 40

| Slice | Title | State | X0 | X1 | X2 | X3 | §9 |
|------:|-------|-------|----|----|----|----|----|
| **0** | Setup + ADR (existence axis · transition/purge · C-2 IdSpace · 19→20 migration · 6 gap rulings · Phase boundary) | **✅ CLOSED / HITL SIGNED (2026-07-09)** | A ✓ / B ✓ codex / HITL ✓ | — | — | ✓ | — |
| **5** | KEYSTONE — existence axis (`state`/`reason` + `active`-only filter) + **SCHEMA 19→20 = existence columns ONLY (no surrogate)** *(HITL-gated bump)* | **§9 PASS (43aa29d6) — AWAITING HITL 19→20 LANDING** | discharged | py/ts written (main-exec pending) | pending | ✓ | **PASS (r2, fix-1)** |
| **10** | `transition`/`purge` verbs + `IllegalTransitionError` + `NotLifecycleAddressableError` + `secure_delete` PRAGMA | BLOCKED (dep: 5 — needs `state` column; waits for Slice-5 landing) | discharged | — | — | — | — |
| **15** | KEYSTONE — C-2 typed `SearchHit.id` swap (TC-8), total via `l:`/`h:`/`p:` **without surrogate** *(breaking, label-only)* | **IN FLIGHT** (worktree `0.8.19-slice-15` off `9ea3ecc2`, preflight pass) | discharged | pending | pending | pending | pending |
| **40** | Verification + Phase-1 release-readiness (label-only close) | BLOCKED (dep: 5,10,15) | discharged | — | — | — | — |

**Keystones / gates (re-derived per F-23 / 1a):** Slice 5 = schema keystone + HITL landing gate (19→20,
existence columns only). Slice 15 = id-surface keystone (breaking, built label-only; publish = 0.8.20) — the
C-2 swap is total via `l:`/`h:`/`p:` **without the surrogate**, so **Slice 15 no longer depends on Slice 5**
and is **parallelizable off Slice 0**. Slice 10 depends on Slice 5 (its verbs key on the `state` column).
**Fan graph: {Slice 5 ∥ Slice 15} off Slice 0; Slice 10 after Slice 5; converge at Slice 40.**
X0 design-review gate DISCHARGED for all of Phase-1 (Slice-0 SIGNED) → slices go straight to RED/GREEN TDD →
codex §9, no fresh per-slice design review. Slice-5 (19→20) + Slice-15 (breaking id-swap) landings ALWAYS
HITL-gated. Canary discipline: launch Slice 5 as the first real spawn; fan Slice 15 once the canary's cycle
validates the worktree+preflight+implementer+codex machinery.

## The six Slice-0 gap rulings (design package §8 — **ALL HITL-APPROVED 2026-07-09; master F-23**)

1. **LOAD-BEARING — surrogate ↔ `h:`:** RECOMMENDED doc-seeded hits **stay `h:`**; surrogate *mechanism* lands
   but doc-seeded corpus is **not backfilled** (addressability → Phase-2). Narrows R-ID-3 → HITL sign-off.
2. **Migration vs real-gold:** under (1), **no remap needed** — `id` VALUE = today's `stable_id`, eu7 keying no-op.
3. **Purge edge-cascade:** in Phase-1 scope; **cascade-remove** (not stub — no Phase-1 reader for stubs).
4. **`secure_delete`:** standing connection PRAGMA + documented pre-20 freelist residual (no forced VACUUM).
5. **Read without `ReadView`:** acceptable — complete surface; undelete via `transition` by known `logical_id`.
6. **`PreparedWrite::Node` state/reason:** in scope (required to create `pending`); X1 + AC-074 delta.

## Log

- **2026-07-09 (orchestrator):** Slice-0 design package drafted (`dev/design/0.8.19-slice-0-opp12-phase1-design.md`),
  board stood up. Base + prereqs verified from git.
- **2026-07-09 (orchestrator) — X0 (B) codex independent design review COMPLETE (via `codex-nostdin.sh`):**
  **round 1 = OVERALL BLOCK** (gap-1 surrogate mechanism not RED-testable; R-ID-3/R-MIG-1 narrowed without an
  explicit amendment; MISSING: reason semantics, secure_delete open-site, state-index/read-path enumeration,
  purge target-tables, `p:` format). Accepted + revised → **rev-2**: gap-1 sharpened to ruling **1a** (defer the
  anonymous-surrogate mechanism IN FULL to Phase-2; doc-seeded stays `h:`; explicit RED tests +
  `NotLifecycleAddressableError`; R-ID-3/R-MIG-1 amendments flagged as HITL items) + all MISSING items filled.
  **round 2 = OVERALL CONCERN, NO residual BLOCK** (one precise point: separate row-owned purge targets from
  global/kind-level metadata registries). Applied verbatim → **rev-3** (never overridden). Design review is
  **terminal: no residual BLOCK; sole CONCERN resolved per codex's exact wording.**
- **STOP → reported to Steward for verification → HITL sign-off.**
- **2026-07-09 — HITL SIGNED Slice-0 (gap-1 1a APPROVED + 5 gaps ratified); Steward-commissioned adversarial
  audit confirmed 1a correct + no 0.8.20 unwind risk, but found the ADR under-scoped the "lands-together
  GATING" co-requisite split.** Steward reconciled master+plan+ledgers (`5190eb08`, F-23, R-ID-3/R-MIG-1
  narrowed, Slice 15 parallelizable off Slice 0, TC-11). ADR widened (co-requisite split explicit + F-23
  ref) + flipped SIGNED; board reconciled. **X0 gate discharged for all Phase-1.** Slice-0 closure committed
  `9ea3ecc2` + pushed.
- **2026-07-09 — Slice 5 (canary) → §9 PASS.** Worktree `0.8.19-slice-5` off `9ea3ecc2`, preflight pass.
  Implementer impl `79abf1a0` (existence axis + `state`/`reason` + `LifecycleState`/`InitialState` + typed
  create-rejection of deleted/purged + `active`-only filter co-located at the retrieval sites + one 19→20
  migration existence-columns-only, fresh==upgrade + X1 pyo3/napi). **codex §9 r1 = BLOCK** (P1: vector node
  hydration filtered `state='active'` but omitted `superseded_at IS NULL` → superseded-version leak via vector
  search; a latent pre-slice hole the slice's exclusion mandate must close). Never overridden → **fix-1**
  (`43aa29d6`, one guard co-located + RED→GREEN vector-supersession regression test; clippy/check 0). **codex §9
  r2 = PASS** (no findings, scope clean, eu7 no-op preserved). **Canary machinery VALIDATED.** Slice 5 head
  `43aa29d6` is terminal §9 PASS — **AWAITING HITL sign-off for the SCHEMA 19→20 landing** (R-MIG-1; the
  wheel-build + py/ts X1 execution runs on the MAIN tree at landing).
- **2026-07-09 — Slice 15 FANNED** (canary validated). Worktree `0.8.19-slice-15` off `9ea3ecc2`, preflight
  pass; implementer building the C-2 typed `IdSpace` `SearchHit.id` swap (no surrogate; `SCHEMA_VERSION`
  unchanged; id VALUES == prior `stable_id` for eu7 no-op). Slice 10 stays blocked on Slice-5 landing.
