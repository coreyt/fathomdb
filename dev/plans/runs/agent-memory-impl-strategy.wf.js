export const meta = {
  name: 'fathomdb-agent-memory-impl-strategy',
  description: 'Ground a concrete, source-accurate implementation strategy for the G0–G12 agent-memory gaps on FathomDB head-of-main (0.7.2): map real engine mechanisms, design each gap against them, adversarially verify against invariants, synthesize a sequenced plan.',
  phases: [
    { title: 'Map', detail: 'parallel readers map the real 0.7.2 mechanisms each feature hooks into' },
    { title: 'Design', detail: 'one grounded implementation design per gap (G0–G12)' },
    { title: 'Verify', detail: 'adversarial re-read: does each design hold against the actual code + invariants' },
    { title: 'Synthesize', detail: 'sequenced, slice-shaped implementation plan' },
  ],
}

const REPO = '/home/coreyt/projects/fathomdb'

// ---- Schemas -------------------------------------------------------------

const MAP_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['subsystem', 'mechanisms', 'invariants', 'extensionPoints'],
  properties: {
    subsystem: { type: 'string' },
    mechanisms: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['name', 'file', 'lines', 'whatItDoes'],
        properties: {
          name: { type: 'string' },
          file: { type: 'string' },
          lines: { type: 'string' },
          whatItDoes: { type: 'string' },
        },
      },
    },
    extensionPoints: {
      type: 'array',
      description: 'Concrete places new behavior could hook in, with file:line',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['where', 'howToExtend'],
        properties: { where: { type: 'string' }, howToExtend: { type: 'string' } },
      },
    },
    invariants: {
      type: 'array',
      description: 'Hard constraints this subsystem imposes on any change',
      items: { type: 'string' },
    },
    gotchas: { type: 'string', description: 'Sharp edges / non-obvious facts a designer must know' },
  },
}

const DESIGN_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['gap', 'title', 'approach', 'touchPoints', 'migration', 'surfaceChange', 'dependsOn', 'invariantRisks', 'effort', 'confidence'],
  properties: {
    gap: { type: 'string' },
    title: { type: 'string' },
    approach: { type: 'string', description: 'The concrete engineering approach, grounded in real symbols. 1-3 paragraphs.' },
    touchPoints: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['file', 'symbol', 'change'],
        properties: { file: { type: 'string' }, symbol: { type: 'string' }, change: { type: 'string' } },
      },
    },
    migration: { type: ['string', 'null'], description: 'New schema migration step needed (additive-only per policy), or null' },
    surfaceChange: { type: 'string', description: 'SDK/CLI surface impact and AC-057a (five-verb) interaction' },
    dependsOn: { type: 'array', items: { type: 'string' } },
    invariantRisks: { type: 'array', items: { type: 'string' } },
    effort: { type: 'string', enum: ['XS', 'S', 'M', 'L'] },
    confidence: { type: 'string', enum: ['high', 'med', 'low'] },
    openQuestions: { type: 'array', items: { type: 'string' } },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['gap', 'verdict', 'refutations', 'citationCheck', 'correctedApproach'],
  properties: {
    gap: { type: 'string' },
    verdict: { type: 'string', enum: ['sound', 'needs-fix', 'unsound'] },
    refutations: {
      type: 'array',
      description: 'Concrete ways the design is wrong, violates an invariant, or misreads the code. Empty if sound.',
      items: { type: 'string' },
    },
    citationCheck: {
      type: 'array',
      description: 'Each cited symbol/line the design relies on, verified real or not',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['claim', 'real', 'note'],
        properties: { claim: { type: 'string' }, real: { type: 'boolean' }, note: { type: 'string' } },
      },
    },
    correctedApproach: { type: 'string', description: 'If needs-fix/unsound, the corrected design. Empty string if sound.' },
    revisedEffort: { type: 'string', enum: ['XS', 'S', 'M', 'L', 'unchanged'] },
  },
}

// ---- The gaps (defined in dev/design/agent-memory-fit.md §4 + §8c) -------

