---
status: SUPERSEDED
---

> 🕮 **SUPERSEDED by `STEWARD-SESSION-HANDOFF-2026-07-30-B.md` (steward `seq-216`).**
> Retained as history and for the reasoning behind items the `-B` hand-off states as settled.
> The 🕮 annotations inside this file (from `seq-207`–`seq-211`) still carry corrections to its own
> claims. **`-B` is the current truth.** Do not delete this file.

# FathomDB — Steward Session Hand-off (2026-07-30-A)

> **Boot:** run **`/steward`**, do its §3 cold-start reading (**start with `scripts/steward-orient.sh`**),
> then read THIS doc, return a short orientation, and **WAIT for the HITL** before mutating anything.
> Supersedes `STEWARD-SESSION-HANDOFF-2026-07-29-B.md`, which was written before Slice 39 landed, before
> the `seq-196`/`seq-198` publish-trigger and dispatch-guard rulings, and before `seq-202`/`seq-203`/`seq-204`
> re-shaped the tail. Its §2 ("**Nothing else fires a publish**") is now known to be **FALSE** — see §2 here.
>
> 🕮 **THIS DOCUMENT HAS ITSELF BEEN PARTIALLY SUPERSEDED** by steward `seq-207`–`seq-211` (2026-07-30,
> `80f83bf7` … `09d47443`). Six of its statements were overtaken by rulings and measurements that landed
> after it was written. **Nothing below has been rewritten or deleted** — the annotations marked 🕮 carry the
> current truth; where one contradicts the prose it is attached to, the annotation governs.

## 0. State — do NOT copy numbers out of here

`scripts/steward-orient.sh` prints branch / HEAD / worktrees / landed slices + SHAs / SCHEMA / ledger tail /
todos fold / next action, all from the **single writer** `dev/plans/release-state-0.8.20.json`. **Run it and
trust it over this section**, which is a snapshot and will drift.

| | at hand-off |
|---|---|
| `main` | **`aea384bb`** — *"de-ladder Slice 39.5; two cross-cutting units before Slice 40 (seq-204)"* |
| Working tree | clean apart from this hand-off |
| Steward ledger tip | **`seq-204`** (204 entries), `dev/steward/steward-ledger.jsonl` |
| Todos ledger tip | **`TC-132`** (123 ids folded; 91 open) |
| 0.8.20 ladder | LANDED: 0 · 5 · 10 · 15 · 20 · 21 · 22 · 23 · 25 · 30 · 31 · 32 · 33 · **39 (`91db34d8`)**, plus non-ladder DOC-HYGIENE-3. `remaining_ladder` = **40 alone**. |
| SCHEMA | **24** |
| `decisions.unruled` | **THREE entries, not two** — see §8. `publish` and `npm-dist-tag` are genuinely open; the third is a *tracking* entry for an already-ruled item. |
| Manifests | Axis-W still **`0.8.9`** everywhere (`Cargo.toml:28`, `src/ts/package.json:3`, `src/python/pyproject.toml:16`, `src/ts/package-lock.json:3`+`:9`). Axis-E `fathomdb-embedder-api` at **`0.6.1`**. |
| CI on `main` | **RED.** See §6 — this is the single most important thing on this page. |
| Worktrees | primary · **`fathomdb-slice39-changelog` (`d9c66a55`)** · **`fathomdb-slice39-docs` (`dd6daa04`)** — both are Slice 39 leftovers, 39 has LANDED, they are candidates for removal · `0.5.1-memex-build` (Memex vehicle — **leave alone**) · `refactor-background-check` (unrelated). |
| Open PRs | 17, all Dependabot. They publish OPEN (`seq-152`). |

**0.8.20 is a breaking pair and the first REAL publish since `v0.8.9`** (tagged 2026-06-29) — to crates.io,
PyPI and npm. Everything below is downstream of that fact.

## 1. Immediate next action — two cross-cutting units, THEN Slice 40

🕮 **SUPERSEDED — SOMETHING *IS* IN FLIGHT.** `SLICE-ID-HARDENING` was **commissioned at `seq-210`**
(landed `fb05c08b`) and is running in `~/projects/fathomdb-worktrees/slice-id-hardening`, branch
`0.8.20-slice-id-hardening`. The sentence below was true when written.

**Nothing is in flight.** `next_slice` is `40`, and **Slice 40 is the only ladder slice left**. But
`seq-204` placed **two cross-cutting units ahead of it**. Neither is a ladder slice; neither carries a
slice number or an `R-20-xx` position — the **DOC-HYGIENE-3 / TC-86 precedent**.

```text
SLICE-ID-HARDENING  →  "Slice 39.5" (R-20-HARNESS)  →  40 (R-20-PUB)  →  [publish: TWO hard gates]
```

The authoritative statement of this ordering is `release-state-0.8.20.json` `ladder_order` and the
`status-unblocks` generated view on the board. **`dev/plans/plan-0.8.20.md` does not mention either unit**
— see §6.4, because that omission is load-bearing at commission time.

🕮 **FALSE AT HEAD.** The plan names **both**: `SLICE-ID-HARDENING` at `plan-0.8.20.md:492` and
"Slice 39.5" / `R-20-HARNESS` at `:497`. §9 was de-staled at `seq-205` (`190c8b8a`). There is no omission
to plan around at commission time.

### 1a. SLICE-ID-HARDENING (`seq-204` ruling 1)

An adversarial hunt found **six silent-wrong fractional-slice-id sites across four files and five tools,
every one exiting 0**, plus a **fabricated pointer that was already committed**. Measured sites:

- `check-board-currency.sh:123` captures `39` from `"Slice 39.5"`, so Slice 39's real landing merge is
  skipped as *superseded* and its SHA check never runs — in **both** `preflight --landing` **and** the
  always-on CI job.
- `commission-manifest.sh:701` filters a float out of the predecessor set, so Slice 40's brief would print
  a base SHA that silently drops a whole unit's work.
- `commission-manifest.sh:497` compiles an **identical** filename pattern for `39` and `39.5`, which can
  defeat the TC-37 vacuous-pass guard using a neighbour's memo.
- `preflight.sh:131` accepts one unit's CLOSED witness as another's — in the gate whose entire stated
  purpose is catching a dependency that is not actually closed.
- Plus **three latent `int()` / `%d` sites** in `check-release-state-views.sh`.

> ### ⚠ THE TEST ARMS ARE THE DELIVERABLE, NOT THE FIXES
>
> Nothing in the repo today would catch the next occurrence, and
> `scripts/tests/test_commission_manifest.sh:1718` **replicates the generator's own filter verbatim** — so
> it would *confirm* the defect rather than catch it. New recurrence arms are owed in
> `test_check_board_currency.sh`, `test_check_release_state_views.sh`, `test_commission_manifest.sh` and
> `test_preflight_landing.sh`.

