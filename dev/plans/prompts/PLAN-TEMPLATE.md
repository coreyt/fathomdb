<!--
========================================================================
PLAN AUTHORING TEMPLATE  (this comment block is harmless if left in)
Copy to dev/plans/plan-<release>.md and replace every {{PLACEHOLDER}}.
This is the *deliverable* side of the system; the *method* is the
release-independent runbook dev/design/orchestration.md. A plan written
from this template is meant to be driven by `/goal complete <release>`
as an orchestrator session (prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md).

Required fills:
  {{RELEASE}}      e.g. 0.8.6
  {{THEME}}        one line: what this release is FOR
  {{GOAL}}         2-5 bullets: the deliverables this release lands
  {{REQS}}         the R-* requirement rows + their acceptance signals
  {{LADDER}}       the mod-5 slice rows (number, title, work-type, depends-on)
  {{OVERRIDE_REG}} verified file:line sources that diverge (or "none found")
  {{BEHAVIOR_REG}} every silent behavior change this release ships
  {{PREREQS}}      what must be true before Slice 0 opens
  {{DECISIONS}}    decisions already taken (with date + rationale)
  {{OPEN_Q}}       questions that need HITL before/at a gate
  {{NEXT}}         the immediate next slice pointer

PLAN AUTHORING CHECKLIST — a plan is GREAT when each item below is true.
(The slice-authoring checklist defeats under-spec at the slice level;
this one defeats it at the plan level — the gaps an orchestrator only
discovers mid-release.)
  [ ] Every requirement has a FALSIFIABLE, OFFLINE acceptance signal — a
      command/test/witness, not "works". (§ 2). No R-* without one.
  [ ] Every slice traces to a requirement (R-* / G-gap / OPP-id) AND a
      DoD; no orphan slices, no requirement without a slice (§ 3, § 5).
  [ ] Keystones & hard gates named: which slice gates which later slice
      or release, and which transitions are HITL-gated (§ 3).
  [ ] Parallelizable tracks called out with their shared-file conflicts,
      so worktrees don't collide (§ 3).
  [ ] The override/duplication register is filled from a real grep — every
      divergent source of a contract is listed at file:line, or the row
      says "none found" after looking (§ 8). Don't leave it blank-by-default.
  [ ] The behavior-change register lists EVERY silent change (default
      flip, schema bump, error-shape change) → each becomes a changelog
      line. "No silent changes" is a claim you must have checked (§ 9).
  [ ] Cross-cutting DoD (X1 SDK parity + harness, X2 mkdocs, X3 docs +
      DOC-INDEX) is stated as binding EVERY slice (§ 6).
  [ ] Prerequisites are concrete and checkable (a prior release CLOSED +
      docs reconciled; worktree hygiene), not aspirational (§ 7).
  [ ] Reserved-gap policy is stated (gaps are the insertion mechanism;
      HALT on overflow), so follow-on work has a home (§ 4).
  [ ] Decisions taken and open questions are recorded ON THE PLAN, with
      dates — not left in chat (§ 10, § 11). Chat is throwaway.
  [ ] Footprint stated: where does each piece run (in-library CPU-only vs
      caller-side BYO-LLM vs CI), and is the library query path still
      deterministic/CPU-only? (header).
========================================================================
-->

# FathomDB {{RELEASE}} — Plan (state-machine ladder) · **{{THEME}}**

> **Plan-as-state-machine.** Mod-5 slice ladder + reserved-gap policy + the live
> "Immediate Next Slice" pointer. Authoritative slice contracts graduate to
> `{{RELEASE}}-implementation.md`; live state lives in `runs/STATUS-{{RELEASE}}.md`.
> Method (roles, state spine, preflight, decision loop, recovery) is the
> release-independent runbook `dev/design/orchestration.md`. Run via
> `/goal complete {{RELEASE}}` as an **orchestrator** session
> (`prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`).
>
> **Theme.** {{THEME — one paragraph: what this release is for and why now.}}
>
> **Footprint.** {{Where each piece runs — in-library CPU-only / caller-side
> BYO-LLM / CI. Confirm the library query path stays CPU-only/deterministic.}}

---

## 1. Goal & scope

{{GOAL — 2-5 bullets. Each: the deliverable, the requirement it satisfies, and
*why it belongs in this release* (especially why-first for a keystone).}}

**Out of scope (deferred):** {{what is explicitly NOT in this release, and the
release/slice it is deferred to. Deferral is a deliberate decision, not "too hard".}}

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal (falsifiable, offline) |
|----|-------------|------------------------------------------|
| R-{{X}}-1 | {{requirement}} | {{a command / test / on-disk witness that proves it — RED→GREEN, byte-identity, conformance enumeration, CI dry-run, etc.}} |
| R-{{X}}-2 | {{…}} | {{…}} |

{{New ACs (if minted): candidates **AC-NNN+** at Slice 0 (…) and at Slice 40
(release-readiness), per the locked-acceptance policy in § 6.}}

---

## 3. Slice ladder (mod-5)

