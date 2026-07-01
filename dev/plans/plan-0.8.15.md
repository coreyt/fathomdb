# FathomDB 0.8.15 — Plan (state-machine ladder) · **In-library dispatcher build (EXP-Fr)**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `dev/plans/0.8.15-implementation.md` (authored at Slice 0); live state →
> `dev/plans/runs/STATUS-0.8.15.md`; deps/decision record →
> `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.15` as an
> **orchestrator** session.
>
> **Theme.** Build the **in-library query-router / dispatcher (EXP-Fr)** over the kind-tagged
> coexisting-index substrate shipped by EXP-S (0.8.14). This is the keystone of the planner-router track:
> a `RouteDecision`-carrying dispatcher (`route_recommend` + `search_routed`) that classifies query
> intent to one of five feature classes, selects the `(index_kind, retrieval, alpha, pool_n, recency)`
> config tuple from the EXP-B′ joint-tuning results, and returns both the search result _and_ what it
> decided and why. The **locus decision** (in-library vs agent-side vs both-layered) is the release's
> first and most critical HITL call; it is answered at Slice 0 from the 0.8.14 EXP-S and EXP-Fr-acc
> readouts.
>
> **This release is NOT OOB** — it is a real engine build, hard-gated by I-2 (EXP-S @ 0.8.14).
> The "odd-line" label means it is sequenced outside the even-backbone, _not_ that it is a $0 drop-in.
>
> **Footprint.** Dispatcher logic is IN-LIBRARY. The query path stays CPU-only, 1-bit, deterministic.
> The dispatcher is a _rule-based route table_, not an LLM — the footprint invariant forbids an
> in-library LLM on the $0 query path. Cost tiers above CPU (GPU / LocalLLM / NetLLM) are surfaced as
> metadata in `RouteDecision.cost_tier` so cost-aware agents can veto them; they are never crossed
> silently on the default path. EXP-AF productization (the agent-feedback loop) is **out of scope** →
> 0.8.17.

---

## 1. Goal & scope

### What is being built

Today `Engine.search()` is steerable — the caller already controls `alpha`, `pool_n`, `rerank_depth`,
filters, and the graph arm (EXP-0 + EXP-OBS landed). There is, however, **no routing layer**: every
caller must know which `(index_kind, retrieval_mode, alpha, pool_n, recency)` tuple is right for its
intent. A thin consumer cannot make that call; even a sophisticated agent (Memex) benefits from a
batteries-included default it can inspect and override.

This release builds that routing layer as two new governed verbs:

