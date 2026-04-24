# 0.6.0 Rewrite — Clean-Context Plan

Goal: lock requirements, architecture, interfaces, acceptance, and test plan **before**
touching `crates/` or `python/` or `ts/`. Code PRs gated on `0.6.0-design-frozen` tag.

Owner: @coreyt. Branch: `0.6.0-rewrite`.

Optimization target: **quality of deps/sequencing/design/acceptance/code**. No date pressure.

Review loop: every ADR + every locked doc passes `critic` subagent → user (HITL).
Critic prompt-frame: attack hidden assumptions, missed alternatives, vague acceptance,
unacknowledged coupling, over-design, layering for its own sake.

Critic mapping:
- Requirements / acceptance / learnings → `general-purpose` w/ framed attack prompt.
- Architecture / design / ADRs → `architecture-inspector`.
- Interfaces → `code-reviewer`.

Cadence: docs written/updated in batches within a turn. Critic + HITL review **after**
the turn. Any doc changed after lock but before implementation → re-review.

## Non-goals (explicit, do not plan or execute)

- **No data migration.** 0.6.0 is fresh-db-only. No INSERT…SELECT, no migrators, no
  compat readers. Decided pre-plan; do not revisit.
- **No upgrade path for existing 0.5.x users in 0.6.0.** Deferred to later release.
  Do not design, do not prototype, do not stub.
- **No perf baseline capture of 0.5.x.** Absolute gates from product intent, not deltas.
- **No 0.5.x backports during 0.6.0 work.** No bug fixes, no security patches on 0.5.x
  from this branch. If urgent maintenance needed, owner cuts separate branch from `main`.
- **No risk register.** Human-owned activity, not in this plan's scope.
- **No glossary / rename migration map.** This is a rewrite; prior terms are not
  load-bearing. Terms that survive come in via harvest on their own merits.

---

## Sequencing + dependencies

```
Phase 0 scaffold  (incl. doc-types proposal → HITL → done-defs)
      │
      ▼
Phase 1a harvest prior work ──┐  (incl. dep audit, obs/perf req harvest)
      │                       │
      ▼                       ▼
Phase 1b learnings      Phase 2a decision index (draft)
      │                       │
      └──────────┬────────────┘
                 ▼
         Phase 2b critic pass on index
                 ▼
         Phase 2c HITL triage
                 ▼
         Phase 2d ADRs (per decision: draft → critic → HITL → accepted)
                 ▼
Phase 3a requirements.md ── critic ── HITL ── locked
                 ▼
Phase 3b acceptance.md  ── critic ── HITL ── locked     (AC ids issued)
                 ▼
Phase 3c architecture.md ── critic ── HITL ── locked
                 ▼
Phase 3d design/*.md (parallel subsystems, each ── critic ── HITL ── locked)
                 ▼
Phase 3e interfaces/*.md ── critic ── HITL ── locked
                 ▼
Phase 3f test-plan.md (AC→test map) ── critic ── HITL ── locked
                 ▼
Phase 4 freeze tag
                 ▼
Phase 5 interface stubs (compile, no impl)
```