```
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR Kickoff — board + plan triad; the load-bearing ADR(s); policy confirmation | design-adr | — |
| **5** | **{{KEYSTONE}}** — {{…}} | implementation | 0 |
| **…** | {{…}} | {{implementation \| verification \| research \| experiment \| release-op}} | {{…}} |
| **40** | **Verification + Release Readiness** — X1/X2/X3 + R-* AC gate {{+ dry-run publish}} | verification | {{…}} |

**Keystones / hard gates.** {{which slice gates which later slice or release; which
transition is HITL-gated and why — e.g. a backlog push, a default flip.}}

**Tracks (parallelizable).** {{track A ∥ track B ∥ track C}}; name the shared
editable files (e.g. `pyproject.toml`, schema) that force serialization. Each track
is an independent worktree — preflight every spawn (`orchestration.md § 1.6`).

---

## 4. Reserved-gap policy (carried, unchanged)

Planned slices are multiples of 5; gaps `1–4, 6–9, …` are the insertion mechanism
for unplanned follow-on a slice reveals. A reserved-gap slice (N+1…N+4) is **fully
orchestrated** (own prompt, own worktree off a fresh `main` baseline, own
`output.json`, own codex § 9, own CLOSED block) — not an ad-hoc patch. **HALT rule:**
if a gap band fills and follow-on remains, the slice was mis-scoped → HALT to HITL;
never overflow into the next mod-5 number. (Full statement: `0.8.1-plan.md` § Numbering.)

---

## 5. Cross-cutting Definition of Done (X1/X2/X3 — bind EVERY slice)

- **X1 — SDK parity + functional harnesses.** Any surface/behavior/error/result
  change lands in **both** Py and TS SDKs in the same slice, each with a live
  functional harness (not symbol presence), incl. cross-binding equivalence.
- **X2 — `mkdocs build` stays green.** Any `docs/` add/rename updates `mkdocs.yml`
  nav same-slice; Slice 40 enforces it as a release gate.
- **X3 — docs updated per slice + `dev/DOC-INDEX.md` maintained** in the closing
  docs commit.

`runs/STATUS-{{RELEASE}}.md` carries an X1/X2/X3 per-slice column.

---

## 6. Acceptance-criteria policy (carried)

`dev/acceptance.md` stays **locked** (`acceptance-md-locked-no-feature-acs`). Slices
track work by G-gap / OPP-id + TDD test names, NOT invented AC ids. New ACs
(AC-NNN+) are minted **only at gated slices** (Slice 0 conformance, Slice 40
release-readiness), decided HITL.

---

## 7. Prerequisites (before any slice opens)

1. **{{Prior release}} closed** and its docs reconciled — this line baselines off the
   `main` that carries {{the prior keystone}}.
2. **Worktree hygiene:** pre-create + verify each slice worktree off
   `$(git rev-parse main)` and run `scripts/preflight.sh --worktree <wt>`
   (`agent-worktree-stale-base-trap`, `orchestration.md § 1.6`); GPU/maturin builds
   happen on the MAIN tree only.

---

## 8. Override / duplication register (verified file:line)

Every place a single contract has **divergent sources** that could drift — a config
key defined twice, a default declared in two layers, a schema duplicated across
bindings. Fill from a real grep; this register is the anti-drift surface for the
release. If you looked and found none, say so.

| # | Concept | Divergent sources (file:line) | Live consequence if they drift |
|--:|---------|-------------------------------|--------------------------------|
| 1 | {{e.g. CE-rerank α default}} | {{rust: …:NN · py: …:NN · ts: …:NN}} | {{what breaks}} |

{{OVERRIDE_REG — or: "Reviewed <date>; no divergent contract sources found in scope."}}

---

## 9. Behavior-change register (every silent change → a changelog line)

Each row is a behavior an existing consumer could notice (a default flip, a schema
bump, a changed error shape, a new required field). Every row must become a
`docs/changelog` entry by Slice 40. "No silent changes" is a claim you must have
checked, not a default.

| # | Change | Who notices | Changelog entry |
|--:|--------|-------------|-----------------|
| 1 | {{…}} | {{consumer / SDK caller / operator}} | {{the line that ships}} |

{{BEHAVIOR_REG — or: "Reviewed <date>; this release ships no consumer-visible
behavior change (mechanism/CI only)."}}

---

## 10. Decisions taken (recorded)

- {{YYYY-MM-DD}} — {{decision}} · rationale: {{…}} ({{HITL / ADR ref / memory}}).

---

## 11. Open questions for the human (HITL)

1. {{question}} — options + recommendation; which slice/gate it blocks.

---

## 12. Out-of-band / parallel notes

- {{other releases or tracks running concurrently; shared-file conflicts; budget
  notes — e.g. "$0, mechanism-only, can run beside the experiment program".}}

## 13. Immediate next slice

**Slice 0 — Setup + ADR Kickoff.** {{NEXT — stand up `runs/STATUS-{{RELEASE}}.md`
(mirror STATUS-0.8.2 shape), draft the load-bearing ADR, confirm policy. Then fan
out Slices … ∥ … ∥ …}}