Ruled as **one unit, done now** (option (b)) — explicitly **not** split with a remainder deferred to 0.8.21.

### 1b. "Slice 39.5" (`R-20-HARNESS`) — de-laddered at `seq-204`, name kept in prose

Design of record: **`dev/design/0.8.20-slice-39.5-collect-all-test-harness.md`** (`status: ACTIVE`, on
`main`). Read it — it is short and it is the contract.

- Converts `scripts/agent-test.sh` from **fail-fast to collect-all-then-report**, and delivers the **TRUE
  red list**, locally **and** in CI, from one forced full CI run (`seq-204` ruling 2 folded the CI baseline
  in).
- **The invariant it must not break:** a run with any failure still **exits non-zero**. Continue-on-failure
  changes *when* the harness stops, never *whether* a failure counts. A crash or timeout is a **failure**,
  never a skip. The summary must **name every** failing suite.
- **It does not fix what it finds.** The two already-owned red suites stay Slice 40's. The hidden suites'
  disposition is a separate decision.

> ### ⛔ `commission-manifest.sh` CANNOT GENERATE THIS UNIT'S BRIEF
>
> It has no ladder entry and no slice number. The brief must be **hand-written**, exactly as
> DOC-HYGIENE-3's was. Do not try to force a manifest for it — that is precisely the fractional-id class
> `seq-204` just ruled out.

## 2. ⛔ THE TWO THINGS THAT MUST NOT HAPPEN — and there are TWO, not one

> ### `v*` tag push is NOT the only irreversible publish path (`seq-196`)
>
> 1. **Pushing a `v*` tag** — `.github/workflows/release.yml:3-6`, `on: push: tags: - "v*"`.
> 2. **`workflow_dispatch` with the `dry_run` box UNCHECKED** — the else branch calls
>    `scripts/release/cargo-publish-if-new.sh` **without** `--dry-run`, with `CARGO_REGISTRY_TOKEN` in env
>    (around `release.yml:258-264`). `publish-pypi` (`:413`) and `post-publish-smoke` (`:529`) are
>    `if: inputs.dry_run != true`, so they **run**. `scripts/verify-release-gates.sh:58-61` only **prints a
>    warning and continues**, and the tag-format check is skipped entirely on dispatch. Verified at HEAD:
>    that warning branch is still exactly a `printf … >&2`, with no `die`.
>
> Published versions **cannot be unpublished**. crates.io is immutable.

---

> ### ⚠ The mechanism, measured — do not "fix" it backwards
>
> The `workflow_dispatch` input is **`default: true`** (`release.yml:12`), so the checkbox arrives
> **pre-checked**. The hazard is a human *unchecking* it.
>
> `release.yml:20` reads `DRY_RUN: ${{ inputs.dry_run || 'false' }}`. **That `'false'` fallback is CORRECT
> and must stay** — it governs the **tag-push** event, where `inputs` is empty and a real publish is
> intended. Changing it to `|| 'true'` would make a pushed `v*` tag resolve `DRY_RUN=true` and **silently
> never publish** — a false green on the irreversible path.

---

> **Landing on `main` does NOT publish.** There is no `push: branches` trigger and a branch-to-branch push
> carries no tag (`push.followTags` is unset). Do not balk at pushing to `main` at Slice 40, and **never
> improvise a tag to "complete" the release.** Any rehearsal is `-f dry_run=true`, with the **resolved
> `DRY_RUN` read out of the run log** before any `publish-rust-t*` job starts.

**`seq-198` ruling 1 is work Slice 40 owes here:** add a **confirmation input** to `release.yml` that must
literally match the version, and make `verify-release-gates.sh` **EXIT 1** — not warn — when a dispatch has
`dry_run=false` without it. **Neither exists at HEAD.** The `dry_run` default and the `|| 'false'` fallback
stay exactly as they are; this is a *second factor*, not a change to how `DRY_RUN` resolves.

## 3. Slice 40 (`R-20-PUB`) — the brief's substance

The v4 brief (three adversarial review rounds) lived only in a session scratchpad. Its substance is
recorded here so the next Steward is not dependent on a `/tmp` path. **Its stated base was `1f85ca2a`,
which is four commits behind HEAD** — every `file:line` in it needs re-verification, and its "FOUR suites
red" measurement is already stale (see §6.1).

**Sequencing rationale.** The CI rehearsal is **LAST, after landing**: `verify-release-gates.sh` check 3
requires HEAD reachable from `main`, so a dispatch from a slice branch dies at `verify-release`. And
`workflow_dispatch` inputs are read from the **default branch only**, so the new confirmation input is not
dispatchable until after the land — a pre-land dispatch not offering it is **not** evidence the guard is
broken.

### 3a. The phases

- **PHASE 0 — TC-16 / F-30.** *Determine which side is wrong, then fix that side* (`seq-118`); it must pass
  **locally** before a commit reaches CI (`seq-185`). **The determination is already MADE — confirm it, do
  not re-derive it.** `seq-195`: `test_actionlint_fixture.sh:52` loops `t4-engine`/`t5-embedder` while the
  real jobs are `publish-rust-t4-embedder` (`release.yml:317`) and `publish-rust-t5-engine` (`:338`) — the
  **tier names are swapped in the test**. **THE TEST IS STALE; THE WORKFLOW IS CORRECT.** There are at
  least **two independent defects** in that fixture and its loop `exit 1`s on the first (`:56`), so only
  `t1` surfaces. ⚠ `seq-185`'s stated basis ("dead since 0.8.14") is the claim `seq-195` refutes — the
  ruling stands, its rationale does not.
  🕮 **WITHDRAWN AS AN INSTRUCTION — HITL ruling `seq-211`, landed `09d47443` (2026-07-30).** Measured at
  HEAD, the **first** failure of `test_actionlint_fixture.sh` is the arm at `:54` requiring the literal
  `cargo publish --dry-run -p` **including its trailing space**, and `grep -c` for that literal in
  `release.yml` is **0** — every tier delegates to `cargo-publish-if-new.sh --dry-run`, the idempotency
  guard. The `seq-195` tier-name swap is real but **masked behind it**. Slice 40 now receives the *symptom*
  and makes the determination itself. ⛔ **One outcome is forbidden regardless: the resolution may not delete
  or bypass `cargo-publish-if-new.sh`.**
