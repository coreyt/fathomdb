---
status: ACTIVE
---

# DOC-HYGIENE-2 — generated state + record governance (orchestrator hand-off)

> **Commissioned by the Program Steward under HITL directive 2026-07-25** — master **F-34**, steward ledger
> **seq-106**. **This effort runs FIRST — Slice 20 does not open until it lands.** It is a wide docs+tooling
> diff and **must not run while a release orchestrator is live** (master **F-7** collision rule). The 0.8.20
> ladder is between slices right now; that window is the reason this is scheduled here.
>
> **Label: NO pico label** — ruled by HITL (F-34), following the DOC-HYGIENE-1 precedent (**F-33**). This is
> cross-cutting hygiene, not a release increment; the two-tier numbering model is untouched. Land on `main`
> as normal docs/tooling commits.
> **No engine behavior change. No release-slot change. No change to 0.8.20 scope, requirements, or ACs.**

## 0. Base + guardrails (read before touching anything)

- **Base:** cut a **dedicated linked worktree** off a verified `origin/main` tip — **TC-RUBRIC-5**, enforced by
  `scripts/preflight.sh --landing` (hard-fails on the primary checkout). Never work in
  `/home/coreyt/projects/fathomdb`. Verify the base with `git rev-parse main`, not from narration
  (`agent-worktree-stale-base-trap`).
