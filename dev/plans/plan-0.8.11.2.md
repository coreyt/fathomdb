# FathomDB 0.8.11.2 — Plan (pico umbrella) · **OPP-1 / OPP-3 / OPP-6 + Cause-A**

> **Plan-as-pico-umbrella.** This is a **calendar-decoupling landing vehicle**, not an engine release and
> **not a sequencing override.** It gathers four already-decided work items — **OPP-1, OPP-3, OPP-6, and
> Cause-A** — under one label so each can start the moment its OWN Phase-0 prerequisites are met, instead
> of waiting for the 0.8.10 / 0.8.12 / V-gate calendar. Read first:
> `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (findings **F-13** + **F-14** — the disposition of
> record), `dev/plans/runs/experiment-roadmap-placement-proposal.md` (the source placement rationale,
> esp. §3 sequenced action plan + §5 exit criteria), and the concrete V-3 / V-7 experiment designs in the
> Memex repo (`dev/fathomdb/OPP-1-experiments.md` / `OPP-3-experiments.md`).
>
> **Label note (HITL 2026-06-29; two-tier model, F-13/F-14).** `0.8.11.2` is a **normal pico** under the
> standing two-tier numbering model: `x.y.z` = real, publishable releases (manifest bump + `v*` tag +
> registry publish; publishing is always a separate explicit HITL call), and `x.y.z.p` **picos** =
> label-only, **never-published** work-completion increments for OOB/transitory work. This umbrella is
> therefore **transitory + label-only**: NO manifest version bump, NO `v*` tag, NO publish. Cause-A's code
> lands on `main` but **Memex consumes it via a local build**; a publishable `x.y.z` is a separate later
> HITL call. (`13` stays forbidden as minor and micro — no `0.13.x`, no `0.8.13`.)
>
> **Distinct from `0.8.11.1`.** `0.8.11.2` is a **different pico off the same 0.8.11 parent** than
> `0.8.11.1` (the dependency **Library Sweep**, F-12). The two do not share scope; this plan does **not**
> touch, merge, or re-scope `0.8.11.1`.
>
> **Sequencing PRESERVED (the load-bearing caveat).** The pico is a landing vehicle, **not** a sequencing
> override. The hard dependency order is unchanged: **Phase A (runnable now) → V-1 (live-CE
> re-validation) → OPP-1 = V-3 (held behind V-1) → OPP-3 bears at V-7**; Cause-A runs parallel and gates
> only the real-gold *adoption* arms. The acceleration is **decoupling from the release cadence**, not
> pulling any gate forward.
>
> **Footprint.** Experiments produce **verdicts / eval artifacts, not shipped features.** Cause-A is
> additive code (a stable-id field on `SearchHit` + bindings + telemetry/gold keying), default-off and
> behavior-neutral on the existing query path. The priced arms (frontier answerer / oracle extraction)
> are EVAL-ONLY and run under a pre-set `$` ceiling with the priced-run resilience preconditions.
>
> **Autonomous-completion envelope (HITL 2026-06-29 — `/goal complete`-driven).** This plan is written to
> be **driven to completion by a Steward (LBS-type) with NO mid-run HITL pause**, within three pre-set
> envelopes: (1) **spend** — a single pooled **`$75`** ceiling across ALL priced passes, enforced with
> the resilience preconditions + a running ledger, auto-stop at the cap; (2) **cross-repo** — Memex-side
> arms are AUTHORIZED but run via a **Memex orchestrator agent** driving
> `/home/coreyt/projects/memex/dev/plans/plan-0.5.1.md`, coordinated through a **message bus the Memex
> orchestrator creates as its first act** — a branch + worktree at
> `/home/coreyt/projects/memex-worktrees/0.5.1-fathom-chat/`, with `fathom-memex-chat.jsonl` inside it
> (§2A) — **NO git pushes to Memex** from this Steward (the standing fathomdb-only push scope is
> preserved; the held Memex ledger stays held); (3) **stop posture**
> — the Steward **auto-proceeds** through Phase A → V-1 → V-3 → V-7 within the envelopes and hard-stops
> ONLY at the two genuine product commitments: **OPP-1 Adopt-GO** and any **publishable `x.y.z` cut /
> publish**. The §8 questions that previously gated this are now **resolved decisions** (§7/§8).

---

## 0. START HERE — Phase-0 prerequisites (do before any priced run)

The pico decouples from the **calendar**, not from the **prerequisites**: each item starts when its OWN
Phase-0 conditions are met. Stand up `runs/STATUS-0.8.11.2.md` and clear these first (placement proposal
§3 Phase 0):

1. **§8b per-experiment ownership split — CONFIRMED (HITL 2026-06-29); adopt as the standing default** for
   **OPP-1** and the **OPP-6 sweep**: FathomDB owns harness / corpora / at-power protocol / eval-metrics;
   Memex owns the extraction + decompose/oracle LLMs, the dependency call, and the merge
   (decomposer/extractor stay Memex-side per the cohesion seam). No further kickoff negotiation needed.
2. **`$` ceiling — SET (HITL 2026-06-29): a single pooled `$75` envelope across ALL priced passes**,
   enforced **before spend** with the priced-run resilience preconditions (incremental checkpoint +
   verified `--resume` + 429/5xx backoff + window-fit + completeness guard + a running `$` ledger). The
   Steward **auto-stops at the `$75` cap** and records partial verdicts rather than pausing. Priced passes
   drawing on the pool:
   - OPP-1: the frontier-answerer passes;
   - OPP-3: the native-gap characterization (both answerers × corpora) + answerer passes;
   - OPP-6: the C3/C4 frontier/oracle extraction passes.

   (C0/C1 + academic arms are local/`$0` and run freely — they do not draw on the envelope.)
3. **MuSiQue re-pull preserving `question_decomposition`** (FathomDB-side — we hold the corpus): modify
   `tests/corpus/scripts/acquire_musique.py` to retain the native per-hop
   `{question, answer, paragraph_support_idx}` list and re-pull
   `data/corpus-data/raw/musique_dev.jsonl`; verify all 2,417 answerable rows carry it. **Unblocks OPP-1
   A3 (oracle-decompose).** Do **not** synthesize labels.
4. **FathomDB eval-support add (small, OOB):** expose `margin` as a *measurement* (decoupled from the V-7
   verb-shape decision) + the **distractor-injection / gold-rank-demotion** knobs + confirm per-corpus
   `decide_08x`. Rides the Cause-A pico or a tiny eval micro. **Unblocks OPP-3.**
5. **Confirm the Memex 0.5.2 value-test harness** (`memex/dev/plans/plan-0.5.2-valuetest.md`, COMPLETE on
   `feat/opp11-valuetest-harness`) is the runner/vehicle for the as-Memex / real-gold arms; the academic
   arms are **not** blocked on it (they run on FathomDB `decide_08x`).
6. **Stand up the cross-repo message bus (§2A).** The Steward's first cross-repo act is to dispatch the
   **Memex-side kickoff prompt (§2A)** to a Memex orchestrator session. That orchestrator's FIRST action
   is to create a branch + worktree at `/home/coreyt/projects/memex-worktrees/0.5.1-fathom-chat/` and a
   `fathom-memex-chat.jsonl` inside it, then announce readiness on the bus. The FathomDB Steward polls
   that exact path. **No git pushes to Memex.**

---

## 1. Goal & scope

In scope — the **four** decided items, gathered under one label-only pico:

- **OPP-6 — ELPS extraction-coverage sweep (EXP-COV-0..3).** Coverage→outcome sweep; EXP-COV-0 also
  re-measures the per-corpus relevance ceiling. **Output gates the 0.8.10 #6 ELPS-coverage
  invest/de-prioritize decision** (the dependency is intact; only the execution venue moved here).
- **OPP-3 — cascade / CE-default-on evals.** Native-gap characterization first, then the marginal-band
  treatment; **per-corpus, never pooled.** Final cascade/CE bearing is **recorded at V-7**.
- **OPP-1 — agent self-iteration experiments (EXP-ITER-D/-P/-POLICY).** Runs at the **V-3** gate position
  on the improved-recall substrate, **held strictly behind V-1** (NOT pulled forward). A4 / EXP-AF-MH is
  the deferred, lower-priority arm under the EXP-AF cost protocol.
- **Cause-A — stable hit-id for real-gold adoption.** **Size-it-first → cut**: additive stable-id field
  on `SearchHit` + 4 bindings + telemetry/gold keying. **Gates only the real-gold *adoption* arms (+
  OPP-9 join / graph), NOT the academic experiment arms.** Distinct from / lighter than the F-8a G0
  `write_cursor`→`logical_id` swap (additive, not a carrier reshape).

**Out of scope:** anything in `0.8.11.1` (the Library Sweep — do not touch); the F-8a G0 identity-substrate
swap (Cause-A is the lighter, additive alternative); any publishable `x.y.z` cut (a separate later HITL
call); pulling any V-gate forward (the calendar is decoupled, the sequencing is not).

---

## 2. Phased sequencing (the dependency order — UNCHANGED)

```text
Phase A (runnable now) ─► V-1 (keystone) ─► OPP-1 = V-3 ─► OPP-3 bears @ V-7
  OPP-6 EXP-COV-0..3      re-run EXP-B'     EXP-ITER-D/-P    cascade/CE
  OPP-3 cascade evals     on live CE        /-POLICY on the  default-on
  Cause-A (size→cut)      (default-reranker improved-recall  packaging
  OOB margin/rank knobs   ON)               substrate        decision

