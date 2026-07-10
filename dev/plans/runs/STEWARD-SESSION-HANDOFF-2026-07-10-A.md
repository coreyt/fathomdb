# FathomDB — Steward Session Hand-off (2026-07-10-A)

> **Boot:** run **`/steward`** (loads `.claude/agents/steward.md` + `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`),
> do its §3 cold-start reading, then read THIS doc, return a short orientation, and **WAIT for HITL** before
> mutating. You are the **Program Steward**: keep the schedule-of-record true to git, commission + verify
> release orchestrators, propose-first to the HITL. **Do not implement code or hand-drive a ladder.**
> *(Supersedes 2026-07-08-B. This session: 0.8.18 COMPLETE + 0.8.19 OPP-12 Phase-1 driven to RELEASE-READY.)*

---

## ★ IMMEDIATE NEXT STEP — finish the 0.8.19 close (one HITL gate away)

**0.8.19 (OPP-12 record-lifecycle Phase-1) is RELEASE-READY.** All slices landed + independently verified.
**The ONLY thing outstanding is a single HITL decision: the AC-074 governed-surface delta sign-off.** On
that "approve":

1. The **live 0.8.19 orchestrator** (`afa7ea102da8d398f`, resumable via `SendMessage`) does the **label-only
   close** (odd micro — manifests stay `0.8.9`; no tag/publish; OPP-12 publishes at 0.8.20).
2. **You (Steward) then reconcile 0.8.19 COMPLETE**: master §4 0.8.19 row → COMPLETE, plan-0.8.19 banner,
   ledger, a new memory file + MEMORY.md index (mirror the 0.8.18 COMPLETE pattern).
3. **Clean up** the isolated `0.8.19-landing` worktree + the `fathomdb-x1-verify` clone.

**AC-074 delta (drafted, awaiting HITL):** verbs +`transition`/+`purge`; types +`LifecycleState`/
+`IllegalTransitionError`/+`NotLifecycleAddressableError`/+`IdSpace`/`IdSpaceKind`; `PreparedWrite::Node`
+`state`/+`reason`; `SearchHit.id` retype (`write_cursor`+`stable_id` subsumed). **`recovery_denylist`
unchanged (five names); Phase-2 types excluded.** Steward recommendation = approve.

---

## ⚠ CRITICAL OPERATIONAL — the main checkout is SHARED / CONTENDED

**A concurrent `claude` session (the rubric-eval experiment — `dev/design/agent-harness-evaluation-rubric*.md`
+ `dev/experiments/`, untracked) is actively using the SAME working checkout `/home/coreyt/projects/fathomdb`**
and holds it on branch **`rubric-eval-v3-terminal`** (HITL-known parallel work). Mid-landing it switched the
branch out from under the orchestrator once → a cherry-pick hit the wrong branch (recovered clean; main never
corrupted). **Discipline in force:**

- **Do NOT run release git-writes in `/home/coreyt/projects/fathomdb`** — it may be on `rubric-eval-v3-terminal`,
  not `main`. Verify `git rev-parse --abbrev-ref HEAD` obsessively; **assume it is NOT main.**
- **All 0.8.19 landing/close work runs in the isolated worktree `fathomdb-worktrees/0.8.19-landing`** (on `main`,
  own HEAD/index → immune to the branch-thrash). Steward doc/ledger commits this session were done there too.
- `fathomdb-x1-verify` = a throwaway fresh clone used to run the X1 py/ts parity (GREEN) without touching the
  contended checkout or its `.venv`. Safe to `rm -rf` after the close.
- **GUARDRAIL FOLLOW-UP owed to HITL (fix-tooling-not-people):** two `claude` sessions on one working checkout is
  a structural hazard. **Propose a durable fix** (release orchestration always runs in its own dedicated
  checkout/clone) — the HITL asked for this proposal after 0.8.19 lands. See ledger seq 74.

---

## Verified state (git, this session's close)

- **`origin/main = eb2b3d61`** (0.8.19 Slice-40 pre-X1). Verify via the `0.8.19-landing` worktree, not the
  contended checkout.
- **0.8.18 ✅ COMPLETE** (label-only `bce032a0`): #5 vector-equivalence self-check (SCHEMA 18→19; two-stage
  P1 flip=0 / P2 L2 ε=1e-5; degraded-open + query-time typed error; DEFECT #4 open-path baseline HITL-approved)
  + #11-full publish machinery (exercised via staging; the 0.8.20 breaking-pair-publish prerequisite). New
  process: **X0 design-review-before-code gate** (4 codex rounds). `[[0.8.18-complete-vec-equiv-publish]]`.
