# STATUS — 0.8.6 (Foundations & shippability)

> Live state board for 0.8.6. Per `dev/design/orchestration.md` §12.5 the **orchestrator owns this
> board** (one docs commit per transition); slice agents never edit it. **Witnesses (git +
> `output.json` + verdict `.md`) win over this cache** on any conflict.
> Plan: [`../plan-0.8.6.md`](../plan-0.8.6.md). Sequencing of record:
> [`../0.8.6-0.8.16-PROGRAM-SEQUENCING.md`](../0.8.6-0.8.16-PROGRAM-SEQUENCING.md).
> Theme: collapse N provider contracts into one (#8), migrate the consumer onto governed read verbs
> (#9), stand up the minimal viable publish path (#11-min). **All-$0 — mechanism/CI only.**

## 1. Current state + next action

- **STATE: ✅ 0.8.6 COMPLETE — all slices closed, pushed to `origin/main`.** Slices 0/5/6/7/10/15/20/40
  done. Slice 40 verified all R-* + X1/X2/X3 GREEN on merged `main`: Rust **517** / Python **67** /
  TS **51** pass; release gates + mkdocs `--strict` + clippy `-D warnings` clean. **Slice 20 push landed
  2026-06-27 (HITL-approved):** `c8ccbe6e..09027f4b main -> main`, pre-push clippy passed, `origin/main`
  == local (0/0). **No `v*` tag → no registry publish** (0.8.x micro-releases are plan-level, not crate
  tags). R-REL-3 satisfied. The 0.8.6→0.8.16 line now baselines off a pushed `main`.
- **NEXT (out of 0.8.6 scope):** the experiment program + 0.8.7 (OOB GPU embedder) continue;
  GitHub flagged 8 dependabot vulns on the default branch (2 high / 5 mod / 1 low) — triage separately.

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
| **5** | Provider-protocol KEYSTONE | implementation | ✅ **CLOSED** — merged `aabb8d10`; codex §9 clean; byte-identical ELPS | X1 ✓(no-op) · X2 n/a · X3 ✓ |
| **6** | *(reserved-gap)* schema-literal cleanup (engine) | fix | ✅ **CLOSED** `bfc39383` — pr_g1_tokenizer tests assert `SCHEMA_VERSION` not stale `14` | — |
| **7** | *(reserved-gap)* schema-literal cleanup (schema crate) | fix | ✅ **CLOSED** `54487fec` — migrations.rs + step14 tests track step-15 head (surfaced by Slice 40) | — |
| **10** | Coupling hygiene | implementation | ✅ **CLOSED** — embed→Py↔TS parity + governed; consumer-boundary conformance; 0.8.4 `embed` regression FIXED | X1 ✓ · X2 ✓ · X3 ✓ |
| **15** | Release-enablement | implementation (CI) | ✅ **CLOSED** — VERIFIED GREEN, no code change (`runs/0.8.6-slice-15-release-verify.md`) | X1 n/a · X2 ✓ · X3 ✓ |
| **20** | Backlog push (HITL) | release op | pending (15) — 186 commits `main`↑`origin` | — |
| **40** | Verification + Release Readiness | verification | ✅ **CLOSED** — all R-* + X1/X2/X3 GREEN on merged main (Rust 517 · Py 67 · TS 51); only R-REL-3 (push) remains | X1 ✓ · X2 ✓ · X3 ✓ |
| **20** | Backlog push (HITL) | release op | ✅ **CLOSED** — HITL-approved push `c8ccbe6e..09027f4b`; `origin/main`==local; pre-push clippy passed; no `v*` tag | — |

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

- **2026-06-26** — **Slice 40 verification in progress.** Full `cargo test --workspace` surfaced 6
  pre-existing stale schema-literal failures (`fathomdb-schema` migrations + step14; head is 15 since
  `temporal_fallback` step-15, masked because schema gates aren't in per-push CI) → **reserved-gap
  Slices 6 (engine) + 7 (schema)** fixed them (assert the `SCHEMA_VERSION` constant / current head).
  Schema crate green in isolation; authoritative workspace re-run running. mkdocs `--strict` green (X2).
- **2026-06-26** — **Slice 10 CLOSED.** Surfaced + fixed a real 0.8.4 regression: `Engine.embed` shipped
  Python-only (ungoverned, not in TS) → `test_surface.py` RED on baseline. HITL chose **Option A** (govern
  + bring to TS parity): added `embed` to napi (`fn embed`) + TS `engine.embed()` + the governed allowlist;
  added Py & TS functional embed harnesses + a **cross-binding golden anchor** (`conformance/embed-anchor-golden.json`,
  Py ≡ TS within 1e-3) + a consumer-boundary conformance test (R-CH-1) in both bindings; API-ref docs (X3).
  Results: TS **8/8** new + surface **11/11**; Py **31/31** (test_surface now GREEN). **Build identity note:**
  `.venv` `.so` = release (canonical golden); `.node` currently = 0.8.6 **debug/test-hooks** build (canonical
  TS test target) — Slice 40 rebuilds release.
- **2026-06-26** — **Slice 5 GATED (codex §9 clean).** Implementer landed the `ProviderTask`/`ProviderSession`
  typed-task seam (extract byte-identical, `supported_tasks` negotiation). First codex pass flagged one [P1]
  ("not provider.v1") — **stale-ADR artifact** (worktree cut before the per-task-naming amendment `cb0dc1f6`);
  re-ran codex against the corrected ADR → **"no discrete correctness issue."** 352 pass / 2 pre-existing
  baseline fails (schema-literal 14→15, NOT this slice → reserved-gap Slice 6). Branch `0.8.6-slice-5-provider-seam`
  @ `c71be29e`, ready to merge.
- **2026-06-26** — **Slice 15 CLOSED** (release machinery VERIFIED GREEN, no code change).
- **2026-06-26** — Slice 0 opened. Verified 0.8.5 landed on main; cleaned stale `slice-085` worktree;
  verified release machinery + governed surface already built (reconciliation §1); drafted board + ADRs.
