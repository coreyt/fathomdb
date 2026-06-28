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
| **40** | Verify + release readiness | **CLOSED (verification) — awaiting HITL push/merge** | re-verified on the rebased `089-orchestrator` (§2a); codex §9 re-run over the new R-BOOT/R-050c commits = clean PASS; X1 N/A, X2/X3 done. PR + merge gated on HITL (§7) |

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

### 2a-bis. Re-verification on the rebased `089-orchestrator` (2026-06-28, canonical branch)

The CLEAN canonical branch (`089-orchestrator`, off origin/main, no 0.8.8 contamination) was
**rebased onto origin/main** (`d1f2181f`): now **0 behind / 4 ahead**; `git range-diff` confirms
all 4 commits content-identical (`=`) post-rebase; 14-file diff = exactly the 0.8.9 scope. All
checks re-run with **real exit codes** (`PIPESTATUS`, not a trailing echo):

| Check | Result | Exit |
|---|---|---|
| `cargo test -p fathomdb-engine --test recall_gate_predicate` | 3/3 (below/exact/within-floor) | 0 |
| `cargo test -p fathomdb-engine --test perf_gates_devloop` | 3/3 (ac_013/013b/019) | 0 |
| `check-netns-deny-egress-catch.sh` (offline) | 2 off-loopback egress flagged | 0 |
| `npm audit` | 0 vulnerabilities | 0 |
| `mkdocs build --strict` | built clean (X2) | 0 |
| **codex §9 (`--base origin/main`, covers the new R-BOOT/R-050c commits)** | **clean PASS, 0 findings** | 0 |

codex §9 verbatim: *"I did not identify any discrete, introduced correctness issues in the diff.
The added security catch script, recall predicate test, npm overrides, and CI checkout changes
appear consistent with their intended behavior."* This **supersedes** the earlier `--uncommitted`
pass (which predated commits `3d27f23f`/`a40c7cd7`, now rebased to `75c5939a`/`16bdd1ee`).

## 3. HITL sign-off ledger (commits/pushes/outward actions are HITL-gated)

- [x] Working-tree changes reviewed (codex §9) — **clean PASS, 0 findings**
- [x] Memory reconciliation — `perf-recall-gates-masked-and-ac013b-conflation` updated (RESOLVED header)
- [x] Commit 0.8.9 residue — **HITL: branch + PR.** Branch `0.8.9-ci-integrity-micro`,
      commit `d5a68d17`, **PR #93** (10 files; unrelated working-tree changes excluded).
- [x] Dismiss orphaned idna/torch alerts — **HITL: leave open** (documented as orphaned).
- [x] Version-bump / tag — **HITL: no version bump** (zero library-surface change).
- [ ] Merge PR #93 — HITL action. **MERGE-READY** (rebased onto `6d92aebd`). Both remaining CI
      reds are external/non-regressions: markdown = DEFER→0.8.16 (F-7); pyo3 macOS/Windows =
      unowned-external (steward-tracked). **Steward recommends HITL merge now** (heals main's
      security/bootstrap red); admin-merge accepting the two documented external reds (§5 + §7).

## 5. CI status on PR #93 — only EXTERNAL reds remain; 0.8.9 turned `security` GREEN

**Updated 2026-06-28 from the latest PR #93 run** (`gh pr checks 93`, run `28298992713`). The
bootstrap un-mask (Slice 1, `3d27f23f`) + R-050c tag-fetch (`a40c7cd7`) **fixed the `security`
job** — it was red at bootstrap on the original board; it now **PASSES** (8m20s, full
`agent-security.sh` battery incl. the AC-037 live-netns catch). **Three** jobs remain red, all
external/out-of-scope:

| Job | Fails at step | Cause | Owner | Latest |
|---|---|---|---|---|
| `security` | — | bootstrap un-mask + R-050c fixed it; AC-037 catch ran LIVE | **0.8.9 (fixed)** | **PASS** |
| `verify` | **lint** | repo-wide markdownlint debt (7983 errors / 304 prettier-fail) — masked until bootstrap fixed; **unsatisfiable repo-wide** | not 0.8.9 → **RESOLVED: DEFER→0.8.16 (F-7)** | fail (documented debt, non-blocking) |
| `rust-macos` | `cargo test --workspace` | pyo3 0.29 cross-platform test-link (`_PyDict_GetItemWithError`, `_PyExc_*` undefined) | **UNOWNED-EXTERNAL (steward-tracked)** — 0.8.8 did NOT fix it; steward escalating for an owner. Not a 0.8.9 regression. | fail |
| `rust-windows` | `cargo test --workspace` | same pyo3 0.29 test-link | **UNOWNED-EXTERNAL (steward-tracked)** — same. Not 0.8.9. | fail |

All other jobs PASS: `Analyze (actions/javascript/python/rust)`, `CodeQL`, `default-embedder-tests`,
`docs`, all five `wheel-size-gate` matrix legs.

**0.8.9 adds zero failures.** Every CI job that reaches the 0.8.9 changes is green:
`Analyze (rust)` (compiled `recall_gate_predicate.rs`), `docs`, `default-embedder-tests`,
`wheel-size-gate (linux-x64)`. The AC-037 catch live-run on `ubuntu-22.04` could not be
confirmed in CI because the `security` job dies at bootstrap first — but the catch is proven
locally (offline layer green + RED-confirmed) and by `Analyze (rust)`. **Full PR green
requires 0.8.8 (pyo3 link) + a bootstrap infra fix — both out of 0.8.9 scope.**

## 6. Post-Slice-1 CI (commit 3d27f23f) — un-mask worked; exposed pre-existing failures