- **0.8.19 OPP-12 Phase-1 — RELEASE-READY** (label-only; pending AC-074 + close):
  - **Slice 0** SIGNED (`9ea3ecc2`) — design review terminal (codex, no residual BLOCK) + **independent
    adversarial audit**. **gap-1 ruling 1a** + 5 unbundled gaps approved (master **F-23**).
  - **Slice 5** LANDED — existence axis (`state`/`reason`, `active`-only default) + **SCHEMA 19→20**.
  - **Slice 15** LANDED — C-2 typed `SearchHit.id` → `IdSpace` (`to_prefixed()` reproduces prior `stable_id`
    byte-for-byte ⇒ **no gold-remap**; verified). Built label-only.
  - **Slice 10** LANDED — `transition`/`purge` verbs; codex §9 caught a real **GDPR-erasure leak**
    (`secure_delete=ON` was writer-only) → fixed to every-open + a per-worker proof seam.
  - **Slice 40** verification GREEN (cargo AC gate 6/6·8/8·4/4·3/3; workspace clippy/check 0; mkdocs `--strict`
    0; **X1 py/ts parity GREEN** in the fresh clone; eu7 no-op basis code-grounded).
- **Ledgers** (use `ledgerwrite`/`ledgerwatch`; commit them from the `0.8.19-landing` worktree): steward
  `dev/steward/steward-ledger.jsonl` **@ seq 74**; todos `dev/todos-and-considerations-ledger.jsonl` **@ seq 21**
  (TC-11 = 0.8.20 doc-seeded `h:` end-state guardrail).

## F-23 — the load-bearing 0.8.19 design ruling (memorize)

**gap-1 ruling 1a (HITL-approved, adversarially audited):** anonymous/doc-seeded `SearchHit`s **stay
`h:<content-hash>`**; anonymous-surrogate `logical_id` minting is **DEFERRED to Phase-2 (0.8.20)**. So Phase-1's
`SearchHit.id` values are byte-unchanged (no gold-remap), the 19→20 migration carries **existence columns only**,
and Slice 15 was parallelizable off Slice 0. The audit confirmed 1a is correct + creates **no 0.8.20 unwind
risk**, and caught that the Steward had over-bundled 4 independent gaps + under-scoped the reconciliation (now
fixed across master §4 rows + F-23 + plan-0.8.19). **The 5 unbundled gaps:** purge=cascade-remove ·
`secure_delete`+documented-residual · read-without-`ReadView`=complete · `PreparedWrite` state/reason ·
AC-074 delta. `[[opp12-record-lifecycle-protocol]]`.

## Next release — 0.8.20 (OPP-12 Phase-2 + breaking-pair publish)

