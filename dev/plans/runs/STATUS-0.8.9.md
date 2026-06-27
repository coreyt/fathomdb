# STATUS — 0.8.9 (CI integrity micro, OUT-OF-BAND) · live board

> Plan: `dev/plans/plan-0.8.9.md`. Footprint: **$0** — CI/test-harness only; no library
> query-path change, no priced runs. Verify-from-git discipline throughout.
> Opened: 2026-06-27 (`/goal complete 0.8.9`, orchestrator session).

---

## 0. Headline — the plan was substantially stale; most of 0.8.9 already shipped

Slice 0 audited the **actual** gate reality against the plan's premises (which were written off the
`perf-recall-gates-masked-and-ac013b-conflation` memory dated 2026-06-06, the day the defects were
*exposed* at 0.8.0 Slice 40). Verifying from git shows Slice 40 (and later cleanup) **also fixed most of
them**. The honest deliverable (R-PG-1) is this map, not a fabricated five-slice pass.

| Requirement | Plan premise | **Verified reality (2026-06-27)** | Residue |
|---|---|---|---|
| **R-PG-2** ac_013b off the synthetic floor | "asserts 0.90 on ~0.73 synthetic" | **DONE @ Slice 40 (AC-075).** `perf_gates.rs::ac_013b_recall_at_10_floor` is **report-only** (prints `RECALL_FIDELITY_INFO`, no hard assert). Asserting verdict moved to `eu7_real_corpus_ac.rs` (real BGE, vector-stage, one-sided CI `ci_hi≥0.90`). | add a cheap RED unit-test on the catch predicate |
| **R-PG-3** cheap subset runs per-push | "gates never run per-push" | **DONE.** The devloop tier (`perf_gates_devloop.rs`) runs on every `cargo test --workspace` (agent-test → CI `verify` job). Canonical tier is `AGENT_LONG` (release-only, real-embed = hours) — inherently not per-push. | doc the split |
| **R-037-1** AC-037 in CI on userns-permissive runner | "confirmed once on windchill3, not durable" | **DONE @ `8402e59c`.** `ci.yml` `security` job on **`ubuntu-22.04`** runs `STRICT=1 agent-security.sh` (AC-036/037/038/050a/050c); `STRICT=1` makes a toolchain blocker a hard failure (no vacuous pass). | — |
| **R-037-2** demonstrate-the-catch | open | **OPEN.** No egress fixture proves the gate trips. *Cannot execute in this sandbox* (rootless userns unavailable); runs on the `ubuntu-22.04` job. | **author fixture + RED proof** |
| **R-050c-1** removal-detect baseline cleared | "fails pre-existing on baseline" | **DONE @ `a8304652` (0.8.0 Slice 27 fix-1).** Cause: removal-detect was scoping `tests/` files into the public-surface diff (test-helper removals false-positived) and a CHANGELOG operator-gate note was missing. The fix excluded `tests/` and added the note. Passes on baseline now (`base=v0.6.1`, exit 0). The 2026-06-06 memory predates this fix landing. | (done) |
| **R-DEP-3** no mechanical auto-merge | open | **CONFIRMED.** `allow_auto_merge=false`; no auto-merge workflow in `.github/`. | — |
| **R-DEP-1** npm bumps | markdown-it + js-yaml | **OPEN + actionable.** Root `package-lock.json`: markdown-it 14.1.1→**14.2.0**, js-yaml 4.1.1→**4.2.0** (both transitive via `markdownlint-cli2`). | **bump + verify** |
| **R-DEP-1** pip bumps | idna + torch in `python/uv.lock` | **MOOT / orphaned.** `python/uv.lock` was **archived out of the tree** (`39ee2712`; archive removed `df33207a`) and idna was already bumped 3.11→**3.15** (the fix) at `e850052d` before removal. `src/python/uv.lock` carries **neither** idna nor torch. `torch` has **no patched version** (`<=2.12.0`, low-sev, eval-only). | dismiss-with-rationale (manifest gone) |
| **R-DEP-2** dependabot.yml coverage | add root `package-lock.json` + `python/uv.lock` dirs | **OPEN (npm only).** npm root `/` is uncovered today (only `/src/ts`). The pip `/python` dir is **moot** (no lockfile in tree). | **add npm `/`**; note pip moot |

**Net residue (genuinely open, all $0 CI/test-harness):**
1. **R-PG-1** consolidated gate-map table in `dev/design/perf-gates.md`.
2. **R-PG-2** RED unit-test on `recall_ci_clears_floor` (proves FAIL when `ci_hi<floor`).
3. **R-037-2** deliberately-egressing fixture + RED proof for the AC-037 netns gate.
4. **R-DEP-1 (npm)** bump root `package-lock.json` (markdown-it, js-yaml) + re-run md lint.
5. **R-DEP-2** add npm `/` directory to `.github/dependabot.yml`.
6. **R-DEP-1 (pip)** dismiss-with-rationale the orphaned idna/torch alerts (HITL-gated outward action).

---

## 1. Slice board (mod-5)

