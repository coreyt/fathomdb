---
name: g0-identity-scope-logical-id-alone
description: "0.8.0 substrate gate SIGNED 2026-06-05 — active-uniqueness is logical_id ALONE (both canonical_nodes + canonical_edges), reversing the landed compound (logical_id,kind); executed as reserved-gap Slice 31."
metadata: 
  node_type: memory
  type: project
  originSessionId: 7aacd556-dd31-405f-8249-33744be5eb59
---

**HITL SIGNED 2026-06-05 (substrate gate).** FathomDB's G0 canonical-identity active-uniqueness scope
is **`logical_id` ALONE**, on **both** `canonical_nodes` AND `canonical_edges` (uniform, not asymmetric).
This **reverses** the compound `(logical_id, kind)` key that landed in Slice 15 (schema step 12) — which
was a silent regression from v0.5.x (v0.5.6 indexed `logical_id` alone) that was propagated across docs
but never argued. **Migration = AMEND step-12 in place** (no `SCHEMA_VERSION` bump; local v12 DBs are
disposable and keep the old compound index until rebuilt).

**Why:** the compound key caused a **silent identity-fork bug** — re-ingesting one `logical_id` with a
changed `kind` made supersession (`WHERE logical_id=? AND kind=?`) match nothing → a second active row →
entity-resolution forks instead of superseding. It also made Slice 30 `read.get`/`get_many`
(logical_id-alone lookup) lossy+nondeterministic (codex §9 [P2]), and contradicted Slice 20/G8 which
already probes endpoints by `logical_id` alone. For EDGES, `kind` is the relationship type but edge
`logical_id` is opaque/caller-provided (not derived from `(from,to)`) — so the compound key bought edges
no real multi-relationship capability, only forking. Distinct relationships → distinct `logical_id`s.
Two independent reviews converged (the Slice 30 agent + a high-effort adversarial codex consult). Full
verdict: `dev/plans/runs/0.8.0-identity-scope-codex-consult-20260605T021802Z.md`.

**How to apply (reserved-gap Slice 31 scope — done by an AGENT, not the orchestrator):** amend the two
partial-unique indexes (`fathomdb-schema/src/lib.rs:300-303`) to `ON (logical_id) WHERE superseded_at IS
NULL`; remove `AND kind = ?` from the node + edge supersession predicates in `commit_batch`
(`fathomdb-engine/src/lib.rs:~5998/~6031`); change G8 in-batch supersession tracking key from
`(logical_id,kind)` → `logical_id`; **invert** `s15_partial_unique_active_index_rejects_two_active_versions`
(different kind + same `logical_id` is now REJECTED) + add node+edge kind-change supersession tests; update
the schema migrations test's expected index DDL; add **"Decision 5 — identity scope = `logical_id`"** to
the signed substrate ADR and amend all propagated `(logical_id,kind)` active-identity language (parent
ADR Q2, roadmap, plan, slice memos) → `logical_id`; also resolve the parent-ADR↔code drift (retrieval ADR
Q4 says edge supersession is "schema-only/nodes-only," but code supersedes edges). Slice 30's [P2] then
closes with **zero read-API change**. Relates to [[fathomdb-080-plan-approved]],
[[fathomdb-v05-graph-lineage]].

**STATUS — DONE + GRAPH-LENS-CONFIRMED 2026-06-05.** Slice 31 executed + CLOSED (`main`@`b4e90c8`, codex §9
clean PASS); Slice 30 [P2] closed with zero read-API change. The deliberately-reserved EDGE-half concern
(could a multigraph make multiple active `kind`s between one endpoint pair legitimate?) was then evaluated
under the graph lens in **Slice 32** (graph-model ADR, ACCEPTED, `main`@`e1827c4`): the verdict **CONFIRMS
`logical_id`-alone holds for the graph aspect too in 0.8.0** (active multigraph is already representable;
edge addressing stays opaque-id; fact-on-edge enrichment is reserved-additive, not built) — it did NOT
reopen compound-for-edges. So the identity model is now settled through BOTH the point-lookup lens (31) and
the graph lens (32).
