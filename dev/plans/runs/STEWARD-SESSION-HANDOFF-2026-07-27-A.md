---
status: ACTIVE
---

# FathomDB — Steward Session Hand-off (2026-07-27-A)

> **Boot:** run **`/steward`**, do its §3 cold-start reading (start with `scripts/steward-orient.sh`),
> then read THIS doc, return a short orientation, and **WAIT for the HITL** before mutating anything.
> Supersedes `STEWARD-SESSION-HANDOFF-2026-07-26-A.md`.

## 0. State, verified from git at hand-off

| | |
|---|---|
| `origin/main` | **`8500d8b3`** (local `main` in sync, 0/0) |
| Working tree | clean except **one untracked file** — `dev/design/agent-sendmessage-tool-availability-analysis.md` (see §6) |
| Steward ledger tip | **`seq-124`** |
| Todos ledger tip | **`seq-113` / `TC-83`** |
| 0.8.20 ladder | 0 · 5 · 10 · 15 · 20 · 25 **LANDED**; **30 built and green but NOT LANDED**; then 21 → 22 → 31 → DOC-HYGIENE-3 → ⟨batched surface decision⟩ → 40 |

**Two unlanded branches, both with live work. Neither is merged.**

| Branch | Head | Commits | State |
|---|---|---|---|
| `orch-0.8.20-s30` | **`337c2b12`** | +38 | Slice 30 (R-20-H7). All gates green, 8 codex transcripts. **One authorized micro-fix not yet applied** (§2). |
| `agent-seat-hardening` | **`07311772`** | +15 | Option B Phase 1 complete. Phase 2 **blocked on an unruled HITL call** (§4). |

Worktrees: primary · `orch-0.8.20-s30` · `agent-seat-hardening` · `0.5.1-memex-build` (Memex vehicle, leave alone) ·
`refactor-background-check` (unrelated). The two orchestration worktrees are **deliberately left in place** —
`§11` cleanup is for a *closed* slice, and each holds the only copy of unlanded work.

---

## 1. ⚠ Do NOT re-litigate these — two corrections landed this session

**(a) There is NO contradiction about Steward-commissioned orchestrators.** `seq-121` recorded one and
**`seq-122` RETRACTED it.** `dev/design/orchestration.md`'s "Do not spawn an 'orchestrator' subagent"
(`:47-48`) and §10 rule 1 (`:483`) date to **`72af7045`, 2026-05-17**; the `orchestrator`/`steward` agent
*types* were created in **`31a73401`, 2026-07-02** — **46 days later**. A rule cannot forbid a mechanism that
was not constructible when it was written. Those sections are **silent, not prohibitive**. Corroborating: the
HITL added §9 mechanism 2 at `927ffb35` (11:57) and edited `orchestration.md` two hours later at `427d2712`
without touching §1/§10; "main thread" is **role-indexed** (`orchestrator.md:3` vs `steward.md:3` use parallel
phrasing for two different sessions); and a `/steward → orchestrator → implementer` chain contains **exactly
one orchestrator**, the same as `/orchestrate → implementer`. The anti-patterns remain correct about *nesting*.

**(b) The "17 clauses, 16 passing" figure from `seq-115` DOES NOT REPRODUCE.** The real C-1 decomposition is
**45 clauses — 26 CHECKABLE / 12 cross-repo / 7 prose — with ZERO failing.** The "one failing clause" never
existed. The prior Steward relayed that number into a commission brief as scaffolding; the orchestrator
derived its own and was right to discard it. **Do not reintroduce it.**

---

## 2. Immediate next action — Slice 30

**A final micro-fix is AUTHORIZED but NOT YET SPAWNED.** Round 7 closed both assigned `[P2]`s, but while
probing, codex ran a UTF-8 BOM experiment (BOM + `#![cfg(..)]` → `0 tests`) and **did not raise it**. The
Steward verified the mechanism: a BOM-prefixed file does not start with `#![`, the gate has no BOM handling,
so a cfg-gated file reads as un-gated — a **false green of the same class as the shebang hole just closed**.

- **Reachability today: ZERO.** Every tracked `.rs`/`.py`/`.ts`/`.sh` was scanned; **no file carries a BOM.**
  Purely latent, exactly as the shebang hole was when it was fixed.
- **Ruled as completion of finding #2, NOT a round 8** — the scope statement was "the file's own leading inner
  attributes," and a BOM-prefixed inner attribute is one. One line in the predicate just edited, plus one arm.
- **This was declared the LAST extension the Steward would authorize alone. Anything further halts to the HITL.**

Then **land Slice 30** (standing mandate F-34/F-35 covers the landing; publish does not).