const GAPS = [
  { id: 'G0', title: 'Record identity: logical_id + idempotent upsert + per-row id in receipt' },
  { id: 'G1', title: 'Structured search hits: search() returns {id, kind, score, body} not list[str]' },
  { id: 'G2', title: 'By-id read: get(id) / get_many([...])' },
  { id: 'G3', title: 'Operational read: admin.read_collection(name, key?, filter?)' },
  { id: 'G4', title: 'Filtered list: list(kind, *, filter?, limit) — needs kind index + small filter grammar' },
  { id: 'G5', title: 'Bounded graph walk: neighbors(id, *, edge_type?, depth=1)' },
  { id: 'G6', title: 'Retrieve + expand: search(query, *, expand=1, filter?) — composition of G1+G4+G5' },
  { id: 'G7', title: 'History read: history(id) / mutations(collection)' },
  { id: 'G8', title: 'Edge referential integrity: validate/flag dangling from_id/to_id at write' },
  { id: 'G9', title: 'Real fusion + rerank: RRF over vector+text branches, optional MMR/recency reweight' },
  { id: 'G10', title: 'Vector metadata columns: filterable KNN (kind, time, status) in one statement' },
  { id: 'G11', title: 'Bi-temporal edges: valid/invalid + created/expired; invalidate-not-delete' },
  { id: 'G12', title: 'Recency/importance signals: per-record timestamps + optional importance score for rerank' },
]

// ---- Phase 1: Map the real mechanisms ------------------------------------

phase('Map')
log('Mapping 0.7.2 engine mechanisms (head of main) — 6 subsystems in parallel')

const SUBSYSTEMS = [
  {
    key: 'write-path',
    prompt: `Read ${REPO}/src/rust/crates/fathomdb-engine/src/lib.rs and ${REPO}/src/rust/crates/fathomdb-py/src/lib.rs.
Map the WRITE PATH precisely: Engine::write / write_inner / commit_batch; the PreparedWrite enum and its four variants (Node, Edge, OpStore, AdminSchema) and their exact fields; validate_batch; per-row write_cursor assignment; how nodes/edges/op-store rows are INSERTed; how the binding (translate_node/translate_edge/translate_op_store) maps Python dicts to PreparedWrite. Note the single-writer-thread invariant and what a writer-side change must respect. Cite file:line for every mechanism.`,
  },
  {
    key: 'schema-migrations',
    prompt: `Read ${REPO}/src/rust/crates/fathomdb-schema/src/lib.rs and the migration files under ${REPO}/src/rust/crates/fathomdb-schema/migrations/.
Map the SCHEMA + MIGRATION system: SCHEMA_VERSION, the MIGRATIONS registry, apply_one, migrate_with_event_sink, the contiguity check, check_migration_accretion (the accretion guard — what it forbids and the exemption marker), CANONICAL_TABLES, and the exact current columns of canonical_nodes, canonical_edges, operational_*, the vec0/FTS5 virtual tables, and _fathomdb_* metadata tables. State the no-data-migration policy (feedback_no_data_migration) and the additive-ALTER pattern (migrations 8 and 10 are examples). Explain exactly how one would add a new migration step. Cite file:line.`,
  },
  {
    key: 'search-fusion',
    prompt: `Read ${REPO}/src/rust/crates/fathomdb-engine/src/lib.rs (search path) and ${REPO}/src/rust/crates/fathomdb-query/src/lib.rs.
Map the RETRIEVAL path precisely: Engine::search / search_inner; query compilation (CompiledQuery); how the embedder produces query_vector + query_vector_bin and mean-centering; read_search_in_tx — the vector branch (binary-quant MATCH prefilter to TOP_K_BIT_CANDIDATES then vec_distance_l2 rerank to final_limit) and the text branch (FTS5 MATCH over search_index); how the two branches are combined (dedup-by-body via BTreeSet, vector-first); SearchResult shape; SoftFallback; the reader pool dispatch. Identify EXACTLY where RRF fusion, a rerank hook, or a metadata filter predicate would be inserted, and what data (rowid/write_cursor/kind/score) is in hand at each point. Cite file:line.`,
  },
  {
    key: 'projection-runtime',
    prompt: `Read ${REPO}/src/rust/crates/fathomdb-engine/src/lib.rs (projection runtime).
Map the PROJECTION subsystem: projection_dispatcher_loop, projection_worker_loop, _fathomdb_projection_state / _fathomdb_projection_terminal / _fathomdb_vector_kinds / _fathomdb_vector_rows; how a vector-indexed kind gets embedded and inserted into vector_default (the vec0 partition), including embedding + embedding_bin (binary quant) and source_type/kind/created_at columns; ensure_vector_partition / ensure_vector_partition_pack1; how projection freeze works (set_frozen) and excise_source. Note what already lands in vec0 today (so we know which metadata columns already exist). Cite file:line.`,
  },
  {
    key: 'op-store',
    prompt: `Read ${REPO}/src/rust/crates/fathomdb-engine/src/lib.rs (op-store commit + provenance/excise + any read helpers) and ${REPO}/src/rust/crates/fathomdb-schema/migrations/004_op_store.sql.
Map the OPERATIONAL STORE: operational_collections (append_only_log vs latest_state, schema_json validation), operational_mutations (append-only), operational_state (upsert PK collection+record_key); the commit_batch op-store INSERT/UPSERT paths; JSON-schema validation timing; the bootstrap projection_failures collection; trace_source_ref and excise_source (read-shaped queries that already exist); any *_for_test read seams over op tables. Identify exactly where a read path (admin.read_collection) would hook and what SQL it would run. Cite file:line.`,
  },
  {
    key: 'bindings-surface',
    prompt: `Read ${REPO}/src/rust/crates/fathomdb-py/src/lib.rs, ${REPO}/src/rust/crates/fathomdb-napi/src/lib.rs, ${REPO}/src/rust/crates/fathomdb-cli/src/lib.rs, ${REPO}/dev/design/bindings.md (§1 surface-set parity), and ${REPO}/docs/reference/python-api.md.
Map the SURFACE + BINDINGS: the five-verb invariant (AC-057a) and exactly what it forbids; the admin.* namespace (admin.configure) and whether reads could live there without widening the five-verb top-level set; how results cross the FFI boundary (marshalling owned typed rows to Python dataclasses / TS objects) per bindings.md §4; the CLI doctor/recover two-root operator surface and whether app-facing reads are allowed there; how a STRUCTURED search hit or a new read verb would be threaded through PyO3 + napi consistently (SDK parity). Cite file:line.`,
  },
]

