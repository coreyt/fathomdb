# NOTE to the 0.8.x program Steward — Library Sweep program + Dependabot triage (2026-06-29)

> **Purpose:** so you are not caught off-guard. A liaison/cleanup session (not the program Steward)
> triaged the Dependabot backlog, established a new Steward archetype, and made bounded edits to the
> master sequencing doc. This note tells you what changed, what is now owned by whom, and what (if
> anything) you need to do.

## TL;DR

- A **new Steward archetype** exists: the **Library Bump Steward (LBS)**, owning dependency hygiene
  as a recurring, owned program (charter + LBO template under `dev/plans/prompts/`). It does **not**
  touch your release roadmap; it runs **transitory OOB micros between even releases**.
- The 21 open Dependabot PRs were triaged to **4 merged, 8 closed, 9 open**. The 9 open are **not
  urgent** (no security driver — see below) and are **split** into a 0.8.11.1 sweep micro (contained)
  and a net-new **0.8.20** (major migrations, timing-gated).
- The master sequencing doc now carries **F-12** + a **0.8.20** row + a Library-Sweep note on the
  0.8.11 row. These were reconciled in by this session; flag them in your next pass if you want to
  re-word, but the disposition is HITL-approved.

## What landed on `main` already (Dependabot)

- **Merged (4):** #49 setup-node, #50 upload-artifact, #100 actions/cache, #96 prettier — all
  low-risk, CI-green, squash-merged with HITL approval.
- **Closed as orphans (7):** #46/#61/#62/#65 (`/typescript`), #64 (`/tests/cross-language/typescript`),
  #63 (`python/uv.lock`), #44 (untracked `go/` module) — all edit manifests not tracked on `main`;
  the current `.github/dependabot.yml` no longer references those paths, so they will not regenerate.
- **Closed as already-satisfied (1):** #59 `rand` — `rand 0.9.4` is already in `Cargo.lock`
  (transitive-only).

## What is still open (9) and where it is routed

- **Library Sweep #1 — transitory 0.8.11.1 micro (contained):** `sha2` 0.10→0.11, `typescript` 5→6 +
  `@types/node` 25→26, `actions/checkout` 6→7, `action-gh-release` 2→3 (release dry-run), plus a
  `dependabot.yml` reconciliation. Label-only (no version bump / `v*` tag / publish), 0.8.9.1-shape.
- **0.8.20 (net-new, owned engine slices):** `napi` 2→3 (napi + napi-derive + @napi-rs/cli) and
  `rusqlite` 0.31→0.40 + `sqlite-vec` (coupled). **Deferred-with-trigger.** **Its timing is NOT
  confirmed — it must be strongly reviewed for timing-correctness before proceeding** (do not force a
  heavy migration adjacent to an already-heavy even release; F-10 self-completion).

## Security note (so you do not over-react to the alert count)

GitHub shows 2 open Dependabot alerts (`idna` medium, `torch` low). Both are against the **gitignored
local eval environment** `python/uv.lock` — NOT shipped code. The shipped package `src/python` has
`dependencies = []`. So the alerts are noise for the product; they do not gate anything. (HITL prefers
resolving over dismissing, so they are tracked via the orphan-close of #63 and the eval-env's own
upkeep, not a Security-tab dismissal.)

## What this session edited in the master (`dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`)

- **F-12** added (new finding) — records this whole disposition.
- **§4 allocation table:** appended a Library-Sweep scope note to the **0.8.11** row; added a new
  **0.8.20** row (with the timing-review caveat).
- **§4 closing summary:** one sentence noting the Library Sweep program + 0.8.20.

## What you (program Steward) should and should not do

- **Do not re-triage the Dependabot backlog** — it is owned by the LBS now.
- **Do not schedule the 0.8.20 migrations into an even release** — they are deferred-with-trigger and
  timing-gated; let the LBS run them as owned slices when a trigger fires.
- **Do** re-review the F-12 / 0.8.20 wording in your next reconciliation pass if you want it tighter;
  the disposition itself is HITL-approved (2026-06-29) and should not be reversed without HITL.
- **Do** treat the LBS as a peer Steward: coordinate via the program Steward ↔ LBS boundary; LBOs talk
  to the LBS (via SendMessage), not to you directly.

## Pointers

- Charter: `dev/plans/prompts/LIBRARY-BUMP-STEWARD.md`
- LBO prompt template: `dev/plans/prompts/LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md`
- Master disposition: `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (F-12, §4 0.8.11/0.8.20 rows)
