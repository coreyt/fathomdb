---
status: ACTIVE
---

# FathomDB — Steward Session Hand-off (2026-07-29-B)

> **Boot:** run **`/steward`**, do its §3 cold-start reading (**start with `scripts/steward-orient.sh`**),
> then read THIS doc, return a short orientation, and **WAIT for the HITL** before mutating anything.
> Supersedes `STEWARD-SESSION-HANDOFF-2026-07-29-A.md`, which was written before Slices 31/32/33 and
> DOC-HYGIENE-3 landed and is stale in almost every number it quotes.

## 0. State — do NOT copy numbers out of here

`scripts/steward-orient.sh` prints branch / HEAD / worktrees / landed slices + SHAs / SCHEMA / ledger tail /
todos fold / next action, all from the **single writer** `dev/plans/release-state-0.8.20.json`. **Run it and
trust it over this section**, which is a snapshot and will drift.

| | at hand-off |
|---|---|
| `main` | **`d9c5024d`** — *"MIT license ruling; Slice 39 R-20-DOC minted ahead of 40; board de-staled"* |
| Working tree | clean apart from this hand-off |
| Steward ledger tip | **`seq-195`** (195 entries) |
| Todos ledger tip | **`TC-129`** (120 ids folded; 88 open) |
| 0.8.20 ladder | 0 · 5 · 10 · 15 · 20 · 21 · 22 · 23 · 25 · 30 · **31 · 32 · 33** all LANDED, plus non-ladder **DOC-HYGIENE-3**. Remaining: **39 → 40**. |
| SCHEMA | **24** |
| Unruled HITL decisions | **TWO** — `publish` (`halts_run: true`) and `npm-dist-tag` (does not halt) |
| Worktrees | primary · `0.5.1-memex-build` (Memex vehicle — **leave alone**) · `refactor-background-check` (unrelated). **No orchestration worktrees outstanding.** |
| Open PRs | 17, all Dependabot. They publish OPEN (`seq-152`). |

**0.8.20 is a breaking pair and the first REAL publish since `v0.8.9`** (tagged 2026-06-29) — to crates.io,
PyPI and npm. Everything below is downstream of that fact.

## 1. Immediate next action — commission Slice 39, then Slice 40

**Nothing is in flight. The ladder is between slices.** Two slices remain and neither has been commissioned.

```text
39 (R-20-DOC)  →  40 (R-20-PUB)  →  [separate hard HITL PUBLISH gate]
```

**Commission with `scripts/commission-manifest.sh 0.8.20 <slice>`.** Draft briefs for both already exist —
their substance is captured in §2 and §3 below, because the drafts themselves live only in a session
scratchpad and will be gone.

**The ordering 39-before-40 is mechanical, not editorial.** `scripts/verify-release-gates.sh` **check 4
hard-fails** unless `CHANGELOG.md` carries a heading matching the version being released, and there is no
`## 0.8.20` section. Slice 40's `workflow_dispatch` rehearsal therefore *cannot pass* until 39 lands.

