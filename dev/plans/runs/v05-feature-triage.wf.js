export const meta = {
  name: 'fathomdb-v05-feature-triage',
  description: 'Decide which v0.5.0 features enter 0.8.0 scope vs defer, and HOW (web research for world-class design / experiments / design-ADR / scaffolding / v0.5.x-is-enough). Ground in code + consumers + the surface-supersession, adversarially verify, synthesize a triage doc + an edit-plan for existing docs.',
  phases: [
    { title: 'Ground', detail: 'read v0.5.x surface, 0.8.0 scope, consumer needs, supersession governance' },
    { title: 'Assess', detail: 'per-feature: importance + add/defer + the HOW (research/experiment/design/scaffold), with web research where world-class design is needed' },
    { title: 'Verify', detail: 'adversarial re-check against code reality, governance, and whether the HOW is right' },
    { title: 'Synthesize', detail: 'consolidated triage doc + precise edit-plan for existing docs' },
  ],
}

const REPO = '/home/coreyt/projects/fathomdb'

// ---- The v0.5.0 feature set (from dev/profiling/v05-lineage.md, git-verified) ----
// Each is a candidate for 0.8.0 scope-in vs defer. Some already map to a G-item.
const FEATURES = [
  { id: 'F1', name: 'Graph traversal — traverse()/expand()/TraverseDirection', gmap: 'G5/G6', v05: 'fathomdb-query/src/builder.rs:103,374' },
  { id: 'F2', name: 'By-id SDK read — filter_logical_id_eq / read.get', gmap: 'G2', v05: 'builder.rs:120; napi filter_logical_id_eq' },
  { id: 'F3', name: 'Rich typed JSON-path filter DSL (int/timestamp gt/gte/lt/lte, bool, fused-secondary-index predicates)', gmap: 'G4 (small grammar only)', v05: 'builder.rs:167-374' },
  { id: 'F4', name: 'Operational-collection lifecycle/governance (register/validate/secondary-indexes/retention/compact/trace/read)', gmap: 'G3 (read only)', v05: 'admin.rs ~15 verbs; operational_secondary_index_entries / operational_retention_runs tables' },
  { id: 'F5', name: 'FTS property schemas — schema-declared full-text projections over node properties', gmap: 'none', v05: 'fts_property_schemas / fts_node_property_positions; SchemaVer 15; dev/schema-declared-full-text-projections-over-structured-node-properties.md' },
  { id: 'F6', name: 'Per-kind FTS/vec profile config + tokenizer presets', gmap: 'partial (coarse admin.configure)', v05: 'set_fts_profile, resolve_tokenizer_preset' },
  { id: 'F7', name: 'In-process admin/maintenance SDK API (regenerate_vector_embeddings, rebuild_projections, safe_export, restore/purge as SDK calls)', gmap: 'CLI-only now (doctor/recover)', v05: 'admin.rs' },
  { id: 'F8', name: 'Grouped/aggregation queries (execute_compiled_grouped_query)', gmap: 'none', v05: 'fathomdb-query/src/compile.rs:661' },
  { id: 'F9', name: 'Per-fact confidence score (REAL column on nodes/edges)', gmap: 'adjacent to G12 importance', v05: 'bootstrap.rs nodes/edges confidence REAL' },
  { id: 'F10', name: 'Dangling-edge detection + restore_validated_edges (referential integrity)', gmap: 'G8', v05: 'admin.rs:864,1073,2785,4553' },
  { id: 'F11', name: 'Bitemporal superseded_at (transaction/valid time) on nodes+edges', gmap: 'G0/G11', v05: 'bootstrap.rs superseded_at + partial-unique-active index' },
  { id: 'F12', name: 'Structured search hits + HitAttribution', gmap: 'G1 (already restored as knowledge-store anchor)', v05: 'SearchHit / HitAttribution' },
]