Slice 1 fixed bootstrap → `security`/`verify` now run **past** bootstrap. What that revealed:

- **WIN — R-037-2 proven IN CI:** on the `security` job (`ubuntu-22.04`), `AC-037 netns-deny-egress`
  PASS **and** `AC-037 catch (demonstrate)` ran its **live netns** layer and flagged the deliberate
  egress (`live netns: deliberate egress flagged … 8.8.8.8 ENETUNREACH`). AC-036/038/050a PASS.
- **`security` — AC-050c BLOCKER** `fatal: bad revision 'v0.6.1..HEAD'`: the CI checkout was shallow
  (no tags), so removal-detect couldn't resolve its base. **FIXED (in 0.8.9 scope, R-050c):**
  `fetch-depth: 0` added to the `verify` + `security` checkouts in `ci.yml`.
- **`verify` — fails at step=lint** on **pre-existing repo-wide markdownlint debt**: **7983 errors
  across 300 files** (CHANGELOG, every `dev/adr/*`, every `dev/design/*`, **incl. 0.8.9's own
  `perf-gates.md`**). Rules: MD049 (emphasis: expects `_`, repo uses `*`) + MD060 (table-column-style).
  The gate is **unsatisfiable repo-wide** and was masked by the bootstrap failure. `agent-lint.sh:45`
  → `agent-lint-md.sh` is the gate. **R-LINT — RESOLVED: DEFER (HITL 2026-06-28) → 0.8.16, master F-7.**
  PR #94 (`b3bf6f52`) landed the decision: the "1-file config relax" (option A) was **verified
  insufficient** (leaves ~2860 lint errors + the whole prettier wall — measured 7983 markdownlint
  errors + 304 prettier-failing files on main). Disposition = **DEFER + DOCUMENT**: a one-shot
  `prettier --write` + `markdownlint --fix` bulk cleanup is sequenced to **0.8.16** (after release work
  lands, to avoid colliding with live orchestrators). **The `verify` lint-step red is documented known
  debt, NOT a 0.8.9 regression — it does NOT gate 0.8.9's merge.** No markdown lint/format fixes enter
  0.8.9 scope.
- **`rust-macos`/`rust-windows` — pyo3 link error** at `cargo test --workspace` — still **0.8.8** (the
  0.24→0.29 bump did not resolve the macOS/Windows extension-link); not 0.8.9.

**0.8.9's own gates are green where reachable:** AC-037 (+catch live), AC-036/038/050a, `Analyze (rust)`
(compiled `recall_gate_predicate`), `docs`, `default-embedder-tests`, all `wheel-size-gate`. The repo-wide
markdown gate is now **resolved as documented-deferred debt (F-7 → 0.8.16, non-blocking)**; the only
remaining external red is the 0.8.8 macOS/Windows pyo3 link.

## 7. Landing path — PR #93 is NOT contaminated (verify-from-git, 2026-06-28)

The orchestrator handoff said to **supersede** PR #93 because its branch was "contaminated with
duplicate 0.8.8 commits." **Git does not bear this out.** `git fetch origin 0.8.9-ci-integrity-micro`
+ `git log origin/main..FETCH_HEAD` shows PR #93's pushed branch is **content-identical** to
`089-orchestrator` (pre-rebase): the **same 4 commits** (`58e12802`/`2a770788`/`3d27f23f`/`a40c7cd7`),
1-behind/4-ahead, and **zero 0.8.8/EXP-OBS/explain/telemetry commits** in its unique history. The
contamination (per the `shared-checkout-branch-can-be-stale` memory) was in the *local shared
checkout's working state*, not the *pushed* PR-#93 branch.

**Consequence:** opening a new PR + closing #93 would be unjustified churn. Two clean options for HITL:

- **(A — recommended) Update PR #93 in place.** Force-push the rebased `089-orchestrator`
  (0-behind + the closing-docs commit) to `0.8.9-ci-integrity-micro`. Keeps PR #93, its number,
  and its discussion; the only change is a clean rebase onto current origin/main + the Slice-40
  closing docs. No new PR, no close.
- **(B) New branch + new PR + close #93.** The handoff's original plan; now unnecessary since the
  contamination premise is disproven. More churn, no benefit.

Either way the **closing-docs commit** (this board + `dev/DOC-INDEX.md` 0.8.9 entry; 2 files,
currently uncommitted) needs HITL authorization to commit, and the **push** itself is HITL-gated.

**Merge readiness (updated 2026-06-28 — rebased onto `6d92aebd`, MERGE-READY):** PR #93 is rebased
onto current origin/main (`6d92aebd` = 0.8.8 telemetry landed + F-7 + F-8); 0-behind / 5-ahead. 0.8.9's
own gates are green (incl. the now-passing `security` job). The two remaining CI reds are both **external,
not 0.8.9 regressions**:
- `verify`/lint = repo-wide markdown debt — **RESOLVED as documented-deferred (F-7 → 0.8.16), non-blocking.**
- `rust-macos`/`rust-windows` = pyo3 0.29 cross-platform test-link — **UNOWNED-EXTERNAL, steward-tracked**
  (0.8.8 did not fix it; steward escalating for an owner).

0.8.9's release verdict is **COMPLETE on its own gates** (perf-gate honesty, AC-037 live-proven, dependency
hygiene). **The steward recommends HITL merge now** — merging heals main's own `security`/bootstrap red
(0.8.9 *is* the fix for that) and neither external red will clear soon. Merge = admin-merge accepting the
two documented external reds, **pending HITL sign-off**. (Context unchanged: no version bump; idna/torch
alerts left open.)

## 4. $ ledger

$0.00 — no priced runs; CI/test-harness + lockfile work only.