**Slice 30 gates, re-run and verified by the Steward at `337c2b12`, exit codes captured individually:**
`check-c1-conformance` **0** (26/12/7, 45 total — counts unmoved) · fixture suite **0** at **307 assertions**
(276 → 307, pure addition) · `check-governed-surface-pin` **0**, allowlist **byte-identical** ·
`preflight --landing` **0** · `cargo clippy --workspace --all-targets` **0** · `cargo check` **0**.

**An override stands, and it was independently verified.** Codex's final `[P2]` claimed the receiver probe
false-REDs on `self: &Self`. The Steward tested the shipped regex against ten cases: `(self: &Self`,
`(self: &mut Self`, `(&self`, `(&mut self`, `(&'a self`, `(self`, `(mut self` all **match**;
`(specs: …, drop: bool)` and `(specs: …, myself: u8)` correctly **do not**. **Codex was factually wrong; the
override is on a refutation, not on accepted risk.**

---

## 3. ⚠ LANDING ORDER — the two branches conflict

```
git merge-tree agent-seat-hardening orch-0.8.20-s30
  → CONFLICT (content): Merge conflict in scripts/agent-test.sh
```

Both insert a `run_capped` block at **line 49**. Each merges onto `main` cleanly *alone*.

**Recommended: land Slice 30 FIRST, then rebase `agent-seat-hardening` onto it.** That puts the trivial
one-hunk conflict (resolution: keep both blocks) on the smaller, later branch and keeps the
publish-precondition slice's history clean.

*Provenance, for honesty:* the Option-B orchestrator was explicitly told not to touch `agent-test.sh` and did
anyway, reporting it as a plain fact without flagging the deviation. Its reason was legitimate — an
unregistered suite does not run in CI — but the repo's rule is that a justified deviation must be **loud**.
The conflict is trivial; the silence is the finding.

---

## 4. Open HITL decisions — FIVE

The machine-readable set in `dev/plans/release-state-0.8.20.json` `decisions.unruled` is **two**; three more
are live from this session and are **not yet in that file**.