- **PHASE 0b — harden the dispatch path** (`seq-198`, §2 above). ⚠ **This phase reddens
  `scripts/tests/test_verify_release_gates.sh` arm 10**, whose text is *"dispatch+dry_run=false should not
  fail on otherwise-clean state"*. ⛔ **Do not delete arm 10** — it is the only guard against this phase
  introducing a release that quietly never publishes. Re-point it into **two** arms: without confirmation →
  exit 1; with a matching confirmation → exit 0, warning still emitted. ⚠ The confirmation input **cannot**
  be `required: true` (GitHub has no conditional-required input, and the Phase-7 dispatch passes only
  `dry_run`) — declare it optional and enforce it in `verify-release-gates.sh`.
- **PHASE 1 — manifests + the Axis-E call nobody has made.** `bash scripts/set-version.sh --workspace
  0.8.20`. ⚠ It **deliberately SKIPS `fathomdb-embedder-api`** (`set-version.sh:90`, literal comment *"Skip
  Axis E"*; documented in `dev/design/release.md`). **Axis E is at `0.6.1`; the version is undecided.**
  `verify-embedder-api-no-drift.sh` runs in `verify-release` and **fails closed** if the surface moved
  without a bump — determine whether it moved, bring a finding and a recommendation, **do not pick**. ⚠
  `set-version.sh` **never touches `src/ts/package-lock.json`** and `--check-files` cannot catch the drift;
  the root version is at `package-lock.json:3` and `:9`, both `0.8.9`, and `npm ci` runs in both workflows,
  so it bites in CI, not locally — **change only the root package's fields there.** ⚠
  `src/python/fathomdb/__init__.py:73` reads `__version__ = "0.6.0"` and **nothing gates it**. ⚠ **BLK-1**:
  `cargo package -p fathomdb-engine` exits 101 at `0.8.9` because published `fathomdb-embedder 0.8.9` lacks
  `rerank-cuda`; it is version-shaped and should clear after the bump — **prove engine's LICENSE/README
  with real tarball bytes then.** `dev/design/release.md` §"Pre-tag procedure" is the authority, including
  `cargo update --workspace` (CI omits `--locked`). Publish order is **embedder → engine**.
- **PHASE 2 — `scripts/release/local-dry-run.sh`**, named in `release.md` as *"the primary debug loop; CI
  dry-run dispatch becomes a final confirmation."* ⚠ **WF-FIX-2 reads backwards**: dependent crates cannot
  dry-run against unpublished siblings, so `cargo-publish-if-new.sh:190-193` **short-circuits them with a
  "skipped" diagnostic and exit 0**. That green skip is **by design** — neither defect nor coverage.
- **PHASE 3 — parity, the owed binding, the broken smokes.** X1 py/ts parity is the release gate. The
  **`embed_batch_cls` TS binding** is **RULED** (`plan-0.8.20.md:880-881`; `release-state` ruled id
  `embed-batch-cls-ts-parity`) — the §11 item body at `plan:941-943` was never annotated `✅ RULED` and
  still *reads* open; it is ruled. ⚠ **The post-publish smokes will fail against 0.8.20 and they run on the
  real publish:** `scripts/release/smoke/smoke-npm-package.sh` and `.../smoke-pypi-wheel.sh` both write a
  doc item with **no `source_id`** — confirmed at HEAD, `grep -c source_id` is **0** in both — which is now
  mandatory, so both bindings raise `WriteValidationError`. They are `if: inputs.dry_run != true`, so **a
  dry-run rehearsal will not exercise them**; verify another way, and keep
  `scripts/tests/test_smoke_scripts.sh` green.
- **PHASE 4 — the workspace gate, per TC-74.** **SERIALIZED**, with a **NON-BLOCKING parallel arm that
  reports without gating** (HITL 2026-07-27). DoD: `cargo clippy --workspace --all-targets` **and** `cargo
  check --workspace --all-targets`, both exit 0. **ZERO eu7 runs, any backend, any N** (F-28, closed by
  decision) — the `#[ignore]` on `eu7_real_corpus_ac` **stays**. ⚠ Cargo warns on all 7 crates *"only one
  of `license` or `license-file` is necessary"*; Slice 39 set both and `license-file` is what ships the
  file. It is a warning. **Determine whether it can be silenced without losing the shipped LICENSE, and if
  not, say so and leave it.**
- **PHASE 5 — the acceptance criteria.** Mint **AC-079** (governed-surface delta; **PRE-SIGNED** 2026-07-25
  per master F-34, batched decision **SIGNED** at `seq-157` — *pre-signing is not minting*), mint
  **AC-080**, **re-verify AC-041 GREEN**, and close R-20-AC's remaining clauses (`plan:246`): mirror
  **AC-074**, and `recovery_denylist` unchanged at **five**. ⚠ **Mint from AC-079 upward** — AC-077 is
  RESERVED (`dev/acceptance.md:1286`) and AC-078 conditionally reserved (`:1297`); *"max AC id + 1"* is
  unsafe (master F-29).
- **PHASE 6 — obligations that only fire at the cut.** **R-20-H7** re-verified GREEN — ⚠ it is already
  enforced on every landing by `preflight.sh --landing` §10 (`check-c1-conformance.sh`, 26 clauses), so you
  get it for free; say so rather than re-deriving it. **TC-42** (verify every test named in the Slice-0
  acceptance tables exists). **TC-25** — `fathomdb/tests/sdk_only_erasure.rs:36` is
  `#![cfg(not(feature = "operator"))]`, so a feature-unified run compiles it to **zero tests and reports
  success**; **R-20-E4 is unverified at the cut** — fix or report as a known gap. **TC-56's second half** —
  the `Engine::drain` change is owed in the **Slice 40 publish narrative** (Slice 39 did the changelog).
  **§11 item 2 adoption arms** — Phase-2 surface opt-in, erasure fixes ON; the ruling exists at
  `plan:877`, the unannotated item body carrying `**Gated:** Slice 40` is at `plan:929-932` — it is an
  **effecting obligation**, not a hunt for a ruling. **OOS-12** triage (§9). **The license-guard CI
  wiring** handed over by Slice 39 (`scripts/check-license-consistency.sh` + a 19-arm fixture, both green
  at HEAD — I measured `rc=0`).
- **PHASE 7 — LAND, then rehearse from `main`.** Dispatch `-f dry_run=true`. 0.8.18 built this machinery
  and exercised it **only via staging; it has never fired for real.** ⚠ `dry_run` **does not rehearse PyPI
  at all** (`release.yml:10`) — a skipped PyPI job is designed behaviour, neither failure nor coverage. ⚠
  **maturin's version in CI is UNPINNED**: `release.yml:116` uses `maturin-action` and sets no
  `maturin-version`; Slice 39's `>=1.9` floor is a **PEP-517 build-system** constraint that
  `maturin-action` bypasses. `1.7.8`/`1.8.0` cannot build the PEP-639 license form at all. **Either pin the
  action input or read the resolved version out of the run log.** ⚠ npm must publish from a **source
  build** — the corrected TSDoc only reaches consumers via a regenerated `dist/index.d.ts`. Then **STOP.**
  Closure `output.json`. **No tag. No publish.**

### 3b. Slice 40's key guardrails

- **No tag. No real publish. No registry credential.**
- **Governed surface byte-identical** unless the `_comment` finding is brought to the Steward first
  (`check-governed-surface-pin.sh` rc=0, pinned `c239908b`). ⚠ **The signed, byte-pinned allowlist
  `_comment` asserts the window refusal is `InvalidArgument`; Decision #18 made it `WriteValidation`** —
  signed prose contradicts shipped code. The coordinated re-pin belongs with the AC-079 mint.
  🕮 **CORRECTED — the re-pin is DONE, not owed** (steward `seq-208`, landed `1e0173dd`). `grep -c` for
  `AWAITING` and for `NOT SIGNED` in `src/conformance/governed-surface-allowlist.json` are **both 0**:
  `c239908b` ("TC-52 — record the signature in the allowlist; re-issue the pin") replaced all four literals
  with `SIGNED: … (steward seq-157)` ×4 **and re-issued `scripts/governed-surface-pin.json` in the same
  commit**; `c239908b` is the currently pinned commit. **Slice 40 owes one less obligation.** ⚠ The
  `InvalidArgument` vs `WriteValidation` half above is **unmeasured** — a Slice-40 *determination duty*, not
  an established contradiction.
- ⛔ **`fathomdb/src/lib.rs:139` — the `DenseReadiness` "PROPOSED / NOT SIGNED" marker — is CORRECT. Do not
  touch it.** `DenseReadiness` appears **zero** times in the allowlist and is genuinely unsigned. Slice 39
  correctly flipped only the `ReadView`/`BoundaryCrossing` marker. Changing `:139` publishes a **false
  sign-off** onto docs.rs (`seq-197`).
- **SCHEMA stays 24.** ⚠ `plan-0.8.20.md:25` still says **22** *and* miscites the const location — the real
  one is `src/rust/crates/fathomdb-schema/src/lib.rs:24`, reading **24**. Both halves of `:25` are stale.
- **No `categories`** (`seq-198`/`seq-200`) — categories plus slug validation land at 0.8.21. Keywords are
  already in.
- **No `#[non_exhaustive]`** — cancelled at `seq-182`, parked at 0.8.21 by `seq-183`.
- Dependabot advisories **publish open** (`seq-152`). ⚠ Exception: if the rehearsal fails on **npm auth**,
  **TC-78** says Dependabot **#153** re-opens as the diagnosis — report it, don't fix it.
- **`.github/` is Slice 40's exclusive territory this release** — and that is not licence to redesign.
- Do not edit `release-state-0.8.20.json`, the master, `STATUS-0.8.20.md`, or any ledger — **the Steward
  reconciles.**

## 4. Landing (TC-110)

**DETACHED HEAD / ref-to-ref push. NEVER `git checkout -B main`.**

⚠ **Prefer a fast-forward.** `preflight.sh --landing` §7 and CI `board-currency` (`ci.yml:414`) require any
`merge(<version>): Slice[- ]<N>` subject to have its short SHA **literally inside `STATUS-0.8.20.md`** —
which an orchestrator may not edit. A merge commit turns `main` RED until the Steward reconciles.

```text
git fetch origin
git rebase origin/main
bash scripts/preflight.sh --landing    # from the WORKTREE; hard-fails in the primary by design
git push origin <branch>:main
```

⚠ **The harness may DENY the push** — it did for Slice 39, flagging a direct-to-`main` push on a public
repo with AI-only review, and **the HITL pushed it**. If denied: **STOP and hand back with the exact
command.** Do not retry, do not reshape it.

⚠ **Regenerate the generated views before the final commit**: `bash scripts/check-release-state-views.sh
--write`, exit 0. There are **FIVE** generated blocks, not two.

## 5. ⚠ `seq-195` / `seq-196` / `seq-197` — three Steward errors of ONE class

All three are the same defect: **a confidently-worded factual assertion nobody measured, placed in a
commission brief.**

1. **`seq-195`** — the Slice 40 v1 brief asserted `release.yml` fails to invoke `cargo publish --dry-run`
   and pre-authorised inlining `cargo publish` into the workflow. That was **backwards** (the *test* has
   swapped tier names) and would have **deleted an idempotency guard one step from an irreversible
   three-registry publish.**
2. **`seq-196`** — the v2 brief asserted a `v*` tag is the only irreversible publish path. **False**, and it
   understated the danger of the exact command surface Phase 7 hands the agent (§2).
3. **`seq-197`** — two more: the `DenseReadiness` marker instruction would have shipped a **false signed
   status** for an unsigned type onto docs.rs; and both briefs told the agent an `AWAITING HITL SIGN-OFF`
   literal was still present in the allowlist when `grep -c` gives **zero** — an agent "verifying" it would
   have reported a green it could not have observed.

> **The rule, carried forward: state the symptom and the determination duty; do not name the fix unless you
> measured it.** Three independent reviews, and **every** critical finding was against the Steward's own
> drafts. **TC-131** — a mechanical brief fact-checker, using these four instances as test fixtures — is the
> tooling answer, **placed at 0.8.21** by `seq-198` ruling 3, not now. Until it exists, **the adversarial
> review subagent per brief is the only control**, and it has caught 100% of the class. Use it.

## 6. ⚠ WHAT I MEASURED THAT CONTRADICTS THE RECORD

Everything in this section was measured at `aea384bb` on 2026-07-30. **Reconcile it; do not assume it.**

### 6.1 Two suites red locally, not four

The Slice 40 v4 brief (base `1f85ca2a`) says four suites are red. **At HEAD, two are:**

| suite | rc |
|---|---|
| `scripts/tests/test_actionlint_fixture.sh` | **1** |
| `scripts/tests/test_check_governed_surface_pin.sh` | **1** |
| `test_seat_path_guard` · `test_steward_orient` | **0** — cleared at `f464130e` |
| `test_commission_manifest` · `test_check_board_currency` · `test_check_release_state_views` · `test_preflight_landing` | **0** |
| `test_set_version` · `test_verify_release_gates` · `test_check_license_consistency` · `test_check_ledgers` · `test_staged_ledger_sidecar` | **0** |

The **true `agent-test.sh` abort point is `test-check-governed-surface-pin` at `agent-test.sh:73`** —
confirmed by measuring every suite registered before it as `rc=0`. The pin **predicate**
(`bash scripts/check-governed-surface-pin.sh`) is **rc=0**; it is the *suite's provenance arm*, hardcoding
stale commit `427d2712`, that is red. **Do not confuse them.** ⚠ CI cannot catch that arm — it prints
`SKIP … (shallow checkout)`.

⚠ `agent-test.sh` now has **35 `run_capped` call sites / 34 distinct suite labels**. The 39.5 design doc and
`seq-204` both say **31 suites with 23 after the abort**. **Re-measure before quoting either number.**

🕮 **RESOLVED — and the DESIGN DOC IS CORRECT; the Steward's "correction" was the error** (steward `seq-209`,
landed `da203aac`). Both counts hold under different conventions: there are 35 call sites / 34 labels, but
**31 sit at column 0 and are unconditional**; the other 4 are indented inside `if`/`else` guards with
`skip_notice` branches (`:232`+`:235` are the two arms of one `test-python` conditional, `:248`
`test-ledgerwatch`, `:255` `test-ts`). 35−4 = **31**; 27−4 = **23** after the abort. ⚠ **The lesson: when two
records disagree on a count, reconcile the counting CONVENTIONS before declaring either one wrong.**