- **`route_recommend(query, intent?, constraints?) → RouteDecision`** — classify intent (from the
  agent's label, or as a fallback via the internal 5-class table), pick the winning stack from the
  EXP-B′ config table, return a `RouteDecision` _without executing any retrieval_. Zero retrieval cost.
  The agent sees what would be routed and why before paying.
- **`search_routed(query, intent?, route_hint?, constraints?) → SearchResult`** — same classification
  and selection, then dispatches to the appropriate L1 arm (`search`, `search_reranked`, or
  `search_explained`) and attaches the `RouteDecision` as a sidecar on the returned `SearchResult`.

The dispatcher is transparent, hintable, and overridable per `initial-arch-planner-router-0.8.x.md`
§5 (the router contract). It **does not supersede the agent**: the agent owns intent; FathomDB owns
mechanism. A bare `search()` call continues to work byte-identically — default-not-mandate.

### Intent taxonomy (PSD §II.B, portfolio §2)

Five classes, derived from the portfolio features F1–F5:

| Class | Feature | Route (primary path) | Cost tier |
| --- | --- | --- | --- |
| `needle` | F1 — factoid/exact memory | leaf index + fused-RRF + CE-rerank | CPU |
| `multi_session` | F2 — multi-session recall | leaf + wider-k + CE-rerank | CPU |
| `temporal` | F3 — knowledge update | leaf + valid-time filter + CE-rerank | CPU |
| `global` | F4 — sensemaking / map-reduce | coverage index + C map-reduce | NetLLM (HITL-gated cost tier) |
| `multi_hop` | F5 — entity relationship | leaf + iterative or explicit known-anchor walk | CPU (graph arm refuted; route as explicit walk only) |

The `global` path is **router-isolated**: map-reduce / community QFS are forbidden on `needle`,
`multi_session`, and `temporal` intent classes (PSD §II.B "Router-isolation, not a free stack" —
`C × precision-rerank` compound is measured at −0.362).

### The locus decision (key HITL call)

The recommended target is **both-layered** (`initial-arch` §2): ship L2 in-library as an overridable
default, keep L1 fully drivable underneath. This recommendation is **contingent on EXP-S GO and
EXP-Fr-acc's locus call**. The three options and their code consequences:

| Locus verdict | Code consequence |
| --- | --- |
| **In-library (GO, recommended)** | New `dispatch.rs` module in `fathomdb-engine`; `route_recommend` + `search_routed` methods on `Engine`; `RouteDecision` type exported from the engine crate |
| **Agent-side only (KILL)** | No new engine module; the 0.8.11 agent-side L2 prototype is hardened into an L1 config-carrying surface doc + per-intent config constants; Slice 5–10 work collapses to config publication + L1 harness verification |
| **Both-layered (recommended)** | In-library L2 as above _plus_ explicit docs confirming L1 is still fully drivable (the 0.8.11 prototype is the L1 drive reference for Memex) |

**The plan below targets the primary in-library GO path.** Each slice calls out the KILL divergence
explicitly so the ladder can be adapted at Slice 0 without re-planning from scratch.

### Critical engine files

| File | Role in 0.8.15 |
| --- | --- |
| `src/rust/crates/fathomdb-engine/src/lib.rs` | Add `route_recommend()`, `search_routed()`, `RouteDecision`; no changes to existing `search*` signatures |
| `src/rust/crates/fathomdb-engine/src/dispatch.rs` _(new)_ | `IntentClass`, `CostTier`, `RouteDecision`, route table, forbidden-composition guard |
| `src/rust/crates/fathomdb-py/src/lib.rs` | Expose new verbs + types to Python via PyO3 |
| `src/rust/crates/fathomdb-napi/src/lib.rs` | Expose new verbs + types to TypeScript via napi-rs |
| `src/rust/crates/fathomdb/src/lib.rs` | Re-export `route_recommend`, `search_routed`, `RouteDecision` on the governed facade |
| `src/python/fathomdb/_fathomdb.pyi` | Add `RouteDecision`, `IntentClass`, `CostTier` stubs; update `Engine` stub |
| `src/conformance/governed-surface-allowlist.json` | Add `search.routed`, `search.route_recommend` (or chosen verb names — freeze at Slice 0 ADR) |
| `src/rust/crates/fathomdb-engine/tests/dispatch_route_table.rs` _(new)_ | Unit TDD: 5 intent classes, forbidden composition rejection, override validation |
| `src/rust/crates/fathomdb-engine/tests/dispatch_routing_execution.rs` _(new)_ | Integration TDD: `search_routed` against kind-tagged fixtures from EXP-S |

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
| --- | --- | --- |
| R-DISP-1 | `RouteDecision` emitted for every routed search | `search_routed()` result carries non-None `route_decision`; `route_recommend()` returns `RouteDecision` with no `SearchHit`s (zero retrieval) |
| R-DISP-2 | Locus declared at Slice 0 ADR and never implicit | ADR written before Slice 5; KILL path produces agent-side track explicitly; no silent locus assumption |
| R-DISP-3 | Default-not-mandate: `search()` byte-identical to pre-0.8.15 | `search_result_shape.rs`, `pr_g1_search_hits.rs`, `pr_g10_reranker_ce.rs`, and `exp_obs_explain.rs` all green with no code change |
| R-DISP-4 | Forbidden compositions enforced on non-`global` paths | TDD: adversarial test — needle-intent → global-path override attempt returns `ForbiddenComposition` error variant |
| R-DISP-5 | Override/hint: agent-supplied `route_hint` accepted or rejected with reason | `route_recommend(route_hint=Some(…))` validates against forbidden-composition guard; accepted override carries `override_accepted: true` in `RouteDecision` |
| R-DISP-6 | Cost tier surfaced on every `RouteDecision` | `CostTier` enum: `Cpu / Gpu / LocalLlm / NetLlm`; default needle/temporal/multi\_session routes are `Cpu`; `global` map-reduce is `NetLlm` |
| R-DISP-7 | EXP-B′ config tuples carried per intent class | Route table is keyed by `IntentClass`; each row carries `(alpha, pool_n, k, recency)` from EXP-B′ results (stacks-unify vs stacks-diverge verdict recorded at Slice 0) |
| R-DISP-8 | EXP-B′ regression guard: config for intent X does not regress intent Y | Cross-feature regression fixture: routed search on a F1 needle fixture with the dispatcher must not produce a result set that would regress a known F2 result |
| R-X-1 | Py + TS SDK parity for `route_recommend` + `search_routed` + `RouteDecision` | X1 harness (`test_surface.py` + `surface.test.ts`) green; new verbs in `governed-surface-allowlist.json` |
| R-GATE | eu7 fidelity ≥ 0.90 (one-sided CI) holds post-0.8.15 | Dispatcher adds no vectors and triggers no migration; `recall_gate.rs` no-regression verified at Slice 40 |

**Route-accuracy AC (Slice 40 HITL gate):** the dispatcher's per-class routing accuracy on the EXP-Fr-acc
evaluation set must meet or beat the EXP-Fr-acc classifier accuracy baseline. Threshold is read from
EXP-Fr-acc results and written into `0.8.15-implementation.md` at Slice 0. No invented AC id — tracked as
`route_accuracy_gate` TDD name.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 40
```

| Slice | Title | Work-type | Depends-on |
| ---: | --- | --- | --- |
| **0** | **ADR + locus ratification** — read EXP-S (0.8.14) + EXP-Fr-acc readouts; read EXP-B′ config tuples; write dispatcher ADR (`0.8.15-implementation.md`): locus, `RouteDecision` shape, 5-class route table schema, forbidden-composition list, route-accuracy threshold; HITL locus call; if KILL, declare agent-side track and note ladder adaptations; stand up `STATUS-0.8.15.md` | design-adr | EXP-S @0.8.14 closed; EXP-Fr-acc results; EXP-B′ results |
| **5** | **Dispatcher core** — `src/rust/crates/fathomdb-engine/src/dispatch.rs` (new): `IntentClass` enum (Needle / MultiSession / Temporal / Global / MultiHop), `CostTier` enum (Cpu / Gpu / LocalLlm / NetLlm), `RouteDecision` struct, 5-class route table populated with EXP-B′ config tuples, forbidden-composition guard; `Engine::route_recommend()` method (no retrieval, returns `RouteDecision`); re-export on `fathomdb` facade; TDD: `dispatch_route_table.rs` (5 intent classes → expected route; forbidden-composition rejection; override accepted/rejected) | implementation (engine) | Slice 0 ADR ratified |
| **10** | **Routing execution + typed-constraint integration** — `Engine::search_routed()`: calls `route_recommend()` then dispatches to the L1 arm (`search_reranked` for CPU-tier; gated path for NetLlm); OD-4 enforcement in dispatch path (expand → valid-time filter → rerank, never filter before pool expansion); wire `SearchFilter` typed-constraint from #17 (0.8.11) into dispatcher `constraints` field; `RouteDecision` sidecar attached to `SearchResult` on the routed path (as `route_decision: Option<RouteDecision>` field, parallel to `explanation`); `route_hint` override path validated against forbidden-composition guard; integration TDD: `dispatch_routing_execution.rs` against kind-tagged fixtures | implementation (engine) | Slice 5; EXP-S kind-tagged fixture DBs available |
| **15** | **Py + TS SDK parity + governed surface** — Python binding (`fathomdb-py/src/lib.rs`): expose `route_recommend`, `search_routed`, `RouteDecision`, `IntentClass`, `CostTier` via PyO3; update `_fathomdb.pyi` stubs; TS binding (`fathomdb-napi/src/lib.rs`): same surface via napi-rs; `src/ts/src/index.ts` type exports; update `governed-surface-allowlist.json` with new verb names (frozen at Slice 0 ADR); X1 cross-binding harness green; check F-8b `record_feedback` governance reclassification — if graduated to governed command, include in allowlist update | implementation (bindings) | Slices 5, 10 |
| **40** | **Verification + Release Readiness** — full X1/X2/X3; route accuracy against EXP-Fr-acc baseline (adversarial per-class routing fixture); forbidden-composition adversarial test (needle→global attempt must reject); `search()` / `search_reranked()` / `search_explained()` regression suite (byte-identical outputs vs pre-0.8.15 baseline); eu7 fidelity no-regression (`recall_gate.rs`); `mkdocs build` green; HITL release sign-off + route-accuracy AC ratification | verification | Slices 5, 10, 15 |

### Keystones and hard gates

**Slice 5 (dispatcher core) is the release keystone** — Slices 10 and 15 both depend on it. Binding
scaffolding (stub type shapes for Py/TS) may begin in parallel once Slice 5's `RouteDecision` type is
frozen (do not couple to the implementation internals, only to the public type shape).

**Locus KILL gate at Slice 0:** if EXP-S KILL fires, the Slice 5–15 work collapses to the agent-side
track (the 0.8.11 prototype's config-carrying surface is hardened; no new `dispatch.rs` engine module is
added). Slice 40's verification and the route-accuracy gate still apply on the agent-side track.

**eu7 fidelity hard gate:** if anything at this release unexpectedly triggers a re-embed (should not
occur — the dispatcher adds no vectors and changes no schema), the gate `ci_hi ≥ 0.90` is a hard
BLOCK→HITL per `fathomdb-recall-fidelity-vs-relevance`.

---

## 4. Reserved-gap policy

Carried unchanged from `dev/plans/0.8.1-plan.md` §Numbering. Any verification finding at Slice 40 that
requires a fix (e.g. a route-table entry mis-wired to the wrong index kind, a forbidden-composition
bypass) is a fully-orchestrated reserved-gap slice off a fresh `origin/main` baseline — numbered
sequentially off this plan (Slices 21, 22, …), never an ad-hoc patch to an open worktree. Reserved-gap
slices require the same Codex §9 review and HITL sign-off as primary slices.

---

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

- **X1 — SDK parity.** Every new Rust public type (`RouteDecision`, `IntentClass`, `CostTier`) has a
  matching Python type stub in `_fathomdb.pyi` and a TypeScript export. The cross-binding harness
  (`test_surface.py` + `surface.test.ts`) is green at Slice 15 close. New verbs are in
  `governed-surface-allowlist.json` before the Slice 40 surface test runs.
- **X2 — `mkdocs build` green.** No broken doc references introduced. Every new file referenced in a
  plan or ADR exists at Slice close.
- **X3 — Docs + DOC-INDEX.** Every slice closes with an updated `runs/STATUS-0.8.15.md` entry (per-slice
  state derived from on-disk witnesses per `dev/design/orchestration.md` §1.5) plus a DOC-INDEX entry for
  any new design or implementation doc created.

State in `STATUS-0.8.15.md` is _derived_ from durable witnesses, not narrated. The furthest witness
that exists and verifies is the current state.

---

## 6. Acceptance-criteria policy

`dev/acceptance.md` is status:locked (max AC-073, titled 0.6.0). Requirements for this release are
tracked by `R-DISP-N` label (above, §2) and TDD name. New ACs — specifically the route-accuracy gate —
are _candidates_ at Slice 0 (draft threshold from EXP-Fr-acc data, written into
`0.8.15-implementation.md`) and _ratified_ at Slice 40 HITL sign-off. Do not invent AC ids; do not
fill `{{AC_IDS}}` template slots with fabricated identifiers.

---

## 7. Prerequisites

All must be verified at Slice 0 before any implementation begins. Slice 0 _must fail loudly and halt to
HITL_ if any item is absent or its gate is not in a verified-CLOSED state.

1. **EXP-S @ 0.8.14 CLOSED with GO verdict.** Kind-tagged coexisting indexes landed; determinism check
   passed; `runs/STATUS-0.8.14.md` shows CLOSED. This is I-2 — the hard physical gate. No in-library
   dispatcher is possible without the kind-tagged substrate. If the EXP-S verdict is KILL (determinism/perf
   failure), declare DP-A and invoke the agent-side track immediately; do not attempt to build over a
   broken substrate.
2. **EXP-Fr-acc results present.** Intent-classifier accuracy, asymmetric mis-route cost matrix, and locus
   recommendation all recorded. Verify the locus recommendation document exists before writing the Slice 0
   ADR. The route-accuracy threshold for the Slice 40 gate comes from this document.
   **CORRECTION (master F-11):** EXP-Fr-acc was originally floated to run by 0.8.9/0.8.12 — it **never ran**.
   It is now **produced by 0.8.11's folded-in experiment ladder (F-11)**, NOT a pre-existing float artifact.
   Verify the EXP-Fr-acc results as **a 0.8.11 deliverable**.
3. **EXP-B′ joint-tuning results present.** Per-intent `(alpha, pool_n, k, recency)` config tuples — or
   the unified tuple if stacks converged — available in their results doc. The stacks-unify vs
   stacks-diverge verdict (DP-B) must be recorded in `0.8.15-implementation.md` at Slice 0.
   **CORRECTION (master F-11):** like Fr-acc, EXP-B′ **never ran as float** (the assumption that 0.8.7/0.8.9
   produced it was FALSE); it is now **produced by 0.8.11's folded-in experiment ladder (F-11)**. Verify the
   EXP-B′ results as **a 0.8.11 deliverable**.
4. **EXP-OBS landed @ 0.8.8 and green.** Per-arm provenance + score breakdown exist on
   `SearchResult.explanation`; `exp_obs_explain.rs` and `exp-obs-explain.test.ts` green on `origin/main`.
   The routed path must carry full provenance through `search_routed`; the EXPLAIN surface is a prerequisite
   for a transparent router per `initial-arch` §6. **(Satisfied on `origin/main` — master F-6.)**
5. **0.8.11 OOB closed.** Agent-side L2 router prototype and per-intent config tuples pre-staged. The
   agent-side prototype is the KILL-path contingency (DP-A) and the config-tuple seed for Slice 5.
   Verify the 0.8.11 pre-stage document exists.
6. **`record_feedback` governance reclassification resolved (F-8b).** The 0.8.11 checklist must be
   answered before 0.8.15 expands the governed surface further at Slice 15. If `record_feedback`
   graduated to a governed command, the Slice 15 allowlist update must include it. Do not defer this
   check into Slice 15 — it affects the Slice 0 ADR's surface-expansion scope.
7. **Worktrees off `$(git rev-parse origin/main)`.** Verify `git rev-parse --abbrev-ref HEAD` before
   every commit and push. Never cut a worktree from a stale base
   (`shared-checkout-branch-can-be-stale-vs-session-env`). The orchestrator creates every worktree; the
   implementer never creates its own.
8. **MAIN-tree maturin/.venv mutex.** Rust-only unit tests (Slices 5, 10) may run in a worktree. Py/TS
   parity tests (Slice 15) and the Slice 40 full suite require the MAIN tree's `maturin develop` build.
   Only one `maturin develop` at a time on the MAIN tree (`0.8.6-0.8.7-parallel-build-venv-mutex`).

> **Net (per master F-11): the only remaining upstream *experiment* gate for 0.8.15 is EXP-S (0.8.14), the
> long pole.** EXP-OBS is already satisfied on `origin/main` (F-6), and the EXP-B′ + EXP-Fr-acc experiment
> base is **de-risked / produced by 0.8.11's folded-in ladder** rather than by the never-run 0.8.7/0.8.9
> float. The prereq verifications above for items 2/3 therefore check **0.8.11 deliverables**, not stale
> float artifacts.

---

## 8. Dependencies / sequencing

### Inbound hard gates (I-2 and its feeders)

| Upstream | Lands | What it gates at 0.8.15 | Class |
| --- | --- | --- | --- |
| **EXP-S** (0.8.14) | even @0.8.14 | In-library dispatcher code over kind-tagged indexes | **HARD (I-2); long pole** |
| **EXP-OBS** (0.8.8) | even @0.8.8 | Routed path must carry full EXP-OBS provenance; transparent router requires the EXPLAIN surface | **HARD (I-1 consumer)** |
| **EXP-Fr-acc** | **produced by 0.8.11 (F-11)** — never ran as float | Locus decision + mis-route matrix + route-accuracy threshold → Slice 0 ADR | feeds Slice 0 |
| **EXP-B′** | **produced by 0.8.11 (F-11)** — never ran as float | Per-intent `(alpha, pool_n, k, recency)` config tuples → Slice 5 route table | feeds Slice 5 |
| **0.8.11 pre-stage** | @0.8.11 | Per-intent config tuple seed; agent-side L2 prototype (DP-A KILL hedge) | feeds Slices 0 + 5 + KILL path |
| **#17 filter grammar** (0.8.11) | @0.8.11 | Typed `SearchFilter` / constraints surface → Slice 10 wiring | feeds Slice 10 |

### Outbound dependencies

| This release | Downstream | Class |
| --- | --- | --- |
| `search_routed` / `route_recommend` + `RouteDecision` | **0.8.17 router hardening** — per-feature config hardening, EXP-AF productization, EXP-C/D/E forks all build on the 0.8.15 dispatcher | direct build dep |
| Locus decision finalized | **0.8.17 EXP-AF productization** — the agent-feedback loop productizes on top of the settled locus and in-library surface | design dep |

### One-for-one EXP-S slip coupling

If EXP-S slips (0.8.14 delayed), 0.8.15 slips exactly one-for-one. The dispatcher cannot precede the
kind-tagged substrate. **Protect EXP-S's schedule above all other items in the program** (finding F-1,
master sequencing doc §6). The 0.8.11 agent-side prototype is the only hedge: a router ships either way
(agent-side via the prototype, or in-library via 0.8.15); EXP-S only decides the in-library locus
availability.

### Decision points consumed by this release

**DP-A — EXP-S KILL path (Slice 0 gate).** If the 0.8.14 EXP-S determinism/perf check returned KILL
(coexisting indexes not fast or deterministic enough in-product), the in-library dispatcher is off the
table. Slice 5 shifts to hardening the 0.8.11 agent-side L2 prototype into a documented, tested L1
config-carrying surface rather than building a new `dispatch.rs` module. Slices 10 (execution) and 15
(bindings) adapt accordingly. Slice 40's verification and route-accuracy gate still apply. Decision owner:
steward, at Slice 0 with the 0.8.14 EXP-S verdict in hand.

**DP-B — EXP-B′ stacks-diverge verdict (Slice 5).** If EXP-B′ showed per-intent stacks cannot unify
under one config, the Slice 5 route table carries separate hardcoded tuples per intent class. If stacks
converged, one default tuple covers most classes with per-class overrides only for known outliers. Either
way the route table _structure_ is the same; only the values differ. This verdict is already resolved by
EXP-B′; Slice 0 reads and records it.

### Architectural trade-offs — the locus decision

The locus decision is not cosmetic — it changes testability, determinism, and consumer coupling:

| Property | In-library (L2) | Agent-side only |
| --- | --- | --- |
| Determinism | Hermetic inside the Rust engine; same input → same route | Depends on the agent's own routing logic; harder to pin |
| Testability | `dispatch_route_table.rs` unit tests; end-to-end in the engine harness | Must be tested in Memex's own test suite; FathomDB cannot assert its behavior |
| Consumer coupling | Any consumer (thin or sophisticated) gets batteries-included routing | Only Memex (the current L1 driver) benefits; thin consumers have no default |
| Footprint | Rule-based, $0, CPU-only — fits invariant | Same rule-based cost; the agent-side prototype is already CPU-only |
| Override semantics | `route_hint` is validated in-engine before execution; rejection is machine-readable | Override is the agent choosing different L1 params — no protocol change |

The recommended resolution (both-layered) preserves all four advantages while keeping L1 fully drivable:
in-library L2 as the default, L1 as the explicit override path already available today.

---

## 9. Immediate next slice

**Slice 0 — ADR + locus ratification.** Three reads before writing anything:

1. Read `dev/plans/runs/STATUS-0.8.14.md` — confirm EXP-S is CLOSED and the determinism check verdict
   is GO or KILL. If KILL, stop here and surface to HITL before proceeding; the rest of the ladder adapts
   at that point.
2. Read the EXP-Fr-acc results document — extract the locus recommendation and the per-class mis-route
   cost matrix. Record the route-accuracy threshold that the Slice 40 gate must meet or beat.
3. Read the EXP-B′ results document — extract per-intent `(alpha, pool_n, k, recency)` tuples (or the
   unified tuple if stacks converged). Record the stacks-unify vs stacks-diverge verdict (DP-B).

Then write `dev/plans/0.8.15-implementation.md` containing:

- **Locus ADR** — ratify in-library / agent-side / both-layered based on the EXP-S and EXP-Fr-acc
  evidence. The recommendation from `initial-arch` §2 is both-layered; accept or override based on the
  evidence. Mark as a HITL decision — the steward must ratify before Slice 5 opens.
- **`RouteDecision` type shape** — field names and types: `intent_class: IntentClass`, `index_kind:
  String`, `retrieval_mode: RetrievalMode`, `alpha: f64`, `pool_n: usize`, `k: usize`, `recency: bool`,
  `cost_tier: CostTier`, `rationale: String`, `runner_up: Option<IntentClass>`, `confidence: f64`,
  `override_accepted: bool`. Freeze this shape before Slice 5; the Python and TS stubs depend on it.
- **Route table schema** — one row per intent class, populated from EXP-B′ results.
- **Forbidden-composition list** — explicit enumeration: map-reduce / community QFS on `needle`,
  `multi_session`, `temporal`; graph-arm-as-primary-recall on any class (graph arm default-OFF and
  refuted; exposed only for explicit known-anchor walks with `use_graph_arm=true`).
- **Governed verb names** — freeze the public names for `route_recommend` and `search_routed` (or their
  equivalents); these names go into `governed-surface-allowlist.json` at Slice 15.
- **Route-accuracy AC threshold** — the EXP-Fr-acc baseline number, written as the Slice 40 gate.
- **DP-A track declaration** — if KILL, declare the agent-side track change in this document before any
  code is written, and note which slice bodies change.

Stand up `dev/plans/runs/STATUS-0.8.15.md` with Slice 0 OPEN. After HITL ratification of the locus
decision, open Slice 5 (dispatcher core).

---

## 10. Adjacent (NOT this release's theme) — joint FathomDB↔Memex FTS experiments, paired with Memex 0.5.3

> **Scope note.** This section does **not** add to the dispatcher ladder above. It seeds a forward
> pointer so the FTS feature work lands in the right place and is not re-discovered cold.

**FTS feature work** — **multi-field + recursive-payload** FTS (≥0.9.x design-on-spec
`dev/design/0.5.1x0.8.11.2/20-multifield-fts-design.md`) **AND per-kind precision tokenizer**
(`dev/design/0.5.1x0.8.11.2/30-perkind-tokenizer-fts-design.md`) — are **JOINT FathomDB↔Memex
experiments to be addressed WITH Memex, paired with Memex 0.5.3**. **Run the on/off value-tests jointly
before building** (multi-field: `20-...md` §8; per-kind tokenizer: `30-...md` §4) — each feature is
gated by its own value test on real Memex data, and neither is greenlit. Both revive a per-kind
declaration surface that 0.6.0 removed, so each also carries a separate HITL governance sign-off.

**Memex 0.5.1 adopts the governed 0.8.x FTS as-is** (single `body` projection, one global `porter`
tokenizer) **with these as known deferred gaps.** Per the standing R-I4 / Q-B5 resolution (HITL
2026-06-30), FathomDB owes **no** FTS extension for 0.8.x; this adjacency exists for the high-bar,
value-test-gated future case (~0.8.15+) where content-modeling into `body` proves insufficient.

---

## 11. Adjacent (candidate, off-default until scoped) — op-store `latest_state` read-back verbs, paired with Memex 0.5.3

> **Scope note.** Like §10, this does **not** add to the dispatcher ladder. It is a forward pointer so
> the op-store read-back gap lands in the right place and is not re-discovered cold. **Candidate, not
> committed** — off-default until scoped at a Slice 0, same posture as the §10 FTS items.

**The gap.** The op-store has an `operational_state` table (latest-state, PK by `record_key`, upserted
at every write, NEVER FIFO-evicted) but **no governed read-back verb**. `read.collection` /
`read.mutations` read only `operational_mutations` (the append log). The server-side `operational_state`
read-back promised by `dev/adr/ADR-0.6.0-op-store-same-file.md` (§"Two collection kinds": "reads come
directly from `operational_state`") is **unfinished**. Memex therefore reconstructs latest-state with a
client-side log-collapse workaround over the append log.

**The footgun this closes.** There is no per-collection retention/compaction — `retention_json` is
persisted on `operational_collections` but never enforced. The only enforcement is a **global ~1M-row
FIFO cap over ALL `operational_mutations`** (`DEFAULT_PROVENANCE_ROW_CAP`, test-only setter). Because the
cap is per-database and cross-collection, it can evict a cold key's live latest-state append for any
consumer that derives latest-state via the log; tombstone deletes only grow the log. This is a
**theoretical** corruption risk at Memex's single-user-local scale (needs ~1M monotonic op-store rows to
trigger). The nearer-term real concern is **latency**, not corruption: the client-side workaround re-reads
the entire growing log per call — but that is mitigable consumer-side and is **not** a FathomDB ask.

**The durable fix — two additive verbs (A-1-shaped, purely additive):**

- **`read.state(collection, record_key?)`** — keyed read-back, a pure `SELECT` over the already-collapsed
  `operational_state` table.
- **latest-state list/scan** — list/scan over `operational_state`, also a pure `SELECT` over the
  collapsed table.

With both, Memex registers latest-state collections as `latest_state`, reads via the new verbs, and
**deletes its client-side collapse machinery in one localized edit**.

**Why ~0.8.15, not 0.8.11.2.** A new governed verb requires a **publishable micro** plus the full
binding / allowlist / parity-test governance — not a label-only pico. Hence it is sequenced here as a
candidate, **paired with Memex 0.5.3**.

**Memex 0.5.1 ships on the log-collapse workaround** as a known, accepted, scale-limited gap (the
corruption footgun is theoretical at single-user-local scale). HITL 2026-06-30 **DEFER** decision.

---

## 12. Adjacent (candidate, off-default until scoped) — governed touch / last-accessed surface (gap #2), paired with Memex 0.5.3

> **Scope note.** Like §10/§11, this does **not** add to the dispatcher ladder. It is a forward pointer
> so the last-accessed gap lands in the right place and is not re-discovered cold. **Candidate, not
> committed** — off-default until scoped at a Slice 0, same posture as §10/§11.

**The gap (surfaced by the Memex 0.5.1 Phase-2 swap).** `NodeRecord` is `logical_id / kind / body /
write_cursor` only. The flat pre-0.6.0 API had a `LastAccessTouchRequest` verb to record a
last-accessed timestamp; it was **removed at 0.6.0 and has zero analog on the 0.8.x governed surface** —
there is no governed touch verb and no last-accessed column or read. A consumer that wants recency-of-
access (e.g. for LRU-style memory decay or "recently used" surfacing) cannot record or read it through
the governed API.

**Memex 0.5.1 workaround (shipped, non-blocking).** Memex persists `last_accessed_at` **inside the node
`body`** and updates it on access. This works at single-user-local scale but couples access-recency into
content payload and makes it invisible to any server-side recency filter or ordering.

**Roadmap candidate (~0.8.15, paired with Memex 0.5.3).** Either a **governed touch verb** (record
last-accessed for a `logical_id` without rewriting `body`) or a **last-accessed column + governed read**
on `NodeRecord`. Off-default until scoped; a new governed verb / column requires a **publishable micro**
plus the full binding / allowlist / parity-test governance (not a label-only pico), and revives a
write-side surface — so it also carries a HITL governance sign-off. Gated on a real Memex-data value
test (does in-`body` recency actually fall short for 0.5.3's needs).

---

## 13. Adjacent (candidate, off-default until scoped) — `graph.neighbors` edge-label scoping + neighbor cap (gap #3), paired with Memex 0.5.3

> **Scope note.** Like §10/§11/§12, this does **not** add to the dispatcher ladder. Forward pointer
> only. **Candidate, not committed** — off-default until scoped at a Slice 0.

**The gap (surfaced by the Memex 0.5.1 Phase-2 swap).** `graph.neighbors` is **not label / edge-kind
scoped** and **caps at 50 neighbors**. A consumer cannot ask for "neighbors via edge-kind X" or page
past the cap, which limits graph-heavy retrieval (gathering a single entity's full relationship set when
it exceeds 50, or filtering to one relationship type).

**Memex 0.5.1 workaround (shipped, non-blocking).** Memex retrieves via `graph.neighbors` at read-parity
(takes the unscoped, capped result) and filters/assembles consumer-side. Sufficient for 0.5.1; it does
not recover edges beyond the 50-cap.

**Roadmap candidate (~0.8.15, paired with Memex 0.5.3).** **Edge-label scoping** on `graph.neighbors`
(filter by edge-kind) **plus raising / making-configurable the neighbor cap** (with a sane default and a
bounded max). Off-default until scoped; an additive signature change to a governed read verb still
requires the full binding / allowlist / parity-test governance. The graph arm is REFUTED for *recall*
fusion (M1 ΔF1 −0.0405); this candidate is about **explicit known-anchor graph walks**, not reviving the
fused graph arm — scope it as an entity-relationship-retrieval surface, not a recall lever.

---

## 14. Adjacent (committed, deferred to a later sync) — governed physical-purge verb (gap #1), GDPR-style on-disk deletion

> **Posture — DECIDED, not held (HITL 2026-06-30).** The privacy/product question is now **resolved: the
> verb WILL be added.** This is a **committed roadmap item**, no longer parked pending a decision. Like
> §11–§13 it is **off-default until scoped at a Slice 0** and does **not** add to the dispatcher ladder;
> unlike them its **timing is deferred to a later FathomDB↔Memex sync** rather than pinned to ~0.8.15.

**The gap (surfaced by the Memex 0.5.1 Phase-2 swap).** There is **no governed physical-purge verb** on
the 0.8.x surface. The flat pre-0.6.0 `engine.admin.purge_logical_id` (physical delete) has **no
analog** — the recovery-tooling purge verbs (`purge_logical_id`, `excise_source`) are
recovery-tooling-only and unreachable from the runtime SDK by design. On 0.8.x, Memex `delete` / `purge`
/ `forget` now perform **tombstone-retire ONLY**: the row is marked superseded and reads never resurface
it, **but the data stays on disk**.

**The decision (HITL 2026-06-30).** Tombstone-only is **not** sufficient for a user-facing "purge /
forget" where privacy compliance (a true right-to-erasure / GDPR-style delete) requires the bytes to
leave disk. A FathomDB **physical-purge verb** — true on-disk deletion, **distinct from the
tombstone-retire that Memex 0.5.1 ships** — **WILL be added.** This is now a committed surface, not an
open question.

**Roadmap item (committed, timing deferred to a later FathomDB↔Memex sync).** The physical-purge verb is
a **candidate alongside the ~0.8.15 op-store `read.state` work / Memex 0.5.3 window, or a later sync** —
the exact sync is deferred, not pinned. Off-default until scoped; a governed physical-delete verb
requires a **publishable micro** plus the full binding / allowlist / parity-test governance (not a
label-only pico), and crosses the deliberate **recovery-tooling-vs-runtime-surface boundary** (the
physical-delete capability today lives only in recovery tooling) — so it carries a HITL governance
sign-off when scoped. **Interim:** Memex 0.5.1 ships on **tombstone-retire** as the accepted interim
semantics, and Memex's `purge` / `forget` tests stay **xfail-pending** until the verb lands.
