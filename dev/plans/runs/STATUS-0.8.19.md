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

## Current state — **✅ 0.8.19 COMPLETE (2026-07-10, label-only). All slices LANDED · Slice-40 verification GREEN · X1 py/ts parity GREEN (26/26, fresh-clone re-run @ `d4b5cd90`) · AC-074 governed-surface delta HITL-SIGNED. Manifests stay `0.8.9`; no tag/publish (OPP-12 publishes at 0.8.20). Master §4 → COMPLETE (F-24).**

Base verified from `origin/main` @ `9db9d98b`: `SCHEMA_VERSION = 19` (`fathomdb-schema/src/lib.rs:6`),
`SearchHit.id: u64` (`fathomdb-engine/src/lib.rs:1173`), `stable_id`/`derive_stable_id` (`:1196`/`:10491`),
anonymous `logical_id: None` (`:7812`/`:11610`), `PreparedWrite::Node` (`:1859`). Prereqs all satisfied
(plan §7): EXP-S 0.8.14 ✓ · Cause-A `stable_id` ✓ · C-1 RATIFIED ✓ · F9 0.8.16 ✓ · #11-full 0.8.18 ✓.

## Slice ladder (mod-5): 0 → 5 → {10 ∥ 15} → 40

| Slice | Title | State | X0 | X1 | X2 | X3 | §9 |
|------:|-------|-------|----|----|----|----|----|
| **0** | Setup + ADR (existence axis · transition/purge · C-2 IdSpace · 19→20 migration · 6 gap rulings · Phase boundary) | **✅ CLOSED / HITL SIGNED (2026-07-09)** | A ✓ / B ✓ codex / HITL ✓ | — | — | ✓ | — |
| **5 ✅** | KEYSTONE — existence axis + **SCHEMA 19→20 (existence columns only)** | **LANDED `36074f91`+`a6970496`** (HITL-approved) | discharged | Rust ✓ / py+ts ✓ | ✓ | ✓ | **PASS** |
| **10 ✅** | `transition`/`purge` verbs + `IllegalTransitionError` + `NotLifecycleAddressableError` + `secure_delete` PRAGMA | **LANDED `65061fb7`+`9cb9274b`+`dd5eaf82`** (Steward-authorized) | discharged | Rust ✓ / py+ts ✓ | ✓ | ✓ | **PASS** |
| **15 ✅** | KEYSTONE — C-2 typed `SearchHit.id` swap (TC-8), total via `l:`/`h:`/`p:` **without surrogate** *(breaking, label-only)* | **LANDED `6616db93`+`a704c317`+`51c2c785`** (+compose `c8e2a5b3`; HITL-approved) | discharged | Rust ✓ / py+ts ✓ | ✓ | ✓ | **PASS** |
| **40 ✅** | Verification + Phase-1 release-readiness (label-only close) | **✅ CLOSED (2026-07-10) — AC gate GREEN · X1 26/26 GREEN · AC-074 HITL-SIGNED** | discharged | Rust ✓ / py+ts ✓ | ✓ | ✓ | n/a |

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
- **2026-07-09 — Slice 15 → §9 PASS.** Worktree `0.8.19-slice-15` off `9ea3ecc2`, preflight pass. Impl
  `63d52bc6` (`SearchHit.id: u64 → typed IdSpace {space,value}`, `IdSpaceKind {Logical/Content/Passage}`,
  non-null + id-space-total; `write_cursor`+`stable_id` subsumed into `id`, `write_cursor` kept
  engine-internal; synthetic passages mint `p:<ordinal>`; NO surrogate; `SCHEMA_VERSION` unchanged 19). **codex
  §9 r1 = BLOCK** (P1: signed design's "id value == prior stable_id" read literally vs bare `IdSpace.value`;
  P2: stale `PerHitExplain.id` docs). **Adjudicated (verified from code): eu7 no-op genuinely preserved** —
  `IdSpace::to_prefixed()`/`Display` == prior `stable_id` byte-for-byte; telemetry `result_stable_ids` emits
  `to_prefixed()` (`lib.rs:4203`); bare `.value` is the correct C-2 typed form ("not a magic-prefixed
  string"). Never overridden → **fix-1** (`d53bd279`, lock the `to_prefixed` no-op proof in Rust+py+ts +
  telemetry-bytes-unchanged assertion + correct `PerHitExplain.id` docs; engine `lib.rs` untouched). **codex §9
  r2 = CONCERN, no residual BLOCK** (P1 behaviorally resolved — codex agrees bare value is correct C-2; only 3
  `SearchHit.id` doc comments still misleading) → **fix-2** (`b0bb919c`, doc-comment-only, the 3 comments
  corrected per codex's exact instruction; clippy/check 0). §9 **terminal PASS** (sole CONCERN closed by
  construction). **AWAITING HITL sign-off for the breaking `SearchHit.id` surface landing** (built label-only;
  publish = 0.8.20).