// ---- Schemas ----
const ASSESS_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['feature', 'consumerImportance', 'perConsumer', 'recommendation', 'how', 'rationale', 'governance', 'effort', 'confidence'],
  properties: {
    feature: { type: 'string' },
    consumerImportance: { type: 'string', enum: ['table-stakes', 'differentiating', 'world-class', 'low'] },
    perConsumer: {
      type: 'object', additionalProperties: false,
      required: ['openclaw', 'hermes', 'mem0_world_class'],
      properties: {
        openclaw: { type: 'string', enum: ['critical', 'important', 'nice', 'low'] },
        hermes: { type: 'string', enum: ['critical', 'important', 'nice', 'low'] },
        mem0_world_class: { type: 'string', enum: ['critical', 'important', 'nice', 'low'] },
      },
    },
    recommendation: { type: 'string', enum: ['add-0.8.0', 'defer-0.8.x', 'defer-0.9plus', 'drop'] },
    how: {
      type: 'object', additionalProperties: false,
      required: ['v05ReferenceSufficient', 'webResearchNeeded', 'intermediateWork', 'firstStep'],
      properties: {
        v05ReferenceSufficient: { type: 'boolean', description: 'Does the v0.5.x code already encode a world-class-enough approach to implement directly?' },
        webResearchNeeded: { type: 'string', description: 'What world-class design/approach/algorithm must be researched, or "none". If you did the research in this run, summarize the finding.' },
        intermediateWork: { type: 'array', items: { type: 'string', enum: ['experiment', 'design-adr', 'code-scaffolding', 'profiling-baseline', 'none'] } },
        firstStep: { type: 'string', description: 'The concrete first action if picked up.' },
      },
    },
    worldClassNote: { type: 'string', description: 'The world-class approach for this feature (from web research or v0.5.x), with source if web.' },
    rationale: { type: 'string' },
    governance: { type: 'string', description: 'Surface-supersession interaction: new read.* verb? typed-boundary/filter-grammar? recovery-denylist-safe? CLI vs SDK?' },
    dependsOn: { type: 'array', items: { type: 'string' } },
    effort: { type: 'string', enum: ['XS', 'S', 'M', 'L', 'XL'] },
    confidence: { type: 'string', enum: ['high', 'med', 'low'] },
    openQuestions: { type: 'array', items: { type: 'string' } },
  },
}

const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['feature', 'verdict', 'refutations', 'correctedRecommendation', 'correctedHow'],
  properties: {
    feature: { type: 'string' },
    verdict: { type: 'string', enum: ['sound', 'needs-fix', 'unsound'] },
    refutations: { type: 'array', items: { type: 'string' }, description: 'Concrete ways the assessment is wrong: misread consumer need, wrong governance, wrong HOW (e.g. claims web research needed when v0.5.x already answers it, or vice-versa), wrong effort/dep. Empty if sound.' },
    correctedRecommendation: { type: 'string', description: 'add-0.8.0 | defer-0.8.x | defer-0.9plus | drop | unchanged' },
    correctedHow: { type: 'string', description: 'Corrected HOW if the assessment got research/experiment/design/scaffold wrong; else "unchanged".' },
    citationCheck: { type: 'array', items: { type: 'object', additionalProperties: false, required: ['claim', 'real'], properties: { claim: { type: 'string' }, real: { type: 'boolean' }, note: { type: 'string' } } } },
  },
}

// ---- Phase 1: Ground ----
phase('Ground')
log('Reading v0.5.x surface, 0.8.0 scope, consumer needs, surface-supersession governance')

const GROUND = [
  { key: 'v05', prompt: `Read ${REPO}/dev/profiling/v05-lineage.md in full, and spot-verify 2-3 of its claims via git (e.g. \`git show v0.5.0:crates/fathomdb-query/src/builder.rs\` for traverse/expand, \`git show v0.5.0:crates/fathomdb-schema/src/bootstrap.rs\` for confidence/superseded_at). Summarize the EXACT v0.5.x feature surface, what each feature's v0.5.x implementation looked like (so we know if it's a sufficient reference), and which features the 0.6.0 rewrite dropped vs which 0.8.0 already revives. Cite git paths.` },
  { key: 'scope', prompt: `Read ${REPO}/dev/roadmap/0.8.0.md, ${REPO}/dev/design/0.8.0-agent-memory-fit.md (§4 gap ladder G0-G12, §8 table-stakes ranking), ${REPO}/dev/design/agent-memory-impl-strategy.md (the slice plan), ${REPO}/dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md, and ${REPO}/dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md. Summarize EXACTLY what 0.8.0 currently scopes, what is gated/deferred, and — critically — how the surface-supersession ADR changes what is now addable (governed read.* surface, recovery denylist, typed-boundary/filter-grammar). Cite file:line/section.` },
  { key: 'consumers', prompt: `Read ${REPO}/dev/design/0.8.0-agent-memory-fit.md §8 (shipping peers + literature) and ${REPO}/dev/memex-note-on-0.6.0.md. Summarize what Memex, Hermes, and OpenClaw actually need (which retrieval/world-model capabilities are table-stakes vs differentiating vs world-class for each), grounded in the real systems. This is the consumer-importance oracle for the per-feature assessment.` },
]
const ground = (await parallel(GROUND.map((g) => () =>
  agent(g.prompt, { label: `ground:${g.key}`, phase: 'Ground', agentType: 'Explore' })
))).filter(Boolean)

