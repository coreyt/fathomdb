# Initial architecture — the planner/router surface, observability, and who owns routing

**Status:** initial recommendation (steward sign-off pending) · **Date:** 2026-06-26 · **Owner:** program steward
**Companions:** `0.8.x-portfolio-features-and-experiment-tree.md` (the portfolio + experiment tree),
`0.8.x-parity-portfolio-strategy.md` (the measure-routed strategy), `~/projects/memex/dev/fathomdb/LEVERAGE-OPPORTUNITIES-LEDGER.md` (the consumer contract).
**Downstream companion:** `planner-router-psd-0.8.x.md` — the planner/router **solution design & planning input**.
**Doc relationship:** the PSD **informs the planning**; the **contract, architecture, and design realization** are often written **here** (this initial-arch doc — the stance/contract layer). The two are kept **aligned at project start — aligned, not identical**: the PSD plans *against* this contract; this doc records what the design *realizes*.
**Question this answers (HITL, 2026-06-26):** does the portfolio/router expose a surface to a personal agent?
is FathomDB transparent/observable enough that the agent (Memex) can *adjust* the search? and does FathomDB's
routing **supersede** the agent's?

---

## 0. The contract in one box

> **FathomDB's router = SQLite's query planner.** It runs **by default** (batteries-included), it is
> **observable** (`EXPLAIN`-equivalent), it is **hintable**, and it is **overridable** — and it **never hides
> what it did**. It does **not own the decision.** The **agent owns intent**; FathomDB owns **mechanism**.
> Routing is a *judgment*, so it **defaults in Fathom but defers to the agent** — it never supersedes Memex.

This is the existing Memex⇄FathomDB **Guiding Principle** ("mechanism in Fathom, judgment in the agent")
applied to routing. Nothing here contradicts it; it makes the routing/observability corollary explicit.

---

## 1. Why routing is the agent's call, not Fathom's

Routing = "given this query, which (index, retrieval, stack) wins?" That is a **judgment about intent**, and
**the agent holds the intent context FathomDB cannot see**: Memex has the goal graph, proactive nodes, the
user model, the session. FathomDB sees a string + filters. So a FathomDB-internal classifier can only ever be
a *guess at* what the agent already knows. Therefore:

- FathomDB **must not** make routing an authority that the agent has to live with.
- FathomDB **may** offer routing as a **default + a recommendation**, fully transparent and overridable.
- The portfolio's "no single path wins all axes" tension is **relocated into the router** (the 2nd design
  review flagged this) — so the router's correctness and its mis-route cost must be measured and surfaced,
  not assumed (see `EXP-Fr-acc`).

---

## 2. Two surfaces (both exposed; the agent picks its altitude)

| Surface | What | Who it's for | State |
|---|---|---|---|
| **L1 — mechanism surface** | each mechanism as a governed verb + **knobs** + **cost/latency tier**: leaf retrieval, fused RRF, CE-rerank (α/pool_n), C map-reduce, D2 coverage, **graph arm (default-OFF; refuted ×2 as a recall/multi-hop lever — opt-in only for explicit known-anchor relationship walks, not a primary routing arm)** | sophisticated agents (Memex) that **drive** | partial (see §3) |
| **L2 — router surface** | an optional in-library dispatcher: `query(intent=…)` / auto-classify → picks an L1 stack, **returns what it picked + why** | simple consumers; or as a **hint** to sophisticated ones | **unbuilt** (`Fr`) |

The **recommended target** for the open "agent-side **or** in-library dispatcher?" question is **both, layered**
— ship L2 as an overridable default, keep L1 fully drivable underneath — but this is a **recommendation, not a
settled locus**: it is **contingent on `EXP-Fr-acc`** (which owns the router-locus decision) and **`EXP-S`**
(whose KILL path forces "router stays agent-side, indexes stay eval-side"); see §8 and §9.2. Memex lives at L1
(drives) and may consult L2 (as a hint); a thin consumer lives at L2.

---

## 3. Current state — today vs designed vs unbuilt (grounded, not aspirational)

**Control (today): partial yes.** `Engine.search(query, source_type, kind, created_after, status,
rerank_depth, use_graph_arm, alpha, pool_n)` — the caller already steers filters, rerank depth, the graph
arm, and (as of EXP-0, landed 2026-06-25) the CE blend `alpha`/`pool_n`. So Memex **can** adjust the search.
(The `use_graph_arm` knob is **default-OFF / refuted ×2** as a recall/multi-hop lever — exposed for explicit
known-anchor walks, not as a primary routing arm; see the §2 L1 note.)