- **Design-wording note (for Steward/HITL awareness):** ADR §4/§9 "id VALUE == prior stable_id" is best read as
  the id's **serialized/`to_prefixed` form** (== prior `stable_id`), NOT the typed bare `.value` field. The
  implementation + codex both land on: `value` bare (C-2 typed, signed) + `to_prefixed()` no-op. No design
  change; a wording clarification the Steward may fold into the ADR if desired.
- **Both keystones terminal §9 PASS in their worktrees; two HITL-gated landings pending** (Slice-5 19→20 R-MIG-1;
  Slice-15 breaking id-swap). Slice 10 (transition/purge) stays blocked until Slice 5 lands (needs `state`).

- **2026-07-09 — Slices 5 + 15 LANDED to `main`** (HITL approved BOTH landings: Slice-5 SCHEMA 19→20 R-MIG-1;
  Slice-15 breaking `SearchHit.id`, label-only). **Landed in an isolated `0.8.19-landing` worktree** after the
  shared main checkout was contended by a concurrent rubric-eval session (recovered: main was safe; my
  accidental cherry-pick fully reverted). Cherry-picked Slice 5 (`36074f91`+`a6970496`) then Slice 15
  (`6616db93`+`a704c317`+`51c2c785`); the one cherry-pick conflict was a mechanical **import-union** in
  `fathomdb-py`/`fathomdb-napi` `use fathomdb_engine::{…}` (both slices added an import — resolved to the
  union). **Combined-workspace DoD green:** `cargo check`/`clippy`/`fmt --check` `--workspace --all-targets`
  = 0; **19→20 migration 3/3** (fresh==upgrade); tc8_idspace_swap + opp12_existence_axis + telemetry_capture
  green → **the id-swap + existence axis compose.** One cross-slice **compose-fix** (`c8e2a5b3`): Slice-15's
  `tc8_idspace_swap.rs` (built off Slice 0, pre-Slice-5) lacked `PreparedWrite::Node.state`/`reason` — added
  the defaults (nodes active; no test-semantics change). **Pushed `origin/main` `44fd6dee → c8e2a5b3`** (clean
  fast-forward). **X1 py/ts *execution* DEFERRED** to a Steward-arranged quiesced-main single-writer window:
  the Rust binding side compiles clean (`cargo check --workspace` includes `fathomdb-py`+`fathomdb-napi`), but
  the isolated wheel build hit the **native-import module-name quirk** (`PyInit_fathomdb_py` symbol; SDK imports
  `fathomdb`) + napi tooling not installed — the eval-env trap the Steward flagged; not forced.