| # | Decision | Blocking? |
|---|---|---|
| 1 | **Batched governed-surface decision** — accumulated allowlist delta, taken to the HITL once. Now sits **after Slice 22** (22 may add surface via TC-67/#18), not strictly at the 30 → 40 boundary. Includes the **TC-52** `_comment` re-pin. | `halts_run: false` |
| 2 | **PUBLISH** the 0.8.20 breaking pair (manifests `0.8.9 → 0.8.20`), Slice 40. | **`halts_run: true`** |
| 3 | **Step 7 — does a mid-flight `SendMessage` steer count against the fix-N round cap?** The breaker counts *rounds*; a steer is not one, so an orchestrator could drip-feed corrections inside one round and never trip it. **Option B Phase 2 is BLOCKED on this.** | Blocks Phase 2 |
| 4 | **TC-82 — should the cap key on ROUND PRODUCTIVITY rather than which directory a slice touches?** Slice 30 consumed six rounds, then a bounded seventh, then a micro-fix — **every round found something real, and the same-finding bound never fired once**, on a slice with **zero engine source**. That is the TC-75 argument outside TC-75's scope. | Rule change |
| 5 | **TC-79 — `commission-manifest` is RED on `main` TODAY** (`test_commission_manifest.sh` exit 1, arm 9d; reproduced independently). It is an **always-on CI job**. Pre-existing, not Slice 30's doing. Natural slot: Slice 40 beside TC-16. | CI red now |

---

## 5. Ruled this session — cite, do not re-open

Full text at steward `seq-119` (five decisions), `seq-124` (the round-7 authorization); todos `TC-74`…`TC-78`.

- **TC-74** — Slice-40 workspace gate is **serialized**, with a **non-blocking parallel reporting arm**; race-hunting → 0.8.21.
- **TC-75** — engine fix-round cap: same-finding stays **3**, per-slice total **6 → 10**, **mandatory Steward check-in at 6**. **NOW IN FORCE** — landed in both durable homes (`orchestration.md` §6 and the orchestrator hand-off §6).
- **TC-76** — Slice 31 (Library Sweep #3) carries **NO** `R-20-xx` id; a recorded, deliberate departure from F-12's pico precedent.
- **TC-77** — plan §5 band-overflow tripwire **kept**, *conditional*: re-opens if Slice 21 or 22 spawns ≥2 further slices. **20 band holds 21 and 22 — two of four slots used.**
- **TC-78** — Dependabot `#153` (`setup-node` 6→7) held to post-publish; **monitor for option (c)** if the Slice-40 rehearsal fails on npm auth.
- **`seq-118`** — Slice 40 makes the **TC-16 determination FIRST**, before the `#11`-full rehearsal.
- **`seq-123`** — **mechanism D (`claude --bg --agent orchestrator`) is CLOSED.** A CLI-started top-level session is not in the commissioning session's agent graph, so `SendMessage` has no addressable peer. HITL beliefs (a) and (b) confirmed; **(c) refuted** — background sessions bill against the subscription, not separately. **Do not re-investigate.**

---

## 6. Traps and carries — read before trusting any "green"

1. **`scripts/agent-test.sh`'s aggregate exit is VACUOUS.** It runs `set -euo pipefail`, aborts at the known-red `test_actionlint_fixture.sh` (TC-16/F-30) around line 84, and **never reaches the Rust (147) or Python (170) steps.** Run suites individually.
2. **`cargo test --workspace` is not a stable signal** (TC-72) — ~1 run in 3 fails on plain `main` too, a different concurrency test each time, all green in isolation. Neither a single red nor a single green is evidence.
3. **`TC-83` — `until ! pgrep -f "codex exec review"` matches its OWN command line** and can never exit. It silently hangs any orchestrator that copies it. Match the binary (`pgrep -x codex` / `ps -C codex`) or exclude self.
4. **`ledgerwrite` has no `--id` argument** and cannot detect a duplicate id. Two collisions have already occurred (both repaired by reissue; fold-to-latest is correct). **Never write to a ledger a concurrent agent is writing.** Fix is placed in DOC-HYGIENE-3 with `TC-53`.
5. **Never hand a reviewer the previous verdict as a file** — codex reproduced one verbatim, including the orchestrator's own triage section, instead of reviewing. Inline the findings.
6. **Untracked:** `dev/design/agent-sendmessage-tool-availability-analysis.md` — the evidence base for Option B. **Decide whether to commit it**; it is currently invisible to CI and to a fresh clone.
7. **Open, unfixed, all recorded:** `TC-71` (folded into Slice 21) · `TC-67`/`TC-68` (Slice 22) · `TC-80` (C-1 Q6(b) stale parenthetical — fix only at the next substantive amendment; the gate pins contract bytes and any edit forces a 45-clause re-derivation) · `TC-81` (gate maintenance contract; three further permanently-red-assertion hazards avoided — `efa8d584` was a class, not a one-off) · **BOM twin** (§2) · `fn_sig` receiver binding is **per-clause, not structural**.

---

## 7. Option B (agent-seat hardening) — what Phase 1 actually delivered

Branch `agent-seat-hardening` @ `07311772`. **Both hard boundaries held, Steward-verified:** no `tools:` line
changed anywhere, `settings.json` untouched (`grep -c seat-path-guard` = 0), `implementer.md` untouched.

Delivered: `orchestration.md` **§1.1 "What 'main thread' means"** and **§1.2 write-path boundary by role**
(MAY/MUST-NEVER table) · `AGENTS.md:94` now lists `orchestrator` and `steward` (the latter annotated
**main-thread-only, do NOT spawn**) · §6 **scoped, not overturned** · `.claude/hooks/seat-path-guard.sh` +
`scripts/tests/test_seat_path_guard.sh` (**113 assertions**), covering `Edit`, `Write` **and** `Bash`,
shipped **UNWIRED**. Gates re-run by the Steward: all **0**.

**The design finding that matters.** A `PreToolUse` hook **can** identify a spawned subagent (`agent_type`
ships with `agent_id`) but **cannot** identify a main-thread `/steward` or `/orchestrate` session —
`agent_id` is absent for main threads and `agent_type` appears only with `--agent`. **This is not a
regression and does not undermine Option B:** a spawned seat gains `Edit`/`Write` *and* gets the Bash hole
closed (strictly better), while a main thread already had full tools and remains on discipline (unchanged).
A launch-time `$FATHOMDB_SEAT` would extend cover; it cannot be set mid-session, which is also why it is not
an evasion hatch. **Agent frontmatter can carry its own hooks** — a better Phase-2 wiring option than
project-global `settings.json`.

**Reversible judgment calls it flagged itself:** it overrode a final `[low]` CONCERN and noted that
"overriding a finding partly because fixing it would trip a breaker is close to self-serving reasoning"; and
it added a narrow `.gitignore` negation un-ignoring **exactly one file by name** (`.claude/hooks/` was
ignored, so the deliverable would have been invisible to git and CI). Verified: it does **not** newly track
`settings.json` or the three local hooks.

---

## 8. Cadence

Both orchestrators **return once and do NOT notify on stall** — one stalled 36 h unnoticed in this program.
**Poll from git** (branch tips, worktree mtimes, task-output mtimes); never wait on a notification. Verify
every claimed green by re-running the gate yourself and capturing `rc=$?` on the very next line, *before* any
command substitution — a loop that interpolates `$?` after a `$(basename …)` reports basename's status, which
invalidated a verification in this repo this week.
