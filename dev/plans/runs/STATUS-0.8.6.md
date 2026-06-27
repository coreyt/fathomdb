# STATUS — 0.8.6 (Foundations & shippability)

> Live state board for 0.8.6. Per `dev/design/orchestration.md` §12.5 the **orchestrator owns this
> board** (one docs commit per transition); slice agents never edit it. **Witnesses (git +
> `output.json` + verdict `.md`) win over this cache** on any conflict.
> Plan: [`../plan-0.8.6.md`](../plan-0.8.6.md). Sequencing of record:
> [`../0.8.6-0.8.16-PROGRAM-SEQUENCING.md`](../0.8.6-0.8.16-PROGRAM-SEQUENCING.md).
> Theme: collapse N provider contracts into one (#8), migrate the consumer onto governed read verbs
> (#9), stand up the minimal viable publish path (#11-min). **All-$0 — mechanism/CI only.**

## 1. Current state + next action

- **STATE: Slice 0 CLOSED — HITL-SIGNED 2026-06-26.** Both ADRs ACCEPTED; **Slice 5 scope = Option A**
  ("land the seam now"); orchestration mode = **full autonomous** (orchestrator commits through slices,
  hard-pause only at the Slice-20 push). Prerequisite met: 0.8.5 (EXP-0 CE-rerank α/pool_n) is landed on
  `main` (`76bd2952` + codex-fix `0a8f3f1a`, both ancestors of `main`). Baseline = `main` `ad7c0bcf`.
- **NEXT:** fan out the three tracks off Slice 0 in parallel worktrees — **Slice 5** (provider seam,
  Option A) ∥ **Slice 10** (parity-harden) ∥ **Slice 15** (verify gates) — then Slice 20 (push) → 40.

### ◆ SCOPE RECONCILIATION (load-bearing — read before signing) ◆

The plan (`plan-0.8.6.md`) was authored from a **stale pessimistic premise**. Verifying state from git
(not narration) shows two of the three tracks are **already largely built**:

| Track | Plan premise | Repo reality (verified) | Revised slice scope |
|-------|--------------|--------------------------|---------------------|
| **#11-min (Slice 15, release)** | "Rewrite `set-version.sh` to two axes; restore `release.yml` to 8-tier; wire gates; dry-run." | `set-version.sh` is **already** full two-axis with `--check-files` drift detection (`scripts/tests/test_set_version.sh` covers it). `--check-files` **passes** (`ok: Axis W = 0.8.0; Axis E = 0.6.0`, exit 0). `release.yml` (538 ln) **already** has the complete topological pipeline: `verify-release → build-python(matrix) → build-napi(matrix) → build-rust → all-builds-passed → publish-rust-t1…t7 (sequential needs) → publish-pypi ∥ publish-npm (T8, need t5-engine) → post-publish-smoke → co-tagging-assert → github-release`, with a `dry_run` dispatch input. `verify-release-gates.sh` + all `scripts/release/*` helpers present. | **VERIFY, not build.** Run the gates green end-to-end (RED→GREEN skewed-fixture for `--check-files`; `local-dry-run.sh`; confirm 8-tier order vs `design/release.md`). |
| **#9 (Slice 10, coupling hygiene)** | "Complete/stabilize the governed read surface." | The governed read surface is **already complete + LIVE in Py+TS** on a single shared allowlist (`src/conformance/governed-surface-allowlist.json`): `read.get/get_many/collection/mutations/list`, `graph.neighbors/search_expand`. Every gap the Memex 0.6.0 note named (G2/G3/G4/G5/G6) is resolved. | **HARDEN, not build.** The one real gap (Explore): **no cross-binding parity harness for ALL read verbs** (only `read.list` is anchored; graph verbs' Py↔TS equivalence is weaker). Close that. Confirm no internal-engine reach is required for the OPP-5 read paths. |
| **#8 (Slice 5, provider protocol)** | "One typed-task trait + schema + error model; re-express ELPS byte-identical." | **This is the one genuine build.** Foundation exists: `fathomdb.extract.v1` (subprocess + NDJSON, `ADR-0.8.1-byo-llm-extraction-protocol.md`, golden fixture `elps_conformance_golden.rs`). OPP-8 generalizes it to a typed-task envelope so OPP-2 (consolidate) / OPP-4 (summarize) ride one contract. | **BUILD (real), with a YAGNI caveat** — see the provider-protocol ADR §Risk. The second consumer (consolidation) is not yet designed, so the generalization is forward-looking. |

**Implication:** "complete 0.8.6" is materially smaller than the plan reads. The critical path is
**Slice 5 (provider protocol) → Slice 40 verify**; Slices 10 & 15 are verification/hardening; Slice 20
is the HITL-gated 186-commit push. This board records the reconciliation; the plan ladder is updated in
the Slice-0 closing docs commit per `orchestration.md` §12.4 (board records the current pointer).

## 2. Slice scoreboard

| Slice | Title | Work-type | State | X1/X2/X3 |
|------:|-------|-----------|-------|----------|
| **0** | Setup + ADR Kickoff | design-adr | ✅ **CLOSED** — HITL-signed 2026-06-26 (board + 2 ADRs) | n/a |
| **5** | Provider-protocol KEYSTONE | implementation | **IN FLIGHT** — Option A (seam-only, byte-identical ELPS) | — |
| **10** | Coupling hygiene | implementation | **IN FLIGHT** — **revised: parity-harden** | — |
| **15** | Release-enablement | implementation (CI) | ✅ **CLOSED** — VERIFIED GREEN, no code change (`runs/0.8.6-slice-15-release-verify.md`) | X1 n/a · X2 ✓ · X3 ✓ |
| **20** | Backlog push (HITL) | release op | pending (15) — 186 commits `main`↑`origin` | — |
| **40** | Verification + Release Readiness | verification | pending (5,10,15,20) | — |

## 3. $ ledger

**$0 release** — mechanism/CI only. No priced runs, no experiments. Only spend = codex §9 reviews
(local, negligible). Runs beside the experiment program without contending for budget.

## 4. Outstanding worktrees

- `/tmp/fdb-0.8.7-gpu` @ `0a8f3f1a` `[0.8.7-gpu-embedder]` — the **OOB 0.8.7 GPU-embedder track** (plan
  §8); byte-stable/opt-in, shares no files with 0.8.6. **Leave alone.**
- Stale `slice-085` worktree **removed** 2026-06-26 (tip `d04f2eaf` was the pre-merge docs commit,
  superseded by `76bd2952` on main).

## 5. Open HITL gates

- **◆ Slice-0 sign-off (BLOCKS 5/10/15):** (a) `ADR-0.8.6-generalized-provider-protocol.md` (OPP-8 —
  load-bearing, gates 0.8.10), (b) `ADR-0.8.6-governed-verb-coupling-hygiene.md` (OPP-5 scope), (c) the
  scope reconciliation above. **Decision the human owns:** does Slice 5 build the generalized protocol
  now (forward-looking for 0.8.10), or land a thinner increment given the YAGNI tension (ADR §Risk)?
- **◆ Slice-20 push (BLOCKS the "shippable" DoD):** HITL-gated push of 186 commits `main` → `origin`.
  No tag without sign-off (`release-publish-gotchas`).

## 6. Recent decisions (newest on top)

- **2026-06-26** — Slice 0 opened. Verified 0.8.5 landed on main; cleaned stale `slice-085` worktree;
  verified release machinery + governed surface already built (reconciliation §1); drafted board + ADRs.