- **39 — `R-20-DOC`**, publish-facing documentation. Minted by the Steward at `seq-194` under an HITL
  directive to get project / `dev/` / `docs/` documentation **not stale** before the publish. Design of
  record: **`dev/design/0.8.20-slice-39-publish-facing-documentation.md`** (already on `main`, cited from the
  ladder entry's `design_refs`). Depends on 30.
- **40 — `R-20-PUB`**, publish preparation. **STOPS before any tag.** Depends on 5 and 30.

## 2. ⛔ THE ONE THING THAT MUST NOT HAPPEN

> ### `.github/workflows/release.yml` triggers on `on: push: tags: - "v*"`
>
> **A pushed `v*` tag auto-fires the REAL, irreversible publish to all three registries.** No confirmation,
> no undo — crates.io versions are immutable. **Nothing else fires a publish.**

---

> **Landing the version bump on `main` does NOT publish.** Only a tag does. So do not balk at pushing to
> `main` at Slice 40, and never improvise a tag to "complete" the release. Rehearsal is via
> **`workflow_dispatch` with `dry_run: true`**, never via a tag.

## 3. Slice 39 (`R-20-DOC`) — the brief's substance

Read the design doc for the full argument. The brief adds these, and they are the load-bearing parts.

**(a) The MIT license ruling — HITL, `seq-193`.** The repo-root `LICENSE` (MIT, landed `ff8639e4`) is
authoritative. **Four publishable manifests declare Apache-2.0 and are WRONG** — verified at HEAD:

| file | current |
|---|---|
| `Cargo.toml:30` | `license = "Apache-2.0"` |
| `src/python/pyproject.toml:11` | `license = { text = "Apache-2.0" }` |
| `src/ts/package.json:5` | `"license": "Apache-2.0"` |
| `src/ts/npm/linux-x64-gnu/package.json:5` | `"license": "Apache-2.0"` |

`deny.toml:9` already allows both, so nothing else gates the correction.

**The harder half: no artifact ships a license file at all** — established by *running* `cargo package
--list`, `npm pack --dry-run`, and reading the built wheel's `METADATA` (which reads `License: Apache-2.0`),
not by inspection. Ten published units need one: 7 crates, the wheel, both npm packages. **Confirm the fix
the same way it was found — run the packaging commands and read the file lists.**

**Build the guard, not just the fix** (standing policy: fix the tooling so it cannot recur). A
`scripts/check-license-consistency.sh`-shaped check asserting manifest-vs-LICENSE agreement *and* presence
in each packaged artifact, **proven non-vacuous with a control** (break one field, show `rc=1`, restore).
⚠ Wiring it into CI needs `.github/`, which is **Slice 40's exclusive territory** — ship the script,
register it in `scripts/agent-test.sh`, and hand the CI wiring to 40.

**(b) The CHANGELOG.** Today `## [Unreleased]` says *"rolls into the next cut after 0.8.9"* then jumps to
`## 0.8.9`; there are no 0.8.10–0.8.19 sections. Named, verified defects: `SearchHit.id u64 → IdSpace` (the
largest consumer-visible break in the pair) is **absent entirely**; `CHANGELOG.md:160` claims "Schema version
20 → 21" when the real span is **15 → 24**; **migration step 23 destroys all edge data**
(`fathomdb-schema/src/lib.rs:751`, plus a `DELETE FROM search_index_edges`) with no warning; **TC-56** (a
shipped `Engine::drain` behaviour change the ledger says the 0.8.20 changelog must reflect); and
`read.crossed_boundary_since` has no entry. **Derive from git, never invent** — a wrong CHANGELOG is worse
than a thin one, and defect 2 is exactly that failure already in the file.

**(c) Registry surfaces.** Zero READMEs on 7 crates and npm (no `readme` key anywhere). The PyPI landing page
is `src/python/README.md` — a 12-line internal stub opening *"This directory is the Python package root"* and
linking two files that do not ship. `fathomdb-engine/src/lib.rs:1` has no crate-level `//!` doc, so its
docs.rs landing page is blank. No `keywords`/`categories` on any crate.

**(d) `docs/` is two releases behind.** Install pages instruct people to install **0.6.0**; **8 of 8**
node-write snippets omit the now-mandatory `source_id`; **TC-36** is live (`python-api.md:380` /
`typescript-api.md:348` still type `SearchHit.id` as `int`/`number` "write_cursor" — and both files *were*
edited for 0.8.20 at `9392dbc5` and the line four rows up was skipped, so it was passed over, not merely
untouched); `docs/reference/errors.md` is short by **6 shipped error classes** including the one
`erase_source` raises. ⚠ The docs site is **not deployed** — `mkdocs build --strict` runs in CI but nothing
publishes it. That is not permission to skip; these are the pages every registry link lands on. If you touch
`docs/`, `mkdocs build --strict` must exit 0 (X2).

**(e) `dev/interfaces/*.md` — TC-39, firing.** All five carry `target_release: 0.6.0`. `transition`, `purge`,
`erase_source` and `crossed_boundary_since` appear nowhere in `python.md`/`typescript.md`. `dev/**` ships to
no registry, so this is internal contract debt, not a publish blocker — **do it anyway**, it is the cheapest
it will ever be.

**Slice 39 guardrails:** no tag, no publish, no registry credential. **No version bump** — manifests stay
`0.8.9`; 39 changes `license`, never `version`. ⚠ `src/python/fathomdb/__init__.py:73` still reads
`__version__ = "0.6.0"` — **note it for 40, do not fix it in 39.** Nothing in `.github/`. Governed surface
byte-identical (`check-governed-surface-pin.sh` rc=0) and **do not edit
`src/conformance/governed-surface-allowlist.json`** — its `_comment` still reads `AWAITING HITL SIGN-OFF, NOT
SIGNED` ×4 and that literal is load-bearing for the byte pin; its correction is deliberately deferred. SCHEMA
stays 24. Zero eu7 runs. No `#[non_exhaustive]` work. **No AC minted, no `R-20-xx`-numbered acceptance
gate** — gate by test name and by the §3(a) guard script.

## 4. Slice 40 (`R-20-PUB`) — the brief's substance

The draft is **v2** and was **under independent adversarial review** when this hand-off was written. Treat
the phase structure as settled and the wording as re-reviewable. **The phase order is engineered and
load-bearing.**

**Prerequisite: Slice 39 must have LANDED.** Verify from git.

- **PHASE 0 — TC-16 / F-30: DETERMINE which side is wrong, then fix that side.** `seq-118` ordered this
  first; `seq-185` added that the guard must pass **locally** before any commit reaches GitHub CI. The
  symptom only: `bash scripts/tests/test_actionlint_fixture.sh` → `rc=1`, failing on
  `publish-rust-t1-embedder-api dry-run branch is not cargo publish --dry-run -p`. The assertion is
  fail-fast, so **only the first failing tier is named**. Master **F-30**'s "re-point the assertion at the
  delegation" is **a hypothesis to test, not an instruction**. See §5 — the previous draft asserted a
  diagnosis here and it was backwards. ⚠ **Slice 40 is the FIRST and ONLY slice this release permitted to
  touch `.github/`** — that is not a licence to redesign the workflow.
- **PHASE 1 — manifests + the Axis-E call nobody has made.** Bump Axis-W `0.8.9 → 0.8.20` via
  **`bash scripts/set-version.sh --workspace 0.8.20`** (a bare version argument dies with "no mode given").
  ⚠ **`--workspace` DELIBERATELY SKIPS `fathomdb-embedder-api`** (`scripts/set-version.sh:91`, literal
  comment *"Skip Axis E"*; documented at `dev/design/release.md:104`). **Axis E is at `0.6.1` — verified.**
  `scripts/release/verify-embedder-api-no-drift.sh` runs inside `verify-release` and **fails closed** if the
  surface moved without a bump. The orchestrator determines whether it moved and **brings a recommendation;
  the Steward/HITL picks the version.** `dev/design/release.md` §"Pre-tag procedure" is the authority for
  this phase, including `cargo update --workspace` (CI omits `--locked`).
- **PHASE 2 — `scripts/release/local-dry-run.sh`**, named in `release.md` step 4 as *"the primary debug
  loop"*. ⚠ **`WF-FIX-2`: dry-run for dependent crates CANNOT succeed**, in CI or locally. Expected output,
  not a defect — do not start "fixing" `release.yml` when you hit it.
- **PHASE 3 — parity + the owed binding.** X1 py/ts parity; **`embed_batch_cls` TS binding**, HITL-ruled
  2026-07-24 option (a) — Py-only since 0.8.14 and the first publish since 0.8.9 must not ship the asymmetry.
- **PHASE 4 — the workspace gate, per TC-74.** **SERIALIZED**, with a **non-blocking parallel arm that
  reports without gating**. Full-workspace `cargo clippy --workspace --all-targets` **and** `cargo check
  --workspace --all-targets`, both exit 0. **ZERO eu7 runs** (F-28).
- **PHASE 5 — the acceptance criteria.** Mint **AC-079** (PRE-SIGNED 2026-07-25, master F-34; the batched
  surface decision was SIGNED at `seq-157`) — **pre-signing is not minting**. Mint **AC-080**; re-verify
  **AC-041** GREEN — `release-state-0.8.20.json` requires `re_verified_green: ["AC-041","AC-080"]` and
  neither AC exists in `dev/acceptance.md` today. R-20-AC's remaining clauses (`plan-0.8.20.md:246`): mirror
  AC-074 and re-verify `recovery_denylist` unchanged at five. ⚠ **Mint from AC-079 upward** — AC-077 is a
  RESERVED heading and AC-078 conditionally reserved; *"max AC id + 1"* already collided once (master F-29).
- **PHASE 6 — obligations that only fire at the cut.** Re-verify **R-20-H7 GREEN at the cut** (Slice 30
  having landed is not the same fact); **TC-42** (mechanically verify every test named in the Slice-0
  acceptance tables exists); **§11 item 2 adoption arms** (an effecting obligation, not a record); **TC-32**
  (confirm and close — the survey found `docs/operations/erasure.md` already honours it).
- **PHASE 7 — LAND, then rehearse from `main`.** The rehearsal comes **last**: `verify-release-gates.sh`
  check 3 requires HEAD reachable from `main` (0.8.20 is GA, so the RC skip at `:102` does not apply) and
  check 4 requires the CHANGELOG heading. ⚠ **`dry_run` does not rehearse PyPI at all** — `publish-pypi`
  (`:413`) and `post-publish-smoke` (`:529`) are both `if: inputs.dry_run != true`. **A skipped PyPI job is
  designed behaviour — neither a failure nor coverage.** Then **STOP.** No tag, no publish.

**Also in scope at 40:** **TC-129** (the 30-arm `scripts/tests/test_check_design_refs.sh`, guarding the
*active pre-commit gate*, has no automatic runner; `seq-172`'s CI prohibition was scoped to the **sbom-survey**
suite and does not reach this one, so wiring it is permitted — after Phase 0, per `seq-185`); **OOS-12**
(triage harness-vs-real, see §7); **npm dist-tag** (surface it, do not choose — §6).

## 5. ⚠ `seq-195` — the Steward error to not repeat

**The Slice 40 draft brief asserted a diagnosis of the TC-16/F-30 failure that was BACKWARDS.** It claimed
`release.yml` fails to invoke `cargo publish --dry-run` and called the guard dead since 0.8.14.

What is actually true, verified: `scripts/tests/test_actionlint_fixture.sh:52` loops tiers `t4-engine` and
`t5-embedder`, but the real job names are `publish-rust-t4-embedder` and `publish-rust-t5-engine` — **the
tier names are SWAPPED in the test**, so its `awk` block extraction finds nothing. Meanwhile `release.yml:258`
delegates to `scripts/release/cargo-publish-if-new.sh`, whose header states it always forwards to
`cargo publish --dry-run -p`. **The test is stale; the workflow is correct.**

The brief had pre-authorised rewriting `release.yml` to inline `cargo publish` — **which would have deleted
the already-published idempotency guard on a workflow that auto-fires an irreversible three-registry publish
on tag push.** `seq-118` exists precisely to require establishing which side is wrong *first*.

> **This is the third instance of the same class.** The rule, carried forward: **state the symptom and the
> determination duty; do not name the fix unless you measured it.** Brief v2 now withholds the diagnosis
> deliberately and says why it is withholding it. **Keep that withholding when you commission.**

## 6. Open HITL decisions — exactly TWO

Read `dev/plans/release-state-0.8.20.json` `decisions.unruled`. Board §4 items 1-22 are the **historical**
queue and are explicitly not the live set.

| # | Decision | State |
|---|---|---|
| 1 | **PUBLISH** the breaking pair (manifests `0.8.9 → 0.8.20`) | **`halts_run: true`.** HITL *prefers* publish-after-40 and **explicitly deferred** — *"lets see where we are then"* (`seq-135`). **Not a ruling, and not authorization to bump a manifest, cut a tag, or publish.** |
| 2 | **npm dist-tag** for the 0.8.20 publish | **`halts_run: false`.** The platform matrix is **PARTIAL** — gated to `x86_64-unknown-linux-gnu` (R-REL-4e); macOS/Windows commented out as DEFERRED-TO-FOLLOW-ON (R-REL-4d). `release.yml:21-26` states the label is an HITL confirmation, because publishing partial coverage under `latest` serves an incomplete matrix to every consumer. A **rider on the publish gate**, decided *with* it, not a separate ladder stop. Surfaced 2026-07-30 by the Slice-40 brief review. |

**Everything else that was open on 2026-07-29 is RULED** and is in `decisions.ruled` — the batched
governed-surface delta (**SIGNED**, `seq-157`), TC-98, TC-100 placement, `sqlite-vec` 0.1.9 (**HOLD at
`=0.1.7` for 0.8.20; take it at 0.8.22 with rusqlite**), TC-93 (**publish with the six advisories OPEN; no
pre-publish "Slice 4x"; they land at 0.8.22**), and the license ruling. **Cite them; never re-open them.**

## 7. Carried debt and untriaged items

- **OOS-12** — `slice15e_prekn_filterable undeclared_after_concurrent_drop_is_typed_invalidfilter_not_storage_race`,
  a deliberate race test asserting a typed `InvalidFilter` after a concurrent drop and sometimes getting
  `Ok`. **Untriaged harness-vs-real, and it sits on the registry path this release publishes.** No slice owns
  it; the Slice 40 brief assigns the triage. ⚠ **Contradiction to know about:** the superseded
  `STEWARD-SESSION-HANDOFF-2026-07-26-A.md:7` says OOS-12 "resolved to a pre-existing `slice15e` flake of the
  TC-72 family". The **later** record (`seq-116`, 2026-07-27, and TC-74) says explicitly it has **NOT** been
  classified and *"do not file it under harness artifact until someone has actually looked."* The later
  record governs.
- **Axis-E version undecided** — `fathomdb-embedder-api` at **`0.6.1`**; `set-version.sh --workspace`
  deliberately skips it. A release-record decision, owed at Slice 40 Phase 1.
- **Live doc debt: TC-36, TC-39, TC-56** — all `open` in the todos ledger, all in Slice 39's scope.
- **TC-16** — open; Slice 40 Phase 0.
- **TC-129** — the design-refs suite still has no runner; needs `.github/`, hence Slice 40.
- **TC-101 / TC-80** — `OPP-12-C1-converged-contract.md` records `status: UNREVIEWED` despite being the
  ratified contract. Fixable only by a coordinated **pin re-issue**; take the two together so the pin is
  re-issued once.
- **The allowlist `_comment` correction** — still owed, still deliberately deferred (see §3 guardrails).
- **Eight "Requirement traceability" notes** remain in `dev/design/**` from before the `design_refs` fix
  (`d30ef52f`). Redundant now; removing them is owed work. **Do not write new ones.**
- **17 open Dependabot PRs.** They publish OPEN (`seq-152`). **One exception at Slice 40:** if the rehearsal
  fails on **npm auth**, TC-78 says #153 (`actions/setup-node`) re-opens immediately as the diagnosis —
  report it, still do not fix it.
- **Slice 34 (`#[non_exhaustive]`) is CANCELLED** from 0.8.20 (`seq-182`, reversing `seq-178`) and **PARKED
  at 0.8.21** (`seq-183`). It is **not in the ladder** — route no work through it.

## 8. Traps — read before trusting any green

1. **`scripts/agent-test.sh`'s aggregate exit is VACUOUS** — under `set -euo pipefail` it aborts at the
   known-red actionlint fixture (TC-16) and **never reaches the Rust or Python steps**. Run suites
   individually and quote each one's own rc. ⚠ **After Slice 40's Phase 0 fix it may become meaningful for
   the first time in six versions — do not assume that; prove it.**
2. **Capture `rc=$?` on the very next line**, before any pipe or command substitution. A `$(basename …)` in
   between reports *basename's* status. This invalidated verification twice in this release.
3. **`cargo test --workspace` is not a stable signal (TC-72)** — ~1 run in 3 fails on plain `main`.
4. **`ac_029` is a WALL-CLOCK RATIO assertion (TC-97).** **Never run the engine suite alongside a reviewer.**
5. **`src/python`'s `pythonpath = ["."]` shadows an installed wheel (TC-97)** — kills the X1 Python route at
   *collection* with a misleading circular-import error. Use `-o pythonpath=` from a neutral cwd. Directly
   relevant to Slice 39, which reads a built wheel's metadata.
6. **Six engine targets exit 101 WITHOUT `--features operator` (TC-97).** `--all-features` clippy is
   impossible here (`objc2`).
7. **TC-83, WIDENED:** *any* full-command-line matcher — `pgrep -f`, **`pkill -f`**, `ps | grep` — matches
   **its own** invocation. One orchestrator killed its own shell (exit 144). Match the binary
   (`pgrep -x codex`) or exclude self by pid.
8. **Never triage a codex review by grepping `[P1]`/`[P2]` markers (TC-87)** — `lib.rs` alone carries 21 and
   101 such markers before any review runs, and codex echoes source into transcripts. Read the verdict blocks.
9. **`git commit -F` / `git merge -F` with a quoted heredoc — never `-m` with backticks (TC-53).** The
   failure is silent and the artifact looks intentional. It has corrupted the steward ledger twice.
   `ledgerwrite` now refuses shell residue; **never hand-append to a ledger.**
10. **`ledgerwrite` writes a `.seq` sidecar (TC-88).** Stage it **with** the `.jsonl`.
11. **TC-128 — a `git init` with `GIT_DIR` unscrubbed re-initialises whatever `GIT_DIR` names.** It set
    `core.bare = true` on the **PRIMARY** repository **twice** on 2026-07-29. Scrub it.
12. **TC-110 — a landing worktree running `git checkout -B main` CORRUPTS the primary checkout index, and
    `preflight --landing` does not catch it.** Land from a **detached HEAD / ref-to-ref push**:
    `git push origin <branch>:main`.
13. **A clean rebase is not a correct rebase** — re-run every gate *after*. DOC-HYGIENE-3 rebased with zero
    conflicts and then failed six gates.
14. **A vacuous check passes loudly.** A canary in this release piped `find -printf` into `md5sum` on both
    sides — both hashed nothing and compared equal. **Any check added must be proven non-vacuous with a
    control that makes it fail.**
15. **`OPP-12-C1-converged-contract.md` is BYTE-PINNED** (sha256 *and* git blob sha1). **Cite it; never edit
    it.**
16. **Never hand-edit inside a `GENERATED` marker.** `scripts/check-release-state-views.sh --write`
    regenerates; the bare script must stay rc=0. Note it renders a region inside
    `STEWARD-SESSION-HANDOFF-2026-07-24-A.md` — **that old hand-off is a live render target; do not delete or
    restructure it.**
17. **Markdown lint traps:** a banner before the H1 trips MD041; a line starting `+ ` parses as a list and
    cascades MD004; abutting blockquotes trip MD028 — separate with `---`.

## 9. How to commission, and the cadence that will bite you

**The orchestrator is the mechanism** (`/orchestrate`, or a Steward-spawned `orchestrator` agent when the
Steward holds the loop) — **never `/goal`.** The shape that has worked all release:

1. **Regenerate the brief** — `scripts/commission-manifest.sh 0.8.20 <slice>`. It hard-fails on a dead
   citation or an empty design tier, so a green manifest is real evidence.
2. **Re-verify the `{{MANDATE}}` anchor (plan §9) before briefing.** It went stale at three consecutive
   commissions; TC-89's fix now generates that pointer, but check it anyway.
3. **Record the commission in the ledger BEFORE it starts.**
4. **Verify the result from git** — ancestry, byte-level diffs, gates re-run — **never from the report.**

**Fix-round cap (TC-75 / TC-82 / TC-84):** 3 rounds on the same finding · **mandatory Steward check-in at
6** · 7-10 only where the Steward rules every round productive (a new and distinct defect each time) · beyond
10 an HITL halt. The cap keys on **round productivity**, not on which directory a slice touches. **A
mid-flight `SendMessage` steer is not a round** — warm-resume the implementer rather than cold-respawning.
**When you rule a round-6 check-in, pre-commit the fallback** — Slice 23's *"round 7 is authorized for exactly
these two things; if it does not close, take `#[ignore]` yourself; round 8 is forbidden"* closed on round 7.

⚠ **Across Slices 32 and 33, five of six codex fix rounds were defects in the VERIFICATION APPARATUS, not
the function under test** — a criterion graded against a helper while the real boundary went ungraded.
**Grade through the real entry point.**

**A commissioned orchestrator returns ONCE and does NOT notify you on stall.** One stalled **36 hours**
unnoticed in this program; Slice 32's went idle **four** times and was only caught by polling. **Poll from
git** — branch tips, worktree mtimes, task-output mtimes. Put an explicit ANTI-STALL clause in every brief.

## 10. Standing rules that outlive this hand-off

- **Trust git, not narration.** Verify every "closed / landed / green" against the diff and real exit codes.
- **The mandate rule.** Direction and record changes — a release slot, moving an item between releases,
  altering an I-edge, re-sequencing — are **always** explicit HITL. Never inside an implied mandate.
- **You cannot launder authority downward.** A message to an orchestrator is peer-level, not the HITL's.
- **Escalate a pin trip, never clear it.** Widening `governed-surface-allowlist.json` or re-pinning to make a
  gate pass is forbidden at every level (`seq-113`).
- **Push scope is fathomdb-only.** Never push memex without a specific per-push HITL directive each time.
- **Two-tier numbering.** `x.y.z` real/publishable · `x.y.z.p` pico label-only · **`13` forbidden** · publish
  is a separate explicit HITL gate.
- **Verify the branch (`git rev-parse --abbrev-ref HEAD`) before EVERY commit or push.** The tree is shared.
- **Never open a ledger by hand** — `ledgerwrite` to append, `ledgerwatch` to read.
- **Only the Steward edits `release-state-0.8.20.json`, the master, `STATUS-0.8.20.md` and the ledgers.**
  Every brief says so; hold the line.
- **Surface your own errors in the open.** The prior session recorded eight of its own, including `seq-195`
  above. On DOC-HYGIENE-3 the reviewing layers caught more **Steward** errors than Steward review caught
  implementation errors. **That ratio is the finding, and the transparency is the standard, not an anomaly.**