### 6.2 `test_commission_manifest` is rc=0 locally but STILL RED IN CI

`seq-204` and the board gloss both say *"`test_commission_manifest` is locally rc=1."* **At HEAD it is
rc=0** — twice measured — and `bash scripts/commission-manifest.sh --verify-all` is also rc=0. The rc=1 was
**caused by the fractional id itself** (arm 9d: *"ERR next_slice is 39.5, not an int"*) and de-laddering
cleared it. **But the CI `commission-manifest` job fails at `aea384bb` anyway.** That is a genuine
local↔CI divergence — most likely the shallow default checkout — and **explaining it is squarely Slice
39.5's CI-baseline job.**

### 6.3 CI ON `main` IS RED, AND THE "GREEN" THE RECORD CITES DOES NOT EXIST

> ### ⛔ The publish gate (i) — "every `ci.yml` job concludes `success`" — is NOT met at HEAD

🕮 **GATE (i) HAS SINCE BEEN AMENDED** (HITL ruling `seq-211`, landed `09d47443`) to *"every `ci.yml` job
**THAT EXECUTED** concludes `success`"* — `markdownlint` is skipped on every non-docs push (`ci.yml:379`,
`if: needs.changes.outputs.docs_only == 'true'`), so the heading's wording was unmeetable by construction.
Skipped is a **third state**, neither pass nor failure. **The heading's verdict stands: NOT met at HEAD.**

