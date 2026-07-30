# STATUS — FathomDB 0.8.20 · OPP-12 Phase-2 + erasure completeness + the breaking-pair publish

> **Board of record** for 0.8.20 (`orchestration.md` §12.5). Ladder: `dev/plans/plan-0.8.20.md`.
> Design of record: `dev/design/0.8.20-erasure-and-h-end-state-v4.md`.
> Slice-0 design (v5 addendum): `dev/design/0.8.20-slice0-erasure-design.md`.
> **Update at every slice close.** Verify state from git, never from narration.

**Release base:** `4ca70ba6` · **Orchestration worktree:** `/home/coreyt/projects/fathomdb-worktrees/orch-0.8.20`
(branch `orch-0.8.20`, dedicated linked worktree per **TC-RUBRIC-5**).

**✅ Slices 0, 5, 10 and 15 are all COMPLETE and LANDED on `origin/main`.**

| Slice | Landed at |
|------:|-----------|
| **0** | **`403eb254`** — X0 design gate, HITL-SIGNED 2026-07-19 |
| **5** | **`1f8ed8bf`** — erasure completeness (R-20-E1…E8) |
| **10** | **`3cfb3cda`** (merge) — `ReadView`/read-modes + node-validity + TC-31 |
| **15** | **`a2022957`** (merge) — Phase-2 keystone COMPLETE |

**The Slice-15 keystone (`a2022957`) landed the full remainder:** registry **R-20-PR** + EAV **R-20-EAV** +
`filterable` **pre-KNN** vec0 routing + **TC-33** (INTEGER-epoch harmonisation) + **TC-34** + **Finding-1 (A)**
(attribute-filter × edge-hit = edges excluded) + **`#[non_exhaustive] SearchFilter`**. codex §9 **terminal-clean**;
gates **re-verified by the Steward** (clippy 0, check 0, (A) pin 1/1, AC-041 3/3). **SCHEMA is now 24** (keystone
step 24). Ledger tip **`3264114a`** (steward seq-98).