- **Next: Slice 10 (transition/purge), UNBLOCKED** (needs Slice-5's `state` column, now on main) → then Slice 40.
  Label-only; no publish; manifests stay `0.8.9`.

- **2026-07-09 — Slice 10 (transition/purge) LANDED to `main`** (Steward-authorized — clean §9-PASS additive
  slice: new verbs, no schema bump, no breaking surface, no adoption-default). Cycle: impl `6be0faf4` → codex
  §9 r1 BLOCK (P1 `secure_delete=ON` writer-only → reader/runtime leak; P2 `IllegalTransitionError.legal`
  included `purged`) → fix-1 `db9de5ed` (secure_delete every open + per-worker proof seam; verb-specific legal
  targets) → §9 r2 CONCERN no-BLOCK (stale comment) → fix-2 `1c86589c` → terminal PASS. Cherry-picked clean onto
  `main @ f2a10274` (`65061fb7`+`9cb9274b`+`dd5eaf82`); **integrated DoD green** (check/clippy `--workspace
  --all-targets` = 0; opp12_lifecycle_verbs 8/8 + existence 6/6 + tc8-idspace 4/4 + step20 migration 3/3 → all
  three slices compose). Pushed. Slice-10 source worktree cleaned up.
- **Next: Slice 40 (Phase-1 verification + label-only close)** — cargo AC gate + X2 mkdocs --strict + X3
  DOC-INDEX + eu7 no-op basis + AC-074 delta draft, **STOP at X1 py/ts execution** (needs a quiesced-main
  single-writer window; eval-env native-import trap). No publish; manifests stay `0.8.9`.

## Slice 40 — Phase-1 verification (pre-X1) — GREEN except X1 execution (2026-07-09)

Run in the isolated `0.8.19-landing` worktree on integrated `main @ a7921202` (Slices 5+10+15):

- **Cargo AC gate — GREEN.** R-EX (existence axis): `opp12_existence_axis` 6/6. R-TR + R-PG (transition/purge
  + typed errors + erasure + secure_delete): `opp12_lifecycle_verbs` 8/8. R-ID (typed `IdSpace` totality):
  `tc8_idspace_swap` 4/4. R-MIG (19→20 fresh==upgrade): `step20_migration` 3/3. **Full `fathomdb-engine` suite
  all-pass (0 failed, 1 known-ignored long-gated); `fathomdb-schema` suite all-pass.** No regression from the
  three slices.
- **Workspace DoD:** `cargo check` + `clippy --workspace --all-targets` = 0; `cargo fmt --check` = 0.
- **X2 — `mkdocs build --strict` = exit 0.** **X3 — DOC-INDEX:** the 0.8.19 ADR
  (`dev/design/0.8.19-slice-0-opp12-phase1-design.md`) + plan + this board present; docs coherent (strict build
  clean).
- **eu7 no-op basis (recorded, code-grounded — same posture as 0.8.16 D6 / 0.8.18):** the default ranking/vector
  path is byte-unchanged. (a) The C-2 id-swap changes only the `SearchHit.id` *carrier type*; its
  `to_prefixed()`/telemetry `result_stable_ids` bytes are byte-identical to the prior `stable_id` → real-gold
  keying is a true no-op (no gold-remap). (b) The `state='active'` existence filter is a no-op on the shipped
  corpus (no non-active row exists; the migration defaults all rows to `active`). (c) `transition`/`purge` add
  verbs that do not touch the default query path. (d) The embedder / 1-bit vector path is untouched. → eu7 ≥
  0.90 (one-sided CI) expected to hold; AC-012/013/020 latency unaffected.
- **AC-074 governed-surface delta (DRAFT for HITL formal sign-off):** verbs **+`transition`, +`purge`**
  (already in `src/conformance/governed-surface-allowlist.json` — required to keep the conformance subset green
  once the live `Engine` methods shipped in Slice 10). Types/fields: **+`LifecycleState`**,
  **+`IllegalTransitionError`** (`from_state`/`to_state`/`legal`), **+`NotLifecycleAddressableError`**
  (`id_space`), **+`IdSpace`/`IdSpaceKind`**; **`PreparedWrite::Node` +`state`/+`reason`**; **`SearchHit.id`
  retyped** `u64 → IdSpace` (`write_cursor` + `stable_id` subsumed into `id`). `recovery_denylist` UNCHANGED
  (five names; `undelete` used, never `restore`). `ReadView`/`ProjectionSpec`/validity/`dense_readiness` are
  **Phase-2 (0.8.20)** — NOT in this delta.
- **X1 py/ts EXECUTION — PENDING a quiesced-main single-writer window.** The Rust binding side is validated
  (`cargo check --workspace` compiles `fathomdb-py` + `fathomdb-napi`), but the py wheel / napi `.node` test
  EXECUTION hits the eval-env native-import trap in an isolated build (`PyInit_fathomdb_py` module-name; napi
  tooling absent). Steward is arranging the window (pause rubric-eval agent, or fresh-clone fallback). The X1
  parity tests to run: `test_opp12_existence_axis.py` + `opp12-existence-axis.test.ts` (Slice 5),
  `test_idspace_parity.py` + `idspace-parity.test.ts` (Slice 15), `test_opp12_lifecycle_verbs.py` +
  `opp12-lifecycle-verbs.test.ts` (Slice 10).
- **Label-only close pending X1 + AC-074 HITL sign-off. Manifests stay `0.8.9`; no `v*` tag / publish.**

## Slice 40 CLOSED — 0.8.19 COMPLETE (2026-07-10, label-only)

- **X1 py/ts parity — GREEN (resolved).** HITL authorized a fresh X1 re-run; the Steward commissioned it in the
  isolated clone `/home/coreyt/projects/fathomdb-x1-verify` (own `.venv-x1` + `.node`, `origin/main @ d4b5cd90`,
  the live checkout never touched). **maturin-develop** rebuilt the py wheel + **`npm run build:native:debug`**
  rebuilt the napi `.node` from current source; the stale-`.so` / `PyInit_fathomdb_py` shadowing trap was
  resolved (installed module verified to carry `transition`/`purge`/`IdSpace`). **All 6 parity files pass —
  26/26, 0 failures** (py `test_opp12_existence_axis` 5, `test_idspace_parity` 3, `test_opp12_lifecycle_verbs` 5;
  ts `opp12-existence-axis` 5, `idspace-parity` 3, `opp12-lifecycle-verbs` 5; Py≡TS shape 5/3/5). Durable log:
  `fathomdb-x1-verify/x1-rerun-2026-07-10.log`. Steward-verified the artifact + exit codes from primary evidence.
- **AC-074 governed-surface delta — HITL-SIGNED 2026-07-10.** Mechanism already landed in Slice 10
  (`src/conformance/governed-surface-allowlist.json` carries `transition`+`purge`); the `_comment` audit trail
  flipped `proposed → HITL-SIGNED` at this close (per the JSON-governs precedent — AC-074 prose is a frozen
  illustrative subset; not mirrored). Delta = +`transition`/+`purge` verbs · non-command types
  +`LifecycleState`/+`IllegalTransitionError`/+`NotLifecycleAddressableError`/+`IdSpace`/`IdSpaceKind` ·
  `PreparedWrite::Node` +`state`/+`reason` · `SearchHit.id` `u64→IdSpace` retype. **`recovery_denylist`
  UNCHANGED (five names).** Phase-2 read-view/validity/projection types excluded.
- **Label-only close (HITL Option-A, mirror 0.8.18):** manifests stay `0.8.9`; **no `v*` tag / publish** —
  OPP-12 publishes at 0.8.20's coordinated breaking-pair (#11-full @0.8.18 is that prereq). Reconciled: master §4
  0.8.19 row → **✅ COMPLETE** + new **F-24**; plan-0.8.19 `status: COMPLETE`; steward-ledger **seq 76**; memory
  `0.8.19-complete-opp12-phase1-lifecycle-id` + MEMORY.md index.
- **Carry-forward:** TC-11 (0.8.20 doc-seeded `h:` end-state pin + de-stale `plan-0.8.20.md`, do FIRST); the F-22
  open-TC schedule (embed_batch_cls-TS-parity + eu7-basis → 0.8.20; TC-5 → 0.8.23; TC-9/TC-10 per schedule).
- **Worktree/clone cleanup** after this commit lands: `fathomdb-worktrees/0.8.19-landing` + `fathomdb-x1-verify`.
