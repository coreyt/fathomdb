---
status: ACTIVE
---

# FathomDB — Steward Session Hand-off (2026-07-29-A)

> **Boot:** run **`/steward`**, do its §3 cold-start reading (**start with `scripts/steward-orient.sh`**),
> then read THIS doc, return a short orientation, and **WAIT for the HITL** before mutating anything.
> Supersedes `STEWARD-SESSION-HANDOFF-2026-07-27-A.md`.

## 0. State — do NOT copy numbers out of here

`scripts/steward-orient.sh` prints branch / HEAD / worktrees / landed slices + SHAs / SCHEMA / ledger tail /
todos fold / next action, all from the **single writer** `dev/plans/release-state-0.8.20.json`. **Run it and
trust it over this section**, which is a snapshot and will drift.

| | at hand-off |
|---|---|
| `origin/main` | **`603af000`** (local in sync) |
| Working tree | clean |
| Steward ledger tip | **`seq-149`** |
| Todos ledger tip | **`TC-102`** |
| 0.8.20 ladder | 0 · 5 · 10 · 15 · 20 · **21** · **22** · **23** · 25 · 30 **all LANDED**; remaining **31 → 40** |
| SCHEMA | **24** |
| Worktrees | primary · `0.5.1-memex-build` (Memex vehicle — **leave alone**) · `refactor-background-check` (unrelated). No orchestration worktrees outstanding. |

## 1. Immediate next action

**🔴 The batched governed-surface decision is DUE and it is the HITL's.** It is gated at the **close of
Slice 23** (`seq-141`), and Slice 23 landed at `30102ecd`.

**Its input is complete and final, which is the part worth knowing:** Slices 22 and 23 **both contributed a
ZERO allowlist delta** (byte-identical, pin green at 30/5/5, never tripped), and **23 was the last unit that
could move the surface** — the allowlist is SDK *command names*, and neither Slice 31 (a dependency sweep
cannot add a verb) nor DOC-HYGIENE-3 (docs/tooling) can change one. So what remains to weigh is exactly the
accumulated **Slice 20/25/30** delta, unchanged, plus the **TC-52 `_comment` re-pin**.

It is **non-blocking**: Slice 31 and DOC-HYGIENE-3 may proceed while it sits with the HITL. **Slice 40 does
depend on it** (AC-079 mints pinned to the allowlist content).

**Then the sequence is `31 → DOC-HYGIENE-3 → 40`** (F-36 as amended; `seq-141` placed the surface decision).

- **31** — Library Sweep #3, **deliberately no requirement id** (TC-76). ⚠ The **`sqlite-vec` 0.1.9**
  question is **HELD by the HITL** and lands here if taken — see §3 item 4.
- **DOC-HYGIENE-3** — cross-cutting, not a ladder slice. Scope ruled at **TC-53 + TC-88 + TC-89 + TC-92 +
  TC-94** (`seq-137`/`seq-138`). **TC-100 would be a sixth and that is an HITL call.**
- **40** — TC-16 determination **first** (`seq-118`), then the `#11`-full rehearsal, then AC-079 mints, then
  **PUBLISH** (hard HITL gate).

**Commission with `scripts/commission-manifest.sh 0.8.20 31`.** Re-verify the section it cites as
`{{MANDATE}}` (plan §9) **before** briefing — it went stale at *both* the Slice 21 and Slice 23 commissions.
TC-89 is the real fix and is scoped into DOC-HYGIENE-3.

## 2. ⚠ Do NOT re-litigate — and one correction that IS load-bearing

**(a) The open-decision count moved 2 → 6 and NO new work appeared.** Four HITL items that had lived only in
the ledger and in session transcripts were written **into** `decisions.unruled` at `seq-150`. That closes the
under-reporting the `2026-07-27-A` hand-off §4 warned about. `/decisions` is now true; board §4's
enumeration and its generated count agree.

**(b) `seq-106`'s "two stops remain" is HISTORICAL.** It was true of the *ladder gates* when written. Items
3-6 in §3 are later HITL questions, not a re-opening of it.

**(c) TC-94 defect (3) was FALSE and is WITHDRAWN (`seq-146`).** A prior Steward claimed a full token match
*suppresses* `commission-manifest.sh`'s coverage warning. It does not: slices 15/20/21/31 all emit
`NO design doc mentions: <token>` **while other tokens matched**, and `git log -S` shows one commit, never
regressed. **The real defect is TC-100** — the token scan is a bare substring test, so `"C-1"` matches inside
`"TC-15"` and Slice 15 currently cites **18 docs** on that one token. Defects (1) and (2) of TC-94 stand.

**(d) The TC-68 "45 re-embeds per open" figure is STALE.** Measured at HEAD: **0 / 90 / 0**, 20/20 zero
variance. Corrected in the todos ledger; do not re-derive the old number from the older entry.