**Observability (today): weak — the real gap.** Retrieval is largely a black box at the result level:
- `SearchHit` returned the **blended `score` only**; EXP-0 added **`ce_score`** (= `sigmoid(ce_logit)`) — the
  **first retrieval-observability field**, now shipped.
- There is **no per-arm provenance** (did this hit come from vector-ANN, FTS/BM25, or the graph arm?), **no
  score breakdown** (ce vs rrf contribution), **no `EXPLAIN`-for-retrieval**. `EXPLAIN QUERY PLAN` exists
  only as a **graph-BFS test seam** (`explain_graph_neighbors_for_test`), not a caller surface.
- `TraceReport`/`source_id` are **write-lineage** provenance (which source doc a row came from), **not**
  retrieval explainability. `counters()` / `set_profiling()` / `open_report()` are **operational** (timings,
  embedder events), not "why did this result rank here."

**Router (today): does not exist.** `Fr` is unbuilt — which means **today the agent already makes every
routing decision** implicitly, by choosing search params. So today FathomDB does **not** supersede Memex;
Memex drives. The work below is about keeping it that way *when* a router exists, and giving Memex the eyes
to drive well.

---

## 4. The three questions, answered

**Q1 — Does the portfolio/router provide a surface to a personal agent?**
Yes at L1 (verbs + knobs + cost tiers; partially shipped), and intended at L2 (the router, unbuilt). For a
personal agent the operative surface is **L1**; L2 is a convenience to be added, not the only door.

**Q2 — Is FathomDB transparent/observable so the agent can adjust the search?**
**Control yes; observability not yet.** Memex can change inputs but can't yet *see why* a result ranked.
To adjust intelligently it needs **retrieval `EXPLAIN`**: per-hit arm-provenance + score-breakdown + (under a
router) "what would route here and why." `ce_score` is the first brick; this doc specifies the rest (§6).

**Q3 — Does FathomDB routing supersede Memex?**
**No — by design it must not.** Query-planner model: default, observable, hintable, overridable; the agent
owns intent. The only case FathomDB "decides" is the degenerate *no-mode-given, give-me-the-default* call —
and even then it is transparent and overridable.

---

## 5. The router contract (spec)

When `Fr` (L2) is built, it MUST satisfy:

1. **Default, not mandate.** A bare `search()` works without any routing input (today's behavior preserved).
2. **Recommendation API.** The agent can ask "what would you route this to?" and get `(stack, rationale,
   cost_tier, confidence)` **without executing** — so Memex can override before paying.
3. **Override / hint.** The agent can force a stack (`mode=needle|global|…` or an explicit (index, retrieval,
   α, pool_n, MMR, recency) tuple). The router's pick is a *suggestion* the agent can replace — the SQLite
   `INDEXED BY` analogy.
4. **Transparent.** Every routed call returns **what it picked and why** (the same `EXPLAIN` payload as §6).
   No silent routing.
5. **Intent is the agent's.** The router classifies intent **only as a fallback**. Preferred: the agent
   passes intent, **or** Fathom **calls back to Memex** for the intent label (the provider-callback pattern
   already used for community summaries — judgment to Memex, mechanism stays in Fathom).
6. **Cost-honest.** The selected stack's **latency/cost tier** is surfaced (CPU / GPU / local-LLM / net-LLM),
   so a cost-aware agent can veto an expensive route. This requires per-stack cost numbers (an output of
   `EXP-A/B′/C`).
7. **Locus is explicit.** Whether the dispatcher runs agent-side or in-library is a declared choice with
   different determinism/latency/testability — decided as part of `EXP-Fr-acc`, not by default.
8. **Agent relevance feedback is an extension, not the initial contract.** The downstream PSD makes the
   *agent-as-relevance-signal* loop (agent-supplied relevance labels / replan hooks feeding route planning) a
   first-class planning + reward signal. That is an **`EXP-OBS`-gated `EXP-Fr-acc` / `EXP-AF` extension**, **not**
   part of the initial router contract **unless measured net-positive** (cf. `EXP-AF`'s KILL path: keep it an
   overridable L1 opt-in, not the default loop). The MUSTs 1–7 above stand without it; the callback-for-intent
   (#5) is the only agent→router channel the initial contract assumes.

**Config-carrying, not just index-picking.** Because one function serves many features with *conflicting*
configs (`ce_rerank` wants α=0.3/narrow for F1 but a wider pool for F2; `Engine.embed` serves opposite
granularities), the router selects the **(index, retrieval, α, pool_n, MMR, recency) tuple** — and a config
chosen for feature X must not regress feature Y (the §3-coupling hazard, guarded by `EXP-B′.5`).

---

## 6. The observability surface (spec) — retrieval `EXPLAIN`

The minimum that makes "Memex can adjust the search for a better outcome" real:

- **Per-hit provenance:** which arm(s) surfaced this hit (vector-ANN / FTS-BM25 / graph), its rank in each.
- **Per-hit score breakdown:** `rrf_norm`, `ce_score` (shipped), the blended `score`, and which filters (if
  any) excluded candidates.
- **Query-level trace:** k, pool_n, α, MMR, the embedder identity, and timings (reuse `counters()`/profiling).
- **Router rationale (when L2 exists):** the chosen stack, the runner-up, the confidence, the cost tier.
- **Shape:** opt-in (`search(..., explain=True)` → an `Explanation` object), zero-cost when off, so the hot
  path is unaffected. Generalize the existing graph-`EXPLAIN` seam + `TraceReport` rather than inventing new
  machinery.

This is what lets Memex close the loop: observe *why* a result ranked → adjust knobs (α, pool_n, k, filters,
mode) or override the route → re-query. Without it, "adjust the search" is blind.

---

## 7. How Memex uses this (three modes, all agent-authoritative)

1. **Drive (L1).** Memex picks the mechanism + knobs directly from its own intent (goal graph). The router is
   bypassed. *This is the default for a sophisticated agent.*
2. **Recommend-and-override (L1+L2).** Memex asks the router for a recommendation, inspects the rationale +
   cost tier, and either accepts or replaces it.
3. **Callback-for-intent.** Fathom executes mechanism but calls back to Memex for the intent label / summary
   judgment (the established provider-callback pattern). Judgment stays with Memex; mechanism with Fathom.

In all three, **Memex is the authority**; FathomDB never overrides the agent's choice.

---

## 8. How this lands in the experiment tree

This contract adds / formalizes nodes in `0.8.x-portfolio-features-and-experiment-tree.md`:

- **EXP-OBS (new, $0 engineering):** the retrieval-`EXPLAIN` surface of §6 (per-arm provenance + score
  breakdown + opt-in `explain=True`). `ce_score` (EXP-0) is its first increment. **Prerequisite for any
  agent to "adjust for a better outcome," and for the router to be transparent.**
- **EXP-Fr-acc (already in the tree):** now also owns the **router-locus decision** and the **mis-route cost
  matrix** + the **recommendation/override/callback** API shape from §5.
- **EXP-Fr (already in the tree):** builds the dispatcher to the §5 contract, over the `EXP-S` kind-tagged
  substrate, carrying per-feature stack configs.
- **EXP-S (already in the tree):** the substrate the router needs (kind-tagged coexisting indexes) — its KILL
  path (determinism/perf breaks) is precisely "router stays agent-side, indexes stay eval-side."

Suggested sequencing: **EXP-OBS rides alongside EXP-A/B′** (it's the same retrieval path, and Memex needs it
to drive the recall/stacking work), **before** EXP-Fr — you cannot ship a transparent router without the
transparency surface.

---

## 9. Open decisions for the steward

1. **Adopt the query-planner stance** (router = transparent/hintable/overridable default; agent owns intent;
   Fathom never supersedes)? — recommended; aligns with the existing Guiding Principle.
2. **Router locus** — agent-side, in-library, or both-layered (recommended)? Gates `EXP-Fr` design.
3. **Add `EXP-OBS`** to the experiment tree and an observability requirement to the L1 surface? — recommended.
4. **Ledger reflection** — capture the router/observability contract as a Memex⇄FathomDB item (it's a consumer
   contract; Memex is the driving consumer). Candidate: a new OPP, or fold into OPP-7 (CE-verb+score, the
   observability seed) / OPP-1 (multi-hop routing). Steward's call whether to open it.

---

## 10. One honest caveat

Almost all of this is **design intent, not built**. Today FathomDB is *steerable* (L1 knobs, incl. the
just-landed α/pool_n) but only *weakly observable* (`ce_score` is the lone retrieval-explainability field),
and **there is no router at all** — so the "Fathom doesn't supersede Memex" property holds today *trivially*
(the agent does all routing). The contract above is what keeps that property true **once a router exists**,
and what gives Memex the observability to drive well in the meantime.