const maps = (await parallel(SUBSYSTEMS.map((s) => () =>
  agent(s.prompt, { label: `map:${s.key}`, phase: 'Map', schema: MAP_SCHEMA, agentType: 'Explore' })
))).filter(Boolean)

// Build one authoritative mechanism-map context string for the designers.
const mapContext = maps.map((m) => {
  const mech = (m.mechanisms || []).map((x) => `  - ${x.name} (${x.file}:${x.lines}): ${x.whatItDoes}`).join('\n')
  const ext = (m.extensionPoints || []).map((x) => `  - EXTEND @ ${x.where}: ${x.howToExtend}`).join('\n')
  const inv = (m.invariants || []).map((x) => `  - INVARIANT: ${x}`).join('\n')
  return `### Subsystem: ${m.subsystem}\nMechanisms:\n${mech}\nExtension points:\n${ext}\nInvariants:\n${inv}\nGotchas: ${m.gotchas || ''}`
}).join('\n\n')

log(`Mapped ${maps.length} subsystems → designing ${GAPS.length} gaps (design→verify pipelined)`)

// ---- Phases 2+3: Design each gap, then adversarially verify (pipelined) ---

const SHARED = `You are designing/verifying an implementation strategy for FathomDB at head-of-main (0.7.2). Engineering must be GROUNDED in the real code — cite real symbols and file:line, never invent. Respect these hard invariants: single-writer-thread engine; the five-verb SDK surface invariant AC-057a (reads may only be added under admin.* or via the existing search verb's args, NOT as new top-level SDK verbs, unless explicitly flagged as requiring a HITL invariant change); the migration-accretion guard (CREATE TABLE / ADD COLUMN without a matching DROP requires the exemption marker) and the no-data-migration policy (additive ALTER only, legacy rows back-fill to NULL); sqlite-vec vec0 is brute-force (no ANN) so filters should prune via partition/metadata columns; behavior-compat for any change to search() result ordering. The gap ladder is defined in ${REPO}/dev/design/agent-memory-fit.md §4 and §8c; the classification ADR is ${REPO}/dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md. Prefer leveraging mechanisms that ALREADY EXIST in 0.7.2 over new infrastructure.

AUTHORITATIVE MECHANISM MAP (0.7.2, from grounded readers):
${mapContext}`

