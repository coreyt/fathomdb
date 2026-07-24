# DOC-HYGIENE-1 — cross-cutting docs + tooling hygiene effort (orchestrator hand-off)

> **Commissioned by the Program Steward under HITL directive 2026-07-24** (todos ledger **TC-48**, seq 72).
> **This effort runs FIRST — before the 0.8.20 Slice-20 orchestrator is commissioned.** It is a wide docs diff
> and **must not run while a release orchestrator is live** (master **F-7** collision rule). The ladder is
> between slices right now; that window is the reason this is scheduled here and not later.
>
> **Label:** pico label is **HITL-pending** and is assigned **at close**, not now (picos are work-completion
> increments — two-tier model, F-13). Land on `main` as normal docs/tooling commits meanwhile.
> **No engine code. No behavior change. No release-slot change.**

## 0. Base + guardrails (read before touching anything)

- **Base:** cut a **dedicated linked worktree** off a verified `origin/main` tip — **TC-RUBRIC-5**,
  enforced by `scripts/preflight.sh --landing` (hard-fails on the primary checkout). Never work in
  `/home/coreyt/projects/fathomdb`.
- **Verify the branch before every commit** (`git rev-parse --abbrev-ref HEAD`).
- **ARCHIVE IN PLACE — DO NOT RELOCATE OR RENAME ANY DOC.** `dev/plans/README.md` records the standing rule:
  ~120 prompt/run paths are cross-referenced from ~140 files, so moving or renaming a completed artifact breaks
  references or forces rewrites of immutable historical run logs. **Every "archival" action in this effort is a
  banner, a frontmatter field, or an index row — never a `git mv`.**
- **`dev/DOC-INDEX.md` must remain the entry path.** The X3 cross-cutting requirement and the Slice-40 **gate m**
  assert on that exact path. Split its *contents*, keep the file.
- **`dev/plans/runs/codex/**` is OUT OF SCOPE and must not be pruned** — TC-RUBRIC-7 mandates durable §9
  transcripts (39 files / ~15 MB, `.log`, already outside markdownlint scope).
- **All 0.8.20 artifacts are OUT OF SCOPE** — the release is in flight and its run artifacts are live evidence.
- **TDD for anything executable** (tranche T3/T4 scripts + CI): RED first, then GREEN. Standing repo rule.
- **codex §9 gates T3 and T4** (they add executable gates). T1/T2 are prose-only and do not need §9; run
  `./scripts/agent-lint-md.sh` on them instead. **⚠ TC-37: that script exits 0 *vacuously* inside a worktree** —
  do not report its exit code as proof; verify the lint on the landing checkout or by invoking
  `markdownlint-cli2` directly on the changed files.
- **One commit per tranche**, in order. Nothing here is release work — do not touch `plan-0.8.20.md` scope,
  `STATUS-0.8.20.md` slice rows, `dev/acceptance.md`, or any `src/` file.

## 1. Why this exists

Two measured problems, one root cause (the docs tree grew faster than its map):

1. **Context cost.** `dev/DOC-INDEX.md` — the doc every agent is told to read at cold start — is **83 KB
   (≈21k tokens)**: 205 rows whose "purpose" cells have grown into paragraph-length slice histories. Three
   closed-line artifacts (`STATUS-0.8.0.md` 190 KB, `0.8.0-implementation.md` 125 KB, `STATUS-0.8.1.md` 88 KB)
   sit unlabelled in the live search path, and `dev/plans/prompts/` holds **215 files / 2.7 MB**, ~96 % of it
   one-shot slice prompts from ≤ 0.8.4.
2. **Actively misleading docs**, which cost more than volume: a master whose title says `0.8.6-0.8.16` while the
   line runs to **0.8.24**, pre-F-19 sequencing prose still live in the master's §5/§9, and a to-do doc
   ("as of 2026-06-30") superseded by the todos ledger.

## 2. Tranches (one commit each, in order)

### T1 — record hygiene (prose only)

0. **CLOSE THE RED MARKDOWN GATE — do this first.** The gate has been **red on `main` since `c366d6e0`
   (2026-07-02)**: **9 errors**, all in `dev/research/personal-agent-database-market-2026-07-02.md`
   (1 × MD001 heading-increment at `:181`, 8 × MD025 multiple-h1 at `:403/:407/:475/:479/:570` and neighbours —
   the appendix headings are `#` where they should be `##`). **Verified by the Steward 2026-07-24 with the
   pinned `./node_modules/.bin/markdownlint-cli2` from a checkout that has `node_modules`.** This is a
   regression against the 0.8.9.1 "markdown debt → 0" achievement, and it went unseen for three weeks **because
   of TC-37** — `scripts/agent-lint-md.sh` *skips* (exit 0) when the binary is absent, which is the normal state
   inside an orchestration worktree, so every orchestrator has been reading a vacuous green. Fix the headings
   (demote the appendix `#` to `##`); do not silence the rule.

1. **Retitle the master IN PLACE** — `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`: the H1 and the intro box
   must state the true span (**0.8.6 → 0.8.24**, per F-19/F-20). **Do not rename the file** — 39 files reference
   the path. Add a one-line note that the filename is historical and the path is stable by design.
2. **Sweep the master's §5 and §9 pre-F-19 prose** — F-20 flagged this sweep as pending and it was never done.
   §5/§9 still narrate `OPP-12 @ 0.9.x` · `0.8.19 = free-threading` · `0.8.18 = end-of-line`. Either correct the
   prose to the F-19/F-20/F-32 allocation or banner each stale block as historical. **§6 findings F-1…F-31 are
   the historical decision record and must stay verbatim** — do not rewrite them.