Cause-A ── parallel; size-it-first ──► gates real-gold ADOPTION arms only (+ OPP-9 join / graph)
Memex 0.5.2 value-test harness ─► the VEHICLE for all as-Memex / real-gold arms
```

Hard edges (placement proposal §2): **OPP-6/EXP-A (recall) → V-1 → V-3/OPP-1** (HITL §8c: V-3 is *not*
pulled forward). Soft/parallel: OPP-3, Cause-A. Vehicle dependency: the Memex 0.5.2 harness for the
as-Memex arms (academic arms run on FathomDB `decide_08x` without it).

| Phase | Work | Runs when | Gates / consumed by |
|-------|------|-----------|---------------------|
| **A (now)** | OPP-6 EXP-COV-0..3 · OPP-3 cascade evals · Cause-A size-it-first→cut · OOB `margin`/distractor-rank eval-support | Phase-0 prereqs met (no calendar wait) | OPP-6 → 0.8.10 #6 decision; OPP-3 → V-7; recall substrate → V-1 |
| **V-1 (keystone)** | Re-run EXP-B′ on the **live CE engine** (default-reranker ON); fill global + multi_hop; add MMR + recency | after Phase A recall work | everything downstream re-validates against this |
| **V-3 = OPP-1** | EXP-ITER-D/-P/-POLICY on the improved-recall substrate; at-power on MuSiQue + HotpotQA (2Wiki → V-5) | **after V-1** (NOT pulled forward) | the agent self-iteration build/adopt gates |
| **V-7** | CE-default-on packaging decision — records OPP-3's cascade/CE bearing + the `margin` verb-shape decision | gate position | the CE-default-on product decision |
| **Cause-A (parallel)** | additive `SearchHit` id field + 4 bindings + telemetry/gold keying | Phase A; parallel | sequences before the real-gold **adoption** phase of OPP-1/3/6 (+ OPP-9 join / graph) |

(V-2 per-query arm oracle, V-4 real classifier, V-5 multi-corpus, V-6 competitor head-to-head run in
their own gate positions; OPP-1/OPP-3 touch V-3/V-7 specifically.)

---

## 2A. Cross-repo orchestration + autonomous execution envelope

The FathomDB Steward owns this side; the **Memex-owned arms run in a separate Memex orchestrator
session** (driving `memex/dev/plans/plan-0.5.1.md`), coordinated over a file message bus. **This Steward
never git-pushes Memex** (the standing fathomdb-only push scope holds; the held Memex ledger stays held).

**Message bus — `fathom-memex-chat.jsonl`.** Append-only JSONL the **Memex orchestrator creates inside the
worktree it stands up as its first act** (see the kickoff prompt below). Deterministic path both sides
hard-code:

```text
/home/coreyt/projects/memex-worktrees/0.5.1-fathom-chat/fathom-memex-chat.jsonl
```

One JSON object per line: `{ "ts": "<ISO8601>", "from": "fathomdb|memex", "kind":
"ready|request|handoff|result|question|ack", "ref": "<OPP/arm id>", "body": "<text or JSON>" }`.
Each side **appends** its own lines and **polls** (tail) for the other's; never rewrites prior lines.
The Steward writes `request`/`handoff` lines (e.g. an OPP-1 oracle-decompose corpus is ready, or an
answerer pass is requested under the shared `$` envelope) and reads `result`/`question`/`ready` lines.

**Memex-side kickoff prompt (the Steward dispatches this verbatim to a Memex orchestrator session).**

> You are the Memex-side orchestrator for FathomDB pico `0.8.11.2` (OPP-1/OPP-3/OPP-6 + Cause-A).
> **FIRST, before anything else:** from `/home/coreyt/projects/memex` on a verified-clean `origin/main`,
> create branch `feat/0.5.1-fathom-chat` and a worktree at
> `/home/coreyt/projects/memex-worktrees/0.5.1-fathom-chat/`; inside that worktree create the message bus
> file `fathom-memex-chat.jsonl` and append a first line `{"from":"memex","kind":"ready",...}` announcing
> the worktree path + branch. Then act as the orchestrator for `dev/plans/plan-0.5.1.md`, driving the
> **Memex-owned** arms (extraction + decompose/oracle LLMs, the dependency call, the merge) and the
> as-Memex / real-gold arms on the **0.5.2 value-test harness**. Poll `fathom-memex-chat.jsonl` for
> FathomDB `request`/`handoff` lines; reply with `result`/`question` lines. Honor the shared pooled
> **`$75`** priced-run envelope (coordinate spend over the bus so the two sides do not double-spend) with
> the priced-run resilience preconditions. **Do not push to any repo; do not publish.** Hard-stop only at
> OPP-1 Adopt-GO and any publishable cut.

**Autonomous stop posture (HITL 2026-06-29).** Within the spend + cross-repo envelopes the Steward
**auto-proceeds** Phase A → V-1 → V-3 → V-7, auto-recording verdicts/artifacts to
`runs/STATUS-0.8.11.2.md`. It **hard-stops for HITL ONLY at**: (a) **OPP-1 Adopt-GO** (frequency audit +
real personal gold — a product commitment), and (b) any **publishable `x.y.z` cut / registry publish**.
Everything else (eval verdicts, $0 arms, Cause-A size→cut, V-gate re-validations) lands without a pause.

---

## 3. Requirements + acceptance criteria (DoD)

| ID | Requirement | Acceptance signal (falsifiable) |
|----|-------------|---------------------------------|
| R-U-1 | Each of the four items is dispositioned with a landed verdict/artifact, not an `AGREED` | per-item result doc + repro script recorded in `runs/STATUS-0.8.11.2.md` |
| R-U-2 | Sequencing preserved — no V-gate pulled forward | V-3/OPP-1 starts only after V-1 lands; recorded in the status board |
| R-U-3 | OPP-6 output gates the 0.8.10 #6 decision | the EXP-COV curve + invest/de-prioritize recommendation is recorded against 0.8.10 #6 |
| R-U-4 | Label-only: no manifest bump / tag / publish | `git diff` shows no `version =`/`"version":` change in shipped manifests; no `v*` tag; Memex builds Cause-A locally |
| R-U-5 | Cause-A is additive + behavior-neutral on the existing path | default query path byte-unchanged; the new id field is opt-in/telemetry-only |
| R-U-6 | `0.8.11.1` untouched | no edit to `plan-0.8.11.1.md` or the Library Sweep scope |
| R-U-7 | Priced runs respect the pooled **`$75`** envelope + resilience preconditions | running `$` ledger ≤ `$75` across all passes; checkpoint/resume/backoff verified before spend; auto-stop at cap |
| R-U-8 | Cross-repo via the bus; **no Memex pushes** | Memex arms driven by the Memex orchestrator on `plan-0.5.1.md`; all coordination via `fathom-memex-chat.jsonl`; `git log` of Memex shows no push from this Steward |
| R-U-9 | Autonomous stop posture honored | Steward auto-proceeds the V-gates; the only HITL hard-stops recorded are OPP-1 Adopt-GO and any publishable cut |

---

## 4. Per-item exit criteria (lifted from the placement proposal §5)

Verdicts come from **running**, not from `AGREED`:

- **OPP-6.** Coverage is the lever iff Δ(gold-in-pool) or Δ(F1) CI-lower-bound > +0.04 on ≥1 class net of
  cost, with the precision guard, confirmed on real gold; a **flat curve at the per-corpus ceiling
  resolves** OPP-6 by redirecting to embedder/recall.
- **OPP-1.** Build-GO iff A1/A2 show a significant recall + answer lift over A0 (net of read cost) on
  MuSiQue + HotpotQA, A3 confirms head-room; **Adopt-GO is a separate gate** (frequency audit + real
  personal gold).
- **OPP-3.** If no corpus shows answer-gap ≥ ~0.08 with a signal AUROC ≥ 0.70 → cascade **stays parked**
  (structurally confirmed); if a thin/personal regime clears the bar → flip-ON candidate, re-measured on
  real Memex turns before adoption. Always publish the per-corpus `ce_top` dominance table.
- **Cause-A.** Size-it-first confirms additive-only (a field on `SearchHit` + 4 bindings + telemetry/gold
  keying; watch the `logical_id = NULL` doc-node case + the F-8a gold-id-contract revisit), then cut the
  OOB pico; it **gates only the real-gold adoption arms**, not the academic arms.

---

## 5. Cross-cutting DoD (bind every item)

- **X1 — SDK parity.** Cause-A's `SearchHit` field + bindings must keep Py↔TS parity; no SDK drift.
- **X2 — `mkdocs build` stays green** if any `docs/` touched (none expected).
- **X3 — docs/changelog.** Label-only umbrella ⇒ a single changelog/maintenance note (no version bump);
  update `runs/STATUS-0.8.11.2.md` per item.

---

## 6. Prerequisites (before any work opens)

1. **`main` is clean and current** (`git rev-parse --abbrev-ref HEAD` = `main`; `== origin/main`).
2. **Worktree hygiene:** any code work (Cause-A) gets a unique worktree cut from a verified `origin/main`
   tip; never the shared/primary checkout. One `maturin develop` at a time for any binding rebuild.
3. **0.8.11 is complete + merged** (PR #122); this baselines off current `origin/main`.
4. The **Phase-0 prerequisites** in §0 are cleared for the specific item being started (they are
   per-item, not a single global gate).

---

## 7. Decisions taken (recorded)

- 2026-06-29 — `0.8.11.2` pico umbrella established for OPP-1 / OPP-3 / OPP-6 + Cause-A; calendar-decoupled,
  sequencing preserved (Phase A → V-1 → OPP-1@V-3 → OPP-3@V-7; Cause-A parallel) · HITL, F-14.
- 2026-06-29 — `0.8.11.2` is a **normal pico** under the two-tier model (label-only, never-published);
  publish = a separate later HITL `x.y.z`; Memex consumes Cause-A via local build · HITL, F-13/F-14.
- 2026-06-29 — `0.8.11.2` is **distinct from `0.8.11.1`** (the Library Sweep); the latter's scope is
  untouched · F-12/F-14.
- 2026-06-29 — **`/goal complete`-driven, no mid-run HITL pause.** Spend = a single pooled **`$75`**
  envelope across all priced passes (auto-stop at cap) · HITL.
- 2026-06-29 — **Memex-side arms AUTHORIZED to execute** via a Memex orchestrator session on
  `memex/dev/plans/plan-0.5.1.md`, coordinated over `fathom-memex-chat.jsonl` (created in a Memex
  worktree); **NO git pushes to Memex** from this Steward — fathomdb-only push scope preserved · HITL.
- 2026-06-29 — **Stop posture: auto-proceed** the V-gates; hard-stop ONLY at OPP-1 Adopt-GO and any
  publishable `x.y.z` cut/publish · HITL.

---

## 8. Resolved at kickoff (HITL 2026-06-29) — no remaining pre-run questions

These previously gated the run; all four are now decided, so the Steward proceeds without a Phase-0 pause:

1. **`$` ceiling** — **a single pooled `$75` envelope** across all priced passes (not per-experiment caps);
   auto-stop at the cap with the running ledger. (Phase 0 #2.)
2. **§8b ownership split** — **confirmed default**: FathomDB owns harness/corpora/protocol/metrics; Memex
   owns extraction + decompose/oracle LLMs + the dependency call + the merge. (Phase 0 #1.)
3. **OOB `margin`/knobs eval-support** — **bundle with the Cause-A pico** (single additive code landing),
   not a separate micro.
4. **Label-only** — **confirmed**: no publish/tag for this umbrella; Cause-A reaches Memex via local
   build until a later HITL-cut publishable `x.y.z`.

The only HITL hard-stops that remain are **downstream product commitments** (OPP-1 Adopt-GO; any
publishable cut) — see the §2A stop posture.

---

## 9. Immediate next step (autonomous kickoff — no pause)

The Steward runs this without a HITL pause (envelopes in §0/§2A are pre-set):

1. Stand up `runs/STATUS-0.8.11.2.md` (the per-item verdict board + running `$` ledger, seeded at `$0` of
   the `$75` pool).
2. Dispatch the **§2A Memex-side kickoff prompt** to a Memex orchestrator session; it creates the
   branch + worktree + `fathom-memex-chat.jsonl` and announces `ready`. The Steward begins polling the bus.
3. Begin **Phase A in parallel** as each item's §0 prereqs clear (no calendar wait): OPP-6 EXP-COV-0..3
   ($0/academic arms freely; C3/C4 draw the pool), OPP-3 cascade evals, **Cause-A size-it-first → cut**
   (additive `SearchHit` id + 4 bindings + telemetry/gold keying, **bundling the OOB `margin`/distractor
   knobs**), and the MuSiQue re-pull preserving `question_decomposition`.
4. Proceed **V-1 → V-3 → V-7 in strict order** (sequencing preserved), auto-recording verdicts.
5. **Hard-stop ONLY** at OPP-1 Adopt-GO and any publishable `x.y.z` cut; everything else lands
   autonomously within the `$75` + no-Memex-push envelopes.