| Slice | Title | State | Notes |
|------:|-------|-------|-------|
| **0** | Setup + audit; map gate reality | **CLOSED (this doc)** | scope reconciled; residue identified |
| **1** *(reserved-gap)* | Bootstrap un-mask (R-BOOT) | **CLOSED** | Fix A `# type: ignore[import-not-found]` on the two httpx imports → `pyright -p src/python` 0/0; Fix C dropped `--quiet`/`>/dev/null` masking; R-BOOT-1 clean `[dev]` venv green; R-BOOT-2 demonstrate-the-catch proven (broken import → visible error + exit 1; old masked path hid it) |
| **5** | Perf-gate honesty (R-PG-1/2) | **CLOSED** | `perf-gates.md` per-AC map; `recall_gate_predicate.rs` catch test (3/3 green, RED-confirmed) |
| **10** | AC-037 catch + AC-050c (R-037-2/R-050c) | **CLOSED** | shared `lib-egress-allowlist.sh`; `check-netns-deny-egress-catch.sh` (offline catch green + RED-confirmed, live netns CI-only); R-050c cause documented |
| **15** | Dependency hygiene (R-DEP) | **CLOSED** | npm overrides → markdown-it 14.2.0/js-yaml 4.2.0, `npm audit`=0; dependabot.yml npm `/` added; pip idna/torch = orphaned (dismiss pending HITL) |
| **40** | Verify + release readiness | in progress | cargo test, mkdocs, codex §9, HITL |

## 2. Cross-cutting DoD (X1/X2/X3)

- **X1 SDK parity** — no library API change (CI/test-harness only). N/A by design.
- **X2 `mkdocs build --strict`** — keep green (perf-gates.md edits stay in nav).
- **X3 docs + DOC-INDEX** — reconcile the stale gate-map references in the closing docs commit.

## 2a. Slice 40 verification (all local, $0)

- `cargo test -p fathomdb-engine --test perf_gates_devloop` → **3/3 green** (per-push tier).
- `cargo test -p fathomdb-engine --test recall_gate_predicate` → **3/3 green**; RED-confirmed
  (a tautological allowlist flags nothing → test exits 1).
- `check-netns-deny-egress-catch.sh` → **PASS** (offline catch flags 2 egress, clean trace
  not flagged; live netns skipped — no userns in sandbox); RED-confirmed.
- `agent-security.sh` battery → catch gate PASS (AC-037 live = BLOCKER here, expected; runs
  on the `ubuntu-22.04` CI `security` job).
- `mkdocs build --strict` → **exit 0** (perf-gates.md lives under `dev/`, outside published
  `docs/`; X2 satisfied).
- `npm audit` → **0 vulnerabilities** (was 3 moderate). Override is lint-behavior-neutral
  (identical markdownlint output before/after; `npm run lint:md` is **not** a CI gate —
  `agent-lint.sh` doesn't run it; the doc gate is `mkdocs --strict`).
- **codex §9 review (`--uncommitted`)** → **clean PASS, 0 findings**: "No discrete
  correctness issues … The added security catch and recall predicate test both pass."

## 3. HITL sign-off ledger (commits/pushes/outward actions are HITL-gated)

- [x] Working-tree changes reviewed (codex §9) — **clean PASS, 0 findings**
- [x] Memory reconciliation — `perf-recall-gates-masked-and-ac013b-conflation` updated (RESOLVED header)
- [x] Commit 0.8.9 residue — **HITL: branch + PR.** Branch `0.8.9-ci-integrity-micro`,
      commit `d5a68d17`, **PR #93** (10 files; unrelated working-tree changes excluded).
- [x] Dismiss orphaned idna/torch alerts — **HITL: leave open** (documented as orphaned).
- [x] Version-bump / tag — **HITL: no version bump** (zero library-surface change).
- [ ] Merge PR #93 — HITL action (blocked on pre-existing CI red; see §5)

## 5. CI status on PR #93 — pre-existing red on main, NOT caused by 0.8.9

Verified from git (main's last 3 runs are red on the SAME 4 jobs, on docs-only commits):

| Job | Fails at step | Cause | Owner |
|---|---|---|---|
| `verify` | **Bootstrap dev tooling** | `bootstrap.sh` Python-tooling `.venv` install dies (~4 min → exit 1) — infra | not 0.8.9 |
| `security` | **Bootstrap dev tooling** | same bootstrap failure — aborts **before** `agent-security.sh`, so my AC-037 catch + recall test never execute in CI | not 0.8.9 |
| `rust-macos` | `cargo test --workspace` | pyo3 link error (`_PyDict_GetItemWithError`, `_PyExc_*` undefined) | **0.8.8** (pyo3 0.24→0.29) |
| `rust-windows` | `cargo test --workspace` | same pyo3 link error | **0.8.8** |

**0.8.9 adds zero failures.** Every CI job that reaches the 0.8.9 changes is green:
`Analyze (rust)` (compiled `recall_gate_predicate.rs`), `docs`, `default-embedder-tests`,
`wheel-size-gate (linux-x64)`. The AC-037 catch live-run on `ubuntu-22.04` could not be
confirmed in CI because the `security` job dies at bootstrap first — but the catch is proven
locally (offline layer green + RED-confirmed) and by `Analyze (rust)`. **Full PR green
requires 0.8.8 (pyo3 link) + a bootstrap infra fix — both out of 0.8.9 scope.**

## 4. $ ledger

$0.00 — no priced runs; CI/test-harness + lockfile work only.