## 3. Open HITL decisions — SIX, and the file now knows all of them

Read `dev/plans/release-state-0.8.20.json` `decisions.unruled`. **Only item 2 halts the run.**

| # | Decision | State |
|---|---|---|
| 1 | **Batched governed-surface** | 🔴 **DUE NOW.** Input final (see §1). Non-blocking. |
| 2 | **PUBLISH** the breaking pair (`0.8.9 → 0.8.20`) | **`halts_run: true`.** HITL *prefers* publish-after-40 and **explicitly deferred** (`seq-135`) — **not a ruling, and not authorization to bump a manifest, cut a tag, or publish.** |
| 3 | **TC-98** — does `#18`'s one named exception satisfy "one family"? | Deferred past Slice 23 (`seq-143`); **23 has landed, so it is due.** Retrospective about the landed Slice 22, not a forward gate. |
| 4 | **`sqlite-vec` 0.1.9 (TC-76)** | **HELD** (`seq-143`). Trigger FIRED. |
| 5 | **TC-100 placement** | A sixth id on a DOC-HYGIENE-3 scope ruled at five. |
| 6 | **TC-93** — pre-publish "Slice 4x" for the Dependabot advisories | 6 open, 2 HIGH on the default branch. Does **not** overturn "Dependabot stays out of the sequence" or TC-78. |

**On item 4, the detail that decides it:** the defect is upstream **`#274`, not `#99`** — a mis-citation this
repo carried for months, so any search against `#99` comes back empty and looks reassuring. **`0.1.8` still
carries it; `0.1.9` fixes it**, so a routine bump-to-latest-patch would look like a remedy and deliver none.
FathomDB ships an engine-side workaround across all six by-rowid delete sites **plus a tripwire that asserts
the upstream defect STILL EXISTS** — so **a bump and the tripwire must move together, or a *successful*
upgrade turns the suite red.**

## 4. What this session actually found — the two that change how you work

**(a) A live GDPR-erasure failure, caught by a probe commissioned only to "record a finding either way".**
`sqlite-vec` 0.1.7 aborts a `vec0` DELETE for TEXT metadata over 12 bytes, which the Slice-15e
**caller-supplied** `attr_<hex>` columns reach routinely. **`erase_source` AND `purge` both returned
`Err(Storage)` and left the row at rest** — data surviving an erasure request, in the release that publishes
to three public registries. Measured boundary: raw 11 → Ok, raw 12 → Err. Fixed engine-side in Slice 22.
**A probe is not a formality.**

**(b) A characterization template that would have filed a FALSE NEGATIVE.** TC-90 reproduces **10/10** under
a stress arm — and **0/10** under the TC-57-shaped paced protocol that the commission told the orchestrator
to follow. An agent obeying that brief would have concluded `Engine::transition` is clean. **The template
needed a stress arm; the commission did not say so.** Also from the same slice: **reproduction, not rate, is
the bar** — the rate did not replicate across sessions on the same commit (5.9/40 vs 3.8/40) while
reproduction held 10/10 both times.

## 5. Traps — read before trusting any green

1. **`cargo test --workspace` is not a stable signal (TC-72)** — ~1 run in 3 fails on plain `main`.
2. **`ac_029` is a WALL-CLOCK RATIO assertion (TC-97)** — it failed once at `baseline=899ms stalled=4.24s`
   **while codex ran concurrently**, then 3/3 quiet. **Never run the engine suite alongside a reviewer.**
3. **`src/python`'s `pythonpath = ["."]` shadows an installed wheel (TC-97)** — kills the X1 Python route at
   *collection* with a misleading *circular import*. Use `-o pythonpath=` from a neutral cwd.
4. **Six engine targets exit 101 WITHOUT RUNNING absent `--features operator` (TC-97).** `--all-features`
   clippy is impossible here (`objc2`).
5. **`scripts/agent-test.sh`'s aggregate exit is VACUOUS** — aborts at the known-red actionlint fixture
   (TC-16, placed at Slice 40) and never reaches the Rust or Python steps. Run suites individually.
6. **Capture `rc=$?` on the very next line**, before any pipe or command substitution.
7. **TC-83, WIDENED:** *any* full-command-line matcher — `pgrep -f`, **`pkill -f`**, `ps | grep` — matches
   **its own** invocation. One orchestrator killed its own shell (exit 144) with `pkill -f` *after* being
   briefed against the `pgrep` form. Match the binary (`pgrep -x codex`) or exclude self by pid.
8. **Never triage a codex review by grepping `[P1]`/`[P2]` markers (TC-87).** `lib.rs` alone carries **21**
   and **101** such markers before any review runs, and codex echoes source into transcripts. A tally
   measures echoed source, not findings, and it **inflates**. Read codex's own verdict blocks.