`seq-204` and the 39.5 design doc both state: *"the green on main at `788b74c1` was FOUR jobs on the
docs-only path with `commission-manifest`, `verify` and `security` never running."* **Measured with `gh`,
both halves are wrong:**

- Run `30546942720` at `788b74c1` concluded **`failure`**, not success.
- It ran **23 jobs**, not four. Only `markdownlint` was skipped. `commission-manifest`, `verify`,
  `security` and `rust-windows` all **executed**, and all **four FAILED**.
- The last three CI runs on `main` (`f464130e`, `2159fb03`, `788b74c1`) **all concluded `failure`.**
- The run at HEAD `aea384bb` was still in flight when this was written, with `commission-manifest`,
  `verify` and `security` **already failed**.

The measured failure causes, for the record:

| job | cause |
|---|---|
| `verify`, `security` | **7 pre-existing pyright errors** — `dense_disabled`, `dense_disabled_reason`, `vector_equivalence_refusal_count` unknown on `Engine`/`OpenReport` (`src/python/fathomdb/engine.py:548/552/556/669/670`); `graph.py:153` `fathomdb._fathomdb.IdSpace` not assignable to `fathomdb.types.IdSpace`; `test_vector_equivalence_probe.py:32` no parameter named `reason`. Both jobs die in **"Bootstrap dev tooling"**, before their real work. |
| `commission-manifest` | at `788b74c1`: **arm 9d**, `next_slice is 39.5, not an int` — now fixed locally, **still red in CI at HEAD** (§6.2). |
| `rust-windows` | `-p fathomdb-engine --test tc57_worker_commit_pressure` fails. Was named nowhere in the ledger, plans or briefs when found. **⚠ HITL RULED IT INTO SLICE 39.5's CI BASELINE REPORT (`seq-206`) — REPORT IT, do not triage or fix it.** Disposition of everything the baseline surfaces is ONE decision taken once the full list exists. It is still a genuine red on the publish path: gate (i) needs every `ci.yml` job green, so it must eventually pass or be explicitly waived. |

**Consequence:** the record's framing — "CI green can be vacuous, and 39.5 will establish what a real clean
CI looks like" — understates the position. **CI is not vacuously green; it is loudly red, on four jobs,
three of which are pre-existing and unrelated to any pending unit.** 39.5's job is unchanged in shape but
larger in fact.

### 6.4 `plan-0.8.20.md` §9's un-generated prose is STALE — and it is the `{{MANDATE}}` anchor

🕮 **BOTH CLAIMS IN THIS SECTION ARE RESOLVED** (steward `seq-205`, landed `190c8b8a`). `plan-0.8.20.md` §9
was de-staled: it now correctly names Slice 39 as **LANDED** and names **both** cross-cutting units
(`SLICE-ID-HARDENING` at `plan-0.8.20.md:492`, "Slice 39.5" / `R-20-HARNESS` at `:497`). **The `{{MANDATE}}`
anchor is safe.** Read the rest of this section as the historical record of the defect, not as a live warning.

The **generated** pointer inside the markers is correct: *"IMMEDIATE NEXT: Slice 40 (`R-20-PUB`) …
Remaining ladder: 40."* But the hand-written prose **immediately below it, outside the markers**, still
reads **"Slice 39 (`R-20-DOC`) is the immediate next"** and goes on to describe Slice 39's deliverables as
future work. Slice 39 landed at `91db34d8`.

⚠ `scripts/commission-manifest.sh` resolves **`## 9. Immediate next slice`** as a brief's `{{MANDATE}}`
anchor, and its CHECK 2 verifies **that the heading exists**, not that the prose under it is current. So
**Slice 40's brief would inherit Slice 39's mandate prose with the manifest's authority behind it** — the
exact TC-89 failure, recurring in the half that generation deliberately left hand-written. `--verify-all`
is rc=0 and cannot see this. **De-stale §9 before commissioning anything.**