*(**HISTORICAL**, 2026-07-24: "Slices 20 and 25 are NOW UNBLOCKED — they depended on **R-20-PR**, which now
exists on `origin/main`." Both have since LANDED — 20c `841c307b`, 25 `83b1c818`.)*

> **⚠ SUPERSEDED FRAMING — do not act on it.** Earlier revisions of this board described Slice 15 as
> "PARTIAL / IN PROGRESS (TC-34 only; R-20-PR/R-20-EAV/TC-33 not started)" and Slices 10 and 15 as "not landed."
> **All of that is now false** — the keystone landed at `a2022957` and Slice 10 at `3cfb3cda`. The historical
> close records (§11 Slice 5, §12 Slice 10, §13 Slice 15b) are retained **as history**; their "not landed" /
> "Slice 15 OPEN" banners describe the on-branch state at the time they were written, **not** current truth.

**Last updated:** 2026-07-30. **Library Sweep #3 COMPLETE** — Slices 31 `d0287620` (close record §20), 32
`31d33293`, 33 `f02dc5b4`. **DOC-HYGIENE-3 COMPLETE** — `fd281358` + `85d44c74`. **Slice 34 was CANCELLED**
(`seq-182`) and **PARKED at 0.8.21** (`seq-183`) — it is **NOT in this ladder**. **Slice 39 (`R-20-DOC`) is
COMPLETE — LANDED `91db34d8`** (close record §21). **Ladder remaining: 40 alone.**
(Prior lands: Slice 23 `30102ecd`, Slice 21 `77be504b` §18, Slice 30 `9b3ed0e3` §17.)

---

## 1. Current state

| | |
|---|---|
| **Slice in flight** | **NONE.** The most recent land is **Slice 39 (`91db34d8`)** — **R-20-DOC COMPLETE** (MIT reconciled across all four publishable manifests, a LICENSE that actually ships in all 10 published units, the `## 0.8.20` CHANGELOG section; close record §21). Before it: **Library Sweep #3 COMPLETE** (31 `d0287620` · 32 `31d33293` · 33 `f02dc5b4`) and **DOC-HYGIENE-3 COMPLETE** (`fd281358` machinery + `85d44c74` remainder, TC-53/TC-92). **Forward sequence: two cross-cutting units, then Slice 40** — (1) `SLICE-ID-HARDENING`, (2) "Slice 39.5" (`R-20-HARNESS`), then **Slice 40 (`R-20-PUB`)**. **Neither cross-cutting unit is a ladder slice**; neither carries a slice number or an `R-20-xx` ladder position (the `DOC-HYGIENE-3` / TC-86 precedent, `seq-204`) — `next_slice` stays **40**. ⛔ **Slice 34 is CANCELLED and is NOT in this ladder** (`seq-182`; parked at 0.8.21, `seq-183`) — route no work through it. Slice 31's close record (23 RED tests wired into **no gate**, 7 codex rounds / 12 findings / zero repeats, terminal-clean) is **§20**. **SCHEMA stays 24**; governed-surface allowlist byte-identical since 15d, `check-governed-surface-pin.sh` exit 0. Prior: 23 (`30102ecd`), 22 (`572475f2`, §19), 21 (`77be504b`, §18), 30 (`9b3ed0e3`, §17 — publish precondition SATISFIED). |
| **Status** | **Every ladder slice through 39 is COMPLETE and LANDED on `origin/main`** — 0, 5, 10, 15, 20 (+20c), 21, 22, 23, 25, 30, 31, 32, 33, 39 — plus the non-ladder DOC-HYGIENE-3. **SCHEMA 24.** The **batched governed-surface delta is SIGNED** (HITL, steward `seq-157`); **AC-079 mints into `dev/acceptance.md` at Slice 40** — minting is not signing. **Remaining ladder: 40 alone**, then the separate HITL **PUBLISH** gate — the release's only **`halts_run`** unruled decision. *(`decisions.unruled` holds **THREE** rows but only **TWO live open decisions**: `publish` and the `npm-dist-tag` rider that rides with it. The third, `platform-publish-schedule`, is a **parked tracking row** — it was **RULED at `seq-203`** and is retained only until the 0.8.21 plan exists to carry it — §4.)* |
| **Unblocks** | <!-- BEGIN GENERATED release-state:0.8.20:status-unblocks -->**Slices 40 are NOW UNBLOCKED** — the Slice 39 landing (91db34d8), then the two cross-cutting units named in `ladder_order` (Slice 40 is the last ladder slice. Ahead of it, off-ladder: SLICE-ID-HARDENING (the fractional-id tooling fixes + recurrence arms, seq-204 ruling 1) and 'Slice 39.5' / R-20-HARNESS (collect-all harness + the true red list local AND CI, seq-204 ruling 2). ⚠ CORRECTED at seq-205/seq-209: main's CI is NOT vacuously green, it is RED — the last completed runs concluded failure with 23 jobs executing and only markdownlint skipped, four of them failing (commission-manifest, verify, security, rust-windows). And `test_commission_manifest.sh` is rc=0 LOCALLY (69 PASS) while its CI job fails — a genuine local-to-CI divergence, not a local red. Publish gate (i) (seq-202) is therefore NOT met today. Establishing the true CI red list, and explaining that divergence, belongs to Slice 39.5 (seq-204 ruling 2, seq-206).) now exists. Slice 30 (H7) depends on 10/15/20/25. **AC-079 is PRE-SIGNED** — the HITL signed off on the accumulated governed-surface delta (Slices 5d + 10b + 15b + 15d) on 2026-07-25 (master F-34), pinned to the content of `src/conformance/governed-surface-allowlist.json`; any diff to that file re-opens it (the T1e pin). Pre-signing is NOT minting: AC-079 is minted and recorded as SIGNED at Slice 40 (§4 #1). **Publish is gated by the separate HITL publish gate, not by this AC.**<!-- END GENERATED release-state:0.8.20:status-unblocks --> |
| **Immediate next action** | **`SLICE-ID-HARDENING` is COMMISSIONED and in flight** (`seq-210`) — **not** Slice 40. Order (`seq-204`): **(1) `SLICE-ID-HARDENING`** → **(2) "Slice 39.5" (`R-20-HARNESS`)** → **(3) Slice 40 (`R-20-PUB`)**. ⛔ Neither cross-cutting unit is a ladder slice, and `commission-manifest.sh` **cannot generate either brief** (no slice number — DOC-HYGIENE-3 / TC-86 precedent). Briefs of record, all three, in `dev/plans/runs/`: `0.8.20-slice-id-hardening-commission-brief.md`, `0.8.20-slice-39.5-commission-brief.md`, `0.8.20-slice-40-commission-brief.md`. ⚠ **Slice 40's is a SUBSTANCE DRAFT — regenerate it after both units land** (`commission-manifest.sh:701` computes its base SHA and is itself under repair). **Slice 40 STOPS before any tag.** ⚠ **CI on `main` is RED**: publish gate (i) — every job **that executed** green (`seq-202` as amended by `seq-211`) — is **not met**; the four reds are reported by 39.5's CI baseline (`seq-206`), not triaged by Slice 40. Unruled: `publish`, `npm-dist-tag`, TC-133 placement. |

**Slices 0, 5, 10, 15 close records** are §11 (Slice 5), §12 (Slice 10), §13 (Slice 15b — TC-34 only; the
registry/EAV/TC-33 remainder that also landed in the keystone `a2022957` is summarised in §8 and in
`plan-0.8.20.md` §9). Slice 0 was HITL-SIGNED (`403eb254`, 2026-07-19). **eu7 is CLOSED BY DECISION — ZERO runs,
any backend, any N** (§6 / plan §10 F-28 · steward `seq-84`); the earlier "baseline capture BLOCKED" framing in §6.3 is superseded
by that ruling.

---

## 2. Slice ladder

| Slice | Title | Depends-on | Status |
|------:|-------|-----------|--------|
| **0** | **X0 design gate** | — | **COMPLETE — HITL-SIGNED, landed `403eb254`** |
| **5** | **Erasure completeness (R-20-E1…E8, +E9a)** | 0 | **COMPLETE — LANDED `1f8ed8bf`** (in `origin/main`). Close record §11 |
| **10** | **`ReadView` / read-modes + node-validity (R-20-RV, R-20-NV)** | 0 | **COMPLETE — LANDED `3cfb3cda`** (merge) — SCHEMA 21→22. Close record §12 |
| **15** | **Projection registry (C-1) + EAV/property-FTS (R-20-PR, R-20-EAV) + TC-34 + TC-33 + Finding-1 (A) + `#[non_exhaustive] SearchFilter`** | 0, 10 | **COMPLETE — LANDED `a2022957`** (merge, Phase-2 keystone) — SCHEMA →24; codex §9 terminal-clean; Steward-verified gates. §13 = the TC-34 sub-part only; registry/EAV/TC-33 remainder summarised in §8 |
| **20** | `dense_readiness` + flush-to-readiness barrier (R-20-DR) **+ TC-45 supersession-terminal fix** | 15 | **✅ COMPLETE — 20b `26b237c0` + 20c `841c307b`** (merges). **TC-45** closed (both `commit_batch` sites record `'up_to_date'`; SCHEMA stays 24). **R-20-DR closed:** part 1 = `DenseReadiness {Ready, Embedding}` derived onto `ProjectionSpec.vector` + the atomic flip; part 2 = the **flush-to-readiness barrier, shipped by REUSING `drain`** per `api-surface.md` **C4** — **there is NO `flush_embeddings()` verb** (TC-55 = INSTRUMENTATION). Fixed entirely on the **enqueue** side; `drain` stays passive and `connection_has_pending_projection_work` was not restructured (TC-56). **ZERO net-new governed commands; allowlist byte-identical; pin exit 0; SCHEMA 24.** Five codex §9 rounds, all RED-first; **one finding left OPEN at the circuit breaker = TC-71**. Close record **§16** |
| **25** | Surrogate minting — governed entities ONLY (R-20-SUR) | 15 | ✅ **LANDED `83b1c818`** — D1 static migration guard + D2 dynamic whole-ladder proof (`NULL → NOT NULL == 0`) + D3 registration-inertness; zero engine source change, SCHEMA stays 24, allowlist byte-identical, pin exit 0. codex §9: 3 fix rounds, halted at the circuit-breaker on a 4th same-family [P2]; residual ACCEPTED by Steward ruling (D2 is row-based ⇒ shape-independent). Residual recorded as **TC-66**. |
| **30** | **RUBRIC-H7 `can-i-deploy` contract gate (R-20-H7)** | 10,15,20,25 | ✅ **COMPLETE — LANDED `9b3ed0e3`** (merge). **The PUBLISH PRECONDITION is SATISFIED.** 45 C-1 clauses (26 CHECKABLE / 12 cross-repo / 7 prose), **zero failing**; zero engine source; allowlist byte-identical, pin exit 0; SCHEMA stays 24. Seven codex §9 rounds + the fix-6c micro-fix, **every round productive** and the 3-same-finding bound never firing — fix-4 alone found **18 of 26** clauses had a demonstrable false green; fixtures 106 → 276 → **317**. Final [P2] **REFUTED** by Steward test, not accepted as risk. fix-6c review **terminal-clean** (no P1/P2/low). Close record **§17** |
| **21** | **Concurrency + test-oracle repair (R-20-CR)** — TC-57 · ac_002 oracle · TC-71 | 20 | **COMPLETE — LANDED `77be504b`**. Close record §18 |
| **22** | **Vector-arm consumer contract (R-20-VC)** — TC-67 (c) · TC-68 · decision #18 · #99 probe | 15,20 | **COMPLETE — LANDED `572475f2`** (merge). All four legs closed. **SCHEMA stays 24**; governed surface **byte-identical**, `check-governed-surface-pin.sh` **0**, the AC-079 pin **never tripped**; `check-c1-conformance.sh` **0** (26/26) — the Slice-30 publish precondition intact. **`#99` REPRODUCED and was LIVE** — `erase_source`/`purge` errored and left the vec0 row at rest; remediated engine-side. codex §9 **4 rounds**, one [P1], CLOSED. **X1 ran live before the land** (TS 250/250, Py 46/46). Close record **§19** |
| **23** | **Spec-validation reject + carried-defect characterization (R-20-SV)** — the `fts`/`vector` reject · TC-90/TC-91 characterization | 22 | **COMPLETE — LANDED `30102ecd`** (merge). **Leg 1:** an `fts`/`vector` sub-object without the `searchable` role is now an **INVALID SPEC** rejected with **`EngineError::WriteValidation`** (HITL 2026-07-24, §11 item 4 (b); decision **#18**'s family, NOT re-opened), replacing accept-and-round-trip; validation precedes any mutation, so a mixed valid/invalid request is a total no-op. The **TC-71 runtime gate is preserved** — legacy `{filterable, vector}` rows at rest are unreachable by a configure-time reject, so nine pinned tests were **converted, not deleted**, and a legacy row's only remedies are ADD `searchable` or DROP (re-declaring its valid half alone is `ProjectionDestructive`) — asserted in **all three** languages. `dev/interfaces/{rust,python,typescript}.md` updated incl. the **overruled** accept-inert precedent; `cli.md`/`wire.md` carry no projection surface and were left alone. **Leg 2 — CHARACTERIZATION ONLY, NO FIX** (`seq-136`), engine source **byte-identical**: **TC-90 REPRODUCES**, and the TC-57-shaped protocol **alone would have filed a FALSE NEGATIVE** (paced **0/10** vs stress **10/10**, control 0/10) — the template needed a **stress arm**, and **reproduction, not rate**, is the bar (per-run count did not replicate: 5.9 vs 3.8 per 40). Mechanism captured not assumed: plain **`SQLITE_BUSY` (5)**, NOT 517, in 0 ms, busy handler invoked **zero** times; `BEGIN IMMEDIATE` transfer **measured**. Separate diagnosability defect: `Engine::transition` **never calls `emit_sqlite_internal_error`**, so the code is never produced at all. **TC-91(a)** 52.0 % governed vs 52.5 % anonymous (⇒ not the TC-57 race), falling to 0 as spacing goes 1 ms→25 ms; **(b)** forcing yields 86.4/96 duplicates while `failed` terminals are **0/10 runs** ⇒ terminal-state counting is **structurally blind**. **(a) is NOT proven to be (b), and the reason it cannot be closed by measurement IS (b).** Corrects a stale figure of record: **TC-68 took reopen probe cost to 0, not 45** (measured 0/90/0, 20/20). **Governed-surface delta ZERO** (byte-identical, pin **0**) — **23 was the last unit that could move it**. codex §9 **7 rounds / 4 fix rounds**; a terminal-clean round 1 was **deliberately not banked** (it never engaged the riskiest change) and the targeted re-review found a [P2] + [P3]. Round-6 cap **halted to the Steward**, who ruled round 7 bounded with `#[ignore]` pre-committed as fallback; round 7 **closed it**. **X1 live before land** (TS 251/251, Py 268 passed/2 skipped/0 failed via a throwaway-venv wheel; shared `.venv` verified unrebound) |
| **31** | **Library Sweep #3, leg 1/3** — SBOM-survey tool: req + AC + design + RED tests, **NO code**; **no requirement id** (TC-76) | — | **COMPLETE — LANDED `d0287620`**. Spec of record `dev/design/0.8.20-slice-31-sbom-survey-tool.md`: requirements, **23 acceptance criteria each bound 1:1 to a named test**, design. Suite `scripts/sbom-survey/tests/` is **23 failed / 0 passed / 0 skipped / 0 errors**, RED by construction and **wired into no gate**. Tier rules derived from `.github/dependabot.yml` rather than re-decided; discovery is `git ls-files`-derived so untracked manifests (the gitignored `/python/`) are structurally out of scope; the 8 `dev/release/fixtures` skew manifests are excluded by **tracked data**, auditably; the published-version lookup sits behind an **injectable seam** so the suite never needs the network and an unknown latest can never render as up-to-date. Slice 33's findings home ruled: `dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md`. **Surfaced en route (TC-106):** `dev/tools/mermaid/package.json` is a tracked npm manifest under **no** configured Dependabot directory. codex §9 **7 rounds / 6 fix rounds**, terminal-clean. Close record **§20** |
| **32** | **Library Sweep #3, leg 2/3** — implement the SBOM-survey tool against Slice 31's RED tests. **CODE ONLY**; no bump, no manifest or lockfile edit; **no requirement id** (TC-76) | 31 | **COMPLETE — LANDED `31d33293`**. 24/24 GREEN. Governed surface byte-identical; SCHEMA stays 24 |
| **33** | **Library Sweep #3, leg 3/3** — RUN the tool, produce the dependency survey (**ascertain, never implement**) + the TC-115 install-then-run smoke; **no requirement id** | 32 | **COMPLETE — LANDED `f02dc5b4`**. Survey of record: `dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md`. Output is an **input to 0.8.22**; no bump applied |
| **39** | **Publish-facing documentation (R-20-DOC)** — **MIT license reconciliation + a LICENSE that actually ships** · the **0.8.20 CHANGELOG section** · registry-facing READMEs · `docs/` de-stale · `dev/interfaces` currency · docs.rs doc-comment corrections | 30 | **COMPLETE — LANDED `91db34d8`** (fast-forward ref-to-ref push, **no merge commit** — TC-110). The MIT reconciliation (`seq-193`) is applied across all four publishable manifests and a real LICENSE now ships in all **10** published units, with `scripts/check-license-consistency.sh` as the standing guard; the `## 0.8.20` CHANGELOG section exists, so `verify-release-gates.sh` check 4 can match it. Close record **§21** |
| 40 | Verification + release readiness (publish-or-hold); **TC-16 determination FIRST** (`seq-118`) · **the DISPATCH GUARD (`seq-198` ruling 1)** — a **required confirmation input on `workflow_dispatch` that must literally match the version**, plus `scripts/verify-release-gates.sh` **exiting 1, not warning**, when a dispatch carries `dry_run=false` without it (today `verify-release-gates.sh:58-61` warns-and-continues **and** skips the tag-format check, leaving the real publish at `release.yml:261` reachable with **no tag**). **Slice 40 is the ONLY slice permitted to touch `.github/` this release** | 5,30,39 | not started |

**Ladder remaining: 40 alone**, then the separate HITL **PUBLISH** gate (with its
`npm-dist-tag` rider — §4). Ahead of Slice 40, off-ladder and in this order: **`SLICE-ID-HARDENING`**, then
**"Slice 39.5" (`R-20-HARNESS`)** — **neither is a ladder slice** and neither carries a slice number or an
`R-20-xx` ladder position (`seq-204`). Slices
**0/5/10/15/20/21/22/23/25/30/31/32/33/39 are all LANDED**, as is the non-ladder **DOC-HYGIENE-3**
(`fd281358` + `85d44c74`). **TC-86 is DONE (`2956d98d`).** ⛔ **Slice 34 is NOT in this ladder** — it was
**CANCELLED** at `seq-182` (reversing `seq-178`) and all `#[non_exhaustive]` work is **PARKED at 0.8.21**
(odd micro ⇒ label-only build) to **publish at 0.8.22** (`seq-183`). The **batched governed-surface decision
is no longer a pending ladder stop** — it was **SIGNED** at `seq-157`; only the **AC-079 mint** remains, and
that happens inside Slice 40. **No 0.8.20 slice applies a dependency bump**; the one deliberate manifest edit
is Slice 39's Apache-2.0 → MIT license-field correction (`seq-193`).

**Band occupancy (plan §5):** the 20 band holds 21, 22 and 23 — **three of four slots used, one spare**. The
**30 band** (gaps **31–39**, nine slots) holds **31, 32, 33** — Library Sweep #3's three legs (`seq-153`) —
**and 39** (R-20-DOC), so **four of nine used, five spare**. *(Slice 34 was minted into this band at `seq-178`
and then **CANCELLED** at `seq-182`; it does not occupy a slot.)* The tripwire stays at band overflow
(**TC-77**), *conditionally*: if 21 or 22 spawns two or more further slices, it returns to the HITL. **Raised
unprompted at `seq-153` and NOT TRIPPED** — 32 and 33 were spawned by an HITL scope ruling on 31, not by 21
or 22, so the trigger does not apply, and neither band has overflowed.

**Merge discipline (still binding for every remaining slice).** Slices touching `engine/src/lib.rs` **serialize the merges**
(rebase-then-merge one at a time). **One `maturin develop` at a time** (shared `.venv` mutex) — and **never from
a worktree**. **Max 3 concurrent worktrees.** Canary the first launch of each new work-type before parallelizing.

---

## 3. AC scoreboard

**⚠ The plan's "new ACs continue from the AC-077 ceiling" is a RESERVED-ID COLLISION.** AC-077 is a *reserved
placeholder* for the IR-eval IR-1/IR-2 initiative (`dev/acceptance.md:1286`) and **AC-078 is conditionally
reserved to the same initiative** (`:1297`). Highest **defined, non-reserved** AC = **AC-076**.
**Recommendation: mint from AC-079.** *(HITL decision #4.)*

| AC | Covers | Status |
|---|---|---|
| **AC-079** | Governed-surface delta (erasure API + the accumulated 5d/10b/15b/15d delta) vs the conformance allowlist | **BUILT · ✅ SIGNED** — pre-signed by the HITL 2026-07-25 (master **F-34**) and the batched governed-surface decision **SIGNED at steward `seq-157`** (2026-07-29). **MINTS into `dev/acceptance.md` at Slice 40** — minting is not signing (below) |
| **AC-080** | Erasure completeness at rest — body absent from every row-owned projection **and** `-wal` bytes | **BUILT, GREEN** (below) |
| **AC-041** | REQ-054 five-name recovery denylist | **VERIFIED GREEN, denylist UNCHANGED at five** (below) |

**AC-079 — what was built, what was signed, and what is still owed.** Slice 5d added to the *positive allowlist* in
`src/conformance/governed-surface-allowlist.json`: the command verb **`erase_source` / `eraseSource`**
(`Engine.erase_source` in Python, `Engine.eraseSource` in TypeScript) plus the non-command types
**`EraseReport`** (Py + TS), the Rust facade's net-new **`SourceId`** provenance newtype, and **`ExciseReport`**
moved from the `operator`-gated re-export block to the always-present one (it is `erase_source`'s return type).
`excise_source` **remains CLI-only** and is deliberately **not** allowlisted — it stays the recovery seam and
alone may address the engine's reserved `_`-prefixed namespace.
**✅ SIGNED — this is no longer a proposal, and it does not block publish.** The HITL **pre-signed** the
accumulated governed-surface delta on **2026-07-25** (master **F-34**), pinned to the *content* of
`src/conformance/governed-surface-allowlist.json`, and the **batched governed-surface decision was SIGNED at
steward `seq-157`** (2026-07-29): **seven net-new allowlist members = four logical verbs**
(`erase_source`/`eraseSource` · `read.crossed_boundary_since`/`read.crossedBoundarySince` ·
`configure_projections`/`configureProjections` · `read.projections`) plus the non-command types.
`recovery_denylist` is **UNCHANGED at five** ⇒ AC-041 unaffected. Any diff to the allowlist file **re-opens**
the gate (the **T1e** content pin). **Publish is gated by the SEPARATE HITL publish gate, not by AC-079.**

**What is still owed is a MINT, not a signature.** AC-079 is minted into `dev/acceptance.md` and recorded as
SIGNED at **Slice 40** — see §4 #1 and the §4 CLOSED block.

🕮 **CORRECTED 2026-07-30 (steward `seq-207`) — this paragraph was FALSE and is rewritten.** It previously
said the allowlist `_comment` "still contains the literal `AWAITING HITL SIGN-OFF, NOT SIGNED` (×4)" and that
a coordinated re-pin was therefore owed. **Measured at HEAD: `grep -c 'AWAITING' ` on
`src/conformance/governed-surface-allowlist.json` is `0`, and so is `grep -c 'NOT SIGNED'`.** The four
literals were replaced by `SIGNED: the HITL signed this delta on 2026-07-29 … (steward seq-157)` ×4 at commit
**`c239908b`** (*"TC-52 — record the signature in the allowlist; re-issue the pin"*), **which re-issued
`scripts/governed-surface-pin.json` in the same commit.** The coordinated re-pin is **DONE, not owed**, and
`c239908b` is the currently pinned commit.

⚠ **Do not edit the allowlist `_comment` anyway.** `check-governed-surface-pin.sh` hashes the **raw bytes**,
so any prose change still requires a coordinated pin re-issue. ⚠ **A SEPARATE and still-unverified claim
survives:** that the `_comment` asserts the window refusal is `InvalidArgument` while Decision #18 made it
`WriteValidation`. **Nobody has measured whether shipped code agrees** — that is a Slice-40 *determination
duty*, not an established contradiction. Do not act on it as fact.

**AC-080 — built and green.** `erasure_completeness` 10/10 asserts the erased body is absent from **every**
row-owned projection (registry-driven, incl. `search_index_v2`, the table that previously retained the body)
**and** absent from the `-wal` bytes after the verb's WAL truncation.

**AC-041 — re-verified GREEN, unchanged.** `no_recovery_surface` 1/1. The recovery denylist is still **exactly
the five REQ-054 names** — `["recover","restore","repair","fix","rebuild"]`. **`erase_source` is not one of
them**, so the denylist is untouched by this slice.

**Slice 10 minted NO AC.** Its governed-surface delta was recorded at the time as a **PROPOSAL, NOT SIGNED**
(§12.5), the same shape Slice 5d used. ***That proposal has since been SIGNED*** — `read.crossed_boundary_since`
/ `read.crossedBoundarySince` are inside the `seq-157` signed set. **`AC-079` remains unminted** — the mint
happens at Slice 40 — but it is **no longer awaiting a signature**. **AC-041 is GREEN on the Slice-10 branch too**, verified
**live in both bindings**: `test_no_recovery_surface.py` and `no-recovery-surface.test.ts` ran inside the
zero-failure suite runs of §12.3. **Denylist unchanged at exactly five.**

Everything else is tracked by **requirement id + TDD test name** per the locked-`acceptance.md` policy — see
`0.8.20-slice0-erasure-design.md` §4.

---

## 4. Open HITL decisions (options + recommendation)

> **⚠ THIS SECTION IS A HISTORICAL QUEUE, NOT THE LIVE OPEN SET (Steward, 2026-07-25).** Items 1–6 below were
> **resolved at the 2026-07-19 Slice-0 X0 sign-off**; item 7 (**AC-079**) was **PRE-SIGNED by the HITL on
> 2026-07-25**. Item 1 (**eu7 basis**) is doubly settled — eu7 is **CLOSED BY DECISION, zero runs on any
> backend at any N** (§6 / master F-28 · steward `seq-84`), so its "conditional on Slice-40 proof" wording is
> dead. Item 6's "confirm Memex co-land readiness" is **CLOSED BY DECISION** (master F-34 · steward
> `seq-106` ruling 5): Memex adapts to 0.8.20's surface and **no confirmation is to be sought**. The rows are
> retained as the decision record; **do not act on them as open**.
>
> **THE LIVE OPEN SET IS EXACTLY <!-- BEGIN GENERATED release-state:0.8.20:status-live-open-count -->THREE<!-- END GENERATED release-state:0.8.20:status-live-open-count -->** (was TWO until 2026-07-29 — the count is
> generated from `decisions.unruled`. It went TWO → SIX on 2026-07-29 when four HITL items that had existed
> only in the ledger and in session transcripts were written INTO the single writer at `seq-150` — **the
> count grew because the record got honest, not because new work appeared** — then SIX → **ONE** across that
> same day as the HITL ruled five of them: `seq-151`, `seq-152`, `seq-155`, `seq-156`, `seq-157`; then
> **ONE → TWO** on 2026-07-30 when the Slice-40 brief review surfaced the npm dist-tag question and it was
> written into the single writer — again, **the record got honest, no new work appeared**):
>
> 1. **PUBLISH** (Slice 40) — the substantive item, and the only **`halts_run: true`** one. HITL prefers
>    publish-after-40 and explicitly deferred (`seq-135`); **not a ruling, and not authorization to bump a
>    manifest or cut a tag.**
> 2. **npm dist-tag for the 0.8.20 publish** (Slice 40 / publish, **`halts_run: false`**) — the platform
>    matrix is **PARTIAL**: gated to `x86_64-unknown-linux-gnu` (**R-REL-4e**), with macOS/Windows targets
>    commented out as DEFERRED-TO-FOLLOW-ON (**R-REL-4d**). `release.yml:21-26` states the label is an **HITL
>    confirmation**, because publishing partial coverage under `latest` would serve an incomplete matrix to
>    every consumer. Surfaced by the Slice-40 brief adversarial review, 2026-07-30; it is a **rider on the
>    publish gate**, not a separate ladder stop.
>
> **CLOSED 2026-07-29/30 — cited, never re-opened, never re-confirmed:**
>
> - **Batched governed-surface — SIGNED** at **`seq-157`**. The delta: **7 net-new allowlist members = 4
>   logical verbs** (`erase_source`/`eraseSource` · `read.crossed_boundary_since`/`read.crossedBoundarySince`
>   · `configure_projections`/`configureProjections` · `read.projections`) plus non-command types;
>   **`recovery_denylist` UNCHANGED at five** (AC-041 unaffected); `excise_source` stays CLI-only.
>   **AC-079 mints at Slice 40**, still pinned to this content — any diff re-opens it (T1e). ⚠ **PUBLISH IS
>   NOT AUTHORIZED BY THIS SIGNATURE** (nor is publish blocked by AC-079 — it is gated by the separate HITL
>   publish gate). 🕮 **CORRECTED 2026-07-30 (`seq-207`): this item is NOT owed — it is DONE.** It read
>   *"OWED, and still owed: the allowlist `_comment` still reads `AWAITING HITL SIGN-OFF, NOT SIGNED` ×4."*
>   **Measured at HEAD: that literal occurs ZERO times.** It was replaced by `SIGNED: … (steward seq-157)` ×4
>   at **`c239908b`**, which re-issued `scripts/governed-surface-pin.json` in the same commit — the
>   coordinated re-pin this item was waiting on. **The "HELD until Slice 31 lands" condition is DISCHARGED** — Slice 31
>   landed at `d0287620` and no orchestrator is in flight against the allowlist. The re-pin is now simply
>   **unscheduled work owed**; its natural home is the **AC-079 mint at Slice 40**, and until then **no agent
>   may edit that JSON**.
> - **TC-98 — (a) FOR NOW** at **`seq-156`**. Accept `#18`'s one named exception. Nothing reclassified,
>   nothing minted, governed surface unchanged. **ACCEPTED, NOT ENDORSED — revisit as `TC-103`**: parts (a)
>   and (c) at **0.8.22**; part (b), the `EngineError` `#[non_exhaustive]` attribute, is **PARKED at 0.8.21**
>   (built label-only) to **publish at 0.8.22** (`seq-183`). **None of it is 0.8.20 work** (`seq-182`).
> - **`sqlite-vec` 0.1.9 (TC-76)** at **`seq-151`** — hold at `=0.1.7`; the 0.1.9 bump lands at **0.8.22**
>   with `rusqlite 0.31→0.40`.
> - **TC-93** at **`seq-152`** — publish with the six advisories open, no Slice 4x; they land at **0.8.22**.
> - **TC-100 placement** at **`seq-155`** — **joins DOC-HYGIENE-3 as a SIXTH id** (TC-53, TC-88, TC-89,
>   TC-92, TC-94, **TC-100**); **DoD consequence: DOC-HYGIENE-3 must verify with a before/after citation-set
>   diff across the landed slices.** *(DOC-HYGIENE-3 is now **COMPLETE** — `fd281358` + `85d44c74`.)*
> - **Slice 34 — CANCELLED** at **`seq-182`** (reversing `seq-178`): **none** of the `EngineError`
>   `#[non_exhaustive]` / TC-103 work happens in 0.8.20. **PARKED at 0.8.21** at **`seq-183`** — 0.8.21 is an
>   odd micro ⇒ OOB label-only, so the attribute is **built there and published at 0.8.22**, which thereby
>   becomes a declared breaking venue. **Slice 34 is not in the ladder; route no work through it.**
> - **LICENSE — the project is MIT**, at **`seq-193`**. The repo-root `LICENSE` (MIT, `ff8639e4`) is
>   authoritative; the **four publishable manifests declaring Apache-2.0 are WRONG** (`Cargo.toml:30`,
>   `src/python/pyproject.toml:11`, `src/ts/package.json:5`, `src/ts/npm/linux-x64-gnu/package.json:5`) and
>   must read MIT **before the 0.8.20 tag** — crates.io versions are immutable. A LICENSE file must also
>   actually **ship** in every artifact; today **none** does. Forecloses Apache-2.0 and any publish that
>   leaves manifest and file disagreeing. **Owned by Slice 39 (R-20-DOC).**
> - **THREE RULINGS on the Slice 39/40 brief-review findings**, at **`seq-198`** (2026-07-30):
>   **(1) DISPATCH GUARD — YES, WITH EXIT 1, in Slice 40.** `.github/workflows/release.yml` is hardened so a
>   `workflow_dispatch` with `dry_run=false` cannot publish on **one unchecked checkbox**: add a **required
>   confirmation input that must literally match the version being released**, and make
>   `scripts/verify-release-gates.sh` **exit 1 — not warn** — when a dispatch has `dry_run=false` without it.
>   Measured today: `verify-release-gates.sh:58-61` only prints an *"emergency-republish path"* WARNING and
>   continues, **and the tag-format check is skipped on dispatch**, so the real publish path at
>   `release.yml:261` is reachable **with no tag at all**. Fixing the brief wording alone would protect one
>   agent and nobody after it — the standing rule is that guardrail failures are fixed in the repo.
>   **Slice 40 is the only slice permitted to touch `.github/` this release.**
>   **(2) crates.io `categories` — DROPPED from 0.8.20, RE-ENTERED at 0.8.21** so the metadata is READY for
>   the **0.8.22** publish. **`keywords` STAY in 0.8.20** — `cargo package` validates them locally (max 5,
>   each ≤ 20 chars). Rationale: `categories` slugs are validated **SERVER-SIDE ONLY at the real publish**,
>   **mid-tier, after earlier crates have already uploaded immutably** — the **v0.8.9 partial-publish shape**.
>   **(3) TC-131** (commission-brief fact-checker) — **BUILT AT 0.8.21, not now.** Two briefs remain this
>   release and the per-brief adversarial review has caught **100 %** of the defect class; 0.8.21 can use the
>   four recorded failure instances (`seq-195`, `seq-196`, `seq-197`) as **test fixtures**.
>   *None of the three adds to `decisions.unruled` — all three are ruled.*
>
> Reserved-gap band overflow (`plan-0.8.20.md` §5) still halts. Authorization: master **F-34** · plan §11's
> 2026-07-25 rulings block. **`seq-106`'s "two stops remain" is HISTORICAL** — it was true of the ladder
> gates then, and items 3-4 are later HITL questions, not a re-opening of it.
>
> *This section is a hand-maintained duplicate of state that lives in three other files — exactly the
> fan-out DOC-HYGIENE-2's **N2** replaces with a generated ruled/unruled table. **The enumeration above is
> hand-written while the count beside it is generated: change `decisions.unruled` and you MUST update both
> in the same commit,** or the board will name a different number of items than its own count claims.*

🕮 **Rows 1–6 below are the HISTORICAL DECISION RECORD of the 2026-07-19 Slice-0 X0 sign-off. They are all
resolved. Read them as history; do not act on any of them as an instruction.**

| # | Decision | Recommendation |
|---|---|---|
| 1 | **eu7 basis** (F-22) — no-op vs bounded re-baseline | 🕮 **HISTORICAL — SUPERSEDED. DO NOT ACT.** *Original recommendation, retained as the record:* "no-op, conditional on Slice-40 proof (design §7); must be decided on CPU numbers only — §6." **Now CLOSED BY DECISION** (master **F-28** · steward `seq-84`): the basis is **no-op, unconditionally**, and there are **ZERO eu7 runs on any backend at any N**. The "Slice-40 proof" condition is **dead** — Slice 40 must **not** run eu7 |
| 2 | **`embed_batch_cls` TS parity** (F-22) | **add the TS binding.** Already a documented blind-spot (`napi:709`, `py:2088`); X1 parity is a release gate and this is the first published release since 0.8.9 |
| 3 | **Erasure-audit durability** (design §2 D-A — **new finding**) | **exempt the audit collection from `enforce_provenance_retention`.** Retention-policy change ⇒ HITL |
| 4 | **AC id allocation** (§3) | **start at AC-079** |
| 5 | **Adoption arms** (build ≠ adopt, F-21) | read-modes/registry/readiness **opt-in**; erasure fixes **ship ON**; **`SourceId` is BREAKING — own call** |
| 6 | **Publish gate** (R-20-PUB) | Out of Slice-0 scope. Separate per-`x.y.z` gate — **that half is still true and is the release's one `halts_run` unruled decision** (with the `npm-dist-tag` rider alongside it). 🕮 **The "confirm Memex `0.5.x-successor` co-land readiness" half is CLOSED BY DECISION** (master **F-34** · steward `seq-106` ruling 5): Memex adapts to 0.8.20's surface and **no confirmation is to be sought** |

**Raised by Slice 5** (details in §11.5):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 7 | **AC-079 governed-surface sign-off** — `erase_source`/`eraseSource`, `EraseReport`, `SourceId`, `ExciseReport` | — | **✅ CLOSED — SIGNED.** ~~"Sign or amend before publish."~~ ~~**Publish is blocked until signed.**~~ **PRE-SIGNED 2026-07-25** (master **F-34**), pinned to the *content* of the allowlist; the **batched governed-surface decision was SIGNED at steward `seq-157`** (2026-07-29). The REQ-037 carve-out (2026-07-12) had already approved `erase_source` as an SDK verb in principle; the signature covers the *exact* symbol set. **Minting into `dev/acceptance.md` still occurs at Slice 40 — minting is not signing.** **Publish is gated by the separate HITL publish gate, not by this AC.** 🕮 **CORRECTED 2026-07-30 (`seq-207`):** this cell claimed the allowlist `_comment` "still contains the literal `AWAITING HITL SIGN-OFF, NOT SIGNED`" and that the coordinated re-pin was owed at the Slice-40 mint. **Measured at HEAD: that literal occurs ZERO times** — replaced by `SIGNED: … (steward seq-157)` ×4 at **`c239908b`**, which re-issued the pin in the same commit. **The re-pin is DONE; Slice 40 does not owe it.** **Do not edit that JSON** regardless — the T1e pin hashes raw bytes. |
| 8 | **Design-text correction** — the `logical_id IS NULL ONLY` backfill rule is right for NODES and **wrong for EDGES** | **TC-26** | **Correct plan §R-20-E8 + v4/v5 prose** to the shipped asymmetry. Code is right; the prose is not. TC-11 unaffected |
| 9 | **eu7 no-run prohibition is UNENFORCEABLE** — `eu7_real_corpus_ac` had no `#[ignore]` and no env gate; `scripts/agent-test.sh` carried a bare `cargo test --workspace`. Raised on **three consecutive** codex rounds. **GUARD SHIPPED in fix-4** (`eu7_real_corpus_ac.rs:760` `#[ignore]`; `agent-test.sh` can no longer invoke it) — **verified by INSPECTION ONLY, zero eu7 runs**, with a control proving the check was not vacuous | **TC-20** | **✅ CLOSED BY DECISION — no longer a decision, and the follow-on is SUPERSEDED.** ~~"Slice 40 must carry a non-skip witness and opt in with `-- --ignored`."~~ **R-20-EU7 was closed by ruling** (master **F-28** · steward `seq-84`): **ZERO eu7 runs, on any backend, at any N.** The premise that "eu7 IS wanted at Slice 40" is **false** — nothing downstream runs it, so the shipped `#[ignore]` creates **no** vacuous-green hazard. ⛔ **The `#[ignore]` on `eu7_real_corpus_ac` STAYS and must NOT be opted around** — no `-- --ignored`, no env opt-in, no `#[ignore]` removal, by any slice |
| 10 | **Python X1 was OWED** — 5c's `SourceId` is BREAKING and broke ~50 Python fixtures, swept but **only statically verified** (`py_compile` + `ruff` + AST audit). **DISCHARGED:** the suite has now been executed in an **isolated fresh clone with its own venv** (never the shared `.venv`) ⇒ **`2 failed, 754 passed, 7 skipped`**, and **the identical two tests fail on `origin/main`** — see §11.8. It was exactly this run that caught the fix-4 regression, vindicating the "landing blocker, not a follow-up" call | **TC-22** | **Satisfied.** The two residual failures are **pre-existing** and tracked as **TC-31** (#13) |
| 11 | **`maturin develop` fires AUTONOMOUSLY** from `src/python/tests/conftest.py::_ensure_test_hooks_binding` — merely running the Python suite from a worktree attempts to rebind the **shared** `.venv`. Observed live in fix-3. **GUARD SHIPPED in fix-4, then CORRECTED in fix-6** — fix-4's env-var guard raised at import time and made the *documented* default path permanently red; fix-6 restates the policy positively as a pure function returning `PROCEED`/`REBUILD`/`DEGRADED`/`CONTRADICTORY` (`src/python/tests/_test_hooks_gate.py`). The load-bearing check is **`venv_belongs_to_source_tree()`** — `maturin develop` may run **only** when the venv prefix lies **inside the repo root** — and **the opt-in env var CANNOT override it**. See **§11.9** | **TC-27** | **RESOLVED** (ledger **seq-48**), closed by tooling (fix-the-tooling, not a be-careful note), and closed **structurally** rather than by an env var. *No damage occurred:* the shared `.venv` was re-verified intact — `/home/coreyt/projects/fathomdb/.venv/.../fathomdb.pth` mtime still **2026-07-09**, still pointing at the **main** repo |
| 12 | **Pending-redaction queue hardening** — its "a row is removed ONLY when the obligation is discharged" invariant is upheld by **three correct call sites, not structurally**. codex found a defect in this one mechanism on **each** of rounds 1, 2 and 3 | **TC-28** | **Make it structural** (own table with no generic `DELETE` verb, or a trigger). Every known path is now closed but **nothing prevents a fifth.** Deliberately NOT attempted inside a fix round |

**Raised by Slice 5 fix-4** (details in §11.7/§11.8):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 13 | **✅ RESOLVED at Slice 10** (`63dfbc08` — `source_id` now populated on **every** hit path, not just the graph arm; RED test `f29f7d91`). **WRITE/READ PROVENANCE ASYMMETRY.** 0.8.20 makes provenance **mandatory on write** (R-20-E2, `SourceId`) but it is **unreadable on a text or vector hit**: `PySearchHit.source_id` is populated **only for graph-arm hits** and is `None` for every two-arm hit (`fathomdb-py/src/lib.rs:537-539`). Consumers therefore fall back to `int(sh.id)`, which has raised `TypeError` since 0.8.19 made `SearchHit.id` an `IdSpace` (C-2). This is the measured form of the known "NO SDK EXPOSURE" erasure gap — a caller cannot tell which document a hit came from, so it cannot audit or scope an erasure | **TC-31** | **Schedule a read-side fix** — populate `source_id` on every arm. **One fix likely closes BOTH** residual Python failures (§11.8). **OUT OF SCOPE for Slice 5**: `_doc_id_of` is **byte-identical on `main`** and both failures reproduce there |
| 14 | **ENTITY-DEDUPE ERASURE GAP, adjacent to R-20-E2 — found in fix-4, NOT fixed. ✅ RULED ON (HITL, 2026-07-20): ACCEPTED AS-IS, no behavior change** — annotated in code at Slice 10 (`e62309e1`). **Carry-forward caveat: the erasure guarantee MUST NOT be stated unconditionally to users while co-named-entity dedupe stands.** Entities dedupe **within a batch** by `logical_id` derived from `(kind, name)`, so two documents naming the same entity **collapse to one row** carrying the **FIRST** document's provenance. Erasing the second document therefore **leaves that entity behind**, still attributed to the first. An **erasure-completeness gap**: the slice's own guarantee ("erase every row owned by this source") does not hold for a co-named entity | **TC-32** | **Entity-identity design question, not a fix round.** Options: per-source entity rows, or a multi-valued provenance edge set. Must be decided before the erasure guarantee is stated unconditionally to users |

**Raised by Slice 10** (details in §12):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 15 | **Node validity has NO write-side authoring verb.** `valid_from`/`valid_until` are **queryable but not settable from any SDK** — the tests author windows via **direct SQL**. **Is R-20-NV met without it?** The read half is complete and closed; the write half does not exist on the governed surface | **TC-34** | 🕮 **✅ RESOLVED — CLOSED at Slice 15b** (option (b)): the authoring path shipped as **optional `valid_from`/`valid_until` fields on the existing node write batch item**, not a new verb, so it added **zero** governed commands (§13). *Original open question, retained as the record:* "HITL call — either (a) ratify R-20-NV as read-only and schedule the authoring verb, or (b) re-open Slice 10 to add it." ⚠ **`TC-34` is a ledger id and has nothing to do with ladder Slice 34**, which was CANCELLED (`seq-182`) |
| 16 | **TEMPORAL-MODEL SPLIT.** Node validity is **INTEGER epoch**; the shipped edge `t_valid`/`t_invalid` are **ISO-8601 TEXT**. Edges were **deliberately untouched**, and the divergence is **pinned by two tests** so it cannot drift silently | **TC-33** | **Accept long-term, or schedule a unifying slice.** Recorded as a deliberate divergence with an explicit migration note in the step-22 SQL — **not** an accident. Unifying is a breaking migration and belongs in its own slice if wanted |
| 17 | **Slice-10 governed-surface delta.** Adds commands `read.crossed_boundary_since` / `read.crossedBoundarySince` and types `ReadView`, `BoundaryCrossing`. *(Recorded at the time as PROPOSED / NOT SIGNED, exactly as Slice 5d recorded its own.)* | — | **✅ CLOSED — SIGNED** inside the batched governed-surface decision at steward **`seq-157`**, together with **AC-079** (#7). ~~"Sign or amend before publish."~~ **Recovery denylist UNCHANGED at five; AC-041 GREEN** |

Also logged by Slice 10 and **not** requiring a decision: **TC-35** (napi `#[napi(object)]` **OMITS** the property
for `Option::None` rather than emitting `null` — **measured, not reasoned**; drove the `9a6e4896` shape fix) and
**TC-36** (the published API docs still declare `SearchHit.id` as `int`/`number` "write_cursor" — **stale since
0.8.19 C-2** made it an `IdSpace`; a docs defect, pre-existing, not introduced here).

**Raised by Slice 15b** (details in §13):

| # | Decision | Ledger | Recommendation |
|---|---|---|---|
| 18 | **Error-variant choice.** An unsatisfiable window raises **`EngineError::InvalidArgument`** (carrying **both** bounds) rather than the **message-less `WriteValidation`** its sibling checks use. Python therefore raises **`InvalidArgumentError`** for an inverted window but **`WriteValidationError`** for a non-integer bound | — | **Deliberate** — a semantic violation is not a type-shape violation, and the caller needs the bounds in the message. But it **is** a family inconsistency. **One line plus tests to reverse**; cheapest to settle now, before the surface is signed |
| 19 | **`search` view is scoped to the VALIDITY AXIS ONLY.** `include_superseded` / `include_inactive` on a **search** view are a **typed refusal** | — | **Accept.** Search hydrates from **projection indexes that are not version-complete**, so there is **no truthful answer** to give — a refusal is honest where a silent partial answer would not be. **Reversible via one guard function** if the indexes later become version-complete |
| 20 | **Vector-cutoff limitation** (§13.2). Recall is restored **only within the 192-candidate bit-KNN pool**; with >192 expired near-neighbours the result set can still be short | — | **Either** accept the bounded-192-pool behaviour as the pre-existing ANN bound, **or** schedule the `canonical_nodes(write_cursor)` index + the `EXISTS` general fix. The latter is a **schema step**, and was **deliberately not taken inside a fix commit** |
| 21 | **The five read verbs still call `view.now_param()` directly.** **Correct today** — they are single-arm queries — but it is the **same latent shape** as the fix-3 defect: an instant re-read per arm rather than bound once per query | — | **Follow-up, not scope creep.** ~**24 call sites**. Recommended as its own small slice rather than folded into a fix round |
| 22 | **✅ SUPERSEDED 2026-07-24 — RESOLVED.** *(Original, as of Slice 15b:)* Slice 15 incomplete; R-20-PR/R-20-EAV/TC-33 not started; Slices 20/25 blocked. **Now: the keystone LANDED at `a2022957`** — R-20-PR + R-20-EAV + TC-33 all shipped; **Slices 20 and 25 are UNBLOCKED** (§8) | — | Done — the remainder was commissioned and landed as the keystone. **Next: Slice 20 (R-20-DR).** |

---

## 5. Process pins (bind every later slice)

### 5.1 TC-RUBRIC-7 — codex §9 transcript path (**PINNED**)

The rubric requires "a durable release-namespaced path" but names none. **Pinned for 0.8.20:**

```text
dev/plans/runs/codex/0.8.20/<slice>-<UTC-timestamp>.log
```

`.log`, not `.md` — matches the existing transcript convention (`0.8.16-slice-*-codex-review-*.log`) and keeps
raw transcripts out of markdownlint scope.

e.g. `dev/plans/runs/codex/0.8.20/slice-0-20260719T033434Z.log`. **Every slice persists its §9 transcript here**,
including fix-N re-reviews (one file per review round). Invoke codex **only** via
`dev/agent-tools/codex-nostdin.sh` (bare `codex exec` deadlocks on stdin).

### 5.2 TC-RUBRIC-5 — dedicated checkout

Orchestration and **all landing git-writes** run in a dedicated linked worktree. `scripts/preflight.sh --landing`
**HARD-fails on the primary checkout**, detecting primary via
`git rev-parse --git-dir` == `git rev-parse --git-common-dir`. **Built in Slice 0** (it did not previously exist).

### 5.3 Release DoD (every slice)

`cargo clippy --workspace --all-targets` **and** `cargo check --workspace --all-targets`, **both exit 0**.
Read the **real** exit code (`$?` / `PIPESTATUS`) — a trailing `echo` masking a non-zero exit is a live trap in
this repo, and **it recurred during Slice 0** (see §6.2).

---

## 6. R-20-EU7 baseline — **CLOSED BY DECISION; this whole section is HISTORICAL**

> 🕮 **⛔ SUPERSEDED — DO NOT ACT ON ANYTHING BELOW AS AN INSTRUCTION.** **R-20-EU7 was closed by ruling**
> (master **F-28** · steward `seq-84`): **ZERO eu7 runs, on any backend, at any N.** No slice — **Slice 40
> included** — captures a baseline, re-baselines, tunes `BATCH`, raises the drain timeout, or investigates the
> throughput shortfall **as 0.8.20 work**. The `#[ignore]` on `eu7_real_corpus_ac` (`:760`) is the enforcement
> and **STAYS**: it must not be removed, env-gated around, or opted into with `-- --ignored`.
>
> Everything in §6.1–§6.3 is the **Slice-0 investigation record** — the backend analysis, the two harness
> hazards, and the root-caused capture failure. It is retained because the findings are true and were
> expensive to obtain, and because the closure ruling rests on them. Every sentence phrased as *"Slice 40
> must…"* or *"Options (Slice 40)…"* describes the plan **as it stood before F-28** and is **dead**.

### 6.1 Backend constraint — **eu7 was a CPU same-backend gate** *(historical)*

| Backend | n=7667 vector-stage recall@10 | CI | vs 0.90 floor (one-sided `ci_hi >= floor`) |
|---|---|---|---|
| **CPU** (GA-signoff) | **0.8960** | [0.8640, **0.9250**] | **PASS** |
| **GPU** (0.8.14 run log `:73`) | **0.8330** | [0.8010, **0.8640**] | **FAIL** |

The GPU figure is a **cross-backend artifact**, not a regression (TC-5 re-baseline driver, scheduled 0.8.23).
**The HITL GPU-eval mandate does not apply to eu7** — by its own fidelity caveat. A GPU eu7 run would manufacture
a false regression. **Baseline captured on CPU.**

### 6.2 Two vacuous-green hazards found in the harness *(historical — no slice guards them; nothing runs eu7)*

1. **The documented run command is wrong.** `tests/eu7_real_corpus_ac.rs:85-86` omits the required `operator`
   feature ⇒ **exit 101**. Working: `--features default-embedder,operator`. *Fix the docstring in Slice 5.*
2. **The corpus is unreachable from a worktree.** `data/corpus-data/` is gitignored (`.gitignore:9`) and lives
   only in the primary checkout (2.1 GB). From a linked worktree the harness **SKIPS and exits 0**.
   ~~Slice 40 must assert a **non-skip witness**, not merely exit 0.~~ **DEAD (F-28):** the harness is not run
   at all, so there is no non-skip witness to assert. The `#[ignore]` is the guard, and it is verified **by
   inspection only** (§11.8).

*(For this capture the corpus was bridged into the orchestration worktree by symlink, excluded locally via
`.git/info/exclude`; no tracked file changed.)*

### 6.3 Captured numbers — **CAPTURE BLOCKED (root-caused)**

**No baseline was captured at `4ca70ba6`. Do not fabricate one, and do not carry the historical GA-signoff
figures forward as if they were measured here.**

**The eu7 harness cannot complete on CPU on this box.** Root-caused by bisecting N (three runs, real exit codes):

| Run | N | Real exit | Outcome |
|---|---|---|---|
| full | 7667 (batched 256) | **101** | panic `eu7_real_corpus_ac.rs:414` — `seed drain (batch): Scheduler` |
| probe | 200 | **101** | identical panic, same line |
| minimal | 20 | **0** | **PASSED** — vector-stage recall@10 = 1.0000, 258.54 s |

`drain(600_000)` → `wait_for_idle` timeout → `EngineError::Scheduler`.

**The worker is NOT wedged — it is throughput.** n=20 passed cleanly, so the embed/projection path is
functionally correct. That run reports **`seed_ms=111670` for 20 docs = 0.179 docs/sec**, about **7.3× slower**
than the **1.3 docs/sec** the harness docstring assumes (`:97-99`). At 0.179 docs/s a **`BATCH = 256`** needs
**~1430 s**, so it can **never** drain inside the hardcoded **600 s** — the harness is **structurally unable to
run here at any N**, because it fails on the **first batch**. A full 7667-doc seed would need **~11.9 hours**
even with the timeout raised.

Excluded causes: weights cache is **complete** (`config.json` + `tokenizer.json` + `model.safetensors`);
CPU load was **4.5 of 24 cores**.

**The tension as it stood at Slice 0:** §6.1 forbids GPU for comparability, and CPU cannot finish ⇒
**R-20-EU7 had no runnable path.**

🕮 **HOW IT WAS RESOLVED — BY DECISION, NOT BY MEASUREMENT.** ~~"Options (Slice 40): **(a)** reduce `BATCH`
256 → 64 (358 s, fits inside 600 s) or make `BATCH`/timeout env-tunable; **(b)** raise the drain timeout and
accept a ~12 h CPU run; **(c)** investigate the 7.3× shortfall. Recommend (a) + (c)."~~ **None of (a)/(b)/(c)
is 0.8.20 work.** The HITL closed **R-20-EU7 by ruling** (master **F-28** · steward `seq-84`): the basis is
**no-op** and there are **ZERO eu7 runs, any backend, any N**. Do **not** price a confirming run, and do
**not** re-derive these options — the no-runnable-path finding is *why* the ruling exists, not a task list.

**Side-effect hazard.** The harness **writes `dev/plans/runs/eu7-latest-measurements.json` into the repo on every
run**, so a reduced-N scouting run silently produces a file that *looks* authoritative — the n=20 run wrote
`recall=1.0000` there. It was **deleted, not committed**. Never commit it from a scouting run. *(TC-19)*

## 7. Outstanding worktrees

All Slice 0/5/10/15 worktrees are **reclaimable** — their work is landed on `origin/main`. 🕮 **SUPERSEDED:
Slice 39 has LANDED (`91db34d8`); the next commission is `SLICE-ID-HARDENING`** (§1). Every commission cuts a
fresh dedicated worktree off a verified `origin/main` tip per **TC-RUBRIC-5**. ⚠ **Two Slice-39 worktrees are
still on disk** — `fathomdb-slice39-changelog` and `fathomdb-slice39-docs` — and each carries commits **not
reachable from `main`** (2 and 4 respectively, measured 2026-07-30). Slice 39 landed as a fast-forward, so
that is expected pre-squash residue, **but confirm the content landed in `91db34d8` before removing either.** *(The table below is a Slice-0…15 snapshot and has not tracked the 20/21/22/23/25/30/31/32/33
worktrees — all of those slices have landed, so their worktrees are reclaimable too. Reconcile against a live
`git worktree list`, never against this table.)*

| Path | Branch | Purpose | State |
|---|---|---|---|
| `fathomdb-worktrees/orch-0.8.20` | `orch-0.8.20` | orchestration + Slice-0 docs (TC-RUBRIC-5) | **reclaimable** |
| `fathomdb-worktrees/slice-0-preflight-landing` | `slice-0-preflight-landing` | `preflight.sh --landing` guardrail | Slice 0 landed — **reclaimable** |
| `fathomdb-worktrees/orch-0.8.20-s5` | `orch-0.8.20-s5` | Slice 5 erasure completeness | Slice 5 **landed** (`1f8ed8bf`) — **reclaimable** |
| `fathomdb-worktrees/orch-0.8.20-s10` | `orch-0.8.20-s10` | Slice 10 `ReadView` + node-validity | Slice 10 **landed** (`3cfb3cda`) — **reclaimable** |
| `fathomdb-worktrees/orch-0.8.20-s15` | `orch-0.8.20-s15` | Slice 15 keystone (registry/EAV/TC-33/TC-34) | Slice 15 **landed** (`a2022957`) — **reclaimable** |

Clean up per `orchestration.md` §11 — **one destructive op per Bash call**; never `find -delete`. Verify the
actual `git worktree list` before removing anything (this table is a snapshot, not live state).

---

## 8. Recent decisions (newest first)

- **2026-07-30 — LICENSE: the project is MIT** (HITL, steward `seq-193`). The repo-root `LICENSE` (MIT,
  `ff8639e4`) is authoritative; the **four publishable manifests declaring Apache-2.0 are WRONG**
  (`Cargo.toml:30`, `src/python/pyproject.toml:11`, `src/ts/package.json:5`,
  `src/ts/npm/linux-x64-gnu/package.json:5`) and must read **MIT before the 0.8.20 tag** — crates.io versions
  are immutable. **No artifact ships a LICENSE file today.** Owned by **Slice 39 (R-20-DOC)**, minted into the
  ladder ahead of Slice 40 because `verify-release-gates.sh` check 4 hard-fails without a matching CHANGELOG
  heading.
  🕮 **EFFECTED — the two present-tense claims above are now FALSE at HEAD** and are retained only as the
  decision as it was taken. Slice 39 LANDED `91db34d8`: all four manifests read **MIT**, and a LICENSE ships
  in all **10** published units, with `scripts/check-license-consistency.sh` as the standing guard. Close
  record **§21**.
- **2026-07-30 — DOC-HYGIENE-3 COMPLETE** (`fd281358` machinery + `85d44c74` remainder). Six ids closed:
  TC-88, TC-89, TC-94, TC-100, TC-53, TC-92. Cross-cutting, **not** a ladder slice.
- **2026-07-29 — Slice 34 CANCELLED, then PARKED at 0.8.21** (`seq-182`, reversing `seq-178`; placement
  `seq-183`). **No `EngineError` `#[non_exhaustive]` / TC-103 work happens in 0.8.20**, and **Slice 34 is not
  in the ladder** — route no work through it. 0.8.21 is an odd micro ⇒ OOB label-only, so the attribute is
  **built at 0.8.21 and published at 0.8.22**, which thereby becomes a declared breaking venue. The scoping
  study survives as **TC-122**.
- **2026-07-29 — BATCHED GOVERNED-SURFACE DECISION SIGNED** (HITL, `seq-157`). Seven net-new allowlist
  members = **four logical verbs** plus non-command types; `recovery_denylist` unchanged at five.
  **AC-079 mints at Slice 40** — minting is not signing. **PUBLISH is NOT authorized by this signature** and
  is **not** gated by AC-079. Owed: the allowlist `_comment` re-pin (see §4).
- **2026-07-29 — Library Sweep #3 COMPLETE** — Slices 31 `d0287620`, 32 `31d33293`, 33 `f02dc5b4`. The survey
  is an **input to 0.8.22**; **no bump applied, no manifest or lockfile edited** in 0.8.20.
- **2026-07-24 — Slice 15 KEYSTONE LANDED** at **`a2022957`** (merge, in `origin/main`); ledger tip
  **`3264114a`** (steward seq-98). The full Phase-2 keystone: **R-20-PR** (row-owned projection registry,
  the C-1 co-land) + **R-20-EAV** (EAV / property-FTS via `canonical_attributes`, Slice 15d) + **`filterable`
  pre-KNN** vec0 routing (Slice 15e, **non-destructive** reshape per TC-46 Option 1) + **TC-33** (temporal model
  harmonised to **INTEGER epoch**; BYO-LLM extractor boundary keeps ISO-8601 with engine-side **hard-reject**
  round-trip normalisation, **TC-47**) + **TC-34** (from Slice 15b) + **Finding-1 (A)** (attribute-filter drops
  edge hits on both arms; **(D) reserved**, B/C declined) + **`#[non_exhaustive] SearchFilter`**. **SCHEMA →24**
  (keystone step 24). codex §9 **terminal-clean**; **gates re-verified by the Steward** — clippy 0, check 0,
  (A) pin 1/1, AC-041 3/3, denylist unchanged at five. **Slices 20 and 25 are NOW UNBLOCKED.** **TC-46, TC-47
  RESOLVED; TC-11, TC-32 CLOSED — do not re-open.** Governed-surface delta still **PROPOSED / NOT SIGNED**;
  **AC-079 sign-off gated to Slice 40** (plan §11 #1). **Immediate next: commission Slice 20 (R-20-DR).**
- **2026-07-24 — Slice 10 LANDED** at **`3cfb3cda`** (merge, in `origin/main`). R-20-RV + R-20-NV closed;
  **SCHEMA 21 → 22**; **TC-31 RESOLVED**; **TC-32 annotated** per the HITL ruling. Close record §12 (written
  when it was on-branch at `93a57b10`; that "not landed" framing is superseded).
- **2026-07-20 — Slice 10 COMPLETE on-branch** at **`93a57b10`**. **R-20-RV + R-20-NV closed**; **SCHEMA 21 → 22**
  (node validity window); **TC-31 RESOLVED** — `source_id` is now readable on **every** search-hit path, closing
  the measured "NO SDK EXPOSURE" erasure gap on the read side. **Two codex §9 terminal PASSes.** The Python
  failure Slice 5 attributed to TC-31 **now passes**. Opens **TC-33/TC-34** (§4 #15/#16) and logs **TC-35/TC-36**.
  Governed-surface delta **PROPOSED / NOT SIGNED**; **no AC minted**. **Zero eu7 runs.** (§12)
- **2026-07-20 — TC-32 ACCEPTED AS-IS, no behavior change** (HITL). Co-named-entity dedupe is **annotated, not
  fixed** (`e62309e1`). **The erasure guarantee must NOT be stated unconditionally to users while it stands.**
- **2026-07-20 — Slice 5 LANDED** at **`1f8ed8bf`**, in `origin/main`. *(As written then: "AC-079 is still
  UNSIGNED and still blocks publish — landing the code did not discharge the sign-off.")* 🕮 **SUPERSEDED:**
  AC-079 was **pre-signed 2026-07-25 (F-34)** and the batched delta **SIGNED at `seq-157`**; **AC-079 does not
  block publish.**
- **2026-07-20 — fix-7: the test-hooks probe was NARROWER than the surface it gated** (`7c353ac5`). It checked
  one of three symbols, so a **partial** binding read as "hooks present" and a marked test **failed on a
  missing import instead of skipping**. Now probes all three, fails safe to DEGRADED, and carries a drift guard
  against `lib.rs`. Found by isolated-clone verification **after** codex's terminal PASS. (§11.10)
- **2026-07-20 — fix-6: codex found a P2 in our OWN TC-27 guard** (`5452016f`, `d710721a`). fix-4's env-var
  guard turned a silent-rebuild hazard into a **permanently red default pytest path**. TC-27 is now stated
  **positively** as a pure policy function, and the load-bearing check is `venv_belongs_to_source_tree()` —
  **the opt-in env var cannot override it**, so the shared `.venv` is protected **structurally**. codex
  returned a **terminal PASS** on the delta. **TC-27 RESOLVED** (ledger seq-48); **TC-16 corrected**
  (seq-49: the dead assertion is in `test_actionlint_fixture.sh`, aborting `agent-test.sh` at line 63 **before**
  the Rust and Python steps, so its exit code is not a suite verdict). **Lesson: a guard that breaks the
  documented default path is a worse defect than the hazard it closes.** (§11.9)
- **2026-07-20 — Slice 5 RE-CLOSED after a post-closure REGRESSION** (fix-4, `9c87d758`). Independent Steward
  verification — **fresh clone, isolated venv, A/B against `origin/main`** — found multi-document
  `ingest_with_extractor` failing with `ExtractorError`. **The engine was not the defect and was not changed**
  (`engine/src/lib.rs` byte-identical across fix-4); the **extractor protocol** was, in never requiring
  per-entity attribution. **Behavioral contract change:** multi-doc extractor batches now require **per-entity
  attribution**; a caller who cannot attribute must submit **single-document batches**. Also shipped the TC-20
  eu7 hard gate and the TC-27 `maturin` opt-in guard. New: **TC-31**, **TC-32** (§4 #13/#14). **Lesson: a codex
  PASS plus four green on-branch gate runs did not substitute for one honest execution.** (§11.7/§11.8)
- **2026-07-20 — Slice 5 CODE-COMPLETE** on `orch-0.8.20-s5` @ `8e09b950`; **codex §9 terminal PASS** after three
  fix rounds (§11). Proved the **`logical_id IS NULL ONLY` backfill rule wrong for EDGES** (TC-26); shipped the
  HITL-ruled erasure-audit retention exemption (§4 #3). **Six HITL items owed** (§4 #7–#12) — *as written
  then,* "AC-079 is **NOT signed** and **blocks publish**"; main-tree Python X1 is a **landing blocker**.
  🕮 **SUPERSEDED:** AC-079 is **SIGNED** (`seq-157`) and does **not** block publish; the X1 blocker was
  discharged (§11.8).
- **2026-07-19 — Slice-0 HITL-SIGNED and landed** at `403eb254`. X0 gate open; slices 5+ authorized.
- **2026-07-19 — Slice-0 (this board):** eu7 baseline pinned to **CPU same-backend**; TC-RUBRIC-7 transcript path
  pinned; AC allocation recommended from **AC-079** (reserved-id collision found); **four defects found in the v4
  design of record** (design §2), incl. the **non-durable erasure audit trail**.
- 2026-07-12 — **TC-11 pin A RATIFIED** (HITL). Anonymous nodes stay `h:` permanently; §2(ii) **OVERRULED**;
  surrogate leg **CANCELLED** for the anonymous class. **CLOSED — do not re-open.**
- 2026-07-12 — **REQ-037 lawful-erasure carve-out APPROVED** (HITL). `excise_source` stays CLI-only;
  **`erase_source()` ships as an SDK lifecycle verb**; AC-041 unchanged and stays GREEN.
- 2026-07-11 — TC-RUBRIC-5 dedicated-checkout guardrail ADOPTED (HITL).
- 2026-07-11 — Erasure axis = **PROVENANCE**, not the `l:`/`h:` id-space (HITL steer).
- 2026-07-10 — RUBRIC-H7 `can-i-deploy` gate; **absent-or-failing gate HOLDS the breaking pair** (HITL).
- 2026-07-09 — F-22 open-TC schedule; 2026-07-08 — F-21 build-authorized (build ≠ adopt);
  2026-07-07 — F-19/F-20 scope.

---

## 9. Compaction-resume checklist

1. `git -C fathomdb-worktrees/orch-0.8.20 rev-parse --abbrev-ref HEAD` ⇒ must be `orch-0.8.20`.
2. Read this board §1 (current slice) + §4 (open HITL decisions).
3. Read `plan-0.8.20.md` §4 (ladder) + `0.8.20-slice0-erasure-design.md` §2 (defects) and §4 (work items).
4. `git worktree list` — reconcile against §7.
5. **Never** trust a "green" without a printed real exit code.
6. **TC-11 is CLOSED.** Do not re-open the `h:` pin.

---

## 10. Slice-0 findings the Steward must reconcile into the master

Slice 0 proved several things in `plan-0.8.20.md` / the v4 design wrong or under-specified. **The orchestrator did
not edit the master plan** — these are handed up for reconciliation.

| # | Where | Finding | Ledger |
|---|---|---|---|
| 1 | plan §3 | **"AC ceiling = AC-077, continue from it"** is a reserved-id collision. AC-077 is a *reserved placeholder* for IR-1/IR-2; **AC-078 is conditionally reserved to the same initiative**. Highest defined non-reserved AC = **AC-076**. Recommend minting from **AC-079**. | **TC-14** |
| 2 | plan §8 / v4 §3.6 | **"`source_id` retained permanently in `excise_source_audit`" is FALSE.** `enforce_provenance_retention` (`:10070`) sweeps `operational_mutations` with **no collection filter** (`:10083`), so audit rows are swept like any other. The erasure **audit trail is destructible**, and it shares a retention pool with the op-store payloads work-item 9 must erase. | **TC-15** |
| 3 | plan §0.1 / v4 §2.2 | Registry model too coarse. The write path **enqueues** vector work (`_fathomdb_projection_state`, **`kind TEXT PRIMARY KEY`** — verified) rather than projecting it. Registry must split **row-owned** (`write_cursor`-keyed) from **kind-owned**, or the guard demands a per-cursor delete on a kind-keyed table. | — |
| 4 | v4 §1/§2.2/§6 | The registry consumer is **`rebuild_shadow_state` (:6515)**, not `rebuild_projections` (:5949, the public entry). Taking v4 literally patches the wrong function. | — |
| 5 | plan §0 / v4 §3.4 | `derive_logical_id` **lowercases** its inputs (`:11156`). Strengthens the dictionary-attack rationale; the stated derivation is incomplete. | — |
| 6 | plan §7 prereq 4 | **"Baseline captured" was listed as an assumed precondition — no baseline existed.** Capture attempted at Slice 0 and is **BLOCKED, root-caused** (§6.3): the harness's `BATCH = 256` cannot drain inside its hardcoded 600 s at the measured **0.179 docs/s** (~7.3× below the documented rate), so it fails on the **first batch at any N**. Combined with §6.1 (GPU forbidden for comparability), **R-20-EU7 has no runnable path today.** | **TC-13**, **TC-19** |
| 7 | R-20-PUB | **The publish dry-run guard is DEAD and has been red since 0.8.14.** `test_actionlint_fixture.sh:53` greps `release.yml` for `cargo publish --dry-run -p`, but the job now delegates to `cargo-publish-if-new.sh --dry-run`. **Behavior is intact** (the helper forwards correctly) — but `./scripts/agent-test.sh` exits 1 wholesale, so a **real** publish-wiring regression would be invisible **in the first release that publishes for real**. **⚠ CORRECTED (ledger seq-49):** the dead assertion is **NOT** in `test_pypi_publish_roundtrip.sh` (that script passes cleanly) — it is in **`scripts/tests/test_actionlint_fixture.sh`**, invoked at **`scripts/agent-test.sh` line 63**. Because `set -euo pipefail` aborts there, **`agent-test.sh` never reaches the Rust or Python steps**, so its aggregate exit code says **NOTHING** about whether those suites pass. Confirmed **pre-existing**: that script and `.github/workflows/release.yml` are byte-identical to `origin/main`. | **TC-16** |
| 8 | v4 §3.2 | **Slice 5's `SourceId` newtype will break the eu7 harness** (`eu7_real_corpus_ac.rs:405` builds `PreparedWrite` with `source_id: None`). v4 enumerated only two internal callers and missed the test-side ones. Sweep `src/` **and** `tests/`. | **TC-17** |
| 9 | TC-RUBRIC-7 | Committing a §9 transcript **into the reviewed range** pollutes the next review's diff (codex re-read its own prior findings as if unfixed). Recommend committing transcripts **after** the final review round. | **TC-18** |

**Also carried:** the eu7 basis and `embed_batch_cls` decisions (§4 #1/#2) remain **HITL calls**, recorded with
recommendations, not decided here.

---

## 11. Slice 5 close — erasure completeness (R-20-E1…E8)

> 🕮 **HISTORICAL CLOSE RECORD — written while the work was on-branch. Slice 5 LANDED at `1f8ed8bf`
> (2026-07-20).** Its "**Not landed**" wording below is the on-branch state at the time, not current truth.
> Its **"AC-079 blocks publish / still NOT SIGNED"** statements are **SUPERSEDED**: AC-079 was pre-signed
> 2026-07-25 (master **F-34**) and the batched governed-surface delta was **SIGNED at steward `seq-157`**;
> **publish is gated by the separate HITL publish gate, not by AC-079.**

**Branch `orch-0.8.20-s5`, terminal HEAD `d710721a` + fix-7** — cut from `origin/main` `19b568e2`, rebased onto
`30ad3524`. **Not landed.** The Steward lands it.

**⚠ The first closure at `8e09b950` was premature.** Post-`8e09b950` history:

| Commit | Content |
|---|---|
| `9898fd8e` | ledger TC-30 + Slice-5 docs closure artifact |
| `ff2d641c` | **fix-4** — RED: multi-document extractor batches require per-entity attribution |
| `9550bcde` | **fix-4** — GREEN: ELPS harness must attribute ENTITIES, not just edges |
| `265c54c0` | **fix-4** — tooling guards: TC-20 eu7 hard gate, TC-27 `maturin` opt-in |
| `9c87d758` | **fix-4** — docs: the per-entity-attribution contract |
| `93eca45a` | **fix-5** — `cfg`-gate `is_erasure_bookkeeping_collection` (non-`operator` `dead_code`) |
| `5452016f` | **fix-6** — codex **P2**: fix-4's TC-27 guard broke the default pytest path; restate the policy positively (§11.9) |
| `d710721a` | **fix-6** — docs/ledger for the above |
| `7c353ac5` | **fix-7** — probe **all three** test-hook symbols so a partial binding DEGRADES (§11.10) |

### 11.1 What shipped

| Sub-slice | Head | Content |
|---|---|---|
| **5a** | `bdd8750e` | **R-20-E1** — row-owned projection registry + **total** node/edge projectors; the five hand-rolled projection lists deleted |
| **5b** | `18197495` | **R-20-E5/E6/E7** — WAL truncation on erasure · selective telemetry redaction · record-level op-store erasure · erasure-audit durability |
| **5c** | `875017a2` | **R-20-E2/E3/E8** — `SourceId` newtype (**BREAKING**) · reserved `_engine:` / `_legacy:` provenance · **caller-grounded** ingest provenance |
| **5d** | `4b78658d` | **R-20-E4** — `erase_source` SDK verb (Py + TS + Rust) · `doctor orphan-provenance` · governed-surface delta · user docs |
| fix-1 | `00b46b84` | codex **P1** legacy-edge backfill (via `989fd7ef`) + **P2** durable pending-redaction queue |
| fix-2 | `7be20ec3` | codex **P2** doctor edge accounting + **P2** drain-before-freeze |
| fix-3 | `8e09b950` | codex **P1** refuse excising erasure bookkeeping + **P2** rotated-sink ⇒ `ErasureIncomplete` |
| **fix-4** | `9c87d758` | **REGRESSION** — multi-doc extractor batches require **per-entity attribution** (§11.7) · TC-20 eu7 hard gate · TC-27 `maturin` opt-in guard |
| fix-5 | `93eca45a` | `cfg`-gate `is_erasure_bookkeeping_collection` — the fix-3 guard lacked the `#[cfg(feature = "operator")]` its only call site carries, warning `dead_code` on every non-`operator` build. Behavior unchanged |
| **fix-6** | `d710721a` | **codex P2 in our OWN TC-27 guard** — fix-4 turned a silent-rebuild hazard into a **permanently red default pytest path**. Policy restated positively as a pure function; the ownership check, not the env var, is load-bearing (§11.9) |
| fix-7 | `7c353ac5` | The test-hooks probe checked **one** of the **three** symbols it gates, so a partial binding read as PROCEED and a marked test **failed instead of skipping**. Probe all three; fail safe to DEGRADED (§11.10) |

The central defect this slice closes: **`search_index_v2` stores the body**, so before R-20-E1 an excised body
**survived erasure** in that table. It never surfaced in results (both read paths gate on `canonical_nodes`),
which is exactly why it went unnoticed — a data-at-rest leak, invisible to any result-level assertion.

### 11.2 codex §9 — four rounds on the branch, then two delta rounds, terminal PASS

Transcripts under `dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7 path), committed **after** the final round per
TC-18.

| Round | Transcript | Verdict |
|---|---|---|
| 1 | `slice-5-20260719T231341Z.log` | **P1** legacy edges erasable by **no verb**; **P2** telemetry-redaction retry falsely reports success |
| 2 | `slice-5-fix-1-rereview-20260719T234803Z.log` | P1 cleared; **P2** doctor gives false assurance on unerasable edges; **P2** freeze-before-drain timeout |
| 3 | `slice-5-fix-2-rereview-20260720T001616Z.log` | **P1** `excise_collection_record` could delete the pending-redaction queue; **P2** rotated sink treated as redacted |
| 4 | `slice-5-fix-3-rereview-20260720T005056Z.log` | **TERMINAL PASS** — *"No actionable correctness issues were found in the reviewed diff. The added erasure/provenance paths appear consistently wired through Rust, Python, TypeScript, CLI, schema migration, and tests."* |
| 5 (fix-4/5 delta) | `slice-5-fix-4-5-delta-20260720T022544Z.log` | **P2** — the fix-4 TC-27 guard **broke the documented default pytest path** (import-time raise before collection). Fixed in fix-6 (§11.9) |
| 6 (fix-6 delta) | `slice-5-fix-6-rereview-20260720T024726Z.log` | **TERMINAL PASS on the delta** |

**Read the round count honestly:** rounds 1–4 reviewed the **full branch** (P1+P2 → P2+P2 → P1+P2 → PASS);
rounds 5–6 reviewed only the **fix-4/5 and fix-6 deltas**. **fix-7 has NOT been through codex** — it was found
by isolated-clone verification after the terminal PASS and is covered by §11.10's executed evidence.

### 11.3 Gates — re-verified on the terminal HEAD (real exit codes)

Re-run at **fix-7** (`7c353ac5`). Read via `$?` / `PIPESTATUS`, never a trailing `echo`.

**⚠ Invocation matters — a bare invocation of the first two is NOT a run.** `erasure_projection_registry` and
`provenance_mandatory` live in **`fathomdb-engine`** and **require `--features operator`** (without it, `cargo
test` exits **101**). `sdk_only_erasure` lives in **`fathomdb`** and needs the explicit
`cargo test -p fathomdb --test sdk_only_erasure` (TC-25).

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **0** — and **zero `dead_code`** on a non-`operator` build (fix-5) |
| `cargo check --workspace --all-targets` | **0** |
| `erasure_projection_registry` | **4/4** — `-p fathomdb-engine --features operator` |
| `provenance_mandatory` | **3/3** — `-p fathomdb-engine --features operator` |
| `multidoc_extractor_provenance` (**fix-4**) | **5/5** |
| `erasure_completeness` (AC-080) | **10/10** |
| `sdk_only_erasure` | **3/3** — via **explicit non-operator invocation** (TC-25: it is `#![cfg(not(feature = "operator"))]`, so any feature-unified run compiles it to **zero** tests and reports success having asserted nothing) |
| `no_recovery_surface` (**AC-041**) | **1/1** — denylist unchanged at five |
| `governed_surface` | **3/3** — *against an unsigned proposal*, see §3 |
| `fathomdb-schema`, all targets | green, incl. new `step21_migration.rs` **5/5** |
| `fathomdb-cli`, all targets | green |
| TypeScript | **170/170**, `tsc` **0** |
| **Python** | **`2 failed, 766 passed, 7 skipped`** (hooks available) in an **isolated fresh clone** — failure set **identical to `origin/main`**, both pre-existing (**TC-31**). See **§11.8**; do **not** read this row without it |
| `ruff check src/python` | **0**; `py_compile` clean on every file fix-7 touched |
| `test_test_hooks_gate.py` (fix-7) | **20/20** — synthetic complete / partial / import-failure bindings; **no compiled extension required** |
| `SCHEMA_VERSION` | **20 → 21** |

**⚠ `scripts/agent-test.sh`'s aggregate exit code is NOT a suite verdict** — it aborts at line 63 on the
pre-existing dead publish assertion (**TC-16**, §10 #7) and never reaches the Rust or Python steps. Gate on the
individual commands above, not on that script. The **invocation** `agent-test.sh` uses for pytest was run
directly and is healthy (**`2 failed, 766 passed, 7 skipped`**); `cargo test --workspace --no-fail-fast` exits
**0** across 148 test binaries.

### 11.4 What Slice 5 proved WRONG

1. **The `logical_id IS NULL ONLY` backfill rule is correct for NODES and WRONG for EDGES** *(codex P1; ledger
   **TC-26**)*. `purge_inner` resolves its target **exclusively** via
   `SELECT state FROM canonical_nodes WHERE logical_id = ?1`, then erases edges by **endpoint**
   (`from_id`/`to_id`). It **never** resolves an edge by edge `logical_id` — an edge's `logical_id` is only a
   **supersession identity** and confers **no purge-addressability whatsoever**. So a legacy edge with
   `source_id IS NULL AND logical_id IS NOT NULL` was unreachable by `excise_source`/`erase_source` (no
   provenance) **and** unreachable by `purge` (not addressable) — **erasable by no verb at all**, disappearing
   only incidentally when a connected node happened to be purged. That defeats R-20-E8's entire purpose.
   **Shipped step 21 is deliberately asymmetric:** nodes keep the `logical_id IS NULL` gate; edges back-fill on
   `source_id IS NULL` alone. **TC-11's pin is NOT affected** — the statement *reads* `logical_id` as its
   predicate and **never writes one**; no row transitions `logical_id` NULL → NOT NULL and no stored row's
   id-space is re-derived (`s21_backfill_populates_no_logical_id` asserts both).
   ⚠ **`plan-0.8.20.md` R-20-E8 (`:197`) and the v4/v5 design prose still state the unqualified rule and must be
   corrected** (§4 #8). The code is right; the design of record is not.
2. **v4 §3.6's "the audit retains `source_id` permanently — by design"** was already known false at Slice 0
   (§10 #2 / TC-15): `enforce_provenance_retention` swept `operational_mutations` with **no collection filter**,
   so the erasure audit trail was destructible. Slice 5 implements the **HITL-ruled** fix (§4 #3) — the
   erasure-audit collections are **exempt** from the sweep, and so is the new pending-redaction queue.
   Consequence, and it is a **behaviour change to a shipped knob** (**TC-24**): `cap` now bounds **sweepable**
   rows, not physical rows. An operator who sized `cap` against a physical row count will see the table exceed
   it. Changelogged.

### 11.5 Owed to the HITL / Steward

In §4 as decisions **#7–#14**, with ledger ids: **AC-079 sign-off** *(recorded then as "blocks publish, still
NOT SIGNED" — 🕮 **SUPERSEDED: SIGNED at `seq-157`; it does not block publish**)* ·
**design-text correction** TC-26 · **eu7 guard shape** TC-20 (guard now **shipped**) · **Python X1** TC-22
(**discharged**, §11.8) · **`maturin develop` conftest guard** TC-27 (**shipped**) · **pending-redaction
structural hardening** TC-28 · **write/read provenance asymmetry** TC-31 · **entity-dedupe erasure gap** TC-32.
Also logged by this slice and **not** requiring a decision: **TC-21** (`pr_g10_reranker_ce` has not compiled
under `--features default-reranker` since 0.8.19 — **pre-existing**, file byte-identical to baseline; it survived
because the release-DoD full-workspace gate does **not** fan out over feature combinations), **TC-23**
(untracked closure `output.json` artifacts are destructible by routine git hygiene — it happened **twice** in
this slice; implementers should **commit** their closure witness), **TC-25** (the `sdk_only_erasure`
vacuous-green hazard above — **CI must carry the explicit invocation** or the R-20-E4 guarantee is untested),
and **TC-29** (`run_rebuild` is the last remaining freeze-before-drain instance, unaudited; and
`operator_cli::t_s34_dump_mutations_lock_held_exits_71` is flaky under cross-binary lock contention — touches no
erasure path).

### 11.6 Closure artifacts

`dev/plans/runs/0.8.20-slice-5{a,b,c,d}-output.json` and
`dev/plans/runs/0.8.20-slice-5-fix-{1,2,3,4,5}-output.json` (nine), plus the four §9 transcripts in §11.2.
Committed with this close **(TC-23** — an untracked closure witness is destructible by routine git hygiene, and
**two implementers in this slice destroyed work exactly that way**; the witness gets committed, not left loose).

### 11.7 fix-4 — the REGRESSION found AFTER the first closure

**Found by independent Steward verification** — a **fresh clone with its own isolated venv**, run **A/B against
`origin/main`**. Not by codex (four rounds, terminal PASS), and not by any on-branch gate.

**What broke.** Multi-document `ingest_with_extractor` failed with `ExtractorError`.

**Mechanism.** `resolve_provenance` (`engine/src/lib.rs:3933-3943`) admits the model's echo **only as a
SELECTOR** among the caller-supplied batch ids: a single-document batch short-circuits to the one caller id, but
on a **multi-document** batch the echo **must name one of them** or the ingest fails loudly. Meanwhile
`src/python/eval/elps_live_harness.py` backfilled `source_doc_id` onto **edges only**, and the stub entities
carried none — so **every multi-doc batch failed at the entity loop** (`lib.rs:3972`, resolving at `:3979`).

**The engine was NOT the defect and was NOT changed.** `engine/src/lib.rs` is **byte-identical across all of
fix-4** (verified by `git diff 9898fd8e..9c87d758 -- src/rust/crates/fathomdb-engine/src/lib.rs`, empty).
Accepting the echo as a **value** rather than a selector is precisely the **R-20-E2 defect this slice exists to
fix** — provenance must be **caller-grounded**, never taken from the LLM's own echo. **Failing loudly is
correct.** The defect was in the **extractor protocol**, which never required per-entity attribution.

**The fix (contract side, `9550bcde`).**

1. Entities are now backfilled **symmetrically with edges** (`elps_live_harness.py:233-237`).
2. `_STUB_ENTITIES` (a module-level list) became **`_stub_entities(doc_id)`** returning **fresh dicts** (`:99`).
   The module-level list was a **latent aliasing bug**: backfilling it in place would have let the **last**
   document overwrite **every earlier document's** provenance — silently mis-attributing, which for an erasure
   slice is worse than the loud failure.
3. The per-entity requirement is now **explicit in the extractor prompt and schema** (`:42`, `:46`, `:70`).
4. A new **engine-level multi-doc test target** was added — `multidoc_extractor_provenance` (**5/5**).

**⚠ BEHAVIORAL CONTRACT CHANGE — record it.** Multi-document extractor batches now require **per-entity
attribution**. **A caller whose extractor cannot attribute must submit single-document batches.**

**The coverage gap that let it through: every existing extractor test was SINGLE-DOC only** — and a single-doc
batch takes the short-circuit path that never consults the echo at all. The regression was invisible by
construction.

### 11.8 Python verification — the honest number

Full suite, **isolated fresh clone with its own venv** (never the shared `.venv`). **Re-executed at `d710721a`**
across all three environment states — the numbers below are **runs, not reasoning**:

```text
hooks available (in-tree venv)      2 failed, 766 passed,  7 skipped   ·   exit 1
default path (hook-less, no opt-in) 1 failed, 762 passed, 12 skipped   ·   exit 1
degraded (FATHOMDB_TESTS_NO_REBUILD=1) 1 failed, 762 passed, 12 skipped · exit 1
```

**What each state proves.**

- **Default path is NOT red-by-construction any more** (the fix-4 defect, §11.9): collection **succeeds**, 775
  items, **no import-time raise**, and the degraded banner is on screen before the first test.
- **The hook-dependent tests genuinely RAN and PASSED** when hooks were available — **verified three ways**,
  including an explicit verbose re-run. They did **not** skip. This is the check that distinguishes a real pass
  from a vacuous one.
- **Degraded is not a session-wide self-skip:** **exactly two** marker skips, each with a clear reason, and
  **762 tests still ran**. The extra skips vs the hooks-available run are the two markers plus three
  `test_verify_embed_db` tests whose module-scoped fixture cannot build a real embed DB without the hooks.
- **The ownership check holds:** in-clone venv → owned; **shared `/home/coreyt/projects/fathomdb/.venv` → NOT
  owned**; worktree venv → not owned; and `decide(allow_rebuild=True, venv_owned=False)` → **`degraded`, not
  `rebuild`**. **The opt-in env var cannot override it.**

**This is NOT a regression, and this board says why.**

- **The identical two tests fail on `origin/main` `f22e4947`** in the same isolated clone (targeted main run:
  `2 failed, 8 passed`). **The branch's failure set is identical to main's.**
- An expected **"755 passed / 1 failed" was NOT REACHABLE.** It assumed
  `test_option2_elps_pipeline::test_build_fathomdb_elps_path_uses_ingest_with_extractor` would go green once the
  ingest regression was fixed — but **that test also fails on main**, at a later point.
- **The regression IS fixed.** The ELPS test now gets **all the way past ingest** (`blocker is None`,
  `adapter is not None`, `_use_graph_arm is True`, `db.exists()` all pass) and dies at `adapter.retrieve(...)`
  — **the same pre-existing failure point as main**.
- **Both remaining failures share ONE pre-existing root cause** (**TC-31**, §4 #13): `PySearchHit.source_id` is
  populated **only for graph-arm hits** (`fathomdb-py/src/lib.rs:537-539`), so `_doc_id_of` falls through to
  `int(sh.id)`, which has raised `TypeError` since 0.8.19 made `SearchHit.id` an `IdSpace` (C-2). `_doc_id_of`
  is **byte-identical on main**. **One read-side fix likely closes both.** Scheduled separately —
  **out of scope for Slice 5.** Re-verified at fix-7: `git diff origin/main...HEAD` is **empty** over
  `src/python/tests/test_option2_elps_pipeline.py`, `src/python/tests/test_verify_embed_db.py` **and**
  `eval/r2_parity_eval.py` — this branch did not touch the failing surface at all.
- **7 skips, all environmental/opt-in**: musique corpus absent (`data/corpus-data/` is gitignored),
  `RELEASE_SURFACE_TESTS != 1`, `FDB_S15A_INTEGRATION` opt-in. **No skip masked a pass; no skip came from a
  missing binding.**
- **Shared `.venv` integrity verified intact:** `/home/coreyt/projects/fathomdb/.venv/.../fathomdb.pth` mtime
  still **2026-07-09**, content still `/home/coreyt/projects/fathomdb/src/python`; the shared `.so` untouched.
  Re-verified after **every** round through fix-7.

**TC-20 eu7 hard gate — verified by INSPECTION ONLY, with ZERO eu7 runs.** `eu7_real_corpus_ac` now carries
`#[ignore]` (`:760`) and `scripts/agent-test.sh` can no longer invoke it; a **control** was run to prove the
check was not vacuous. The prohibition on running eu7 was honored in the course of enforcing it.

### 11.9 fix-6 — codex found a P2 in our OWN TC-27 guard

**The guard for a hazard became a worse defect than the hazard.** fix-4 closed the autonomous-`maturin develop`
hole by **raising at import time** when a rebuild was not authorized. But `conftest.py` is imported before
collection, so a clean, **documented** checkout — `pip install -e 'src/python[dev]'`, whose
`[tool.maturin] features` deliberately ships **no** `test-hooks` surface — raised **before a single test was
collected**. The default path was **permanently red**, and the fix traded a silent-corruption risk for a
guaranteed outage.

**fix-6 restates TC-27 positively.** The policy is now a **pure function** —
`src/python/tests/_test_hooks_gate.py`, no I/O, no environment, no subprocess — returning one of
`PROCEED` / `REBUILD` / `DEGRADED` / `CONTRADICTORY`. It is unit-tested **without a binding, a venv, or a
build**, which is exactly the configuration the policy exists to handle.

**The load-bearing check is `venv_belongs_to_source_tree()`, NOT the env var.** `maturin develop` may run
**only** when the venv prefix lies **inside the repo root** that owns `src/python`. The opt-in env var
**cannot override it**: `decide(allow_rebuild=True, venv_owned=False)` → **`degraded`**. The shared `.venv` is
therefore protected **structurally**, by the shape of the filesystem, rather than by an environment variable
someone might export. `scripts/agent-test.sh` now sets the opt-in **itself**, when it has selected the in-tree
`.venv` — the authorization is issued by the thing that knows it is safe.

A missing surface **degrades**: the suite runs, and only the tests marked `@pytest.mark.requires_test_hooks`
skip — visibly, with the reason, plus a banner in the pytest header at any verbosity.

### 11.10 fix-7 — the probe was narrower than the surface it gated

**Found by isolated-clone verification on a real `.so`, after codex's terminal PASS.**

`_binding_has_test_hooks()` probed **one** symbol, `Engine._write_vector_for_test`, while the gate it drives
protects **three** (`src/rust/crates/fathomdb-py/src/lib.rs`, each behind
`#[cfg(any(test, feature = "test-hooks"))]`): `Engine._configure_vector_kind_for_test` (`:1239`),
`Engine._write_vector_for_test` (`:1247`), and module-level `force_panic_for_test` (`:2038`).

**The observed failure mode.** A binding can carry both `Engine` methods while module-level
`force_panic_for_test` is **absent** — reachable from a stale or interrupted build. The single-symbol probe
called that **"hooks present" ⇒ PROCEED**, so the `requires_test_hooks` skips did **not** apply and
`test_panic_surfaces_as_python_exception` **failed on a missing import instead of skipping cleanly**. The
narrow probe is the classic vacuous-gate shape: a check weaker than the thing it certifies.

**The fix.** The probed set lives next to the gate as `TEST_HOOK_SYMBOLS`, so it cannot drift from the surface
it gates, and the surface counts as present only if **all** of it is. A partial binding yields **DEGRADED** and
leads its reason with what is actually missing — *"built WITHOUT test-hooks"* is the wrong diagnosis for it. A
crashed or unparseable probe **fails safe** to "the whole surface is absent": DEGRADED, never PROCEED.

**Evidence is executed, not reasoned:** the probe was run against **synthetic** bindings — complete, partial
(both `Engine` methods, no `force_panic_for_test`), and import-failure — so the tests need **no compiled
extension**. `test_test_hooks_gate.py` **20/20**. A drift guard asserts each probed symbol is still
`test-hooks`-gated in `lib.rs`. The three fix-6 requirements were re-verified and hold: the default path still
collects and runs, the ownership check is untouched and still un-overridable, and a missing surface still
produces a visible skip.

**Left deliberately unfixed (cosmetic, and NOT worth the risk).** Three `test_verify_embed_db.py` tests depend
on the hook surface without carrying the marker, so in degraded mode they skip with an internal error string
(`'Engine' object has no attribute '_configure_vector_kind_for_test'`) rather than the gate's reason. **They do
skip visibly** — this is presentation only. It was left alone because `test_verify_embed_db.py` is currently
**byte-identical to `origin/main`**, and that identity is load-bearing evidence for the **TC-31**
pre-existing-failure attribution above. Editing it for cosmetics would destroy the proof.

---

## 12. Slice 10 close — `ReadView` / read-modes + node-validity (R-20-RV, R-20-NV)

> **✅ SUPERSEDED 2026-07-24 — Slice 10 LANDED at `3cfb3cda`** (merge, in `origin/main`). This section is the
> historical close record, written while the work was on-branch; its "NOT landed" banner is no longer current.

**Branch `orch-0.8.20-s10`, terminal HEAD `93a57b10`** — rebased onto `origin/main` **`ae44770f`**.
**COMPLETE on-branch** *(landed 2026-07-24 at `3cfb3cda`; the "NOT landed" note below is historical)*.

**R-20-RV and R-20-NV are CLOSED. TC-31 is RESOLVED. TC-32 is ANNOTATED** per the HITL ruling (accepted, no
behavior change). **No AC was minted** — see §3.

### 12.1 What shipped

| Commit | Content |
|---|---|
| `f29f7d91` | **RED** — `source_id` must be readable on every search-hit path |
| `63dfbc08` | **TC-31 fix** — populate `SearchHit.source_id` on **every** hit path, not just the graph arm |
| `e62309e1` | **TC-32** — annotate the accepted single-provenance entity dedupe |
| `b90c9a0d` | **TC-31** — IdSpace-safe doc-id resolution at the two remaining eval sites |
| `9392dbc5` | **TC-31 fix-1** — correct the remaining stale "`source_id` is graph-arm-only" contract text |
| `43ae248f` | Slice-10a closure artifacts + the codex §9 PASS transcript |
| `9c6420e5` | **R-20-NV** — schema **step 22**, `canonical_nodes` validity window (**SCHEMA 21 → 22**) |
| `e3cc071b` | **R-20-RV/R-20-NV** — thread `ReadView` through **all five** read verbs + both bindings |
| `4524ffd2` | Read-mode + validity matrices; Py/TS parity; the surface delta |
| `e069e3a9` | Record the `ReadView` / `BoundaryCrossing` surface delta (Rust docs) |
| `c5e12da6` | Slice-10b closure artifact |
| `742a347e` | `BoundaryCrossing` boundaries are `number \| null`, not `?: number` — **superseded by fix-3** |
| `14d33bba` | **X1** — live Py + TS functional harnesses for the read-view surface |
| `073b2d3a` | Slice-10b fix-2 closure artifact — X1 binding-execution parity |
| `9a6e4896` | **fix-3 (TC-35)** — napi **OMITS** `None` `Option` object fields; **measured, not reasoned** |
| `a6c849ee` | Slice-10b fix-3 closure artifact — the measured napi object-field shape |
| `cf92d1c4` | codex **[P2]** — annotate the neighbors direction matrix as `TraversalDirection` |
| `93a57b10` | Annotate the `_doc_id_of` `getattr` result as `Any` (pyright **12 → 8**) |

**The five read verbs are `read_get`, `read_get_many`, `read_list`, `read_list_filter`, `graph_neighbors`.**
*(The plan's §3 shorthand "`get`/`list`/`neighbors`" named no real symbol; corrected there.)*
**`graph_neighbors` has THREE direction variants, not four** — `Outgoing` / `Incoming` / `Both`
(`engine/src/lib.rs:1948-1952`). The 4th CTE that made the brief say "four" is **`build_bfs_with_depth_sql`**,
which serves **`search_expand`** — **not one of the five read verbs**, and **deliberately left on the strict
path**.

### 12.2 Schema — 21 → 22

Step 22 adds `canonical_nodes.valid_from` / `valid_until`: **INTEGER epoch seconds, nullable**, half-open
**`[valid_from, valid_until)`**, **NULL = unbounded**. **Existing rows back-fill NULL/NULL ⇒ always valid ⇒
default-view visibility is unchanged.** The INTEGER choice **deliberately diverges** from the shipped
ISO-8601 TEXT `canonical_edges.t_valid`/`t_invalid`, which are **untouched** — the divergence is **pinned by two
tests** and carries a migration note in the step-22 SQL, so it cannot drift silently. **That divergence is
TC-33, and it is a decision owed to the HITL** (§4 #16) — it is recorded here as deliberate, not as settled.

### 12.3 Gates — ONE fresh clone at exactly `93a57b10`, everything SERIAL

The clone head was verified **equal to the branch head** before any gate ran. Real exit codes throughout.

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **exit 0** |
| `cargo check --workspace --all-targets` | **exit 0** |
| `cargo test -p fathomdb-engine -p fathomdb-schema -- --test-threads=1` | **exit 0** — **540 passed / 0 failed** |
| `cargo test -p fathomdb --test governed_surface` | **exit 0** |
| **Python** | **787 passed / 12 skipped · exit 0** — fresh clone, **own venv**, `pip install -e "src/python[dev]"`; **never** the shared `.venv` |
| **TypeScript** | **186 pass / 0 fail · exit 0** |
| `pyright -p src/python` | **8 errors, exit 1** — **the pre-slice baseline is ALSO 8**; see below |
| **AC-041** (`test_no_recovery_surface.py`, `no-recovery-surface.test.ts`) | **GREEN, live in BOTH bindings** — inside the zero-failure runs above; denylist unchanged at five |
| **eu7** | **ZERO runs, any backend, any N.** `eu7_real_corpus_ac` is still `#[ignore]`d |

**pyright, stated honestly: the project gate was ALREADY RED before this slice, and is not made worse.** The
slice **introduced 4 errors and cleared all 4**; the residual **8 are the pre-existing baseline**. This is
**not** a green gate and is **not** claimed as one.

### 12.4 Python — the honest comparison

**Baseline at `c82feb80`, same method: `1 failed, 770 passed, 12 skipped`.** The single failure was
`test_option2_elps_pipeline.py::test_build_fathomdb_elps_path_uses_ingest_with_extractor` — the **TC-31**
`int(sh.id)` `TypeError`. **It now PASSES**, and the suite is **787 passed / 12 skipped, exit 0**.

**On the earlier "2 failed" figure — both numbers are real; neither disproves the other.** §11.8 row 1 measured
the **hooks-available** environment; the **hook-less default path** shows **1**. They are different environment
states of the same suite, and are recorded as such rather than one being retconned.

### 12.5 Governed-surface delta — recorded then as **PROPOSED / NOT SIGNED** *(🕮 since SIGNED)*

Recorded in the same shape Slice 5d used, and for the same reason: the branch is not red, but **that is not an
approval**. 🕮 **It has since been approved** — this delta is inside the batched governed-surface decision
**SIGNED at steward `seq-157`** (2026-07-29).

- **Commands added:** `read.crossed_boundary_since` / `read.crossedBoundarySince`
- **Types added:** `ReadView`, `BoundaryCrossing`
- **Allowlist:** the `allowlist` array goes **25 → 27** entries (the two command names above); `core` **unchanged
  at 5**; `recovery_denylist` **UNCHANGED at exactly five** — `["recover","restore","repair","fix","rebuild"]`
- **AC-041 GREEN** in both bindings (§12.3). **No AC minted — `AC-079` remains available and unminted**, since
  Slice 5's delta has not yet consumed it

### 12.6 codex §9 — two terminal PASSes

Transcripts under `dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7 path), committed with this close.

| Round | Transcript | Verdict |
|---|---|---|
| 10a | `slice-10-20260720T155459Z.log` | **PASS** |
| 10b initial | `slice-10b-20260720T175114Z.log` | **CONCERN** — one **[P2]** (pyright). **Fixed in `cf92d1c4`, NOT overridden** |
| 10b re-review | `slice-10b-rereview-20260720T180124Z.log` | **TERMINAL PASS** |

### 12.7 Owed to the HITL

**§4 #15 (TC-34)** node validity has **no write-side authoring verb** — queryable but not settable from any SDK;
the tests author windows **via direct SQL**. **Is R-20-NV met without it?** · **§4 #16 (TC-33)** the temporal-model
split · **§4 #17** the Slice-10 governed-surface delta · and the carried **§4 #7 AC-079 sign-off**, *recorded
here as blocking publish* — 🕮 **SUPERSEDED: both #17 and #7 were SIGNED at steward `seq-157`, and AC-079 does
not block publish.** **§4 #14 (TC-32)** is ruled and closed, but its **carry-forward caveat stands: do not state
the erasure guarantee unconditionally to users** while co-named-entity dedupe stands.

Logged, no decision needed: **TC-35** (napi `#[napi(object)]` omits `None` `Option` properties — measured) and
**TC-36** (published API docs still declare `SearchHit.id` as `int`/`number` "write_cursor", **stale since
0.8.19 C-2**).

### 12.8 Closure artifacts

`dev/plans/runs/0.8.20-slice-10a-output.json`, `0.8.20-slice-10a-fix-1-output.json`,
`0.8.20-slice-10b-output.json`, and `0.8.20-slice-10b-fix-{2,3,4,5}-output.json`, plus the three §9 transcripts
in §12.6. Committed with this close per **TC-23** — an untracked closure witness is destructible by routine git
hygiene.

---

## 13. Slice 15b close — TC-34 node-validity authoring + search-validity coherence

> **✅ SUPERSEDED 2026-07-24 — the FULL Slice 15 keystone LANDED at `a2022957`.** This section is the historical
> close record of the **TC-34 sub-part only** (Slice 15b), written before the registry/EAV/TC-33 remainder was
> built. That remainder (Slices 15d + 15e + TC-33 + TC-47 + Finding-1 (A) + `#[non_exhaustive] SearchFilter`)
> **has since been built and landed** in the keystone merge — see §8 and `plan-0.8.20.md` §9. The "SLICE 15 IS
> NOT COMPLETE" / "NOT STARTED" / "Slices 20 and 25 remain BLOCKED" banners below describe the on-branch state
> **as of Slice 15b** and are **no longer current**: **20 and 25 are now UNBLOCKED.**

**Branch `orch-0.8.20-s15`, terminal HEAD `a8087dfb`** (docs/artifacts at **`cd5620be`**), based on **`29eba153`**.
**COMPLETE on-branch** *(the TC-34 sub-part; folded into the landed keystone `a2022957`)*.

> **⚠ HISTORICAL (as of Slice 15b) — SUPERSEDED, see the note above. This close covers ONE of its four parts.**
> **TC-34 is CLOSED** (plus a search-validity coherence fix that was **not originally scoped**).
> **R-20-PR (projection registry, the C-1 co-land), R-20-EAV (EAV / property-FTS) and TC-33 (temporal
> harmonisation) were NOT STARTED at the time this was written.** *(All three have since landed in `a2022957`.)*

### 13.1 What shipped

| Commit | Content |
|---|---|
| `f2ce7268` | **RED** — TC-34 node-validity write-side authoring path |
| `35523156` | **GREEN** — TC-34 authoring path (Rust + Py + TS) |
| `31f550a8` | Slice 15b closure witness |
| `ab790880` | **fix-1** — interface contracts for the node-validity write fields (codex **[P2]**) |
| `25943ae8` | **RED** — fix-2, validity window must govern search |
| `41044405` | **fix-2 Part 1** — validity governs every search hydration site |
| `0457908c` | **fix-2 Part 2** — `ReadView` on `search` across Py + TS |
| `83566058` | **RED** — fix-3, vector-cutoff recall + one instant per query |
| `a8087dfb` | **fix-3 GREEN** — validity filters before the vector cutoff; one instant per query |
| `cd5620be` | Closure artifacts — witnesses, 4 codex transcripts, TC-38…42 |

**The shape.** Authoring is via **optional fields on the existing node write batch item** (`valid_from` /
`valid_until`, **INTEGER epoch seconds**; TS accepts both `validFrom`/`validUntil` and the snake_case
spellings) — **not a new verb**. That is exactly symmetric with how `PreparedWrite::Edge` has always accepted
`t_valid`/`t_invalid`. **Zero new commands**; allowlist membership is **byte-unchanged**.

Validation lives in the **engine** (`validate_write`), so Rust, Python and TypeScript share **one** rule and
cannot drift: an **unsatisfiable** window (both bounds present, `valid_from >= valid_until`) is refused
**before any INSERT** and **rejects the whole batch**; a **one-sided** window is **never** refused; a
non-integer bound is a **typed refusal** — Python rejects `bool` **explicitly**, since `bool` subclasses `int`
and `True` must not silently become the instant `1`.

### 13.2 The defect codex found — and why it was in scope

**Slice 10 scoped `ReadView` to the five read verbs and deliberately left `search` out.** That was defensible
**only while no SDK caller could author a window** — raw SQL was the sole route, so the gap was **unreachable**.
**TC-34 made authoring reachable and thereby turned a latent gap into a live defect:** an SDK-authored
out-of-window node **still came back from `Engine::search`** while `read_get`/`read_list` correctly hid it. The
implementer **reproduced it at runtime on the unmodified engine** before fixing it.
`dev/design/record-lifecycle-protocol/api-surface.md:50` had **always** specified `ReadView` on **`search`**, so
the five-verb scope was a **narrowing of the contract, not the contract**.

The fix touched **five** node-hydration sites, not the one codex cited — `bfs_graph_arm_candidates` carried
**three more of the same class**, reachable from governed surface via `search_reranked(use_graph_arm=true)`.

Then **two further [P2]s**: validity was filtering **after** the vector KNN cutoff — a **recall** defect, whose
RED reproduced **ZERO** hits for a query with **two valid matches** — and the graph arm **re-read the clock**
independently of the other arms, a **determinism** defect. **Both fixed.**

**The honest limitation on the vector fix.** Recall is restored **only within the existing 192-candidate
bit-KNN pool** (`TOP_K_BIT_CANDIDATES = 192`, `engine/src/lib.rs:8095`). With **more than 192 expired
near-neighbours** the result set can still come back short. That is the **pre-existing ANN bound of the
two-stage design, not a new one** — but it is **not a fully general fix**, and it is recorded here as such.
The rejected alternative was an `EXISTS` join — **legal**, since phase 2 is an ordinary rowid JOIN and so
**ADR-0.8.11 D3 does not bite there** — rejected on **cost**: the only index on `canonical_nodes(write_cursor)`
is the **PARTIAL** `canonical_nodes_state_active_idx … WHERE state = 'active'` (migration step 20,
`fathomdb-schema/src/lib.rs:516-517`), which **cannot serve a general validity join**, so the `EXISTS` form
would impose a **full scan × the 192-row pool on EVERY search** to fix a degenerate case.

### 13.3 Gates — real exit codes, terminal HEAD `a8087dfb`

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **exit 0** (0 warnings) |
| `cargo check --workspace --all-targets` | **exit 0** |
| `cargo test -p fathomdb-engine -p fathomdb-schema -- --test-threads=1` | **exit 0 — 560 passed / 0 failed** (baseline 540 ⇒ **+20, all new**) |
| `cargo test -p fathomdb --test governed_surface` | **exit 0** |
| `cargo test -p fathomdb --test compile_fail_provenance` | **exit 0** |
| **Python** | **exit 0 — 809 passed / 12 skipped** — **fresh clone at `a8087dfb`**, **own venv inside the clone**, clone head **verified == branch head**; baseline 787 ⇒ **+22 = exactly the new tests** |
| **TypeScript** | **exit 0 — 201 pass / 0 fail** (baseline 186) |
| **markdown lint** | Run with the **PRIMARY checkout's** binary — the worktree script is **vacuous** (**TC-37**). **0 errors in every file this slice touched.** The exit 1 is **entirely** the **9 pre-existing** MD025/MD001 errors in `dev/research/personal-agent-database-market-2026-07-02.md`, **untouched here** |
| **AC-041** | **GREEN**, both bindings; recovery denylist **UNCHANGED at exactly five** |
| **eu7** | **ZERO runs**, any backend, any N; `eu7_real_corpus_ac` still `#[ignore]`d, attribute untouched |

### 13.4 Governed-surface delta — recorded then as **PROPOSED / NOT SIGNED** *(🕮 since SIGNED)*

- **Commands added: NONE** from TC-34 — it is **fields only**. **fix-2 Part 2** adds an **optional `view`
  argument to `search`** in **both** bindings.
- **Types:** `ReadView` **reused** — no new type. `recovery_denylist` **UNCHANGED at exactly five**.
  **AC-041 GREEN.**
- **`AC-079` remains UNMINTED** — the mint is at Slice 40.
- Marked **`AWAITING HITL SIGN-OFF, NOT SIGNED`** in the allowlist `_comment` at the time. 🕮 **The delta is
  now SIGNED** (steward `seq-157`); the literal string survives in the JSON only because the T1e pin hashes
  raw bytes — **it is stale prose in a signed artifact, not an open sign-off, and the JSON must not be
  edited.**

### 13.5 codex §9 — four rounds, terminal PASS

Transcripts under `dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7 path), committed with this close.

| Round | Transcript | Verdict |
|---|---|---|
| initial | `slice-15b-20260720T195420Z.log` | **CONCERN** — **[P2]** missing interface-contract docs |
| fix-1 re-review | `slice-15b-fix-1-rereview-20260720T200434Z.log` | **CONCERN** — **[P2]** search ignores validity windows |
| fix-2 re-review | `slice-15b-fix-2-rereview-20260720T205344Z.log` | **CONCERN** — **2×[P2]** vector cutoff + clock re-read |
| fix-3 re-review | `slice-15b-fix-3-rereview-20260720T213603Z.log` | **TERMINAL PASS** |

**No verdict was overridden. Every [P2] was fixed.**

### 13.6 What Slice 15b proved WRONG

1. **A named RED test was never written, and the property it guarded then regressed.**
   `dev/design/0.8.20-slice0-erasure-design.md:308` names **three** RED tests for **R-20-NV**;
   **`valid_as_of_binds_now_once` has ZERO hits in `src/`** — it exists **only in that design table**. R-20-NV
   was nevertheless **CLOSED at Slice 10**, and the exact property it named (`:now` binds **once per query**)
   **regressed in fix-2**. Traceability from the Slice-0 acceptance tables to real tests is **UNENFORCED**.
   **Slice 40 should mechanically verify that every test named in those tables exists** — this was found **by
   accident** and is unlikely to be the only one. (**TC-42**)
2. **`ReadView` never covered `search`** — a five-verb **narrowing** of a contract that named `search`.
   (**TC-38**)
3. **`AGENTS.md:25`'s interface-doc obligation is routinely missed**, and `dev/DOC-INDEX.md` did not track
   `dev/interfaces/python.md`, `typescript.md`, `wire.md` or `README.md` **at all**. Rows were added for the
   **first two**; **`wire.md` and `README.md` remain gate-m debt.** (**TC-39**)
4. **Not a defect, but load-bearing for the unstarted work:** the plan's `roles: {filterable, rankable,
   searchable}` **cannot express the ratified C-1 contract** (**TC-40**), and `filterable` has **two
   incompatible backends** (**TC-41**).

### 13.7 Owed to the HITL

Recorded as **§4 #18–#22**.

### 13.8 Closure artifacts

`dev/plans/runs/0.8.20-slice-15b-output.json` and `0.8.20-slice-15b-fix-{1,2,3}-output.json`, plus the four §9
transcripts in §13.5. Ledger entries **TC-38…TC-42**. Committed with this close per **TC-23**.

---

## 14. Slice 20 close — **PARTIAL** (TC-45 + `dense_readiness` landed; `flush_embeddings()` HELD)

> 🕮 **HISTORICAL CLOSE RECORD — SUPERSEDED by §16.** **Slice 20 is COMPLETE**: the flush-to-readiness barrier
> shipped at **20c `841c307b`** by REUSING the shipped `drain` (`api-surface.md` **C4**) — **TC-55 =
> INSTRUMENTATION** (`seq-110`), and **there is no `flush_embeddings()` verb**. The "**SLICE 20 IS NOT
> COMPLETE**" / "HELD" banners below describe the state at 20b. Likewise the AC-079 "**not signed**" statements
> in §14.4 are **SUPERSEDED** — the batched governed-surface delta was **SIGNED at steward `seq-157`**.

**Merge `26b237c0`** — *"merge(0.8.20): Slice 20 PARTIAL — TC-45 supersession terminal + R-20-DR
dense_readiness/atomic-flip"*. Branch `orch-0.8.20-s20`, cut from **`ff4f07a0`**, terminal HEAD **`15c75c57`**.

> **⚠ SLICE 20 IS NOT COMPLETE.** Two of the three scoped items closed. **`flush_embeddings()` did NOT land**
> and must not be described anywhere as shipped. It is HELD on **TC-59** and **TC-55** (§14.5). The remainder
> is tracked as **Slice 20c**.

### 14.1 What shipped

| Commit | Content |
|---|---|
| `ca32ec81` | **RED** — TC-45: superseded edge cursors never gain a projection terminal |
| `9db32765` | **GREEN** — TC-45: record superseded edge cursors as `'up_to_date'` |
| `a6e1bf21` | **RED** — R-20-DR: `dense_readiness` is never populated |
| `3ae399fd` | **GREEN** — R-20-DR: derive `dense_readiness`; the flip is atomic by construction |
| `d1a1cd20` | **RED** — fix-1: the pending-work probe reports `embedding` forever |
| `2b1f62d0` | **GREEN** — fix-1: probe and scheduler share ONE edge-arm predicate |
| `15c75c57` | **fix-2** — interface docs carry the dense-readiness surface (codex **[P2]**) |

**✅ TC-45 CLOSED** (HITL-ruled 2026-07-24, plan §11 item 7). `record_projection_terminal` wrote
`state='superseded'` under `INSERT OR IGNORE` against `CHECK(state IN ('failed','up_to_date'))`, so the terminal
was **silently dropped** and `advance_projection_cursor` could stall. Both call sites in `commit_batch` — the
**G0** path after `prior_edge_cursors_by_logical_id` and the **G11** path after `prior_edge_cursors_by_triple` —
now record `'up_to_date'`. **No migration; `SCHEMA_VERSION` stays 24**; the terminal CHECK was **not** widened —
exactly the shape the ruling preferred. The semantic question the ruling reserved ("escalate only if the
terminal's semantics demand the distinct token") was **checked, not assumed**: **no consumer discriminates the
token** — `advance_projection_cursor` and `commit_projection_outcomes` test only `.is_some()`, and
`projection_status` maps `_ => UpToDate`.

**✅ R-20-DR part 1 of 2 — `dense_readiness` + the atomic readiness-flip.** `DenseReadiness { Ready, Embedding }`
attaches **additively** to the `ProjectionSpec.vector` sub-object built in 15d, as **engine-set read metadata**.
It is **derived**, not stored — **no column, no schema step**. Surfaces as `vector_dense_readiness` (Python) /
`vectorDenseReadiness` (TypeScript). The reserved token **`pending` is never emitted and never accepted**.

The **atomic readiness-flip** (design §4.1 invariant 1) holds **by construction**, and this was **verified rather
than added**: `commit_projection_outcomes` already performs the vector insert, the terminal record and
`advance_projection_cursor` **inside ONE transaction**. The flip test is therefore a **proof test, not a
regression test** — an honest label, recorded here so a later reader does not mistake it for a fix.

**Deviation accepted by the orchestrator.** A caller-supplied `dense_readiness` on `configure_projections` is
**accept-inert at the engine**, plus a **narrowed hard-reject at the bindings** — readiness with `vector=false`,
or any spelling outside `{ready, embedding}` — reusing the existing `InvalidArgument` / `FDB_INVALID_ARGUMENT`
(**no new error type**). A *full* hard-reject was **rejected**: it would break the shipped, twice-tested invariant
that `read.projections` output must feed back into `configure_projections`. This does **not** pre-empt plan §11
item 4 (the still-open accept-vs-reject question for a sub-object without the `searchable` role).

### 14.2 Side effect — fix-1 closed a **shipped** `Engine::drain` defect

Deriving `dense_readiness` from the pending-work predicate surfaced a **pre-existing, already-published** defect:
the probe omitted the scheduler's `_fathomdb_vector_kinds` join on the **edge** arm, so a live edge body under an
**unregistered `edge_fact` kind** was **phantom-pending forever** — `drain` burned its whole timeout and returned
`Err(Scheduler)`. Confirmed pre-existing **empirically, not by inspection**: reproduced at baseline **`9db32765`**
with **zero Slice-20 code in the build**. The fix gives probe and scheduler **one shared predicate** so they
cannot drift again. This is a **behaviour change on a published surface** and the CHANGELOG / Slice-40 publish
narrative must reflect it. Ledger **TC-56**.

### 14.3 Gates — real exit codes, captured at the slice head

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **exit 0** |
| `cargo check --workspace --all-targets` | **exit 0** |
| `cargo test -p fathomdb-engine` | **exit 0** |
| `cargo test -p fathomdb-schema` | **exit 0** |
| **Python** | **exit 0 — 842 passed / 7 skipped** |
| **TypeScript** | **exit 0 — 231 / 231** |
| `check-ledgers.sh` · `lint-findings.sh` · `lint-plan-anchors.sh` · `lint-design-status.sh` | **exit 0** (each) |
| `check-governed-surface-pin.sh` · `check-release-state-views.sh` | **exit 0** (each) |
| `agent-lint.sh` · `agent-lint-md.sh` | **exit 0** (each) |
| `scripts/agent-typecheck.sh` | **exit 1 — PRE-EXISTING**, exactly **7** pyright errors; identical count verified at baseline **by stashing** |
| `scripts/agent-test.sh` | **DELIBERATELY NOT RUN** — a vacuous aggregate that aborts early on the known-red **TC-16 / F-30** fixture |

**X1 SDK parity ran BEFORE the land**, via **live functional harnesses**, not symbol presence:
`src/python/tests/test_slice20_dense_readiness.py` ↔ `src/ts/tests/slice20-dense-readiness.test.ts`. Python ran
against a **wheel installed into a throwaway venv** — the shared `.venv` was **never touched** and
`maturin develop` / `pip install -e` were **never run** (the standing worktree rule). **The harness earned its
keep:** it caught `src/python/fathomdb/engine.py` silently dropping `vector_dense_readiness` before the native
boundary.

### 14.4 Governed-surface delta — **ZERO net-new governed commands**

- `src/conformance/governed-surface-allowlist.json` and `scripts/governed-surface-pin.json` are
  **byte-identical to the pre-slice baseline**; `scripts/check-governed-surface-pin.sh` exits **0**.
- The one net-new **exported Rust type** is **`DenseReadiness`**. It sits **outside** the 33-entry
  `GOVERNED_SURFACE_ALLOWLIST` const in `src/rust/crates/fathomdb/tests/governed_surface.rs` — **the same place
  the five Slice-15d `Projection*` types sit**. This follows **existing precedent**; it does **not** set new
  policy.
- **Flagged as OWED at the AC-079 sign-off.** *(As written then: "it is not signed and not a decision taken.")*
  🕮 **SUPERSEDED — the sign-off happened:** the batched governed-surface delta was **SIGNED at steward
  `seq-157`**. **AC-079 remains unminted** — minting is at Slice 40, and minting is not signing.
- **AC-041 untouched:** the recovery denylist is unchanged at exactly five.

### 14.5 What is HELD, and why — `flush_embeddings()`

**TC-59 — the pin gate makes "propose AND land" mechanically impossible (p1, blocks 20c / 25 / 30).**
Plan §11 ruling 2 (HITL 2026-07-25) says new governed surface at 20/25/30 is **recorded as a proposal**, the
branch **stays green**, and the accumulated delta goes to the HITL **once** at the 30 → 40 boundary. But
`check-governed-surface-pin.sh` compares a sha256 **and** a git-blob-sha1 over the **raw bytes** of the allowlist,
and `preflight.sh --landing` treats it as a **HARD** fail — so **any** allowlist diff, *including the `_comment`
prose edit that IS the proposal convention used by 5d/10b/15b/15d*, **blocks the land**. The gate's own header
asserts that tripping it is "precisely what lets Slices 20/25/30 proceed without stopping for a per-slice
sign-off"; **as wired, that claim is false.** Recommended fix is **(a) fix the tooling** — give the pin a
`pending_delta` block so the gate passes iff `allowlist == pinned ∪ pending_delta`, reporting the unsigned set
loudly and preserving every guarantee including the locked `recovery_denylist`.

**TC-55 — is `flush_embeddings()` a governed COMMAND or INSTRUMENTATION? (p1, unresolved).**
Plan §11 ruling 2 says it "reads as a net-new command" and expects the pin to trip. `api-surface.md` **C4** says
the opposite — **reuse the shipped `drain(timeout_ms)` barrier instead**. Both design docs are status
**UNREVIEWED**, so the plan is the contract and the plan wins; the conflict is recorded because the docs will
otherwise keep misleading readers. **Mechanically load-bearing:** the shipped `drain` is **not** in the JSON
allowlist — it sits in the `_INSTRUMENTATION` exclusion set hardcoded in **both** `src/python/tests/test_surface.py`
and `src/ts/tests/surface.test.ts`. So *instrumentation* leaves the pinned JSON untouched and 20c lands clean,
while *command* forces an allowlist edit and hits TC-59. **That must not be settled by whichever answer is easier
to land.** Design rider either way: `drain` is a **barrier** (wait-for-idle); `flush` must also be a **trigger**,
so deferred/backfill rows have to be enqueued on the same runtime `drain` waits on.

**Slice 20 was split accordingly:** part **b** (`dense_readiness`) adds **zero** governed commands and landed
clean; part **c** (`flush_embeddings()`) is held pending those two answers.

### 14.6 codex §9 — three rounds, terminal PASS

Transcripts under `dev/plans/runs/codex/0.8.20/` (**TC-RUBRIC-7** path), committed with this close.

| Round | Transcript | Verdict |
|---|---|---|
| 20a (TC-45) | `slice-20a-tc45-20260725T224612Z.log` | **PASS** — no findings |
| 20b (dense readiness) | `slice-20b-dense-readiness-20260725T234510Z.log` | **CONCERN** — **1×[P2]** probe/scheduler edge-arm divergence |
| 20b fix-1 re-review | `slice-20b-fix-1-rereview-20260726T001244Z.log` | **CONCERN** — **1×[P2]** interface docs stale |
| 20b fix-2 re-review | `slice-20b-fix-2-rereview-20260726T002647Z.log` | **TERMINAL PASS** |

**No verdict was overridden. Every [P2] was fixed.**

### 14.7 Closure artifacts

The **four §9 transcripts** in §14.6, the **closure witness** at `dev/plans/runs/0.8.20-slice-20-output.json`,
and the Slice-20 ledger entries in `dev/todos-and-considerations-ledger.jsonl` — **TC-55, TC-56, TC-57, TC-58
and TC-59**, at **seq 82–86**, plus the **seq 87** entry that restores **TC-54**. *(This is deliberately not
stated as a contiguous `TC-54…TC-58` range: `TC-54` is **not** a Slice-20 item — it is the
`.markdownlint-cli2.jsonc` observation, and the Slice-20 pin-gate blocker mis-filed under that id at seq 81 was
reissued as **TC-59** at seq 86. Grepping `TC-54` in the ledger must not send a reader to the pin gate.)* By id:
TC-59 (pin-gate blocker), TC-55 (`flush_embeddings` classification), TC-56 (the shipped `drain` defect), TC-57
(pre-existing governed-write × projection-worker `EngineError::Storage` race, 7/8 at baseline), TC-58 (`ac_002`
scans `$HOME` and false-positives on sibling repos). Committed with this close per **TC-23**.

> **⚠ Two record defects found while writing this close, both corrected here — do not re-introduce them.**
> (1) The five new Slice-20 ledger entries were **written but never committed** — they sat as an uncommitted
> diff in the slice worktree and were minted at **seq 80–84**, which **collides** with the `seq 80` already on
> `main` (the `.markdownlint-cli2.jsonc` observation). They are ported here **renumbered to seq 81–85**; every
> citation of them uses the new numbers. **The same read-then-write race also produced a second, worse defect:
> the entries were minted starting at the already-taken *id* `TC-54`, so the fold-to-latest silently overwrote
> the markdownlint item — repaired **append-only** at **seq 86** (the pin-gate blocker reissued as **TC-59**)
> and **seq 87** (`TC-54` restored to the markdownlint observation, `closed`), with every citation in this
> document and in `plan-0.8.20.md` retargeted to `TC-59`; root cause is that `ledgerwrite` mints no id and
> cannot detect that a caller-supplied id already exists, so concurrent agents reuse ids silently — tooling
> work captured in the `TC-54` restoration entry, adjacent to `TC-53`.** (2) **No `0.8.20-slice-20*-output.json`
> closure witness was persisted per-slice** in the usual location while the slice was in flight, unlike every
> prior 0.8.20 slice — a **TC-23 gap**, recorded rather than papered over. The witness has since been recovered
> from the (now prunable) slice worktree and committed **verbatim** as
> `dev/plans/runs/0.8.20-slice-20-output.json`; producing it per-slice, at close, remains the process gap.

---

## 15. Slice 25 close — surrogate minting, registry-admitted governed entities ONLY (R-20-SUR)

**Landed `83b1c818`** (merge, `--no-ff`), branch `orch-0.8.20-s25`, cut from a verified `origin/main` tip and
**rebased twice** onto live `main` (`efa8d584`, then `3a9e04e4`) as Slice 20c landed alongside it — the full
gate set was re-run after the final rebase, not carried over.

### What this slice actually was

**Enforcement + proof, NOT a new mechanism.** The governed write-time minting path already existed
(`derive_logical_id` in the entity/edge arms of `Engine::ingest_with_extractor`) and was **not rebuilt**. Per
**TC-11 pin A** the anonymous-surrogate leg is **CANCELLED, not deferred**, so no surrogate is minted for
anonymous or doc-seeded content anywhere in this slice. `structural-lifecycle-contract.md` §2(ii) stays
**OVERRULED**.

| Deliverable | What landed |
|---|---|
| **D1** static migration guard | `check_migration_logical_id_pin` in `fathomdb-schema` — rejects any migration step that would populate `logical_id` on an existing canonical row. **No exemption escape hatch**: the pin is a prohibition, not a budget (unlike `check_migration_accretion`). Wired over `MIGRATIONS` *whole* and over `migrations/*.sql`. |
| **D2** dynamic migration guard | Real-SQLite whole-ladder migration of a mixed anonymous/governed corpus (step 12 → head, step 23 → head) asserting `logical_id` NULL → NOT NULL == 0, NOT NULL → NULL == 0, byte-identical snapshot, and byte-identical `IdSpace::to_prefixed()`. **This is the load-bearing proof.** |
| **D3** registration inertness | `configure_projections` add / idempotent re-register / explicit drop / boot re-derive leave every pre-existing row's `logical_id` and `IdSpace` byte-identical — asserted on raw at-rest state **and** on live `SearchHit::id`. |
| **D4** write-time admission audit | **No gap.** Every `PreparedWrite` node/edge call site enumerated; no unadmitted path mints. No machinery manufactured to fill an absent gap. |

### Boundaries held

`SCHEMA_VERSION` unchanged (**24**), no migration step appended, **no new column**, **no new SDK verb or
binding method**. `src/conformance/governed-surface-allowlist.json` **byte-identical** to base;
`check-governed-surface-pin.sh` **exit 0** throughout — the pin never tripped, was never re-pinned.
**Zero engine source lines changed** (only a new engine *test* file), which is why the concurrent Slice 20c
work on `fathomdb-engine/src/lib.rs` rebased without a single conflict.

### codex §9 — CONCERN, 3 fix rounds, halted at the circuit-breaker

| Round | Verdict | Transcript |
|---|---|---|
| initial | CONCERN [P2] — schema-qualified names bypass the guard | `dev/plans/runs/codex/0.8.20/slice-25-r20sur-20260726T162955Z.log` |
| fix-1 | CONCERN [P2] `UPDATE OR <conflict-action>` bypass · [P3] trigger-target over-rejection | `.../slice-25-fix-1-rereview-20260726T165520Z.log` |
| fix-3 | CONCERN [P2] — subquery in an earlier `SET` truncates the clause | `.../slice-25-fix-3-rereview-20260726T172916Z.log` |

Each round produced a **new lexical shape** rather than a defect in the requirement: a lexical scanner cannot
be complete against SQL grammar. The run **halted at the orchestration circuit-breaker** (3 fix rounds on one
finding family) rather than dispatching a 4th round that would buy a 5th `[P2]`.

**Steward ruling: ACCEPT the residual and land as-is** — verified, not taken on assertion: D2 snapshots
identity **rows** before/after and asserts `null_to_not_null == 0`, i.e. it compares **rows, not SQL text**, so
it is shape-independent and catches every evasion the lexical guard misses. **D1 is defence-in-depth; D2 is the
proof.** The known-open shapes (subquery-in-`SET`, rename round-trip, `CREATE TABLE AS SELECT` swap) are stated
openly in the guard's `# Known limits` doc comment, which is what makes accepting the residual honest rather
than a shrug. Recorded durably as **`TC-66`** (seq 94), including the caveat that a future slice must **not**
read a green static guard as evidence the pin holds, nor weaken the D2 whole-ladder tests.

### Verify — real exit codes, re-run after the final rebase

`cargo fmt --all --check` **0** · `cargo clippy --workspace --all-targets` **0** ·
`cargo check --workspace --all-targets` **0** · `cargo test -p fathomdb-schema` **0** ·
`cargo test -p fathomdb-engine --no-fail-fast` **0** (103/103 binaries) ·
`cargo test -p fathomdb-embedder-api` **0** · `check-governed-surface-pin` **0** ·
`agent-lint-migrations` **0** · `lint-findings` / `lint-plan-anchors` / `lint-design-status` /
`check-release-state-views` / `check-board-currency` **0**.

**X1 SDK parity — live, not symbol presence.** A wheel was built into a **throwaway venv** outside the repo
(never `maturin develop`; the shared `.venv` binding was verified un-rebound afterwards) and the TS addon built
worktree-local. Python 24 live tests + TS 12 live tests, plus a live registration-inertness harness on both:
anonymous `h:6d7f8dc2…` and governed `l:x1-gov-1`, **byte-identical across both SDKs** and unchanged across a
`configure_projections` cycle.

### Carried out of this slice

- **`ac_002_no_log_files_without_subscriber`, `ac_029_canonical_writes_complete_under_projection_stall` and
  `tests/lifecycle_observability.rs` are LOAD-SENSITIVE flakes** — they fail under heavy concurrent CPU load
  and pass in isolation and on an unloaded full run. Engine source was byte-unchanged on this branch, so they
  are pre-existing. Recorded, not "fixed".
- **A red gate was inherited from `main` and closed here:**
  `dev/todos-and-considerations-ledger.jsonl.seq` was committed at **87** while the committed ledger's
  `max(seq)` was **93**, so `check-ledgers.sh` was **RED on any clean checkout of `main`** — it only looked
  green in a working tree carrying an uncommitted sidecar fix. Repaired as part of this close (sidecar now
  **94**, verified exit 0 from a clean tree). The lesson is the vacuous-green trap in miniature: a gate
  verified against a dirty tree is not verified.
- **TC-57** (governed-write vs projection-worker race) was **not tripped and not touched** — it is placed
  elsewhere.

---

## 16. Slice 20c close — **R-20-DR COMPLETE**; the flush barrier shipped on the existing `drain`

**LANDED at `841c307b`** (merge). Branch `orch-0.8.20-s20c`, cut from `2afd168c`. **SCHEMA stays 24**, no
migration, **zero net-new governed commands**.

### 16.1 What shipped

The **flush-to-readiness barrier**, implemented by **REUSING the shipped `drain`** per
`dev/design/record-lifecycle-protocol/api-surface.md` **C4**. **There is NO `flush_embeddings()` verb** —
**TC-55 = INSTRUMENTATION** (steward `seq-110`): `drain` is absent from the allowlist and present in
`_INSTRUMENTATION` (`src/python/tests/test_surface.py:63`), so a second governed verb would be surface
duplication. **TC-59** (one re-pin at the batched decision, `seq-113`) was honoured — **no `pending_delta`
tooling was built, the allowlist was never edited, and the pin never tripped.**

**The defect closed.** C4's rider — *"deferred/backfill rows must be enqueued on the same projection runtime
`drain` waits on"* — **was not true of the code**. `configure_projections` recorded vector work as `deferred`
and then dropped it, so `drain()` returned `Ok(())` and `dense_readiness` read `ready` **while no vectors
existed and nothing would ever create them**. That is precisely the false-ready R-20-DR exists to eliminate.

The fix is **entirely on the enqueue side**. `drain` remains a **passive barrier** (the C4 rider forbids making
it a trigger) and the shared `connection_has_pending_projection_work` predicate was **never restructured**, so
the **TC-56** blast radius stayed closed.

### 16.2 codex §9 — five rounds, every one RED-first

| Round | Finding | Resolution |
|------:|---------|-----------|
| 1 | **[P2]** enrolment was one-way; dropping the last vector projection kept embedding | The symmetric inverse. `edge_fact` preserved; **no embeddings deleted** (a drop already left vectors at rest, so un-enrolment is the NON-destructive choice — no design call) |
| 2 | **[P1]** enrolling kinds `resolve_source_type` cannot commit wedged `drain` **forever**; **[P2]** late enrolment stranded rows (**FALSE-READY**) | `kind_is_vector_committable` **DELEGATES** to `resolve_source_type` (no second vocabulary can drift — the TC-56 shape); stranded-row repair extracted and shared by both doors |
| 3 | **[P1]** no-embedder sessions lost writes permanently (**FALSE-READY**) | **Option R2**, not codex's prescription. `ProjectionOutcome::Deferred` records **no terminal**, so the row stays pending and the next live-embedder session embeds it through the **ordinary scheduler**. **R1 was REJECTED** — it would have amended design §4.1 invariant 1 (*"a torn `ready`-without-vector is FORBIDDEN"*) and rewritten two shipped tests |
| 4 | **[P1]** the no-embedder dispatcher filter ran **after** the limited scan and starved edge jobs; **[P2]** late enrolment was not crash-atomic | Filter moved **inside** the scan SQL (CPU win preserved: **0.10 s vs 6.37 s** unfiltered); enrolment + repair in one `BEGIN IMMEDIATE`, pinned by **real SQLite fault injection** |
| 5 | **[P2]** `vector_projection_declared` ignores the `searchable` role | **LEFT OPEN — `TC-71`**, at the **5-round circuit breaker** (see §16.5) |

**Transcripts (TC-RUBRIC-7):** `dev/plans/runs/codex/0.8.20/slice-20c-review-20260726T164236Z.log`,
`…-fix-1-rereview-20260726T173103Z.log`, `…-fix-2-rereview-20260726T183101Z.log`,
`…-fix-4-rereview-20260726T214229Z.log`, `…-fix-5-rereview-20260726T224040Z.log`.

### 16.3 Gates — real exit codes

| Gate | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **0** (forced rebuild, zero warnings) |
| `cargo check --workspace --all-targets` | **0** |
| `cargo test --workspace --no-fail-fast` | **0** (170 suites) |
| `slice20c_flush_barrier` · `slice20_dense_readiness` · `slice15d` · `slice15e` · `projection_runtime` · `rebuild_projections` | **0** (12 · 8 · 24 · 11 · 12+1 · 4) |
| **Python** (wheel → **throwaway venv**; shared `.venv` never touched) | **0** — 850 passed / 9 skipped |
| **TypeScript** `npm test` | **0** — 241 / 241 |
| `check-governed-surface-pin.sh` | **0** — allowlist **byte-identical** at `555e6ae7…92c02b` |
| `check-ledgers` · `lint-findings` · `lint-plan-anchors` · `lint-design-status` · `check-release-state-views` · `check-board-currency` | **0** each |
| `agent-typecheck.sh` | **1 — PRE-EXISTING**, exactly the 7 baseline pyright errors; this slice adds none |
| `agent-test.sh` | **DELIBERATELY NOT RUN** — vacuous aggregate (aborts on the known-red TC-16 fixture) |

**Post-merge re-verification against a `main` control:** the merged tree failed **1 of 3** full-workspace runs
and `main` failed **1 of 3**, each time a *different* concurrency test that passes reliably in isolation
(slice15e 6/6, lifecycle_observability 3/3). **No regression** — the full-workspace parallel run is flaky on
both sides (**TC-72**).

### 16.4 X1 SDK parity — ran BEFORE the land, as one unit

Live functional harnesses, **not symbol presence**: `src/python/tests/test_slice20c_flush_barrier.py` ↔
`src/ts/tests/slice20c-flush-barrier.test.ts`. **Non-vacuity was measured in BOTH bindings** by rebuilding each
against the pre-fix engine and re-running. Python ran against a **wheel installed into a throwaway venv** —
`maturin develop` / `pip install -e` were **never** run and the shared `.venv` was **never** touched.
**Both shipped Slice-20 binding tests pass BYTE-UNCHANGED** (`git diff 2afd168c` over those two files is empty).

### 16.5 Carried out of this slice

- **`TC-71` (OPEN, p2)** — the one codex finding left unfixed, at the circuit breaker. `vector_projection_declared`
  keys off the stored `vector` sub-object alone, so `{roles:[filterable], vector:true}` activates the dense arm
  instead of staying **inert**. **Wasted embed work; NOT a false-ready, NOT data loss.** The predicate gates the
  forward backfill, the drop inverse **and** late enrolment — a fix must re-verify all three together.
- **⚠ CONSUMER-VISIBLE behaviour change.** A no-embedder session over an **already-enrolled** corpus now leaves
  readiness at `embedding` and `drain` times out into `SchedulerError` **for that session**. This is **loud
  rather than silent** and is **not data loss** — the write is accepted, stays lexically searchable, and is
  embedded on the next embedder-backed open. Documented in `dev/interfaces/{rust,python,typescript}.md` +
  `CHANGELOG.md`. Workspaces that have never had a live embedder are unaffected.
- **Ledger:** `TC-60` (third-party RW SQLite connection wedges the pipeline) · `TC-61` (enqueue and notify are
  one operation) · `TC-62` (`drain` spins while frozen) · `TC-63` (`run_rebuild` returns with work outstanding) ·
  `TC-64` (`wait_for_idle` lost wakeup) · `TC-65` (`ProjectionSpec` has no kind axis — the enrolment-scope seam) ·
  `TC-67` (the locked source-type vocabulary vs consumer kinds — **open question for the HITL**) · `TC-68`
  (90 synchronous embeds inside `Engine::open`) · `TC-69` (the edge-path residual) · `TC-70` (the retry ladder
  masks a `SQLITE_BUSY_SNAPSHOT` race) · `TC-72` (full-workspace flake).
- **`TC-57`** (governed-write vs projection-worker race) remains placed at **Slice 21**, untouched.

### 16.6 Closure artifacts

Commits: `ddaf7320` (RED) · `da7c5b76` (GREEN) · `e7a4676a` (Leg B + docs) · `7de05aae` (CHANGELOG) ·
`77bca9d4`/`c7c0f527`/`0759715a` (fix-1) · `63554cee`/`0efcf9cd`/`1b0cf514` (fix-2) ·
`87ac6491` (revert of fix-3) · `3ebfb373`/`35de816c`/`1f46438b` (fix-4 = R2) ·
`d5cc51e0`/`e2df8646`/`30ab62df` (fix-5) · merge **`841c307b`**.

---

## 17. Slice 30 close — **R-20-H7 COMPLETE**; the publish precondition is SATISFIED

**LANDED `9b3ed0e3`** (merge, 2026-07-28), off branch `orch-0.8.20-s30`. **Zero engine source** —
`git diff --name-only main...orch-0.8.20-s30 -- src/` is empty across the whole slice. SCHEMA stays **24**.
Governed-surface allowlist **byte-identical**, pin exit 0, so **AC-079's pre-sign (F-34) is intact** and the
batched ceremony still happens once *(scheduled then for the CLOSE of Slice 23 — `seq-141`; was after 22 at
`seq-134`, after 23 at `seq-140`)*. 🕮 **It has since happened: the batched governed-surface decision was
SIGNED at steward `seq-157`, 2026-07-29.**

### 17.1 What shipped

A Pact-style **`can-i-deploy` contract-conformance gate** (`scripts/check-c1-conformance.sh` + its pin
`scripts/c1-conformance-pin.json`) that mechanically verifies as-built code still satisfies the ratified
`OPP-12-C1-converged-contract.md` — instead of humans re-reading prose at the co-land. Wired into
`.github/workflows/ci.yml` and into `scripts/preflight.sh --landing`, with a **2 240-line fixture suite**.

**C-1 decomposition: 45 clauses — 26 CHECKABLE / 12 cross-repo / 7 prose, ZERO failing.**

> ⚠ **The "17 clauses, 16 passing" figure from `seq-115` does not reproduce and was never true.** The prior
> Steward relayed it into the commission brief as scaffolding; the orchestrator derived its own decomposition
> and was **right to discard it**. Do not reintroduce it.

### 17.2 codex §9 — seven rounds plus a micro-fix, and the review was LOAD-BEARING

Every round found a **new and distinct** defect; the **3-same-finding anti-thrash bound never fired once**.
Round 1 regex shape + open vocabulary · 2 refinements of both · 3 **file scope** (negative probes read one
`lib.rs`, missing a literal violation in a sibling module) · 4 **subject binding** · 5 **wrong subject +
inactive proof** (`fn_defined` accepted a function with `#[test]` removed) · 6 a receiver-less `fn_sig` false
green **and** a nested-inner-attribute false RED · 6b a **leading shebang** false green the round-6 narrowing
had itself opened · **6c a leading UTF-8 BOM**, the same false green through a different first byte.

**fix-4 alone found that 18 of 26 clauses had a demonstrable false green.** Two weak probe kinds were deleted
outright rather than left unused; a third was made unreachable for test paths. Fixture assertions grew
**106 → 276 → 317**. **The round-1 gate would have been substantially decorative.**

**fix-6c (the landing delta).** `read_source` decodes with `errors="replace"`, which does **not** strip a BOM,
so U+FEFF arrived at index 0 and defeated the leading-header walk three ways at once — it is not `#!`, it is
category **Cf** so `.isspace()` is **False**, and it is not `#![` — leaving a BOM-prefixed file with a
file-level `#![cfg(..)]` reading as un-gated. **Reachability today is ZERO**: no tracked source file carries a
BOM, exactly as the shebang hole was latent when it was fixed. Two lines, ordered **before** the shebang branch
as rustc orders it (`rustc` strips one BOM, and only then may line 1 be a shebang), plus fixture arm 12al.
Ruled the **completion of finding #2**, not a round 8.

**An override stands, and it is on a REFUTATION — not on accepted risk.** Codex's final `[P2]` claimed the
receiver probe false-REDs on `self: &Self`. The Steward tested the shipped regex against ten cases:
`(self: &Self`, `(self: &mut Self`, `(&self`, `(&mut self`, `(&'a self`, `(self`, `(mut self` all **match**;
`(specs: …, drop: bool)` and `(specs: …, myself: u8)` correctly **do not**. **Codex was factually wrong.**

**fix-6c review verdict: TERMINAL CLEAN — no [P1], no [P2], no [low].** It independently ran
`rustc --test --list`: BOM+cfg and BOM+shebang+cfg give `0 tests`, BOM-only gives `1 test`, and double-BOM and
mid-file BOM are rustc **syntax errors**, not silent test-evaporation paths.

### 17.3 Gates — Steward-re-run at `a26a7655`, exit codes captured individually

`check-c1-conformance` **0** (26/12/7, 45 total — counts unmoved) · fixture suite **0** at **317** assertions
(307 → 317, pure addition) · `check-governed-surface-pin` **0**, allowlist byte-identical vs `337c2b12` ·
`preflight.sh --landing` **0** *(in the worktree — TC-RUBRIC-5 makes it HARD-fail in the primary checkout by
design, which is not a defect)* · `cargo clippy --workspace --all-targets` **0** ·
`cargo check --workspace --all-targets` **0**.

**Steward-verified independently, not taken from the orchestrator's report.** Diff scope confirmed from git,
and the new BOM arm was demonstrated to **BIND**: reverting only the two-line strip turns the suite **rc=1**
with 7 FAILs naming the right clause and the `CONDITIONALLY COMPILED as a whole` diagnostic; restored, **rc=0**.

### 17.4 What this slice changed about the PROGRAM

Slice 30 is why the fix-round cap now keys on **round productivity** rather than on which directory a slice
touches (**TC-82**, steward `seq-125`, HITL 2026-07-28): it touched **zero engine source** and still ran seven
productive rounds, so TC-75's directory predicate could not reach it. See `orchestration.md` §6.

### 17.5 Closure artifacts

Commits: `9ef5179b` (RED fix-6c) · `0d9b40d7` (GREEN fix-6c) · `a26a7655` (closure `output.json`) ·
`892ba0ec` (fix-6c transcript) · merge **`9b3ed0e3`**. Eight codex transcripts under
`dev/plans/runs/codex/0.8.20/` (TC-RUBRIC-7). Closure `dev/plans/runs/0.8.20-slice-30-output.json`.

---

## 18. Slice 21 close — **R-20-CR COMPLETE** (TC-57 · ac_002 · TC-71)

**LANDED `77be504b`** (merge of `orch-0.8.20-s21` onto `main` @ `3e660f95`). **SCHEMA stays 24.**
Governed surface **byte-identical** — `check-governed-surface-pin.sh` exits **0**, no trip, no re-pin.

### The three legs

| Leg | Outcome |
|---|---|
| **TC-57** — governed-write vs projection-worker race | **CHARACTERIZED FIRST** per the ruled ordering (`seq-111`), then fixed. Repro **10/10 → 0/10**; control **0/10** throughout |
| **ac_002** — test-oracle repair | PWD/HOME/XDG/TMPDIR substring scan replaced by a **per-test sandbox in a child process**; the oracle asserts **emptiness** outside the DB path, not a name pattern. Part 1 (DB-parent allowlist) KEPT verbatim |
| **TC-71** — `vector_projection_declared` | Now requires the **`searchable` role**. Re-verified across **all three** gated paths: forward backfill, drop inverse, late enrolment |

### The characterization corrected the record (this is the durable finding)

TC-57's prior write-ups were **wrong on the mechanism**. Measured, not argued:

- The engine takes **plain `SQLITE_BUSY` (5) with the busy handler invoked ZERO times** — SQLite skips the
  handler for deadlock avoidance when a `BEGIN DEFERRED` transaction tries to promote while another connection
  holds the write lock. It is **NOT** `SQLITE_BUSY_SNAPSHOT` (517). 517 is real and is pinned by a standalone
  probe, but was **never observed from the engine** in 20 runs. **A fix scoped to "retry on 517" would have
  been inert** — which is precisely what the characterize-before-scope ruling exists to prevent.
- The rate is **10/10**, not "7 of 8" — deterministic at the right shape, failing on the third write.
- `PRAGMA busy_timeout` is **proven inert** here (the handler is never consulted on either exit).

Design of record: `dev/design/0.8.20-tc57-write-race-characterization.md` (`status: ACTIVE`).
**The fix is one line** — `commit_batch` takes `TransactionBehavior::Immediate`, so the transaction never
promotes. Unconditional, not conditional: a content-dependent transaction behaviour would be a new correctness
surface (mixed batches, edge arms), and the anonymous path already took the write lock at its first statement.

### codex §9 — 2 rounds, both transcripts persisted (TC-RUBRIC-7)

| Round | Transcript | Verdict |
|---|---|---|
| 1 | `dev/plans/runs/codex/0.8.20/slice-21-review-20260728T184947Z.log` | **0 P1, 1 distinct P2** → fix-1 |
| 2 | `dev/plans/runs/codex/0.8.20/slice-21-fix-1-rereview-20260728T192456Z.log` | **CLEAN** — zero `Review comment:` blocks |

**The round-1 P2 was substantive and was FIXED, not overridden.** Its claim: the role-aware predicate corrects
the *decision* to enrol but not the *already-enrolled state* — `vector_kind_needs_enrolment` checks
`kind_is_vector_indexed` **before** `vector_projection_declared`, and `project_canonical_node_row` gates the
embed enqueue **solely** on `_fathomdb_vector_kinds` membership, so a database that already ran the old code
keeps embedding forever. That is **TC-71's entire stated harm surviving the fix**. Verified against the code,
then closed by an **open-time reconciliation**, narrowly conditioned so legacy and healthy databases are
untouched. The load-bearing condition is `EXISTS(vector_declared = 1)` — **not** the table-exists probe, because
schema step 24 **creates** the registry table on every open, so a legacy database reaches boot with the table
present but empty.

### Gates (each captured individually, no pipe)

`clippy --workspace --all-targets` **0** · `check --workspace --all-targets` **0** · `fmt --check` **0** ·
`tc57_governed_write_race` **0** · `tc57_worker_commit_pressure` **0** · `tc71_fix1_inert_enrolment_reconcile` **0** ·
`slice21c_vector_role_gate` **0** · `lifecycle_observability` **0** · `slice15d_projection_registry` **0** ·
`slice20c_flush_barrier` **0** · `slice20_dense_readiness` **0** · `check-governed-surface-pin.sh` **0** ·
`lint-design-status.sh` **0** · `agent-lint-md.sh` **0** · `check-transcript-hygiene.sh` **0**.

**TC-57 acceptance bar (characterization §1.4), run to the bar and not off one pass:** repro **0/10** ·
control **0/10** · the three `tc57_mechanism_*` pins **0/3 failures** · OOS-17 guard
(`rebuild_projections --features operator`) **rc 0**.

### Carried forward — NOT fixed here, recorded so they are not lost

1. **~50% duplicate embeds at baseline.** An instrument built for this slice measured ~104 of 200 rows embedded
   **twice**, on the **anonymous** arm too (zero caller errors, no race present). Unrelated to TC-57 and
   unchanged by the fix.
2. **Worker-side commit failures are silently discarded.** `let _ = commit_projection_outcomes(...)` — a
   worker-side *commit* failure writes **no** `'failed'` terminal and **no** `projection_failures` row; the row
   keeps `terminal IS NULL` and is re-embedded. The `'failed'` arm is for *embed* failures only.
3. **`Engine::transition`'s own transaction is still a read-then-upgrade DEFERRED transaction** with the same
   exposure TC-57 closed in `commit_batch`, mitigated only by the pre-emptive `drain()` above it.
4. **R2 (bounded busy retry) as defence-in-depth** — needs the error path restructured first, since
   `commit_batch`'s `rusqlite::Error` is discarded and there is nothing to branch on.

---

## 19. Slice 22 close — **R-20-VC COMPLETE** (TC-67 · TC-68 · decision #18 · sqlite-vec `#99`)

**LANDED `572475f2`** (merge of `orch-0.8.20-s22` onto `main` @ `0cf2923c`). **SCHEMA stays 24.**
Governed surface **byte-identical** — `check-governed-surface-pin.sh` exits **0**, the AC-079 pin **never
tripped**, no re-pin, no allowlist diff. `check-c1-conformance.sh` exits **0** (26/26 checkable) — the
**Slice-30 publish precondition is intact**.

### The four legs

| Leg | Outcome |
|---|---|
| **TC-67** — silent unsupported vector kind | **REPORTED**, per HITL option **(c)**. New additive, output-only `ProjectionDelta.vector_unsupported_kinds` (`Vec<String>` / `string[]`), sorted, de-duplicated, **empty-not-absent**. Vocabulary **NOT grown**; **D3 lock untouched**; **readiness unchanged** and pinned in all three languages |
| **TC-68** — 45 synchronous embeds on every open | Cached against a fingerprint stored in the **existing** `_fathomdb_open_state` — **no `SCHEMA_VERSION` bump, no migration**. Fail-safe (`R-VEQ-4`) preserved |
| **#18** — `InvalidArgument` vs `WriteValidation` | `validate_write`'s write-**shape** boundary is **one family** (`WriteValidation`), with **one named, tested exception** |
| **`#99`** — sqlite-vec `vec0` DELETE | **REPRODUCED, and it was LIVE.** Remediated engine-side; **TC-76 → PROMOTE** |

### The finding that mattered — `#99` was not a hypothetical

Commissioned as a characterization probe that could legitimately come back clean. It did not.
**`sqlite-vec 0.1.7`'s `vec0Update_Delete_ClearMetadata` leaves `rc` holding `SQLITE_DONE` (101) on
success and never resets it to `SQLITE_OK`**; the epilogue returns it verbatim and `vec0Update_Delete`
aborts the whole `DELETE`. The INSERT/UPDATE twin has the identical leak but is rescued by an unconditional
`sqlite3_blob_close` — **that asymmetry is the entire bug**, and it is also what makes the workaround sound.

Any `vec0` TEXT metadata value over **12 bytes** trips it, which the Slice-15e `filterable` attribute columns
reach for any caller value of **≥ 12 raw bytes** (the `+1` is Slice 15e's `\x01` present marker). Measured:
11 bytes → `Ok`; 12 bytes → **`Err(Storage)` from BOTH `erase_source` AND `purge`, with the row left at
rest**. **A GDPR-erasure verb, failing, in the release that publishes.**

Reachability was bounded and stated: `kind` is unreachable (every enrolment door gates on
`resolve_source_type`, max 9 bytes), `source_type` is a partition key on different storage, `status` ships
the `''` sentinel. **`attr_*` values are the sole exposure.** The `attr_ + hex(name)` **column-name** axis
already exceeds 12 chars for any attribute name of ≥ 4 characters, so the shipped corpus has always
exercised that shape — which is the evidence that `#99` is about **values**, not names.

**Remediation is engine-side and version-independent:** `delete_vector_partition_row` blanks the `attr_*`
columns first (taking vec0's `n<=12 && prev_n>12` UPDATE branch, which deletes the shadow row **and**
returns `SQLITE_OK`), then deletes by `rowid`. Routed through **all six** by-rowid delete sites. Erasure is
**complete, not merely error-suppressed** — embedding bytes byte-identical, shadow row gone.
**A tripwire test asserts the upstream defect is STILL PRESENT**, so the workaround cannot silently outlive
its cause.

**Upstream tracks this as issue `#274`, NOT `#99`.** Determined at the orchestration level (part A had no
network): **`0.1.8` still carries the defect; `0.1.9` FIXES it**, with upstream's own comment naming the
issue. Latest stable is `0.1.9`; `0.1.10-*` are alphas. **No dependency bump was taken here** — that is
**TC-76**'s call, and the tripwire will correctly go RED on it. Design of record:
`dev/design/0.8.20-sqlite-vec-99-vec0-delete-probe.md` (§7.1 carries the remedy).

### TC-68 — the DoD clause was vacuous, and that was measured, not argued

The plan's DoD said *"open cost independent of enrolled-kind count"*. **That was ALREADY TRUE at baseline
and vacuously so** — the probe gates on `SELECT EXISTS(SELECT 1 FROM _fathomdb_vector_kinds)`, an EXISTS
and not a count, and its body never iterates kinds. A test built to that clause would have passed with zero
code change. Measured with a counting embedder instead:

| workspace | open 1 | open 2 | open 3 |
|---|---:|---:|---:|
| zero enrolled kinds | 0 | 0 | — |
| one enrolled kind | **90** | **45** | **45** |
| six enrolled kinds | **90** | **45** | **45** |

**Both 45 and 90 are true, of different opens** — 90 is the one-time population open (45 to baseline plus
45 for the fix-2 confirm pass); 45 is every open thereafter, forever. The honest acceptance is therefore
*"an open whose fingerprint is unchanged performs **zero** probe embeds; a fingerprint change re-runs the
full probe"*, and that is what is tested.

Fingerprint inputs: recipe tag · identity triple · mean-centering state **plus the live `mean_vec` bytes**
· the compiled-in fixture · both D4 floors · **the stored baseline rows including reference blobs**. The
`mean_vec` and blob inputs are load-bearing, not belt-and-braces: P1 quantizes against the live pinned
mean, and hashing blob content preserves the 0.8.18 fix-2 external-tamper property that would otherwise
have silently re-opened.

**The residual, stated and not buried:** a backend that drifts **without changing its declared identity**
moves no fingerprint input, so the cached verdict answers and the drift is caught only at the next open
whose fingerprint changed — which a pure backend swap never is. Pinned as an executable fact by
`residual_same_identity_backend_drift_is_not_caught_on_a_cached_open`. Retiring it (rather than bounding
it) needs a runtime backend/device descriptor on `EmbedderIdentity` — **described, deliberately NOT
implemented**, since that is public-surface work. Design of record:
`dev/design/0.8.20-tc68-equivalence-probe-fingerprint-cache.md`.

### Decision #18 — one family, and one exception that a prior review earned

`validate_write`'s write-**shape** boundary is now **one family**: `EngineError::WriteValidation`. This
follows `dev/adr/ADR-0.6.0-error-taxonomy.md` (accepted) — `WriteValidationError` is one of the per-module
errors that ADR names, while `InvalidArgument` is a later-minted **direct variant** it never assigned to a
module — and it makes the code agree with `dev/design/errors.md`, which defines `WriteValidationError` as
*"malformed typed write shape"*.

**The exception, and why it stands.** The orchestrator's brief asserted `validate_write` had exactly ONE
`InvalidArgument` site. **The implementer proved that wrong**: a second is reached from the Edge branch via
`reject_unrenderable_edge_epoch`, and its message is the **TC-33 fix-1 contract from a prior codex §9
finding** (an unrenderable epoch renders to `null` on the consolidation wire and silently resurrects an
invalidated edge, so the caller must be told *which* field). Collapsing it would have destroyed a
diagnostic a review specifically required. It is **not** collapsed; the rule is stated as what is actually
true — *the write-shape boundary is one family; the edge-epoch range guard is a named exception* — and is
**pinned in `error_taxonomy.rs` so it cannot drift in either direction**.

⚠ **If "one family" is read as absolute, this is the one item that does not meet it.** Flagged, not buried.

⚠ **BREAKING, on a published surface.** `WriteValidation` is a **unit variant** and both bindings render it
as a fixed, message-less string, so the unsatisfiable-window refusal **no longer carries the bounds** — the
exact diagnostic the pre-settlement split existed to preserve. Recorded in `CHANGELOG.md` and all three
interface docs, and the tests now **assert the bounds are absent** so the loss cannot regress silently.
`errors.md` also gained `InvalidArgument` in **both** its tables; it was genuinely missing from the locked
taxonomy of record despite ~15 engine raise sites and a live SDK class in both languages.

### codex §9 — four rounds, all persisted (TC-RUBRIC-7)

| Round | Transcript | Verdict |
|---|---|---|
| 1 | `slice-22-review-20260729T002907Z.log` | **TERMINAL-CLEAN** — zero `Review comment:` blocks |
| 2 | `slice-22-review-round2-targeted-20260729T004445Z.log` | Risks 1/3/4 **PASS**; **1× [P1]** on the TC-68 cache |
| 3 | `slice-22-fix-1-rereview-20260729T011134Z.log` | **[P1] still OPEN** — the equivalence argument was unsound |
| 4 | `slice-22-fix-2-rereview-20260729T014025Z.log` | **PASS — [P1] CLOSED** |

**Round 1's clean verdict was not banked.** The transcript showed **zero** occurrences of
`force_probe_verdict_rerun` — the helper that adjusted 12 shipped equivalence-probe tests once the verdict
became cached, i.e. the single riskiest change in the slice. **A clean verdict over an unexamined risk is
not a PASS**, so round 2 was commissioned against four named risks. It found the [P1]. Round 2 then
confirmed Risk 1 on the merits: no assertion was relaxed, and the helper only restores the pre-cache
"the probe actually runs" precondition.

**The [P1], and why fix-1 failed.** The cache marker is a SHA-256 over deterministic, publicly derivable
inputs, so a database writer can forge it and skip the 45-probe verification. fix-1 argued the weakness was
**pre-existing** — that the same actor could equally re-baseline `_fathomdb_embed_probe` — and **proved that
by test**. codex accepted the test and **rejected the inference**: re-baselining additionally requires the
target backend's 45 exact embeddings, correctly encoded. *"That is a materially higher bar."* **codex was
right**, and the claim *"the cache adds no new attack surface"* was an over-claim.

**fix-2 conceded rather than argued**, struck the claim everywhere, and stated codex's finding as the
project's own: **the cache does introduce a cheaper bypass.** It then added the bound — a same-identity
drift moves no fingerprint input, so an attacker gets the same payoff **with no forgery at all** from an
honestly recorded marker; the forgery's *incremental* value is confined to opens where no valid marker has
been recorded **or retained**. The implementer **refused to round that to "dominated"** and named the case
that matters: **the open right after a legitimate fingerprint change** — the only recurring detection
opportunity the residual left standing, which a forged marker removes. That is the honest severity.

**Verdicts were read from codex's own verdict blocks, never from a `[P1]`/`[P2]` marker tally (TC-87).**

### X1 SDK parity — ran BEFORE the land, live

- **TypeScript** `npm test` — **250 / 250**, rc **0** (241 at Slice 20c; +9 from this slice).
- **Python** — **46 / 46**, rc **0**, including the **6** TC-67 arms, run against a **wheel built with
  `maturin build` and installed into a throwaway venv OUTSIDE the repo**. `maturin develop` and
  `pip install -e` were **never** run; the shared `.venv`'s `fathomdb.pth` was verified **unchanged**
  (still 2026-07-09) afterwards.
- ⚠ **Trap, new:** `src/python/pyproject.toml` sets `pythonpath = ["."]`, which prepends the source root
  and **shadows an installed wheel** — `import fathomdb` then resolves to `src/python/fathomdb/`, which has
  no native module, and the suite dies at collection with a misleading *"circular import"*. That is a
  **sys.path artefact, not a code defect**. Run with `-o pythonpath=` from a neutral cwd, or use the
  fresh-clone-with-inside-venv route.

### Gates at the landing tip

`cargo check --workspace --all-targets` **0** · `cargo clippy --workspace --all-targets` **0, zero
warnings** · `check-governed-surface-pin.sh` **0** (allowlist sha unchanged) · `check-c1-conformance.sh`
**0** · `check-ledgers.sh` **0** · `agent-lint-md.sh` **0** · `check-transcript-hygiene.sh` **0** ·
`SCHEMA_VERSION` **24** · engine suite (default + `--features operator`) **0** · `fathomdb-schema`,
`fathomdb-query`, `fathomdb-embedder-api` **0**.

⚠ **`ac_029_canonical_writes_complete_under_projection_stall` is a WALL-CLOCK RATIO assertion**
(P-STALL-TOL 1.5×). It failed once — `baseline=899ms stalled=4.24s` — **while a codex review was running
concurrently**, and passed **3/3** on a quiet machine. A timing gate measured under load is not evidence
either way. **Do not run the engine suite alongside a reviewer.**

### Carried out of this slice — for the Steward, not written here

- **`sqlite-vec` → `0.1.9`** is now a concrete, evidenced remedy. **TC-76's re-open trigger has FIRED.**
  🕮 **RULED since** (`seq-151`): **hold at `=0.1.7` for 0.8.20**; the 0.1.9 bump lands at **0.8.22** with
  `rusqlite 0.31→0.40`. Not 0.8.20 work.
- **A message-carrying `WriteValidation { msg }`** — ~14 engine + ~44 binding raise sites and both binding
  payload shapes. Its own slice, not a rider.
- **A runtime backend/device descriptor on `EmbedderIdentity`** — the only change that *retires* the TC-68
  residual rather than bounding it. Public surface; needs sign-off.
- **Bounded-staleness re-probe** — considered and **deferred**, with the trade recorded: it would partially
  undo the constant-open-cost property TC-68 was ruled to deliver.
- **Several engine test targets need `--features operator`** (`excise_source`, `erasure_completeness`,
  `rebuild_projections`, `erasure_projection_registry`, `check_integrity`, `rebuild_vec0`) or they exit
  **101 without running**.
- **`cargo clippy --workspace --all-targets --all-features` cannot work in this repo** — it pulls `objc2`,
  which hard-errors off Apple platforms. Pre-existing.

---

## 20. Slice 31 close — **Library Sweep #3 leg 1/3**: SBOM-survey tool spec + RED tests

**LANDED `d0287620`** (merge). Branch `0.8.20-slice-31-sbom-spec`, cut from `cbb56212`, `origin/main` integrated
before the land. **Carries no `R-20-xx` requirement id** (the TC-76 precedent) and **mints no AC** — gated by TDD
test name, per plan §3a.

### 20.1 What shipped

The four deliverables §3a asks for, and nothing else: **requirements · 23 acceptance criteria · design · RED tests.**

| Artifact | Path |
|---|---|
| Spec of record (req + AC + design + per-round fix records) | `dev/design/0.8.20-slice-31-sbom-survey-tool.md` |
| RED acceptance suite (8 files, 23 tests) | `scripts/sbom-survey/tests/` |
| Operating note — how to run it, why it is not CI-gating | `scripts/sbom-survey/README.md` |
| Index rows | `dev/DOC-INDEX.md` |
| Closure record | `dev/plans/runs/0.8.20-slice-31-output.json` |
| codex §9 transcripts, rounds 1–7 | `dev/plans/runs/codex/0.8.20/slice-31-round{1..7}-*.log` |

**NO implementation code** — the `sbom_survey` package, its `pyproject.toml`, its `tiers.toml` and its CLI are all
**Slice 32**. Running the tool is **Slice 33**.

### 20.2 The RED contract — and why landing a failing suite is safe

The suite is **23 failed / 0 passed / 0 skipped / 0 errors**, exit 1. That shape is the deliverable, not an accident:

- Every test fails **inside its own body**, naming its criterion and the behaviour Slice 32 owes. Nothing fails at
  **collection**, so no failure can hide another — a module-level import of the absent package would have aborted
  collection and masked the rest.
- **Zero skips.** A skip here would be a vacuous green. Even the missing optional CycloneDX validator is made to
  **fail** rather than skip.
- It is wired into **no gate whatsoever** — not `.github/workflows/ci.yml`, not `scripts/agent-test.sh` (whose
  `scripts/tests/**` entries are registered one-by-one, and this is a *different* directory), not `ruff` (scoped to
  `src/python`), and not any pytest config (the repo's only one is `src/python/pyproject.toml`, not an ancestor).
  **Verified, not assumed.** So a permanently-red suite sits on `main` without turning anything red.

⚠ **The forcing function for Slice 32:** do **not** wire this suite into CI or `agent-test.sh` until it is GREEN.

### 20.3 Design decisions of record

- **Discovery is `git ls-files`-derived**, so untracked manifests are structurally out of BOM scope — including the
  wholly-gitignored `/python/` tree, exactly the "gitignored dev/eval lockfile" trap `LIBRARY-BUMP-STEWARD.md` §2 warns of.
- **Tier rules (`shipped` / `dev-tooling` / `eval-only`) are derived from `.github/dependabot.yml`**, an existing
  tracked source of record, rather than re-decided. Matching is **longest-prefix-wins and order-independent**, with
  **no catch-all**: an untiered tracked manifest is a hard error naming the path, and the CLI exits 2.
- The **8 `dev/release/fixtures` skew manifests are excluded by tracked data**, not a hardcoded path test, and the
  exclusion is recorded in `survey.excluded` so it stays auditable. `dependabot.yml` already ruled the same way.
- The **published-version lookup sits behind an injectable seam**: the suite never needs the network, and an unknown
  latest can **never** render as up-to-date — the single worst failure this tool could have, and it has its own test.
- **Slice 33's findings home** is `dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md` (house convention,
  chosen over `dev/design/` and `dev/deps/` after weighing all three). Generated reports stay **gitignored**.

### 20.4 Gates — real exit codes, re-verified by the orchestrator after merging `origin/main`

| Command | rc |
|---|---|
| `bash scripts/check-governed-surface-pin.sh` | **0** |
| `git rev-parse HEAD:src/conformance/governed-surface-allowlist.json` | `93ef3a86…` — **byte-identical to base** |
| `bash scripts/check-ledgers.sh` | **0** |
| `bash scripts/lint-design-status.sh` | **0** |
| `bash scripts/agent-lint-md.sh` | **0** |
| `bash scripts/lint-findings.sh` | **0** |
| `bash scripts/check-transcript-hygiene.sh` | **0** |
| `bash scripts/check-release-state-views.sh` | **0** |
| `python3 -m pytest scripts/sbom-survey/tests -q` | **1** — 23 failed, 0 passed, 0 skipped, 0 errors |
| `grep SCHEMA_VERSION …/fathomdb-schema/src/lib.rs` | **24** |

**Guardrails held:** no dependency bump; **no manifest and no lockfile edited or created**; no `.gitignore` edit;
governed-surface allowlist byte-identical (pin never tripped); SCHEMA stays 24; no AC minted; no CI wiring.
`scripts/agent-test.sh` was **not** used for any green claim (TC-16 — its aggregate exit is vacuous).

### 20.5 codex §9 — seven rounds, twelve findings, **zero repeats**

| Round | Findings |
|---|---|
| 1 | 3×`[P2]` — `setup.cfg` absent from the manifest oracle; no `purl` assertion; dependency graph tolerated **empty** |
| 2 | 3×`[P2]` + `[P3]` — CycloneDX validity **self-certified by the code under test**; fixture exclusion proven only below the survey boundary; determinism tested only on the pinned-`now` path; manifest count stated 28, actually **29** |
| 3 | 1×`[P2]` — longest-prefix tier matching frozen in spec, ungraded by any test |
| 4 | `[P2]`+`[P3]` — injected published-source never driven through `run_survey`; `--describe` tier vocabulary unasserted |
| 5 | 1×`[P2]` — **inverse class**: CLI oracle over-specific, would have rejected a *correct* implementation |
| 6 | 1×`[P2]` — originless **direct** components skipped past the tier oracle |
| 7 | **TERMINAL-CLEAN** — *"I found no discrete, actionable bugs in the added spec/docs and RED acceptance tests."* |

**The dominant class, worth carrying forward (TC-105):** *a criterion graded against a helper while the integration
boundary that must call it goes ungraded.* It recurred three times before a **directed sweep of all 23 criteria** at
fix-4 closed six further instances at once and dropped the finding rate from 3–4 per round to 1. **For a spec-and-RED-
test slice the oracle IS the deliverable**, so a permissive oracle is a defect in the product itself — which is why
none of these was carried.

**Round-cap history.** Round 6 hit the **TC-82 cap**. The orchestrator halted and checked in rather than opening round
7. The Steward ruled the rounds **productive** — twelve findings, all new and distinct, so the same-finding bound of 3
never engaged — and authorized **one bounded round 7** with the exit **pre-committed** (record the residual as a TC and
land; round 8 forbidden), `seq-162`. Round 7 closed it and the fallback did not fire.

### 20.6 Owed / handed on

- **Slice 32** inherits two obligations discovered during review: `cyclonedx-python-lib[json-validation]` must actually
  be installed, and `run_survey` must accept a `tier_map` seam. Both are in the spec of record.
- **TC-106** — `dev/tools/mermaid/package.json` is a **tracked npm manifest under no configured Dependabot directory**,
  the same class as the `R-DEP-2` root-manifest note. Nothing was changed about it; handed to Slice 33 / **0.8.22**.
- **TC-105** — the review-cost generalisation above, for budgeting Slices 32 and 33.
- **Steward reconciliation owed:** `dev/plans/release-state-0.8.20.json` (ladder 31 → LANDED + sha, `next_slice` → 32,
  `landed` list) and the master's generated ladder-progress view. Per the Slice 23 precedent (`603af000`) those are the
  Steward's, not the orchestrator's, and this land deliberately left both untouched.
  🕮 **DONE** — reconciled by the Steward; 31/32/33 are all LANDED in the single writer and `next_slice` is
  now **39**. *(Superseded again: Slice 39 has since landed and `next_slice` is now **40** — §21.)*

---

## 21. Slice 39 close — publish-facing documentation (R-20-DOC)

**LANDED `91db34d8`**, off branch `0.8.20-slice-39-doc`, baseline `3e9d6d12`. **17 commits**, landed as a
**fast-forward ref-to-ref push (TC-110) — there is NO merge commit**, which is why the board-currency gate was
never triggered and why the landing SHA is an ordinary `docs(…)` commit rather than a `merge(0.8.20): …`
subject. Verified: `git rev-list --count 3e9d6d12..91db34d8` = **17**. SCHEMA stays **24**
(`pub const SCHEMA_VERSION: u32 = 24`). Carries requirement id **R-20-DOC**; **mints no AC**.

### 21.1 The license reconciliation — what the diff actually changed

The HITL ruled the project **MIT** at `seq-193`. Confirmed from `git show 91db34d8 -- <manifests>` and re-read at
HEAD, the four publishable manifests each moved **Apache-2.0 → MIT**:

| Manifest | Before → after | At HEAD |
|---|---|---|
| `Cargo.toml` `[workspace.package]` | `license = "Apache-2.0"` → `"MIT"` | `:33` |
| `src/python/pyproject.toml` `[project]` | `license = { text = "Apache-2.0" }` → `license = "MIT"` | `:26` |
| `src/ts/package.json` | `"license": "Apache-2.0"` → `"MIT"` | `:5` |
| `src/ts/npm/linux-x64-gnu/package.json` | `"license": "Apache-2.0"` → `"MIT"` | `:5` |

A **fifth** site moved that the ruling's four-manifest wording does not name: `src/ts/package-lock.json`, the
`""` **ROOT** entry only — one line, `"license": "Apache-2.0"` → `"MIT"`. Every other `license` key in that
lockfile is a dependency's own factual record and was left alone.

**Making the text SHIP is a separate change from the SPDX field, and it is the larger half.** Three mechanisms,
one per ecosystem, all present in the diff:

- **Cargo** — `[workspace.package] license-file = "LICENSE"` (`Cargo.toml:45`) plus an inherited
  `license-file.workspace = true` on each publishable crate. Verified: **7** crate manifests under
  `src/rust/crates/*/Cargo.toml` carry that key. **No per-crate LICENSE copy exists** — the one repo-root file
  is the only Rust-side source.
- **npm** — a **real copy**: `src/ts/LICENSE` and `src/ts/npm/linux-x64-gnu/LICENSE`, 21 lines each, both
  net-new in the diff.
- **Python** — PEP-639 `license-files = ["LICENSE"]` (`src/python/pyproject.toml:27`) plus the real copy
  `src/python/LICENSE` (21 lines, net-new).

Tracked LICENSE files at HEAD are exactly these four: `LICENSE`, `src/python/LICENSE`, `src/ts/LICENSE`,
`src/ts/npm/linux-x64-gnu/LICENSE`.

**Two riders inside the same commit**, both visible in the diff and neither implied by the license ruling:

1. `src/python/pyproject.toml` raised the maturin floor **`>=1.7,<2` → `>=1.9,<2`** in `[build-system]` and in
   the `test` and `dev` extras.
2. `src/ts/package.json` `files` gained **`!dist/tests`** and **`!dist/src`**.

### 21.2 The guard — a mechanism, not a note

`scripts/check-license-consistency.sh` (**611 lines**, net-new) plus `scripts/tests/test_check_license_consistency.sh`
(**310 lines**, 19 arms) and a registration line in `scripts/agent-test.sh`. **Steward-re-run at HEAD in the
primary checkout: rc=0**, emitting one `ok` line per unit — 7 crates + 2 npm packages by real packaging-tool
output. The **wheel is the one unit the default run does NOT prove**; the script says so itself
(`~ wheel: MECHANISM PROXY only … Run with --with-wheel to read a real wheel`). So **"a LICENSE ships in all 10
units" is 9 units proven by tool output plus 1 by mechanism proxy** on the default path.

### 21.3 The CHANGELOG — the reason 39 ran before 40

`CHANGELOG.md:13` at HEAD reads **`## 0.8.20 — 2026-07-30`** — the bare-version heading form
`verify-release-gates.sh` check 4 requires, which is the whole sequencing rationale for putting 39 ahead of 40.
Commit `4a9f467a`, **+447/−5**. Its stated schema span **15 → 24** is corroborated: `SCHEMA_VERSION` is **15** at
`v0.8.9` and **24** at HEAD.

### 21.4 What else shipped

| Leg | Commits | Substance |
|---|---|---|
| 1a — license | `fdc1e810`, `aa01f0d9` | §21.1 + §21.2 |
| 1b — registry surfaces | `ea11151a`, `79bcf881`, `f9cbda88` | 9 net-new READMEs (7 crates + both npm packages, +592), the PyPI landing page (+96/−6), and `readme`/`keywords` keys on all 7 crate manifests — **`categories` deliberately absent per `seq-198` ruling 2** |
| 2 — CHANGELOG | `4a9f467a`, `aa61050b` | §21.3 |
| 3 — currency | `c50c8dc6`, `0a6bb497`, `bb1f91e0`, `1bbf9b43` | `docs/` + root README across 24 files (+1010/−347); `dev/interfaces` × 6 files (+502/−70); crate-level rustdoc across all 7 crates (+298/−34); the 0.6.0/0.6.1/0.8.0 release-notes pages marked historical |
| fix-1 | `76c3f014`, `91fb2783`, `88a96fa7` | binding surfaces name `WriteValidation`, not `InvalidArgument` (TC-95) — this one touched **shipped binding source**: `_fathomdb.pyi`, `engine.py`, `src/ts/src/index.ts` |
| fix-2 | `0625b8d3` | `docs/embedder.md` names `EmbedderNotConfiguredError`, the shipped class (2 lines) |
| closure | `0e6fa813`, `91db34d8` | closure records + 3 codex transcripts; then the TC-86 redaction (§21.6) |

### 21.5 codex §9 — three rounds, terminal PASS

Round 1 `1bbf9b43` **CONCERN** (2 findings) → fix-1 → round 2 `88a96fa7` **CONCERN** (both prior resolved, 1 new)
→ fix-2 → round 3 `0625b8d3` **PASS**. Within the §6 cap; **no finding recurred.** Four transcript artifacts are
tracked under `dev/plans/runs/codex/0.8.20/`: `slice-39-20260730T045914Z.log`,
`slice-39-fix-1-rereview-20260730T052714Z.log`, `slice-39-fix-2-rereview-20260730T053751Z.log`, and the promoted
verdict `slice-39-review.md`.

⚠ **Reviewer limitation, recorded by the orchestrator and not independently re-testable here:** in all three
rounds codex reported `git log/show/diff` failing inside its sandbox (`bwrap: loopback: Failed RTM_NEWADDR`).
It verified **current-file truth only**; every diff-shaped claim was verified by the orchestrator from git.
**UNVERIFIED by the Steward** — the transcripts were not re-read for this close record.

### 21.6 One closure claim did NOT survive its own next commit

The closure commit `0e6fa813` and `dev/plans/runs/0.8.20-slice-39-output.json` both assert
*"Transcript hygiene rc=0; zero TC-86 redactions were needed."* **That claim was false when written.** The very
next commit — `91db34d8`, the landing SHA itself — is
*"TC-86 redact out-of-repo inventory from the round-1 transcript"*, and its diff removes a 36-line out-of-repo
directory listing from `slice-39-20260730T045914Z.log` (**+20/−36**). Its own message explains why the earlier
check passed: a standalone `check-transcript-hygiene.sh` run **before `git add`** returns 0 on an **untracked**
transcript; the honest gate is `preflight.sh --landing` **after** committing. The record is therefore
self-correcting, but the closure JSON's sentence is **stale and should not be cited**.

### 21.7 Claims from the commit messages NOT corroborated here

Each of these is asserted in a Slice 39 commit message and is **not derivable from the diff**; none was re-run
by the Steward. Marked **UNVERIFIED**, not disputed:

- **UNVERIFIED** — that `cargo package --list` / `npm pack --dry-run` / the built wheel shipped **no** license
  file at the `3e9d6d12` baseline. The *post-fix* state is verified (§21.2); the baseline measurement is not.
- **UNVERIFIED** — "6 of 7 crates byte-verified inside a real `.crate`", and that `fathomdb-engine` cannot be
  tarballed at 0.8.9 because published `fathomdb-embedder` 0.8.9 lacks `rerank-cuda`. The default
  `check-license-consistency.sh` run uses `cargo package --list`, which does not build a tarball.
- **UNVERIFIED** — the maturin sweep (`maturin sdist` exits 1 on 1.7.8/1.8.0, 0 on 1.9.0–1.13.1) that motivates
  the `>=1.9` floor.
- **UNVERIFIED** — that npm **silently ignores a LICENSE symlink**, and that packing the stale
  `tsc -p tsconfig.json` tree shipped 97 files with `main` broken. Both are measurement claims behind the
  §21.1 riders.
- **UNVERIFIED** — the CHANGELOG's `### Removed = None` evidence (AC-050c linter rc=0 + an additions-only
  symbol-set diff).

### 21.8 Handed on to Slice 40

- **CI wiring for `check-license-consistency.sh`** — it is registered in `scripts/agent-test.sh` but **not** in
  `.github/workflows/ci.yml` and **not** in `preflight.sh --landing`, because `.github/` is Slice 40's exclusive
  territory this release. Stated explicitly in `aa01f0d9`'s message.
- **Three non-shipped project-license declarations knowingly left at Apache-2.0** for 0.8.22:
  `scripts/sbom-survey/pyproject.toml:29` and the two corpus generators under `tests/corpus/scripts/`
  (changing those without regenerating would desync the committed hash-bearing `manifest.json`).
- **No version field was touched** — the manifests still read `0.8.9`; **Slice 40 owns the bump.**