Hard deps:
- Acceptance blocks architecture (can't design for undefined success).
- Architecture blocks design (subsystem boundaries first).
- Design blocks interfaces (semantics before signatures).
- Interfaces block test plan (what gets tested per AC).
- Test plan blocks freeze (every AC must have a test id).

Soft deps (parallelizable):
- Phase 1b ∥ Phase 2a.
- Phase 3d subsystems ∥ each other once 3c locked.

---

## Phase 0 — Scaffold

Steps:

1. **Propose doc-types + doc list** → HITL review.
   Draft table: doc-type (requirements, acceptance, architecture, design, interface,
   test-plan, ADR, learnings, followups, dep-audit, security-review), purpose, front-matter
   fields, lifecycle states, proposed file list. Pause for user edits before scaffolding.
2. **Define "done" per doc-type** after HITL approval of list. Done-def = checklist
   that must pass before status flips to `locked`. Example (acceptance): every AC id
   testable + mapped to test id + no compound ACs.
3. Create `docs/0.6.0/` skeleton only after 1+2 approved.

Doc front-matter schema (all doc-types):

```
---
title: <doc title>
date: YYYY-MM-DD
target_release: x.y.z
desc: <one line>
blast_radius: TBD | <list of files + methods/functions likely affected if this doc is wrong>
status: draft|review|locked
---
```

ADR naming: `adr/ADR-x.y.z-<kebab-title>.md` where `x.y.z` is target release.
ADR `status` field (collapsed — no separate `decision_status`):
`proposed → critic-reviewed → accepted | rejected`; post-acceptance may flip to `superseded`.

## Done-definitions per doc-type

**index (`README.md`)** — lists every doc + current status; links to plan + ADR index; updated on every status flip.

**requirements** — user-visible outcomes only (no implementation verbs); explicit non-goals; no compound items; harvest citations; critic clears hidden assumptions.

**acceptance** — unique `AC-NNN` id per entry; testable (observable, measurable, falsifiable); no compounds; each traces to ≥1 requirement; each has placeholder test id; no "should/ideally/reasonable"; critic clears vague terms.

**architecture** — crate topology (name, responsibility, deps-in/out); ascii write + read flow; on-disk layout named; every component maps to ADR or requirement; no orphans; architect agent proposes design subsystem list post-draft; critic = `architecture-inspector`.

**design (per subsystem)** — lists owned AC ids; lists applicable ADRs; enumerates interface surface (fns, types, errors); invariants listed; failure modes + recovery; no speculative knobs; critic = `architecture-inspector`.

**interface (per surface)** — every public symbol: signature, example, error cases, stability posture; traces to ≥1 design doc; consistent naming across surfaces; no TODO signatures at lock; critic = `code-reviewer`.

**test-plan** — every AC id has ≥1 test id; each test id: layer, owning crate, fixtures; no AC uncovered; no test id without AC backref; perf/soak gates absolute. **Test scaffolds must exist before lock**, written by a specialized subagent; expect >1 iteration before lock. Critic = `general-purpose` framed.

**ADR** — context, decision, options (≥2), tradeoffs, chosen option, consequences, status; named `ADR-0.6.0-<kebab-title>.md`; cites superseded ADR if replacing; critic = `architecture-inspector`.

**deps (per dep)** — current usage; verdict keep|drop|replace; replacement + migration cost if replace; ≥1 alternative considered; license + maintenance check; `deps/README.md` summarizes. Lockable individually; folder living during 0.6.0. Critic = `architecture-inspector`.

**security-review** — output of `security-review` skill vs. locked set (requirements + architecture + design + interfaces); each finding: severity, affected doc, proposed resolution, status. Lock bar: zero open findings at severity ≥ medium (HITL may adjust); low may carry to followups w/ explicit call-out.

**learnings** — keep-doing + stop-doing sections; every item cites source (SHA / issue / memory id); "Prior Work Disposition" table; living during 0.6.0; no formal lock.

**followups** — per item: title, origin, target release or `TBD`; write-only from agents; not read unless referenced; no formal lock; snapshotted at 0.6.0 freeze.

Skeleton (subject to HITL step 1):

```
docs/0.6.0/
  README.md            index + doc status table (draft|review|locked)
  plan.md              this file
  requirements.md      user needs, non-goals
  architecture.md      crate boundaries, data flow, storage
  design/              one file per subsystem
    engine.md
    projections.md
    vector.md
    retrieval.md
    scheduler.md
    bindings.md        python + ts + cli
  interfaces/          public surface (stub code + signatures)
    rust.md
    python.md
    typescript.md
    cli.md
    wire.md
  acceptance.md        AC-001..AC-NNN; each maps to test id
  test-plan.md         unit / integration / soak / perf strategy
  adr/                 ADR-x.y.z-<title>.md decisions + rationale
  deps/                one file per third-party dep (keep/drop/replace + reason)
  security-review.md   output of `security-review` skill against locked design
  learnings.md         from Phase 1b (what to keep / what to avoid)
  followups.md         deferred → 0.6.1+ (incl. upgrade path for 0.5.x users)
                       (write-mostly during 0.6.0: agents append but do NOT read
                        unless explicitly told — keeps working context clean)
```

Add `docs/0.6.0/README.md` pointer to `CLAUDE.md` so agents load context narrowly.

---

## Phase 1 — Harvest

### 1a. Extract good prior work

Source dirs: `dev/`, `dev/notes/`, `dev/archive/`, `docs/concepts/`, `docs/reference/`.

For each design doc:
- **Keep**: survives to 0.6.0 as-is or lightly edited → move to `docs/0.6.0/design/` or cite in ADR.
- **Fold**: content merges into new consolidated doc.
- **Archive**: still-valid history → `docs/archive/0.5.x/`.
- **Drop**: superseded or wrong → delete (git retains).

Triage output: table in `learnings.md` § "Prior Work Disposition" — columns: file, verdict, target, notes.

High-signal candidates (must review):
- `dev/ARCHITECTURE.md`, `dev/ARCHITECTURE-deferred-expansion.md`
- `dev/USER_NEEDS.md`
- `dev/production-acceptance-bar.md`
- `dev/test-plan.md`
- `dev/engine-vs-application-boundary.md`
- `dev/notes/design-*-2026-04-2[23].md` (freshness SLI, retrieval gates, scheduler, db-wide embedding)
- `dev/notes/managed-vector-projection-followups-2026-04-23.md`
- `dev/design-logging-and-tracing.md`, `dev/design-note-telemetry-and-profiling.md`
  (observability requirements — harvest into `requirements.md` + `acceptance.md`)
- `docs/concepts/*`, `docs/reference/*`

Sub-step **1a.i — Dependency audit**: produce one file per dep under
`docs/0.6.0/deps/<dep-name>.md`. Each: current usage, keep | drop | replace + reason,
alternatives considered. Index page `docs/0.6.0/deps/README.md` summarizes verdicts.
Feeds `architecture.md`. Critic: `architecture-inspector`.

### 1b. Extract learnings

Write `learnings.md` with two sections, each a bulleted list + 1-line rationale per item:

**Keep doing (good practices)**
- Red→green TDD (per memory `feedback_tdd.md`).
- Orchestrator-on-main + implementer-in-worktree + code-reviewer pattern (per memory).
- Per-commit clippy + fmt + cross-platform CI matrix.
- Net-negative LoC on reliability releases.
- ADRs with rationale (not just decision).
- Deprecation shims treated as first-class code w/ own tests.
- Post-publish smoke install from registry before "done."

**Stop doing (anti-patterns from 0.5.x)**
- Cypher / alt-query-language surface — scope creep, not user-needed.
- Per-item variable embedding — identity leaked into vector config; invariant violation.
- Layers-on-layers abstractions (e.g. nested profile→kind→vec configure).
- Over-design: speculative knobs w/o user forcing function.
- Silent degrade on missing schema (fused JSON filters bug).
- Mocked DB in integration tests.
- yaml.safe_load as workflow validator.
- Hardcoded `i8`/`u8` at C boundary.
- Punting reliability bugs to next minor.
- Data migration in feature releases.

Each entry cites source (commit, issue, or memory id).

---

## Phase 2 — Identify outsized decisions (critic + HITL)

Goal: surface decisions where wrong-choice = rewrite, so user reviews before freeze.

Process: I draft `docs/0.6.0/adr/0000-decision-index.md` listing candidate decisions
grouped by category. User marks each **decide-now** / **defer** / **drop**. For each
**decide-now**, I write an ADR (0001.., 0002..) w/ options + tradeoffs + recommendation;
user picks; ADR status → `accepted`.

Categories + candidates:

**Acceptance (what "0.6.0 shipped" means)**
- Single-process durability target (fsync policy, recovery time).
- Projection freshness SLI numerical target.
- Retrieval p50/p99 latency gates.
- API-compat posture vs. 0.5.x (break freely? shims where?).
- Platforms in tier-1 CI (linux x86/arm, darwin, windows).

**Architecture**
- Crate topology: keep `fathomdb-engine` monolith or split (storage / projection / vector / query)?
- Single-writer thread vs. MVCC.
- Vector index location: separate file vs. embedded in SQLite vs. external (sqlite-vec stays?).
- Scheduler: in-process actor vs. external task queue.
- Wire format stability (proto? JSON? versioned?).

**Design**
- Projection model: pull (lazy) vs. push (eager with scheduler) vs. hybrid.
- Embedding identity: embedder-owned (enforce invariant in code, not config).
- Retrieval pipeline shape: fixed stages vs. composable.
- Error taxonomy: single crate-level error enum vs. per-module.

**Interface**
- Python API shape: sync only, async only, or both?
- TS API: mirror Python 1:1 or idiomatic TS?
- CLI scope: admin-only or full query?
- Config surface: TOML file, env, builder API — pick one primary.
- Deprecation policy for 0.5.x names.

HITL checkpoint: after draft index, pause for user to triage. Do not proceed to ADR
drafting until triage done.

---

## Phase 3 — Write frozen docs

Order (each step gates next):

1. `requirements.md` + non-goals — user-visible outcomes, explicit drops (no cypher etc).
2. `acceptance.md` — `AC-001..AC-NNN`, each testable, each maps to a planned test id.
3. `architecture.md` — reflects accepted ADRs, crate boundaries, data flow diagram (ascii OK).
4. `design/*.md` — architect agent proposes the **needed subsystem design docs**
   after first draft of `architecture.md`; then one file per proposed subsystem,
   references ADRs, cites AC ids covered. `design/bindings.md` written first to
   test whether it fills a role distinct from `interfaces/{python,ts,cli}.md`.
5. `interfaces/*.md` — signatures + short examples. Architect agent delegates
   content post-`architecture.md`; `wire.md` written even if short.
6. `test-plan.md` — mapping AC id → test id → layer (unit/integration/soak/perf).
7. `security-review.md` — run `security-review` skill against locked design set
   (requirements + architecture + design + interfaces). Findings resolved via ADR
   amendments or acceptance criteria additions before freeze.

Each doc uses the front-matter schema from Phase 0 (title, date, target_release, desc,
blast_radius, status).

Review protocol: I draft → critic subagent → user HITL → status flips to `locked`.
Re-review required if doc changes after lock but before implementation.

---

## Phase 4 — Freeze

- All docs `status: locked`.
- `security-review.md` findings resolved.
- Tag: `git tag 0.6.0-design-frozen`.
- Update `CLAUDE.md` with: "0.6.0 implementation MUST cite AC id + ADR id in PR body."
- Archive pre-0.6.0 design notes to `docs/archive/0.5.x/` (keep git history).
- Add CI check: PRs touching `crates/` without AC id in body → fail (warn-only first week).

(Implementation work, including interface stubs, is **out of scope** for this plan —
handed to implementer phase downstream.)

---

## Deliverables checklist

- [ ] Doc-type + doc list proposal → HITL approved
- [ ] Done-definitions per doc-type approved
- [ ] `docs/0.6.0/` skeleton
- [ ] `learnings.md` — keep + stop lists w/ citations
- [ ] Prior-work disposition table
- [ ] `deps/*.md` populated (one per dep) + `deps/README.md` index
- [ ] `adr/ADR-0.6.0-decision-index.md` triaged by user
- [ ] ADRs accepted (per decide-now entry)
- [ ] `requirements.md` locked
- [ ] `acceptance.md` locked (AC ids issued)
- [ ] `architecture.md` locked
- [ ] `design/*.md` locked
- [ ] `interfaces/*.md` locked
- [ ] `test-plan.md` locked (AC→test mapping complete)
- [ ] `security-review.md` locked, findings resolved
- [ ] `0.6.0-design-frozen` tag

---

## Answered constraints (2026-04-24)

1. `dev/` fully archivable on `0.6.0-rewrite` branch. Archive early in Phase 1a.
2. No target date. Optimize for: dependency identification, sequencing, design quality,
   acceptance criteria quality, code quality. Speed is not a driver.
3. Review loop: **critic subagent + HITL**. Every ADR, every locked doc passes through
   a `critic` subagent pass before user review. Critic tasked to attack: hidden
   assumptions, missed alternatives, vague acceptance, coupling, over-design.
4. No hard external scope caps yet. Revisit before freeze.