- **Verify the branch before every commit** (`git rev-parse --abbrev-ref HEAD`).
- **ARCHIVE IN PLACE — DO NOT RELOCATE OR RENAME ANY DOC.** `dev/plans/README.md` records the standing rule:
  ~120 prompt/run paths are cross-referenced from ~140 files. Every "archival" action here is a banner, a
  frontmatter field, or an index row — **never a `git mv`**. The master's path stays
  `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (~39 files reference it).
- **`dev/DOC-INDEX.md` must remain the entry path** — the X3 requirement and the Slice-40 **gate m** assert on
  that exact path.
- **`dev/plans/runs/codex/**` is OUT OF SCOPE and must not be pruned** (TC-RUBRIC-7 durable §9 transcripts).
- **TDD for anything executable.** Every gate/script tranche is RED first, then GREEN. Standing repo rule.
- **codex §9 gates every tranche that adds executable code** (T1b, T1c, T1d, T1e, T2a, T2b, T2c, T3a, T3b).
  Invoke **only** via `dev/agent-tools/codex-nostdin.sh` (bare `codex exec` deadlocks on stdin).
  **Reviewer substitution (HITL 2026-07-25):** if codex is rate-limited or out of budget, use the local
  `/code-review` path rather than stalling — **never an ultra-level/billed cloud review**. Whichever reviewer
  runs, **TC-RUBRIC-7 binds**: persist the terminal transcript under `dev/plans/runs/codex/DOC-HYGIENE-2/`.
- **⚠ `scripts/agent-lint-md.sh` used to exit 0 vacuously in a worktree (TC-37).** DOC-HYGIENE-1 T3 made it
  hard-fail, but still **read the real exit via `PIPESTATUS`** and never report a trailing `echo` as proof.
- **Full-workspace gate** for any tranche touching Rust: `cargo clippy --workspace --all-targets` **and**
  `cargo check --workspace --all-targets` both exit 0. (Expected to be a no-op here — this effort should touch
  no `src/` Rust.)
- **One commit per tranche, in order.** Do not batch.
- **Do NOT author master §6 findings.** The Steward reconciles the master at close. You may edit the master
  only where a tranche below explicitly says so.

## 1. Why this exists (measured, not asserted)

A multi-agent cold-start retrospective (2026-07-25) measured why stewardship keeps re-paying for the same
orientation. The root cause is **narrated state with a 5–12 file write-side fan-out** — verified: one
reconciliation commit (`b70629e5`) touched **7 files**, and nothing checks that the 7 agree. Symptoms, all
verified on `main`:

- The master is **129,631 B — 2.3× the readable cap**; **60.6 %** of it is an append-only findings log with
  **zero `###` headings between L315 and L691**, so locating a finding costs three round-trips.
- The live board is **77,647 B (~34.7k tok)** — the board of record **cannot be read whole**.
- A **duplicate `F-11`** sits at master L416 and L418, the **stale copy second**, still saying "*Remaining
  steward reconciliation (not yet done in this doc)*". Its rename was ruled 2026-07-03 (`c6d3449a`) and never
  executed. 18 in-file `F-11` pointers resolve ambiguously.
- `plan-0.8.20.md` carries **18 backticked line-anchors**; the TC-45 pair (`engine:14867`/`:14890`) is
  **~2,100 lines off** — the real call sites are `lib.rs:16963`/`:16986` — and git shows those anchors were
  authored *after* the merge they describe, so they were **never correct**.
- `dev/design/**` is **108 docs / 1.9 MB with only 23 carrying `status:`** — the largest unlinted surface in
  the repo, and the tier a Steward must cite to an orchestrator.
- `ledgerwatch`'s documented invocation (`dev/steward/README.md:39`) uses `--dry-run` with no `--state-dir`,
  and `ledgerwatch.py:537` skips `save_state` under `--dry-run` — **the documented invocation can never advance
  a cursor**, which is why a routine read spilled 110.5 KB.

## 2. Tranches (one commit each, in order)

### T0 — the verified drift fixes (prose only; no §9)

Three items, all verified by the Steward. **Enumerate before editing; do not trust these counts blind.**

1. **18 line-anchors in `plan-0.8.20.md`.** Find them with
   `grep -oE '`[a-zA-Z][a-zA-Z0-9_.-]*:[0-9]{3,}[^`]*`' dev/plans/plan-0.8.20.md`.
   For **each**, resolve the intended target and **replace the line number with a greppable symbol**
   (e.g. `record_projection_terminal` in `crates/fathomdb-engine/src/lib.rs`), not a corrected number — a
   corrected number rots on the next commit. Verify every replacement symbol actually exists
   (`grep -n '<symbol>' <file>`). The TC-45 pair is the known-wrong one and is load-bearing for Slice 20.
2. **The 4th `/goal` instruction site** at `dev/plans/runs/STEWARD-SESSION-HANDOFF-2026-07-24-A.md:15` —
   it instructs commissioning via `/goal complete 0.8.20`, contradicting standing ruling `927ffb35`.
   ⚠ **Fix ONLY that instruction.** `/goal` is a real Claude Code built-in and the ~20 *descriptive*
   references in closed-release plans are **NOT wrong** — a prior Steward proposed sweeping them and the HITL
   retracted it (`0.8.x-STEWARD-HANDOFF.md:247-254`). **Do not sweep.**
3. **Master §4 self-contradiction:** L238 routes deferred dependency migrations to **0.8.20** while the §4
   table row at L219 routes them to **0.8.22** *(was 0.8.20)*. **0.8.22 is correct** (F-19). Fix L238.

Also: verify-then-prune the two orphan checkout dirs `fathomdb-worktrees/verify-15b` and `verify-15b-fix2`
(present on disk, **not** in `git worktree list`). **Verify first** — confirm each has no unmerged commits and
no dirty/untracked files (`--untracked-files=all`) — then remove. If either holds unmerged work, **stop and
report**; do not delete.

### T1a — `ledgerwatch` invocation fix (prose only)

`dev/steward/README.md` prescribes `--dry-run` with no `--state-dir`, which can never advance a cursor.
Correct the prescribed invocation to advance by default (explicit `--state-dir` under `dev/steward/`), keep
`--dry-run` documented as the *peek* mode only. No tool code change required for this tranche.

### T1b — ledger integrity gate (TDD; §9)

`scripts/check-ledgers.sh`, dual-homed exactly like `check-board-currency.sh` (invoked from
`preflight.sh --landing` **and** a CI job — one script, two callers, so the predicate cannot diverge).
**Ship only two checks:** (a) each `.jsonl.seq` sidecar equals `max(seq)` in its file; (b) `seq` is
contiguous. Both pass today on all three ledgers, so include a **fixture-based RED** proving each check fails
on a corrupted copy. *Why it matters:* 19 consecutive commits (`f22e4947`→`3264114a`, 4 days) shipped the
steward ledger with `.seq` frozen at 80 against `max(seq)` 98, and `.seq` is tracked — any clone in that
window mints colliding seqs.
**Also** add a correct fold-to-latest `--project` mode to `ledgerwatch` with an explicit
`unfoldable (no id): N` bucket, and **delete the broken recipe** at
`dev/todos-and-considerations-ledger-readme.md:222-237` (it crashes with `KeyError: 'id'` and its `TERMINAL`
vocabulary matches none of the real statuses). **Do NOT** add a status vocabulary, a `--summary` length cap,
or an id requirement on the steward ledger — that file is a decision trail, and 105/105 entries have no id.

### T1c — findings register (TDD; §9)

In the master, **in place, no file split**:

1. Promote each `- **F-n — …` bullet in §6 to a `### F-n — <title>` heading.
2. Generate an index at §6's head (id · title · one line).
3. **`scripts/lint-findings.sh`** — hard-fails on a **duplicate finding id**. Scope it to the master; exclude
   `dev/archive/**` and `dev/plans/runs/codex/**`, which mint independent `F-0NN` namespaces.
4. **RED→GREEN witness:** the duplicate `F-11` (L416/L418) is your RED. Run the lint, show it fail, then
   **resolve the duplicate** per the disposition already ruled at
   `dev/design/fathomdb-memex-overall-roadmap/00-priorities-and-misalignments.md:210-216` (`c6d3449a`), then
   show it green. Keep every inbound `F-11` reference resolvable.

A file split is explicitly **out of scope** — after splitting, neither half is readable whole, and the split
was refuted in review. Anchors + index deliver ~90 % of the navigation win at a fraction of the risk.

### T1d — line-anchor ban (TDD; §9)

A lint that **fails on a bare backticked `<word>:<3+digits>` reference inside an ACTIVE plan** and requires a
greppable symbol instead. Use a **generic** rule, not an enumerated crate-prefix list —
`fathomdb-cli:389` already escapes any enumeration. **The symbol-existence check is MANDATORY**, not
advisory: a rule that swaps an unverified number for an unverified symbol reproduces this repo's named failure
mode. Wire it to an **always-on** CI job — the `markdownlint` job is `if: docs_only == 'true'` and would never
fire on the code push that renames the symbol. Scope to `status: ACTIVE` plans only; historical run artifacts
are immutable. **Depends on T0** — land T0 first or this arrives red.

### T1e — governed-surface allowlist pin gate (TDD; §9)

The HITL **pre-signed** the accumulated governed-surface delta (Slices 5d+10b+15b+15d) **pinned to the content
of `src/conformance/governed-surface-allowlist.json` as of `427d2712`** — 30 `allowlist` members,
`recovery_denylist` unchanged at the five REQ-054 names. Build the gate that makes that pin real: store the
pinned content hash, and **hard-fail when the file diverges**, with a message routing to the HITL for a fresh
sign-off. This is the mechanism that lets slices 20/25/30 proceed without a stop per slice — **Slice 20 is
expected to trip it**, and tripping is correct behaviour, not a bug.

### T2a — single-writer release state + generated views (TDD; §9)

**The load-bearing tranche.** One machine-readable state file per live release holding: landed slices + SHAs,
`SCHEMA` version, next slice, AC status, and ruled/unruled decisions. Then convert three hand-written regions
into **marker-delimited generated blocks**, with a **regenerate-and-diff gate** that fails when a block is
stale.

**Pre-signed by the HITL under exactly these four bounding conditions — they are acceptance criteria:**

1. Generated regions are marker-delimited and confined to **three named locations**: `STATUS-0.8.20.md` §1,
   the master §4 **0.8.20 row**, and the hand-off next-step. Nothing else.
2. The generator **must reproduce today's verified-true content** before it may own a region. If
   generate-and-diff cannot reproduce it, **it does not land**.
3. **No content is deleted** — only already-existing regions are placed under generation.
4. **Fully reversible** by removing the markers.

If N1 turns out to need *restructuring* rather than regenerating in place, **STOP and escalate to the
Steward** — that re-opens the HITL pre-sign.

### T2b — ledger as ruling registry (TDD; §9)

`ledgerwatch --project rulings` emits the live ruled/unruled table; `STATUS-0.8.20.md` §4 becomes a
**generated block** over it. Prose homes cite `seq-N` rather than restating a ruling. Depends on T1b.
*Why:* rulings currently live in ≥4 homes with no index, and board §4 listed ≥4 already-ruled items as open —
the direct generator of re-decided settled calls.

### T2c — `dev/design/**` frontmatter governance (TDD; §9)

Require `status:` (and `superseded_by:` where applicable) on `dev/design/**/*.md`, gated in the same shape as
`scripts/lint-plans-status.sh` (which is scoped `dev/plans/*.md` **top-level only** — verify before assuming
coverage). Backfill the 85 uncovered docs.

**Backfill rule (HITL 2026-07-25, todos `TC-50`): default to `status: UNREVIEWED` for anything not
confidently classifiable.** Only docs with clear evidence — an existing supersession banner, a closed-release
tie — get `ACTIVE` or `SUPERSEDED`. **A doc wrongly marked `ACTIVE` is worse than one with no marker.** The
gate proves *presence* of a status field, never its *truth*; a dedicated post-0.8.20 slice is already owed for
the real classification (TC-50). Do not attempt it here.

### T3a — `scripts/steward-orient.sh` (TDD; §9)

A **stateless** cold-start briefing that prints **≤4 KB and writes no file**: branch/HEAD/status/worktrees
**plus orphan checkout dirs**, the live board's next-action verbatim, landed slices with SHAs, `SCHEMA`,
the last 5 ledger entries, the todos ledger **folded to latest-per-id**, open PR count, and the path of the
newest `STEWARD-SESSION-HANDOFF-*`. **Read state from T2a's state file — do not re-scrape.**

Four corrections are load-bearing: **derive the release number from the live board filename** (a hardcoded
`0.8.20` grep silently prints "nothing landed" the day 0.8.20 closes — this repo's TC-37 vacuous-pass class);
`fathomdb-worktrees/` is a **sibling** of the repo root, not a child; **hard-fail on any zero-result section**;
and share the board-CLOSED detector with `check-board-currency.sh:102-104` (which uses `head -n 15`, not 5).
Ledger reads route through `ledgerwatch`. It should **annotate** `0.8.x-STEWARD-HANDOFF.md` §3, not delete
items from it.

Also: **delete the dangling §3 item-5 pointer** to `0.8.x-SEQUENCING-WATCH-HANDOFF.md` (§1 and §58-59) — that
file has **never been committed** (`git log --all` is empty for it). Do not restore it; its report format is
already at `STEWARD-HANDOFF.md:262-272` and its live rows are inlined at `:72-75` and master §2a.

### T3b — generated commission manifest (TDD; §9)

A script that, given a release + slice, emits the citation list an orchestrator brief needs: design-of-record
paths, contract paths, plan section anchors, base SHA, worktree rules, and the stop conditions. Inputs are
T2a's state file and T2c's frontmatter. **This replaces a hand-maintained brief template** (refuted in review:
the shape already exists in `LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md` and `0.8.0-SLICE-TEMPLATE.md`). It will be
used to brief Slices 20/25/30/40 — validate it by generating the **Slice 20** manifest and checking every
emitted path exists.

## 3. Definition of done

- All eleven tranches landed on `main`, one commit each, in order.
- Every executable tranche: **RED→GREEN witness** + **codex §9 terminal-clean** (or the substituted reviewer),
  transcript persisted under `dev/plans/runs/codex/DOC-HYGIENE-2/`.
- Gates green on the landing checkout, verified via `PIPESTATUS`: `agent-lint-md.sh`,
  `check-board-currency.sh`, `lint-plans-status.sh`, plus every new gate this effort adds.
- T2a's generated blocks reproduce the verified-true state (condition 2 above).
- T3b generates a Slice-20 manifest whose every path resolves.
- **No pico label. No release-slot change. No 0.8.20 scope/requirement/AC change.**

## 4. Where you STOP — these never travel down to you

- **Any HITL gate.** Specifically: **publish**, the **batched governed-surface decision**, and any change to
  program direction, a release slot, or the schedule of record.
- **T2a restructuring** beyond regenerate-in-place (condition 4 above) — escalate to the Steward.
- **Reserved-gap band overflow** — HALT rather than spilling scope.
- **Circuit-breaker (STANDING RULE, HITL 2026-07-25):** escalate after **3 fix-N rounds on the same finding,
  or 6 rounds total on a tranche**. Escalate **to the Steward**, who decides re-commission / re-scope /
  escalate to HITL. Never escalate past the Steward.
- **If T0's orphan-dir check finds unmerged work** — stop and report.
- **Anything requiring a push to a repo other than `fathomdb`** — push scope is fathomdb-only, absolutely.

## 5. Anti-stall (read this — it is not boilerplate)

A previously commissioned background orchestrator **stalled for 36 hours with no notification** and the
auto-resume never fired. Therefore:

- **Never wait idle on anything.** If a subagent, a review, or a build has produced nothing new, re-derive
  state from git and act; do not block.
- **Land each tranche as it completes.** Do not accumulate a large uncommitted diff — partial progress that is
  committed survives your death; partial progress in a worktree does not.
- **If you become blocked, say so and hand back** with the tranche list marked landed/not-landed. A partial
  effort honestly reported is a valid result; a silent stall is not.

## 6. Handing back

Report to the Steward: tranches landed (with SHAs), tranches not landed and why, every gate's real exit code,
the §9 verdict per executable tranche, and anything you escalated. **The Steward verifies from git and
reconciles the master** — do not write master §6 findings yourself.
