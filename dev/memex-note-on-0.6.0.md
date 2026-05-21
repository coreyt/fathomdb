# Memex note on 0.6.0

The key conclusion is:

Memex cannot adopt FathomDB 0.6.0 as a drop-in replacement for the current FathomStore without losing functionality.
It can use 0.6.0 without product-level feature loss if FathomDB stops being Memex’s primary structured store and becomes a retrieval/index subsystem behind a
Memex-owned store.

The reason is the public 0.6.0 surface is intentionally narrow: Engine.open, engine.write, engine.search, engine.close, and admin.configure, with search
returning SearchResult(..., results: list[str]) rather than typed nodes, graph traversals, or operational rows
/home/coreyt/projects/fathomdb/docs/reference/python-api.md:24,
/home/coreyt/projects/fathomdb/docs/reference/python-api.md:48,
/home/coreyt/projects/fathomdb/docs/reference/python-api.md:58.
Memex today depends on structured CRUD, filters, graph walks, operational collections, provenance reads, and direct query-builder/admin internals across goals,
scheduler, meetings, knowledge, and world-model data
docs/developer/fathom-storage.md:100,
docs/developer/fathom-storage.md:134,
docs/developer/fathom-storage.md:189,
docs/developer/fathom-storage.md:214,
src/memex/store.py:51.

How to use 0.6.0 without functionality loss

1. Make a Memex-owned store authoritative again.
   Use SQLite or another local app store for all structured state:
   Goal, scheduler runs, notifications, meetings, world-model records, settings, session context, audit log, intake log, source registry, and relationship
   tables. Those require point lookup, filtered list, updates, cascades, and traversal that 0.6.0 does not expose publicly.
2. Use FathomDB only for retrieval/index workloads.
   Build a narrow adapter around only:

- Engine.open
- engine.write
- engine.search
- engine.close
- admin.configure

1. Materialize Memex “search documents” into FathomDB canonical rows.
   For each searchable Memex record, write a retrieval document that includes:

- stable document_id
- family / record type
- authoritative record id
- source_ref
- title / summary / body / denormalized searchable text

This gives Memex a way to parse search().results and map each hit back to the authoritative store.

1. Re-rank and hydrate in Memex.
   Treat FathomDB search as candidate generation only.
   After engine.search(query), Memex should:

- parse returned strings
- dedupe by document_id
- load the real objects from the authoritative store
- run its current ranking / enrichment / routing logic locally

That is viable because Memex already uses position-based synthetic scoring on some FTS paths rather than engine-native scores
src/memex/fathom_facade.py:360.

1. Move projection / recovery work to admin.configure and CLI.
   Current Memex behavior like open-time vector/FTS setup, regeneration, source tracing, and rebuilds should become:

- admin.configure(...) for schema/embedder/projection config
- fathomdb doctor ... for diagnostics
- fathomdb recover --accept-data-loss --rebuild-projections
- fathomdb recover --accept-data-loss --rebuild-vec0
- fathomdb recover --accept-data-loss --excise-source <id>

That matches the public 0.6.0 operator model
/home/coreyt/projects/fathomdb/docs/concepts/index.md:74,
/home/coreyt/projects/fathomdb/docs/reference/cli.md:43.

What this preserves

- Knowledge search
- Conversation search
- World-model candidate retrieval
- Hybrid retrieval as a product capability
- Recovery / rebuild workflows
- Existing Memex domain behavior over goals, meetings, scheduler, provenance, and settings

What cannot be preserved if FathomDB 0.6.0 remains the only store

- direct graph-backed CRUD/traversal as Memex uses today
- operational collection reads/writes as an app-facing API
- by-id loads and filtered list\_\* queries over Memex models
- logical-id purge/restore workflows through the SDK

Those are outside the public 0.6.0 surface, and logical-id verbs are explicitly deferred to 0.7.x
/home/coreyt/projects/fathomdb/docs/release-notes/0.6.0.md:98.

Practical verdict

If the requirement is “use only FathomDB 0.6.0 and keep the current single-store Memex architecture,” the answer is no.
If the requirement is “ship Memex on top of the public 0.6.0 interfaces without user-visible feature loss,” the answer is yes, but only with a two-layer design:

- Memex-owned authoritative store for structured/domain state
- FathomDB 0.6.0 as retrieval/index engine plus operator CLI

---

have an architecture subagent familiar with fathomdb as well as ~/projects/memex/ read the note from Memex that it effectively cannot use FathomDB  
 (dev/memex-note-on-0.6.0.md). see iffathomdb is now too limited, or if additional documentation / knowledge about FathomDB can help Memex see opportunity. If  
 FathomDB is missing key functionality, identify it.