- **Scope (master §4 0.8.20 row):** `ReadView`/read-modes · node-validity (`valid_from`/`valid_until`) ·
  projection registry (**C-1 co-land**) + EAV/property-FTS · `dense_readiness` · **anonymous-node surrogate
  `logical_id` minting (deferred from Phase-1, F-23)** · X1 · **coordinated breaking-pair publish** (Memex
  `0.5.x-successor` pairs; #11-full @0.8.18 is the prereq).
- **F-23 GUARDRAIL / TC-11 (do FIRST):** `plan-0.8.20.md` is **STALE** (holds pre-F-19 dep-migrations content,
  re-homed to 0.8.22). Before commissioning 0.8.20: **author the Phase-2 ladder** AND **pin the doc-seeded `h:`
  end-state** — terminal-forever / forward-mint-only / backfilled? (Leading hypothesis: **terminal** — doc-chunks
  aren't `EntityTypeSpec` entities, so the C-1 surrogate path may not apply; confirm at Phase-2 Slice-0.) The real
  risk is a permanent `l:`(new)/`h:`(old) split for re-ingested content — pin it before building.
- **X0 gate carries** (requirements+ACs → codex design review → HITL sign-off → TDD → §9); **build-authorized
  (F-21)** but **build ≠ adopt**; publish is a separate per-`x.y.z` HITL gate. **codex via `dev/agent-tools/codex-nostdin.sh`.**
- Build 0.8.20 **in-sequence** (0.8.18 done; no concurrent-migration hazard — 0.8.19's 19→20 is landed).

## Open-TC schedule (master F-22, HITL-ratified 2026-07-09)

TC-8→0.8.19 (done, it was Phase-1) · embed_batch_cls-TS-parity + eu7-basis→0.8.20 · **TC-5** eu7 grown-corpus
floor re-baseline→0.8.23 (scale-bound) · **TC-10** #5 open-latency (measure→identity-gated probe caching)→≥0.8.21
· bm25f perf gate→0.8.21 · **TC-9** ort 2.0-stable + `ort::init()` GPU-EP fix→0.8.22/pico · typescript-6→Library
Sweep pico · GPU-eval tooling enforcement→opportunistic w/0.8.23-24. **TC-11** doc-seeded `h:` end-state→0.8.20.

## The workflow that worked (reuse it)

- **Design-review-before-code (X0)** — commission the orchestrator to produce a requirements+ACs+design package,
  drive **codex design review to terminal** (via `codex-nostdin.sh`; withhold leaning; accept+revise on BLOCK,
  never override), Steward verifies from git → HITL sign-off. It caught real pre-code defects on 0.8.18 + 0.8.19.
- **Adversarial audit of load-bearing rulings** — a Steward-commissioned independent audit of gap-1 confirmed the
  ruling AND caught over-bundling + an under-scoped reconciliation. Worth doing for any keystone design call.
- **codex §9 per slice is a real gate** — caught a GDPR-erasure leak, a fail-open→fail-safe defect, a wrong-pooling
  calibration bug, npm-topology holes. Trust it; drive to terminal PASS in-foreground.
- **Isolated-worktree landing** when the main checkout is contended (own HEAD/index → immune). **Fresh-clone**
  for X1/wheel verification (own `.venv` → sidesteps the native-import trap AND contention; non-disruptive).
- **Verify EVERYTHING from git** before relaying/recording — landed shas, DoD exit codes, source anchors.
- **Full-workspace clippy+check on the COMBINED main after parallel slices land** (per-slice green masks cross-slice breaks).

## Standing guardrails (load-bearing)

- **Push-scope fathomdb-only**; **two-tier numbering** (`x.y.z` real · `x.y.z.p` pico · `13` forbidden);
  **even=publishable, odd label-only (may publish by HITL exception)**; **publish is always a separate per-`x.y.z`
  HITL call**; **build ≠ adopt**.
- **No standing landing mandate is in force** (0.8.18's spent; 0.8.19's never granted). Report slices for landing;
  **schema-migration / breaking-surface / codex-override / BLOCK / publish / adoption-default = always HITL.**
- **Footprint invariant:** library query path CPU-only, 1-bit Hamming, deterministic; only LLM seam = BYO-LLM in
  the caller; GPU/frontier = OFFLINE-BUILD/EVAL. **eu7 fidelity gate CPU same-backend.**
- **Release DoD:** full-workspace `cargo clippy` + `cargo check` both exit 0 (`[[release-dod-requires-full-workspace-gate]]`).
- **Verify the branch before EVERY commit** — the shared checkout is the live example of why.
- **Direction / record / release-slot changes are always the HITL's** — propose + recommend, never self-widen.

## Memory pointers

`[[0.8.18-complete-vec-equiv-publish]]` · `[[0.8.16-complete-f9-onnx]]` · `[[opp12-record-lifecycle-protocol]]` ·
`[[0.8.x-release-numbering-publish-governance-policy]]` · `[[release-dod-requires-full-workspace-gate]]` ·
`[[steward-delegate-dont-hand-do]]` · `[[background-agent-silent-death-proactive-check]]` ·
`[[guardrail-failures-fix-tooling-not-people]]` · `[[agent-worktree-stale-base-trap]]` ·
`[[use-ledger-tools-for-all-ledger-ops]]` · `[[steward-handoff-filename-format]]` · `[[push-scope-fathomdb-only]]`.

## Open HITL queue

1. **AC-074 delta sign-off** → unblocks the 0.8.19 label-only close (the immediate step).
2. **Shared-checkout guardrail fix** proposal (owed; the HITL requested it after 0.8.19).
3. **0.8.20 commission** (author Phase-2 ladder + de-stale plan-0.8.20 + pin the `h:` end-state per TC-11 first).
4. Standing: the F-22 TC schedule; the deferred cross-platform follow-on orchestrator (0.8.18 prompt exists).