const groundCtx = ground.join('\n\n---\n\n')

// ---- Phases 2+3: Assess (with web research) -> adversarial Verify, pipelined ----
const SHARED = `You are triaging v0.5.0 features for FathomDB 0.8.0 (repo ${REPO}, head of main = 0.7.2). v0.5.x was a full document+graph+KV store; 0.6.0 stripped it to a 5-verb retrieval engine; 0.8.0 selectively revives a subset. The five-verb scope CAP is being superseded (see ${REPO}/dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md) by a GOVERNED surface: read verbs MAY be added under a new read.* namespace, BUT must preserve (a) SDK parity (Py+TS lockstep), (b) the recovery-name denylist {recover,restore,repair,fix,rebuild,doctor} — recovery/mutation stays CLI-only; non-destructive reads are allowed, (c) the typed-write boundary + a small fixed filter grammar (NOT raw SQL / not a DSL). Single-writer + no-data-migration (additive ALTER only) + vec0-brute-force + migration-accretion-guard invariants still hold.

Decide, per feature: (1) consumer importance, (2) add-0.8.0 / defer-0.8.x / defer-0.9plus / drop, and (3) THE HOW — is the v0.5.x code a sufficient reference to implement directly, or is web research needed for a world-class design/approach/algorithm, and what intermediate work (experiment / design-ADR / code-scaffolding / profiling-baseline) is required. Where a feature needs a world-class approach NOT already encoded in v0.5.x (e.g. confidence/uncertainty scoring, consolidation/forgetting, entity resolution, tokenizer policy, aggregation-in-agent-memory), USE WebSearch/WebFetch to find the current best practice and record the finding + source. Prefer leveraging v0.5.x's working reference impl over greenfield; prefer the smallest-blast-radius governed addition.

GROUND CONTEXT (v0.5.x surface, 0.8.0 scope, consumer needs):
${groundCtx}`

const results = await pipeline(
  FEATURES,
  (f) => agent(
    `${SHARED}

Assess feature ${f.id}: ${f.name}
(v0.5.x impl: ${f.v05}; current G-mapping: ${f.gmap})

Read the relevant current code and, if useful, the v0.5.x reference via git. If the world-class approach is not already encoded in v0.5.x, do the web research now and record it. Return the full structured assessment.`,
    { label: `assess:${f.id}`, phase: 'Assess', schema: ASSESS_SCHEMA }
  ),
  (assess, f) => agent(
    `${SHARED}

Adversarially verify the assessment of ${f.id}: ${f.name}. Try to REFUTE it: (a) is the consumer-importance right per the real systems (don't inflate graph for OpenClaw/Hermes, which have no graph; don't deflate by-id, which all three need)? (b) is the recommendation right given the supersession governance (is it actually addable now, or still genuinely blocked)? (c) is THE HOW right — does it claim web research is needed when v0.5.x already answers it, or claim v0.5.x is sufficient when the world-class bar has moved since 2026? (d) are deps/effort/governance correct (recovery-denylist, typed-boundary, single-writer)? Verify cited symbols are real. Provide corrections.

ASSESSMENT UNDER REVIEW:
${JSON.stringify(assess, null, 2)}`,
    { label: `verify:${f.id}`, phase: 'Verify', schema: VERDICT_SCHEMA }
  ).then((verdict) => ({ feature: f, assess, verdict }))
)