⚠ Also: **`plan-0.8.20.md` contains no mention of `39.5`, `R-20-HARNESS` or `SLICE-ID-HARDENING`.** The
two cross-cutting units exist only in `release-state-0.8.20.json` `ladder_order`, the board's generated
`status-unblocks` gloss, the design doc and the ledger.
🕮 **FALSE at HEAD** — `seq-205` (`190c8b8a`) named both units into `plan-0.8.20.md` §9 (`:492`, `:497`);
see the annotation at the head of §6.4.

### 6.5 `decisions.unruled` has THREE entries, not two

`platform-publish-schedule` is still in `unruled`, self-described as *"RULED at `seq-203`; retained as a
tracking entry … kept in the list only until the 0.8.21 plan exists to carry it."* The board's generated
`status-live-open-count` correctly renders **THREE**. So the **live open set is two decisions plus one
parked tracking row** — say it that way, and remove the row when the 0.8.21 plan lands. Anyone reporting
"two unruled" is reading prose, not the state file.

## 7. Platform + publish schedule (`seq-202`, `seq-203`)

- **PUBLISH NOW HAS TWO INDEPENDENT GATES, NEITHER SUFFICIENT ALONE** (`seq-202` ruling 3): **(i)** every
  `ci.yml` job concludes `success` on the landed commit, **AND (ii)** explicit HITL approval. The HITL
  noted (i) may be relaxed later but it stands while the line has gone this long unpublished. **No conflict
  with `seq-152`** — the `ci.yml` `security` job runs `agent-security.sh` (AC-036/037/038/050a/050c), not a
  Dependabot advisory scan.
  🕮 **GATE (i) AMENDED — HITL ruling `seq-211`, landed `09d47443` (2026-07-30): it now reads "every
  `ci.yml` job THAT EXECUTED concludes `success`."** `markdownlint` is
  `if: needs.changes.outputs.docs_only == 'true'` (`ci.yml:379`), so it is **skipped on every non-docs
  push** — both measured full runs show 23 jobs, exactly 1 skipped, and the skip is `markdownlint`; the
  original wording was **unmeetable by construction**. A skipped job is a distinct **third state**, neither
  pass nor failure. **This narrows nothing: no executing job may fail.**
- **0.8.20** publishes **x86_64-linux only**, npm dist-tag **`next`** (`release.yml:28`). CI already *tests*
  Windows and macOS; what is linux-only is the **publish/wheel** matrix at `release.yml:91-105`, where
  aarch64-linux, both darwin targets and win32-x64 sit commented out.
- **0.8.21** wires and greens **aarch64-darwin → x86_64-darwin → win32-x64** and **publishes NOTHING** (odd
  micro, OOB label-only). ⚠ **napi 2→3 was PULLED FORWARD from 0.8.22 to 0.8.21** (`seq-203`) so the matrix
  is validated on the binding version that actually ships. Ordering is by increasing risk: Apple Silicon
  first (dominant developer machine for the named consumers; pyo3-mac was fixed at 0.8.8/0.8.9), Windows
  late (0.8.9 was the first green Windows in this line).
- **0.8.22** publishes all four and **flips the npm dist-tag to `latest`**. Retains rusqlite 0.31→0.40 with
  `sqlite-vec`, the `sqlite-vec` 0.1.9 bump (`seq-151`), the six Dependabot advisories (`seq-152`), and
  TC-98 parts (a) and (c).
- **aarch64-linux at 0.8.24+** — needs cross-compilation or ARM runners; lowest urgency for a local-first
  library.
- **0.8.21 now also carries:** `#[non_exhaustive]` (`seq-183`), crates.io categories + slug validation
  (`seq-198`/`seq-200`), **TC-131** (brief fact-checker) and **TC-132** (transcript-hygiene fix).

## 8. Open HITL decisions

Read `release-state-0.8.20.json` `decisions.unruled` — **not** board §4 items 1-22, which are the historical
queue.

| # | Decision | State |
|---|---|---|
| 1 | **PUBLISH** the breaking pair (manifests `0.8.9 → 0.8.20`) | **`halts_run: true`.** Two gates, §7. HITL *prefers* publish-after-40 and **explicitly deferred** — *"lets see where we are then"* (`seq-135`). **Not a ruling, and not authorization to bump a manifest, cut a tag, or publish.** |
| 2 | **npm dist-tag** for the 0.8.20 publish | **`halts_run: false`.** Defaults to `next`. `release.yml:21-26` states the label is an HITL confirmation, because publishing partial coverage under `latest` serves an incomplete matrix to every consumer. A **rider on the publish gate**, decided *with* it. **Surface it; do not choose.** |
| 3 | `platform-publish-schedule` | **Already RULED at `seq-203`** (§7). A tracking row only — see §6.5. |

**Everything else is RULED** and lives in `decisions.ruled`: the batched governed-surface delta (**SIGNED**,
`seq-157`), TC-98, TC-100 placement, `sqlite-vec` 0.1.9 (**HOLD at `=0.1.7` for 0.8.20**), TC-93 (**publish
with the six advisories OPEN**), the MIT license ruling (`seq-193`), `embed-batch-cls-ts-parity`, and the
three `seq-198` rulings. **Cite them; never re-open them.**

## 9. Carried debt and untriaged items

- **OOS-12** — `slice15e_prekn_filterable::undeclared_after_concurrent_drop_is_typed_invalidfilter_not_storage_race`,
  a deliberate race test asserting a typed `InvalidFilter` after a concurrent drop and sometimes getting
  `Ok`. **Untriaged harness-vs-real, on the registry path this release publishes.** ⚠ **Known
  contradiction:** the superseded `STEWARD-SESSION-HANDOFF-2026-07-26-A.md:7` calls it *"resolved to a
  pre-existing `slice15e` flake of the TC-72 family."* The **later** record — `seq-116` and **TC-74** —
  says it has **NOT** been classified: *"do not file it under harness artifact until someone has actually
  looked."* **The later record governs.** Slice 39 independently saw it fail ~1 run in 4 with no code
  change. Slice 40 owns the triage.
- **`rust-windows` / `tc57_worker_commit_pressure`** — **new**, §6.3. **OWNED: reported by Slice 39.5's CI baseline (`seq-206`)**, not triaged separately.
- **The 7 pyright errors** blocking `verify` and `security` in CI — pre-existing. Same disposition: reported by Slice 39.5's CI baseline, fixed by nobody yet, and blocking publish gate (i) until they are.
  Gate-relevant because of `seq-202` (as amended by `seq-211`): 39.5 reports them; **disposition is a
  separate decision.**