const designed = await pipeline(
  GAPS,
  // Stage 1 — design
  (gap) => agent(
    `${SHARED}

Design the implementation of gap ${gap.id}: ${gap.title}.

Produce a concrete, source-accurate design: the engineering approach (leveraging existing 0.7.2 mechanisms wherever possible), exact touch-points (file + symbol + change), whether a new additive migration is required (and its shape), the SDK/CLI surface impact and its AC-057a interaction, dependencies on other gaps, invariant risks, effort (XS/S/M/L), and your confidence. Read the actual files named in the mechanism map to ground every claim. If the gap is best implemented by composing other gaps, say so explicitly.`,
    { label: `design:${gap.id}`, phase: 'Design', schema: DESIGN_SCHEMA }
  ),
  // Stage 2 — adversarial verify
  (design, gap) => agent(
    `${SHARED}

A design was produced for gap ${gap.id} (${gap.title}). ADVERSARIALLY VERIFY it. Re-read the cited files/symbols and try to REFUTE the design: (a) are the cited symbols/lines real and used as claimed? (b) does it violate single-writer / AC-057a / the accretion guard / no-data-migration / vec0-brute-force / behavior-compat? (c) does it under- or over-state effort or dependencies? (d) is there a simpler way using an existing 0.7.2 mechanism? Default to skepticism: if a claim cannot be confirmed against the source, mark it not-real. If the design needs changes, provide the corrected approach.

DESIGN UNDER REVIEW:
${JSON.stringify(design, null, 2)}`,
    { label: `verify:${gap.id}`, phase: 'Verify', schema: VERDICT_SCHEMA }
  ).then((verdict) => ({ design, verdict }))
)

const results = designed.filter(Boolean)

// ---- Phase 4: Synthesize the sequenced plan ------------------------------

phase('Synthesize')
log(`Synthesizing plan from ${results.length} verified gap designs`)

const dossier = results.map(({ design, verdict }) => {
  return `## ${design.gap} — ${design.title}
VERDICT: ${verdict.verdict}${verdict.revisedEffort && verdict.revisedEffort !== 'unchanged' ? ` (effort revised → ${verdict.revisedEffort})` : ''}
Approach: ${design.approach}
Touch-points: ${(design.touchPoints || []).map((t) => `${t.file} :: ${t.symbol} — ${t.change}`).join(' | ')}
Migration: ${design.migration || 'none'}
Surface/AC-057a: ${design.surfaceChange}
Depends on: ${(design.dependsOn || []).join(', ') || 'none'}
Invariant risks: ${(design.invariantRisks || []).join('; ') || 'none'}
Effort: ${design.effort} | Confidence: ${design.confidence}
Refutations: ${(verdict.refutations || []).join(' || ') || 'none'}
Corrected approach: ${verdict.correctedApproach || '(design stands as written)'}`
}).join('\n\n')

const plan = await agent(
  `${SHARED}

Below is the full dossier of verified per-gap designs (design + adversarial verdict) for implementing the G0–G12 agent-memory features on FathomDB 0.7.2. Synthesize a single, sequenced IMPLEMENTATION STRATEGY document in Markdown that a FathomDB engineer could act on.

Requirements for the document:
1. Lead with the core engineering thesis: which gaps are pure-additive over existing 0.7.2 mechanisms vs which need new schema/surface, and what the keystone dependency (G0 identity) unlocks.
2. A dependency-ordered build sequence, grouped into FathomDB-style slices/PRs (the repo uses slice prompts + closure output.json; align to that). For each slice: which gaps, which files, whether it needs a migration, whether it touches AC-057a, and the behavior-compat handling.
3. For EACH gap, fold in the adversarial verdict — if a design was refuted or corrected, the plan must reflect the corrected approach, not the original.
4. A test strategy grounded in the repo's conventions (acceptance tests, perf gates, the *_for_test seams).
5. An explicit "what we leverage that already exists in 0.7.2" subsection (e.g. excise_source's read queries, the projection terminal state, vec0 metadata columns, the admin.* namespace, op-store tables) so the strategy is clearly about leverage, not greenfield.
6. Call out anything that genuinely cannot be done well within 0.7.2's constraints and must wait for an explicit HITL invariant decision.
Keep it rigorous and concrete. Return ONLY the Markdown document body (no preamble).

DOSSIER:
${dossier}`,
  { label: 'synthesize:plan', phase: 'Synthesize' }
)

return { mapsCount: maps.length, gaps: results.map((r) => ({ gap: r.design.gap, verdict: r.verdict.verdict, effort: r.verdict.revisedEffort && r.verdict.revisedEffort !== 'unchanged' ? r.verdict.revisedEffort : r.design.effort })), plan }