const triaged = results.filter(Boolean)

// ---- Phase 4: Synthesize triage doc + edit-plan ----
phase('Synthesize')
log(`Synthesizing triage from ${triaged.length} verified feature assessments`)

const dossier = triaged.map(({ feature, assess, verdict }) => `## ${feature.id} — ${feature.name}
G-map: ${feature.gmap} | v0.5.x: ${feature.v05}
Importance: ${assess.consumerImportance} (OpenClaw=${assess.perConsumer?.openclaw}, Hermes=${assess.perConsumer?.hermes}, Mem0/world-class=${assess.perConsumer?.mem0_world_class})
Recommendation: ${assess.recommendation}${verdict.correctedRecommendation && verdict.correctedRecommendation !== 'unchanged' ? ` → CORRECTED: ${verdict.correctedRecommendation}` : ''}
HOW: v05-sufficient=${assess.how?.v05ReferenceSufficient}; webResearch=${assess.how?.webResearchNeeded}; intermediate=${(assess.how?.intermediateWork || []).join(',')}; firstStep=${assess.how?.firstStep}${verdict.correctedHow && verdict.correctedHow !== 'unchanged' ? ` → CORRECTED HOW: ${verdict.correctedHow}` : ''}
World-class note: ${assess.worldClassNote || '(v0.5.x reference)'}
Governance: ${assess.governance}
Depends on: ${(assess.dependsOn || []).join(', ') || 'none'} | Effort: ${assess.effort} | Confidence: ${assess.confidence}
Verdict: ${verdict.verdict}${(verdict.refutations || []).length ? ' — refutations: ' + verdict.refutations.join(' || ') : ''}
Open Qs: ${(assess.openQuestions || []).join('; ') || 'none'}`).join('\n\n')

const plan = await agent(
  `${SHARED}

Below is the verified per-feature triage dossier. Do TWO things and return BOTH as one markdown document:

PART 1 — Write the FULL BODY of a new consolidated triage document for ${REPO}/dev/design/0.8.0-v05-feature-triage.md. It must contain:
- A lead thesis (what the supersession unblocks; the add/defer split at a glance).
- A decision table: feature | consumer importance | DECISION (add-0.8.0 / defer-0.8.x / defer-0.9+ / drop) | HOW (v0.5.x-ready / needs-research / needs-experiment / needs-design-ADR / needs-scaffolding) | effort | depends-on.
- Per-feature detail folding in the adversarial corrections (use the CORRECTED recommendation/HOW where given, not the original).
- A "research findings" subsection capturing every world-class approach the assess agents surfaced via web (with sources), so the research isn't lost.
- A "sequenced next actions" list grouped by HOW (what to research first, what to scaffold, what to design-ADR, what's ready to slice).
- An explicit "deferred / dropped (and why)" section.

PART 2 — An EDIT-PLAN (a clearly delimited section titled "## EDIT-PLAN FOR EXISTING DOCS") listing, for each existing doc that should be updated, the exact target section and the precise text to add/replace. Targets to consider: dev/design/0.8.0-agent-memory-fit.md (add a §9 consumer-impact-of-v05-delta or extend §8d), dev/design/agent-memory-impl-strategy.md (new/updated slices for any newly-in-scope feature), dev/roadmap/0.8.0.md (scope additions), dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md (scope reclass), dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md (which read verbs land), dev/profiling/v05-lineage.md ("how to use in 0.8.0" per feature). Give each edit as: FILE -> SECTION -> exact insertion/replacement text. Do NOT edit the files yourself — only produce the plan; the main thread applies it.

Be rigorous and concrete; honor the adversarial corrections. Return ONLY the markdown.

DOSSIER:
${dossier}`,
  { label: 'synthesize:triage', phase: 'Synthesize' }
)

return {
  groundCount: ground.length,
  features: triaged.map((t) => ({ id: t.feature.id, importance: t.assess.consumerImportance, decision: (t.verdict.correctedRecommendation && t.verdict.correctedRecommendation !== 'unchanged') ? t.verdict.correctedRecommendation : t.assess.recommendation, verdict: t.verdict.verdict })),
  document: plan,
}