- **Axis-E version undecided** — `fathomdb-embedder-api` at `0.6.1`; owed at Slice 40 Phase 1.
- **TC-25** — `sdk_only_erasure.rs` compiles to zero tests under a feature-unified run; **R-20-E4 is
  unverified at the cut.**
- **TC-16** — open; Slice 40 Phase 0.
- **TC-129** — `scripts/tests/test_check_design_refs.sh` (**32** arms) guards the *active pre-commit gate*
  (`.git/hooks/pre-commit:45-48`) but has no automatic runner. `seq-172`'s CI prohibition was scoped to the
  **sbom-survey** suite and does not reach it. ⚠ The "after Phase 0" ordering is a Steward **inference, not
  a ruling** — `seq-185` does not mention TC-129.
- **TC-130** — `dev/plans/runs/**` is **excluded from markdownlint** (`.markdownlint-cli2.jsonc:45`), so a
  green `agent-lint-md` proves **nothing** for `STATUS-0.8.20.md` or for any hand-off, **including this
  one**.
- **TC-131 / TC-132** — both **placed at 0.8.21** (§7). Until then, §5's discipline and §10 trap 1 are the
  only controls.
- **TC-101 / TC-80** — `OPP-12-C1-converged-contract.md` records `status: UNREVIEWED` despite being the
  ratified contract. Fixable only by a coordinated **pin re-issue**; take the two together.
- **The allowlist `_comment` correction** — still owed; the `InvalidArgument`/`WriteValidation`
  contradiction (§3b) belongs with Slice 40's AC-079 mint.
  🕮 **CORRECTED — NOT owed: DONE at `c239908b`** (steward `seq-208`, landed `1e0173dd`). All four
  `AWAITING`/`NOT SIGNED` literals were replaced with `SIGNED: … (steward seq-157)` ×4 and the pin was
  re-issued in the same commit; `grep -c` for both literals is now **0**. Only the **unmeasured**
  `InvalidArgument`/`WriteValidation` question survives, as a Slice-40 determination duty.
- **Eight "Requirement traceability" notes** remain in `dev/design/**` from before the `design_refs` fix
  (`d30ef52f`). Removing them is owed. **Do not write new ones.**
- **Two stale Slice-39 worktrees** — `fathomdb-slice39-changelog`, `fathomdb-slice39-docs`. Slice 39 has
  landed; confirm nothing is unmerged, then remove.
  🕮 **DONE** in the `seq-207` reconcile (landed `80f83bf7`). `git cherry main <branch>` marked **all six
  commits `-`** — every patch is already in `main`, they were pre-rebase copies — and the single untracked
  file in the docs worktree was **byte-identical** to its landed counterpart. **Both worktrees removed; both
  branches preserved** (`0.8.20-slice-39-changelog`, `0.8.20-slice-39-docs`).
- **17 open Dependabot PRs** — publish OPEN (`seq-152`), with the TC-78 npm-auth exception (§3b).
- **Slice 34 (`#[non_exhaustive]`) is CANCELLED** from 0.8.20 (`seq-182`) and **PARKED at 0.8.21**
  (`seq-183`). It is **not in the ladder** — route no work through it.

## 10. Traps — read before trusting any green

1. **TC-132 — transcript hygiene is VACUOUS at pre-commit.** On 2026-07-30 a raw transcript with **36 lines
   of out-of-repo directory listing** was staged and committed with **no hygiene arm firing at all**;
   `check-transcript-hygiene.sh` exits **1** on that same file when run directly, and returns **0 on an
   untracked file**. **Run the hygiene check EXPLICITLY on every transcript before committing it. Never
   rely on the hook.**
2. ⚠ **Never `git add -A`.** That is exactly how the TC-132 incident happened — it swept untracked
   transcripts from a sibling checkout into a commit. **Stage explicit paths.**
3. **`scripts/agent-test.sh`'s aggregate exit is VACUOUS (TC-16)** — it aborts at `:73`
   (`test-check-governed-surface-pin`) and never reaches the Rust, Python or TS steps. **Run suites
   individually and quote each rc.** Ending this is 39.5's whole purpose; **do not assume it is fixed —
   prove it.**
4. **TC-130 — `dev/plans/runs/**` is excluded from markdownlint.** Green proves nothing there.
5. **Capture `rc=$?` on the very next line**, before any pipe or command substitution
   (`seq-108`/`seq-109`). A `$(basename …)` in between reports *basename's* status.
6. **`cargo test --workspace` is not a stable signal (TC-72)** — ~1 run in 3 fails on plain `main`.
7. **`ac_029` is a WALL-CLOCK RATIO assertion (TC-97).** **Never run the engine suite alongside a codex
   reviewer.**
8. **`src/python`'s `pythonpath = ["."]` shadows an installed wheel (TC-97)** — kills the X1 Python route
   at *collection* with a misleading circular-import error. Use `-o pythonpath=` from a neutral cwd.
9. **Six engine targets exit 101 WITHOUT `--features operator` (TC-97).** `--all-features` clippy is
   impossible here (`objc2`).
10. **`agent-lint-md.sh` HARD-EXITS 1 in a worktree** without `node_modules` —
    `ln -s /home/coreyt/projects/fathomdb/node_modules node_modules`. (The hard-fail is the TC-37 **fix**,
    not the defect.)
11. **TC-83, WIDENED:** *any* full-command-line matcher — `pgrep -f`, `pkill -f`, `ps | grep` — matches
    **its own** invocation. One orchestrator killed its own shell (exit 144). Match the binary
    (`pgrep -x codex`) or exclude self by pid.
12. **Never triage a codex review by grepping `[P1]`/`[P2]` markers (TC-87)** — `lib.rs` alone carries 21
    and 101 such markers before any review runs. Read the verdict blocks.
13. **`git commit -F` with a quoted heredoc — never `-m` with backticks (TC-53).** It has corrupted the
    steward ledger twice. `ledgerwrite` now refuses shell residue; **never hand-append to a ledger.**
14. **`ledgerwrite` writes a `.seq` sidecar (TC-88).** Stage it **with** the `.jsonl`.
15. **TC-128 — a `git init` with `GIT_DIR` unscrubbed re-initialises whatever `GIT_DIR` names.** It set
    `core.bare = true` on the **PRIMARY** repository **twice** on 2026-07-29. Scrub it.
16. **TC-110 — a landing worktree running `git checkout -B main` CORRUPTS the primary checkout index**, and
    `preflight --landing` does not catch it. Land from a **detached HEAD / ref-to-ref push** (§4).