Listed 1 directory (ctrl+o to expand)

● Both exist. Spawn architecture-inspector.

● architecture-inspector(Memex-vs-fathomdb fit analysis)
⎿  Done (39 tool uses · 135.5k tokens · 2m 18s)  
 (ctrl+o to expand)

● Architecture-inspector report complete. Memex's note is internally consistent with fathomdb's actual surface. Net verdict:

Net verdict

0.6.0 IS insufficient for memex as single-store drop-in. Not a documentation gap — architectural. The 5-verb surface lock (AC-057a,  
 dev/design/bindings.md:30-46) deliberately excludes by-id reads, filtered lists, JSON-property filters, graph traversal, and op-store reads — all of which  
 memex's 9 sub-repositories depend on.

0.6.0 IS sufficient for memex IF they adopt the two-layer design they themselves proposed (memex-owned authoritative SQLite + fathomdb as retrieval-only). The  
 note's verdict ("ship Memex on top of public 0.6.0 with a Memex-owned authoritative store + FathomDB-as-retrieval-only: yes") is consistent with the evidence.

Missing functionality (cited)

┌─────────────────────────────────────────────┬─────────────────────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ Gap │ Class │ Where it lives / doesn't │
├─────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
│ SearchResult.results: list[str] too thin │ GENUINE │ types.py:35-40; no logical*id column on canonical_nodes (lib.rs:3439). │
│ │ │ Blocked on 0.7.x identity substrate. │
├─────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
│ No by-id read / filtered list / │ GENUINE (biggest) │ 5-verb invariant AC-057a; engine doesn't implement reads either │
│ nodes(kind).filter*\*() DSL │ │ │
├─────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
│ Op-store reads │ GENUINE — engine has data, SDK │ operational_state + operational_mutations tables landed; PyO3 only │
│ (trace_operational_collection) │ has no read path │ translates writes (fathomdb-py/src/lib.rs:512-520) │
├─────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
│ Graph traversal │ GENUINE │ Edge writes work; no walker code anywhere in engine │
├─────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
│ Admin/recovery as in-process API │ DEFERRED (correctly per spec) │ Moved to CLI Phase 10a; memex must shell out for safe-export + recovery │
├─────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
│ Logical-id verbs │ DEFERRED to 0.7.x │ --excise-source is the workaround │
├─────────────────────────────────────────────┼─────────────────────────────────┼──────────────────────────────────────────────────────────────────────────┤
│ Open report │ DEFERRED to 0.6.1 │ Already queued │
│ │ (12-TX-OPENREPORT) │ │
└─────────────────────────────────────────────┴─────────────────────────────────┴──────────────────────────────────────────────────────────────────────────┘

Top 5 recommended actions (ranked)

1. doc: docs/concepts/when-to-use.md — "0.6.0 is a retrieval/index engine, not a general document store." Prevents next memex-shaped misunderstandings.
2. slice 0.6.1-OPSTORE-READ: add admin.read*collection(engine, *, name, record*key=None, filter=None). Data is on disk; only read surface missing. Does NOT
   break 5-verb invariant (adds to admin.* namespace which already has configure). ~1-2 weeks.
3. HITL decision: structured SearchHit { logical_id, kind, score, body } — wait for 0.7.x logical-id substrate or ship partial in 0.6.1? Inspector recommends
   wait (id is the value; without it, partial carrier risks two binding-coordination amendments within months).
4. doc: docs/concepts/architecture-patterns.md — Pattern A sole-substrate vs Pattern B retrieval-subsystem with worked examples (cite memex's note as Pattern B
   canonical example).
5. flag 12-TX-OPENREPORT to memex so they don't build a parallel "did migration run?" log-scraper.

Open HITL questions

- Is the 5-verb surface invariant negotiable for 0.7.x to support memex-style structured-CRUD clients?
- Op-store reads under admin.\* (no invariant change) OR new top-level read verb (invariant change)?
- Does fathomdb want memex as a 0.6.0 reference client or 0.7.x reference client?
- Has memex committed to the two-layer design, or is the note still exploratory?

Subagent ID acba633f684aa4e0c available for follow-up via SendMessage if you want deeper drill-down on any concern.

Want me to:

- (a) Land Action 1 + Action 4 docs now (orchestrator-direct, ~30min)?
- (b) Spawn 0.6.1-OPSTORE-READ slice?
- (c) Surface HITL questions back to memex first?