3. **`dev/plans/runs/0.8.x-remaining-todos.md`** — header banner: **SUPERSEDED**, pointer to
   `dev/todos-and-considerations-ledger.jsonl` (read via `ledgerwatch`) as the live to-do surface.
4. **Delete `dev/plans/runs/0.8.x-renumber-memex-handoff.md`** — untracked, never landed on `origin/main`, and it
   tells Memex its sync window is FathomDB **0.8.15**, which **F-18 PARKED**. It exists only in the primary
   checkout's working tree. Confirm `git log --all -- <path>` is empty before removing.

### T2 — context cost (prose + one structural split)

5. **Split `dev/DOC-INDEX.md`.** Keep `dev/DOC-INDEX.md` as a **thin map** — one row per doc, `path → ≤120-char
   purpose → owner → last-touched` — targeting **≤ 12 KB**. Move the accumulated slice-history prose into
   per-area detail files (e.g. `dev/doc-index/<area>.md`) linked from the map, or back onto the owning release
   board. **No doc may lose its row.** State the ≤120-char rule in the file so the next slice keeps it.
6. **Status banners on the closed-line giants** — `STATUS-0.8.0.md`, `STATUS-0.8.1.md`,
   `0.8.0-implementation.md`, and the other `≤ 0.8.19` boards/implementation docs: a **one-line header banner**
   (`CLOSED — historical record, archived in place; current state: <pointer>`) so a grep hit is self-labelling.
7. **Add `dev/plans/prompts/README.md`** (currently missing) stating the archived-in-place convention, that
   ≥ 96 % of the directory is closed-line one-shot prompts, and where the live prompts are
   (`0.8.x-STEWARD-HANDOFF.md`, `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`, the LBS templates, this file).

### T3 — recurrence guard (TDD; codex §9)

8. **Universal `status:` frontmatter** on `dev/plans/*.md` (`ACTIVE | COMPLETE | PROPOSED | SUPERSEDED`) —
   `plan-0.8.19.md` and `plan-0.8.21.md` already model the shape. Machine-readable so an agent can filter
   without reading.
9. **A lint that enforces #8**, wired into `scripts/agent-lint-md.sh` (or a sibling invoked by it) and CI —
   *fix the tooling, not the people*. RED-first.
9b. **FIX TC-37 — the vacuous green that hid T1/0 for three weeks.** `scripts/agent-lint-md.sh` currently
    `skip_notice`s (and exits 0) when `node_modules/.bin/markdownlint-cli2` is missing — the normal state in an
    orchestration worktree. Make a missing linter a **hard failure**, or have the script resolve the pinned
    binary from the primary checkout / bootstrap it. A gate that cannot run must say so loudly, never report
    green. RED-first; this is the load-bearing half of this tranche.
10. **Prune transient run artifacts for CLOSED releases only** — `dev/plans/runs/*-output.json` for **≤ 0.8.19**
    (102 files / ~880 KB total across all releases; recoverable from git history — the same precedent as the
    prior pass that removed ~507 artifacts). Use `scripts/repo-prune/`. **Excluded: everything `0.8.20-*`, all
    `codex/**`, every `STATUS-*.md`.** Record what was pruned in `dev/archive/README.md`'s manifest section.

### T4 — status-board currency (HITL-directed 2026-07-24; TDD; codex §9)

Implement **items 1–3** of `dev/design/status-board-currency-enforcement.md` (todos ledger seq 70) — folded in
here because it is the same class of work (docs + `preflight.sh` + one CI job, no engine code), which also keeps
the Slice-20 orchestrator a pure engine slice:

11. **Seam-ownership contract line** — the **Steward** owns the board's **LANDED** row + next-slice pointer,
    updated **in the landing merge**: edit `.claude/agents/steward.md` and `dev/design/orchestration.md` §12.5.
12. **Board-currency gate in `scripts/preflight.sh --landing`** — refuse a land that would leave
    `STATUS-0.8.z.md` stale. **RED-first test.** (Placement rationale, already settled: *not* pre-commit — the
    board legitimately reads "in flight" mid-build, so pre-commit yields false positives.)
13. **CI drift detector on `main`** — red when the board disagrees with git ancestry; the non-bypassable
    backstop for whatever slips preflight or predates it.

**Item 4 of that design (machine-derived LANDED table) is explicitly LATER** — only if drift persists despite
1–3. Do not build it.

## 3. Definition of done

- T1–T4 landed on `main` as four clean commits, each verified from git.
- `dev/DOC-INDEX.md` ≤ 12 KB, still the entry path, **no doc lost its row**.
- Markdown gate green **verified outside the vacuous-worktree path** (TC-37).
- T3/T4 gates demonstrated RED → GREEN, with the codex §9 transcript persisted under
  `dev/plans/runs/codex/doc-hygiene-1/<step>-<UTC>.log` (TC-RUBRIC-7 shape).
- **Zero `git mv`**, zero `src/` changes, zero release-scope changes.
- A closure note handed back to the Steward: what landed, what was pruned, and anything found but not fixed.

## 4. Handing back

Report to the Steward, not to the HITL. The Steward verifies from git, reconciles into the master, assigns the
pico label at close (HITL call), and only then commissions the **0.8.20 Slice-20** orchestrator
(`dense_readiness` + `flush_embeddings()` **+ the TC-45 supersession-terminal fix**).