17. **A clean rebase is not a correct rebase** — re-run every gate *after*. DOC-HYGIENE-3 rebased with zero
    conflicts and then failed six gates.
18. **A vacuous check passes loudly.** A canary in this release piped `find -printf` into `md5sum` on both
    sides — both hashed nothing and compared equal. **Any check added must be proven non-vacuous with a
    control that makes it fail.** *(Exception: WF-FIX-2's skip is a legitimate green — §3a Phase 2.)*
19. **`OPP-12-C1-converged-contract.md` is BYTE-PINNED** (sha256 *and* git blob sha1). **Cite it; never edit
    it.**
20. **Never hand-edit inside a `GENERATED` marker.** `scripts/check-release-state-views.sh --write`
    regenerates; the bare script must stay rc=0. ⚠ It renders a region inside
    **`STEWARD-SESSION-HANDOFF-2026-07-24-A.md`** — **that old hand-off is a LIVE RENDER TARGET
    (`generated_views` id `handoff-next-step`). Do not delete or restructure it.**
21. **Markdown lint traps:** a banner **before** the H1 trips MD041; a line beginning with a plus sign
    followed by a space parses as a list and cascades MD004; abutting blockquotes trip MD028 — separate
    with `---`; heading levels must increment by one (`###` directly under `#` trips MD001); a heading
    ending in a full stop trips MD026; **and a code span whose content starts or ends with a space trips
    MD038** — which is how the previous hand-off's own description of the first trap became an error.
    ⚠ **None of these fire in `agent-lint-md.sh` for this directory (TC-130).** To actually lint a
    hand-off, copy it somewhere outside `dev/plans/runs/` and run `npx markdownlint-cli2` on the copy.
22. ⚠ **TC-121 (open)** — `seat-path-guard.sh` reads prose as a write target, false-positived twice during
    Slice 33, and reaches **commit messages**. Steward and orchestrator work is heavy prose. Expect it; **do
    not weaken the guard to silence it.**

## 11. How to commission, and the cadence that will bite you

**The orchestrator is the mechanism** (`/orchestrate`, or a Steward-spawned `orchestrator` agent when the
Steward holds the loop) — **never `/goal`.** The shape that has worked all release:

1. **Regenerate the brief** — `scripts/commission-manifest.sh 0.8.20 <slice>`. It hard-fails on a dead
   citation or an empty design tier, so a green manifest is real evidence. ⚠ **It cannot generate a brief
   for either cross-cutting unit** — those are hand-written (§1b).
2. **Re-verify the `{{MANDATE}}` anchor (plan §9) before briefing.** ~~⚠ **It is stale right now** — §6.4.~~
   🕮 **NO LONGER STALE** — plan §9 was rewritten at `seq-205` (`190c8b8a`) and correctly records Slice 39 as
   LANDED plus both cross-cutting units. **The step itself stands**: re-verify it every time, because the
   manifest copies the whole section and it went stale at three consecutive commissions when nobody did.
3. **Run an adversarial review subagent over every brief before commissioning.** Until TC-131 exists at
   0.8.21 this is the only control on the §5 defect class, and it has caught 100% of it.
4. **Record the commission in the ledger BEFORE it starts.**
5. **Verify the result from git** — ancestry, byte-level diffs, gates re-run — **never from the report.**

**Fix-round cap (TC-75 / TC-82 / TC-84):** 3 rounds on the same finding · **mandatory Steward check-in at
6** · 7-10 only where the Steward rules every round productive (a new and distinct defect each time) ·
beyond 10 an HITL halt. The cap keys on **round productivity**, not on which directory a slice touches. **A
mid-flight `SendMessage` steer is not a round** — warm-resume rather than cold-respawning. **When you rule a
round-6 check-in, pre-commit the fallback** — Slice 23's *"round 7 is authorized for exactly these two
things; if it does not close, take `#[ignore]` yourself; round 8 is forbidden"* closed on round 7.

⚠ **Across Slices 32 and 33, five of six codex fix rounds were defects in the VERIFICATION APPARATUS, not
the function under test** — a criterion graded against a helper while the real boundary went ungraded.
**Grade through the real entry point.**

**A commissioned orchestrator returns ONCE and does NOT notify you on stall.** One stalled **36 hours**
unnoticed in this program; Slice 32's went idle **four** times and was only caught by polling. **Poll from
git** — branch tips, worktree mtimes, task-output mtimes. Put an explicit **ANTI-STALL** clause in every
brief.

## 12. Standing rules that outlive this hand-off

- **Trust git, not narration.** Verify every "closed / landed / green" against the diff and real exit codes.
  §6 exists because the *ledger itself* carried an unmeasured CI claim.
- **The mandate rule.** Direction and record changes — a release slot, moving an item between releases,
  altering an I-edge, re-sequencing — are **always** explicit HITL. Never inside an implied mandate.
- **You cannot launder authority downward.** A message to an orchestrator is peer-level, not the HITL's.
- **Escalate a pin trip, never clear it.** Widening `governed-surface-allowlist.json` or re-pinning to make
  a gate pass is forbidden at every level (`seq-113`).
- **Push scope is fathomdb-only.** Never push memex without a specific per-push HITL directive each time.
- **Two-tier numbering.** `x.y.z` real/publishable · `x.y.z.p` pico label-only · **`13` forbidden** ·
  publish is a separate explicit HITL gate. ⚠ **`seq-202`: the fractional `39.5` was a ONE-OFF and does NOT
  become common numbering practice** — and `seq-204` then removed even that one from the ladder.
- **Verify the branch (`git rev-parse --abbrev-ref HEAD`) before EVERY commit or push.** The tree is shared.
- **Never open a ledger by hand** — `ledgerwrite` to append, `ledgerwatch` to read. Full unfiltered tail
  read before every append; verify the seq is contiguous.
- **Only the Steward edits `release-state-0.8.20.json`, the master, `STATUS-0.8.20.md` and the ledgers.**
  Every brief says so; hold the line.
- **Guardrail failures are fixed in the repo, not in a note.** When something slips a guard, fix the
  hook / lint / CI so it cannot recur for anyone.
- **Delegate; don't hand-do.** Commission subagents for mechanical, edit and investigative work; spend
  Steward context on judgement and on verifying from git.
- **Surface your own errors in the open.** The prior sessions recorded eleven of their own, including
  `seq-195`/`196`/`197`. On DOC-HYGIENE-3 the reviewing layers caught more **Steward** errors than Steward
  review caught implementation errors. **That ratio is the finding, and the transparency is the standard,
  not an anomaly.**