9. **`ledgerwrite` writes a `.seq` sidecar (TC-88).** Stage it **with** the `.jsonl`. A stranded sidecar
   breaks `preflight --landing` on `main` **for the next agent** while you see nothing wrong.
10. **Use `git commit -F` / `git merge -F` with a quoted heredoc — never `-m` with backticks.** A Steward lost
    two words out of a merge message to shell substitution this session (`design_refs`, `contract`). Caught
    before pushing. The failure is **silent** and the artifact looks intentional. This is the TC-53 class that
    has corrupted the steward ledger twice.
11. **`OPP-12-C1-converged-contract.md` is BYTE-PINNED** (sha256 **and** git blob sha1, by
    `scripts/c1-conformance-pin.json`; `check-c1-conformance.sh` hashes on-disk bytes and a whitespace-only
    change fails **deliberately**). **Cite it; never edit it.** It also records `status: UNREVIEWED` despite
    being the ratified contract — **TC-101**, fixable only by a coordinated **pin re-issue**, and it should be
    taken together with **TC-80** so the pin is re-issued once.

## 6. How this session commissioned, and why it worked

**The orchestrator is the mechanism (`/orchestrate`, or a Steward-spawned `orchestrator` agent when the
Steward holds the loop) — never `/goal`.** Every commission this session followed the same shape, and it is
worth copying:

1. **Regenerate the brief** — `scripts/commission-manifest.sh 0.8.20 <slice>`. It hard-fails on a dead
   citation or an empty design tier, so a green manifest is real evidence.
2. **Re-verify the `{{MANDATE}}` anchor before briefing.** Plan §9 was stale at two of three commissions.
3. **Record the commission in the ledger BEFORE it starts.**
4. **Verify the result from git** — ancestry, byte-level diffs, gates re-run — **never from the report.**

**The `design_refs` fix (`d30ef52f`) removed the tax that made this painful.** A ladder entry may now carry
`design_refs`, so a reserved-gap slice no longer hard-fails the TC-37 guard, and `dev/adr/**` /
`dev/interfaces/**` — previously unreachable at any status — can be cited. **Do NOT go back to writing
"Requirement traceability" notes into design docs.** Eight such notes remain in `dev/design/**` from before
the fix; **removing them is owed work**, deliberately deferred because Slice 23 was reading three of those
files. They are redundant now.

## 7. Cadence — and the thing that will bite you

**A commissioned orchestrator returns ONCE and does NOT notify you on stall.** One stalled **36 hours**
unnoticed in this program. **Poll from git** — branch tips, worktree mtimes, task-output mtimes. This session
had a one-hour idle gap because a codex review finished and nobody was watching.

**The fix-round cap (TC-82/TC-84):** 3 rounds on the same finding · **halt and check in with the Steward at
6** · 7-10 only where the Steward rules the rounds **productive** (a new and distinct defect each time) ·
beyond 10 an HITL halt. A mid-flight `SendMessage` steer is **not** a round.

**When you rule a round-6 check-in, pre-commit the fallback.** Slice 23's went: *"round 7 is authorized for
exactly these two things; if it does not close, take `#[ignore]` yourself without checking back; round 8 on
this pin is forbidden."* It closed on round 7. Pre-committing the exit stops the decision being re-litigated
under sunk cost.

**Escalate a pin trip, never clear it.** Widening `governed-surface-allowlist.json` or re-pinning to make a
gate pass is forbidden at every level (`seq-113`).

## 8. Standing rules that outlive this hand-off

- **Trust git, not narration.** Verify every "closed / landed / green" against the diff and real exit codes.
- **The mandate rule.** Direction and record changes — a release slot, moving an item between releases,
  altering an I-edge, re-sequencing — are **always** explicit HITL. Never inside an implied mandate.
- **You cannot launder authority downward.** A message to an orchestrator is peer-level, not the HITL's.
- **Push scope is fathomdb-only.** Never push memex without a specific per-push HITL directive each time.
- **Two-tier numbering.** `x.y.z` real/publishable · `x.y.z.p` pico label-only · **`13` forbidden** · publish
  is a separate explicit HITL gate.
- **Verify the branch (`git rev-parse --abbrev-ref HEAD`) before EVERY commit or push.** The tree is shared.
- **Never open a ledger by hand** — `ledgerwrite` to append, `ledgerwatch` to read deltas.
- **Surface your own errors in the open.** This session's Steward recorded five of its own — a false blocker,
  a vacuous DoD clause, an invented interface-doc count, a withdrawn TC-94 defect, and a mangled merge
  message. Each is in the ledger with its cause. **That is the standard, not an anomaly.**
