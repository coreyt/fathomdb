export const meta = {
  name: 'fathomdb-0.8.0-slice-plan',
  description: 'Plan the 0.8.0 implementation as orchestrated slices: N-mod-5 numbering (0,5,10,…) with reserved gaps, each slice a slice-orchestrator with ≤1 level of subagents (N.a/N.b), each following the documented orchestration rules. Slices may be research/experiment/verify/docs, not only impl. Ground in triage+ADRs+orchestration rules, design a skeleton, detail+adversarially-verify each slice, synthesize the plan doc.',
  phases: [
    { title: 'Ground', detail: 'read triage/ADRs/impl-strategy + orchestration rules → consolidated build DAG + governance' },
    { title: 'Skeleton', detail: 'propose the ordered N-mod-5 slice list + dependency DAG + subagent sketch + HITL gates' },
    { title: 'Detail', detail: 'flesh each slice into a slice-orchestrator spec (per-slice, pipelined)' },
    { title: 'Verify', detail: 'adversarial check vs orchestration rules, numbering scheme, dependency soundness, triage fidelity' },
    { title: 'Synthesize', detail: 'assemble the 0.8.0 implementation plan doc' },
  ],
}

const REPO = '/home/coreyt/projects/fathomdb'

// ---- Shared planning constraints (the rules the plan MUST obey) ----
const RULES = `You are planning the FathomDB 0.8.0 implementation as a sequence of orchestrated SLICES. Hard planning constraints (from HITL + the repo's documented rules):

NUMBERING (HITL directive):
- Start with **Slice 0**, then number only in multiples of 5: 0, 5, 10, 15, 20, 25, ...
- The gaps (1-4, 6-9, 11-14, ...) are RESERVED ON PURPOSE: if Slice 5 needs follow-on work before the next planned slice (Slice 10), that follow-on becomes Slice 6/7/8/9. So the plan lists only the mod-5 slices; the gaps are documented as the insertion mechanism for unplanned follow-on/fix work.

SLICE STRUCTURE (HITL directive):
- Each slice has a GOVERNING AGENT — "Slice N agent" — which acts as the slice ORCHESTRATOR.
- Each slice may have ONE level of subagents (Slice N.a, N.b, ...). Subagents do NOT spawn further subagents (one level only).
- NOT every slice or subagent does implementation. Valid work types: research, experiment, verification, documentation, design/ADR, AND implementation. Choose the right type per slice.

ORCHESTRATION RULES the slice-orchestrator MUST follow (from ${REPO}/dev/design/orchestration.md — cite sections):
- §1 three-role separation: main thread orchestrates (no orchestrator subagent); implementers run as the \`implementer\` subagent in a MAIN-THREAD-OWNED git worktree (\`git worktree add\` from a chosen baseline; never Agent-native isolation); reviewer is \`codex exec --sandbox read-only\` against the worktree branch.
- §1.5 state spine: NEW→PROMPTED→WORKTREE_CREATED→IMPLEMENTING→IMPLEMENTED (output.json present + branch advanced)→REVIEWED→(PASS|fix-N)→CLEANED. Each step gated on the prior witness.
- §8 closure: every implementation slice writes \`dev/plans/runs/<id>-output.json\` (phase, baseline_sha, branch, head_sha, commits, findings_addressed, blockers_encountered, agent_verify_result, next_step_for_orchestrator).
- §9 decision loop: read output.json → cherry-pick to mainline (never merge) → codex review → promote verdict to \`<id>-review-<ts>.md\` → PASS close / CONCERN(structural)→override / CONCERN(substantive)|BLOCK→fix-1 → edit plan + advance pointer in the SAME docs commit.
- §6 fix-N: fresh implementer into the EXISTING worktree/branch; ~1 fix-N per BLOCK; unbounded fix-N or unclearable BLOCK → HALT to HITL.
- §10 hard rules: never override BLOCK; cherry-pick not merge; per-spawn facts (worktree/branch/baseline/output path) in the Agent prompt; \`implementer\` agent omits Agent/Task (anti-chain guard).
- §11 worktree cleanup after a phase/slice family closes (one \`worktree remove\` per Bash call; never find -delete).
- §12 context discipline: each slice gets a self-contained \`dev/plans/prompts/<id>.md\`; implementer never sees prior-slice conversation; plan doc is canonical state; phases >4 slices get a \`STATUS-0.8.0.md\` board.
- KNOWN DISCREPANCY to surface (not resolve): orchestration.md names the \`implementer\` subagent type, but MEMORY note \`orchestration-execution-traps\` says that type may not exist in this harness and \`general-purpose\` is the working substitute. Flag this as an open item in the plan; do not silently pick one.

SCOPE the plan must implement (the verified triage — ${REPO}/dev/design/0.8.0-v05-feature-triage.md, gap ladder G0-G12 in ${REPO}/dev/design/0.8.0-agent-memory-fit.md §4, slice shapes in ${REPO}/dev/design/agent-memory-impl-strategy.md):
- ADD-0.8.0: G1 structured hits (F12), G2 read.get (F2), G3 read.collection READ (F4), G8 dangling-edge flag (F10), G0 identity substrate (F11, keystone), G9 RRF fusion, G10 metadata-filtered KNN, G12 recency. Global FTS5 tokenizer default upgrade (porter unicode61 remove_diacritics 2) — KEPT, zero-surface.
- GOVERNED (not blocked) read verbs under ADR-0.8.0-supersede-five-verb-surface-cap: G2/G3 add-0.8.0; G4 read.list / G5 read.neighbors / G6 search(expand=) / G7 read.history defer-0.8.x.
- DESIGN-ADRs to author (gate slices): canonical-identity-substrate (gates G0), graph-traversal-scope, G4 filter-grammar, confidence-vs-importance (F9), fielded-text/BM25F (F5).
- DEFER-0.8.x: G5/G6/G4 verbs, F9 confidence, F4-governance, F5 fielded-FTS. DROP: F6 per-kind tokenizer, F7 in-process admin SDK.
- KEYSTONE: G0 gates G2/G5/G7/G8/G11. The supersession ADR sign-off + conformance-test rewrite (set-equality → allowlist+parity) gates the SDK read verbs.

Honor the triage's adversarial corrections: G0 transaction-time = table-stakes (keystone); reads are governed not blocked; OpenClaw/Hermes need no graph (don't sequence graph early on their behalf); G1 reshapes BOTH search branches + drops Eq on SearchResult.

PER-SLICE EXECUTION DISCIPLINE (HITL directive — every slice's lifecycle MUST encode this, adapted to its work-type):
1. **Orchestrate to the plan.** The Slice N agent (slice orchestrator) FOLLOWS the plan; if reality diverges it ADJUSTS the plan and records the adjustment in the plan doc / STATUS board (per §12.4 plan-as-state-machine) — it does not silently deviate. Plan drift is captured, in the same docs commit that closes the slice (§9 step 7).
2. **Design-first, self-checked (implementer side).** Before writing code, the implementation subagent writes a SHORT DESIGN (a design memo / design section of the prompt or a \`dev/design/<slice>-design.md\`) covering approach, touch-points, and test plan — and self-CHECKS its own work (runs the suite, lint, typecheck, the agent-verify script) BEFORE returning. The design + self-check evidence go in the closure output.json (§8 \`agent_verify_result\`).
3. **TDD.** Implementation follows test-driven development: RED first (write failing tests that pin the acceptance criteria — e.g. the \`pr_<slice>\`/AC tests), then GREEN (minimal code to pass), then refactor. The output.json records the RED commit sha + the test files (matching the repo's existing tdd_evidence shape).
4. **Codex review after implementation.** Once implemented + self-checked, the orchestrator spawns the read-only codex reviewer on the worktree branch (§3) and promotes the verdict (§4).
5. **FIX-N until PASS.** Run §6 fix-N remediation cycles (fresh implementer into the existing worktree/branch) until the verdict is PASS (or CONCERN-override per §7). Never override BLOCK; an unclearable BLOCK or fix-N past a small bound HALTS to HITL (§1.5 invariant 3).
For NON-implementation slices (design-ADR / research / experiment / verify / docs) this discipline adapts: the "design" is the ADR/research-plan; "TDD" becomes a falsifiable acceptance bar stated up front; "self-check" is the agent validating its own output against that bar; "codex review" becomes a review/adversarial-verify pass on the artifact; "FIX-N" iterates the artifact until the bar is met or HITL signs off. State the adapted loop explicitly per slice.`

// ---- Schemas ----
const SKELETON_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['slices', 'reservedGapPolicy', 'hitlGates', 'parallelizable', 'criticalPath'],
  properties: {
    slices: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['number', 'title', 'workType', 'gaps', 'dependsOn', 'rationale', 'subagents'],
        properties: {
          number: { type: 'integer', description: 'Multiple of 5: 0,5,10,15,...' },
          title: { type: 'string' },
          workType: { type: 'string', enum: ['design-adr', 'research', 'experiment', 'implementation', 'verification', 'documentation', 'hitl-gate', 'mixed'] },
          gaps: { type: 'array', items: { type: 'string' }, description: 'G-ids / F-ids this slice delivers (may be empty for pure-orchestration/setup slices)' },
          dependsOn: { type: 'array', items: { type: 'integer' }, description: 'slice numbers this depends on' },
          rationale: { type: 'string' },
          subagents: {
            type: 'array',
            description: 'one level only (N.a, N.b). Each is the work the slice-orchestrator delegates.',
            items: {
              type: 'object', additionalProperties: false,
              required: ['id', 'role', 'workType'],
              properties: {
                id: { type: 'string', description: 'e.g. "5.a"' },
                role: { type: 'string' },
                workType: { type: 'string', enum: ['design-adr', 'research', 'experiment', 'implementation', 'verification', 'documentation'] },
              },
            },
          },
        },
      },
    },
    reservedGapPolicy: { type: 'string', description: 'How the 1-4/6-9/... gaps are used for unplanned follow-on (the N-mod-5 mechanism).' },
    hitlGates: { type: 'array', items: { type: 'string' }, description: 'slice numbers / points that are HITL decision boundaries' },
    parallelizable: { type: 'array', items: { type: 'string' }, description: 'which slices can run concurrently' },
    criticalPath: { type: 'string', description: 'the keystone-gated critical path through the slices' },
  },
}

const DETAIL_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['number', 'title', 'workType', 'objective', 'governingAgent', 'subagents', 'orchestrationLifecycle', 'closure', 'successCriteria', 'dependsOn', 'hitl'],
  properties: {
    number: { type: 'integer' },
    title: { type: 'string' },
    workType: { type: 'string' },
    objective: { type: 'string', description: '2-4 sentences: what this slice delivers and why, grounded in the triage/ADRs.' },
    governingAgent: { type: 'string', description: 'What the "Slice N agent" (the slice orchestrator) does: how it sequences its subagents and applies the orchestration decision loop.' },
    subagents: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['id', 'workType', 'task', 'agentType', 'producesWorktree', 'output'],
        properties: {
          id: { type: 'string' },
          workType: { type: 'string' },
          task: { type: 'string', description: 'concrete task + key file:line touch-points or research/verify question' },
          agentType: { type: 'string', description: 'implementer (worktree, per §1) | codex-reviewer | research/verify agent (no worktree). Note the implementer/general-purpose discrepancy where relevant.' },
          producesWorktree: { type: 'boolean', description: 'true only for implementation subagents (main-thread-owned worktree per §1)' },
          output: { type: 'string', description: 'artifact: output.json | ADR file | research note | verification report | prompt file' },
        },
      },
    },
    orchestrationLifecycle: { type: 'string', description: 'How this slice maps onto the §1.5 state spine + §9 decision loop. For non-impl slices, the adapted lifecycle (e.g. ADR draft → HITL sign-off; experiment → KEEP/REVERT verdict per §12.8).' },
    closure: { type: 'string', description: 'the closure artifact + schema this slice writes (output.json per §8 for impl; ADR/note/report for others).' },
    successCriteria: { type: 'array', items: { type: 'string' } },
    dependsOn: { type: 'array', items: { type: 'integer' } },
    hitl: { type: 'string', description: 'HITL gate / sign-off this slice needs or produces, or "none".' },
    reservedFollowOn: { type: 'string', description: 'What kind of follow-on would use the reserved gap numbers after this slice (N+1..N+4), if any.' },
  },
}

const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['number', 'verdict', 'refutations', 'corrected'],
  properties: {
    number: { type: 'integer' },
    verdict: { type: 'string', enum: ['sound', 'needs-fix', 'unsound'] },
    refutations: { type: 'array', items: { type: 'string' }, description: 'Concrete failures: violates an orchestration rule (e.g. 2-level subagents, orchestrator subagent, merge-not-cherry-pick, missing output.json, BLOCK-override), wrong dependency order (e.g. read-verb before G0), wrong numbering, mislabels work-type, contradicts a triage decision (e.g. revives F6, blocks a governed read verb), or claims impl where research/design is needed. Empty if sound.' },
    corrected: { type: 'string', description: 'the corrected slice spec if needs-fix/unsound; else "unchanged".' },
  },
}

// ---- Phase 1: Ground ----
phase('Ground')
log('Grounding: triage+features, impl-strategy slice shapes, orchestration rules, ADRs+governance')

const GROUND = [
  { key: 'scope', prompt: `Read ${REPO}/dev/design/0.8.0-v05-feature-triage.md and ${REPO}/dev/design/0.8.0-agent-memory-fit.md (§4 gap ladder G0-G12, §8d ranking, §9 v05-delta). Produce the AUTHORITATIVE 0.8.0 build scope: every gap/feature that is ADD-0.8.0 vs DEFER-0.8.x vs DROP, with its consumer-importance tier and its dependency on G0 / the supersession ADR / a design-ADR. Be precise about what "governed (not blocked)" means for read verbs and which design-ADRs gate which gaps.` },
  { key: 'slices', prompt: `Read ${REPO}/dev/design/agent-memory-impl-strategy.md in full. Summarize each existing Slice (A-H) and the new G2-read/G3-read slices: gap, files/symbols touched, migration?, AC-057a/governance interaction, dependencies, effort, behavior-compat events, and the adversarial corrections folded in. This is the raw material the slice plan re-sequences into the N-mod-5 scheme.` },
  { key: 'rules', prompt: `Read ${REPO}/dev/design/orchestration.md in full. Produce a tight checklist a slice-plan must satisfy: the three roles, the §1.5 state spine, the §8 closure schema, the §9 decision loop, §6 fix-N, §7 CONCERN-override / never-override-BLOCK, §11 worktree cleanup, §12 context discipline (per-slice prompt/runs artifacts, STATUS board for >4 slices, plan-as-state-machine). Also note the §12.8 perf-experiment KEEP/REVERT loop (for experiment-type slices). Flag the \`implementer\` vs \`general-purpose\` subagent-type discrepancy (orchestration.md says implementer; MEMORY orchestration-execution-traps says it may not exist — use general-purpose).` },
  { key: 'adrs', prompt: `Read ${REPO}/dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md, ${REPO}/dev/adr/ADR-0.8.0-agent-memory-retrieval-and-identity.md, and ${REPO}/dev/roadmap/0.8.0.md. Summarize: (a) the governed-surface decision + what HITL sign-off + conformance-test rewrite it still needs (gates SDK read verbs); (b) the bi-temporal-aware G0 design constraint + the to-be-drafted canonical-identity-substrate ADR (gates G0 impl); (c) the security-hardening items (SR-005/SR-011) and logical-id CLI verbs already on the 0.8.0 roadmap; (d) any other HITL gates. These become design-ADR slices and HITL-gate boundaries in the plan.` },
]
const ground = (await parallel(GROUND.map((g) => () => agent(g.prompt, { label: `ground:${g.key}`, phase: 'Ground', agentType: 'Explore' }))) ).filter(Boolean)
const groundCtx = ground.join('\n\n---\n\n')

// ---- Phase 2: Skeleton ----
phase('Skeleton')
log('Designing the N-mod-5 slice skeleton + dependency DAG + subagent sketch')

const skeleton = await agent(
  `${RULES}

GROUND CONTEXT:
${groundCtx}

Design the 0.8.0 SLICE SKELETON. Produce the ordered list of mod-5 slices (0,5,10,15,...) covering the full ADD-0.8.0 scope + the gating design-ADRs + the supersession sign-off + a setup/STATUS slice + a final release/verification slice. For EACH slice give: number, title, workType, the gaps/F-ids it delivers, dependsOn (slice numbers), rationale, and a one-level subagent sketch (N.a/N.b with role + workType). Then: the reserved-gap policy (how 1-4/6-9 absorb follow-on), the HITL gates, what's parallelizable, and the keystone-gated critical path.

Sequencing principles to honor: G0 (identity) is the keystone and gates G2/G5/G7/G8 — but G1/G9/G10/G12 + the global tokenizer are AC-057a-clean read-path work that can land first to de-risk the recall floor. Design-ADRs precede the impl they gate. The supersession sign-off + conformance-test rewrite gates the SDK read verbs (G2/G3). Slice 0 should be setup (STATUS board, plan scaffolding, the substrate + supersession ADR authoring kickoff) — design/docs, not impl. Put deferred-feature design-ADRs (F5/F9/G4-grammar/graph-scope) as their own slices so 0.8.x work is teed up. End with a verification + release-readiness slice. Keep it to a sensible number of mod-5 slices (roughly 7-12).`,
  { label: 'skeleton', phase: 'Skeleton', schema: SKELETON_SCHEMA }
)

const sliceList = (skeleton.slices || []).slice().sort((a, b) => a.number - b.number)
const skeletonCtx = `SLICE SKELETON (authoritative ordering):
${sliceList.map((s) => `- Slice ${s.number} [${s.workType}] ${s.title} — gaps:[${(s.gaps||[]).join(',')}] dependsOn:[${(s.dependsOn||[]).join(',')}] subagents:[${(s.subagents||[]).map((x)=>x.id+':'+x.workType).join(', ')}]\n    ${s.rationale}`).join('\n')}
RESERVED-GAP POLICY: ${skeleton.reservedGapPolicy}
HITL GATES: ${(skeleton.hitlGates||[]).join(', ')}
PARALLELIZABLE: ${(skeleton.parallelizable||[]).join(', ')}
CRITICAL PATH: ${skeleton.criticalPath}`

log(`Skeleton: ${sliceList.length} slices (${sliceList.map((s)=>s.number).join(', ')}) → detailing + verifying each`)

// ---- Phases 3+4: Detail -> adversarial Verify, pipelined per slice ----
const detailed = await pipeline(
  sliceList,
  (s) => agent(
    `${RULES}

GROUND CONTEXT:
${groundCtx}

${skeletonCtx}

Flesh out **Slice ${s.number} — ${s.title}** (workType: ${s.workType}; gaps: ${(s.gaps||[]).join(',')||'none'}; dependsOn: ${(s.dependsOn||[]).join(',')||'none'}) into a full slice-orchestrator spec. The "Slice ${s.number} agent" is the slice orchestrator: describe how it sequences its ≤1-level subagents (${(s.subagents||[]).map((x)=>x.id).join(', ')||'none/solo'}) and applies the §9 decision loop (or the adapted lifecycle for a research/design/experiment/verify/docs slice). Ground every touch-point in real files/symbols from the impl-strategy. Be explicit about: worktree ownership (§1), the closure artifact (§8 output.json for impl; ADR/note/report otherwise), success criteria, dependency gates, and the HITL sign-off it needs/produces. Note where the implementer/general-purpose subagent-type discrepancy applies.

The \`orchestrationLifecycle\` field MUST spell out the PER-SLICE EXECUTION DISCIPLINE explicitly as an ordered loop: (1) orchestrator follows/adjusts the plan; (2) design-first + self-check before code; (3) TDD RED→GREEN→refactor (name the RED test files / AC ids it pins); (4) codex review; (5) FIX-N until PASS/override, BLOCK→HITL. For a non-impl slice, give the ADAPTED loop (design=ADR/plan, TDD=falsifiable acceptance bar stated up front, self-check, review/adversarial-verify, iterate-to-bar). Make it concrete to THIS slice, not generic.`,
    { label: `detail:${s.number}`, phase: 'Detail', schema: DETAIL_SCHEMA }
  ),
  (detail, s) => agent(
    `${RULES}

${skeletonCtx}

Adversarially verify the spec for **Slice ${s.number}**. Try to REFUTE it against: (a) orchestration rules — does it keep ≤1 subagent level? main-thread-owned worktree (not Agent isolation)? cherry-pick not merge? closure output.json for impl? never-override-BLOCK? no orchestrator subagent? (b) numbering — is it a correct mod-5 slot, and does it correctly reserve 1-4/6-9 for follow-on? (c) dependency soundness — does it depend on G0 before using identity? does it put a governed read verb after its gate (supersession sign-off + G0), and NOT mislabel it "blocked"? (d) work-type honesty — is it impl where it should be design/research/verify, or vice-versa? (e) triage fidelity — does it revive F6 / F7 (forbidden), drop the kept global tokenizer, or mis-tier a consumer need? (f) PER-SLICE EXECUTION DISCIPLINE — does \`orchestrationLifecycle\` actually encode all five steps (orchestrate-to-plan / design-first+self-check / TDD RED-first / codex review / FIX-N-until-PASS), concretely for this slice and adapted correctly for non-impl slices? Refute if it's missing TDD, missing the design-before-code step, missing self-check, skips codex review, or omits the fix-N→HITL escalation. Provide corrections.

SPEC UNDER REVIEW:
${JSON.stringify(detail, null, 2)}`,
    { label: `verify:${s.number}`, phase: 'Verify', schema: VERDICT_SCHEMA }
  ).then((verdict) => ({ slice: s, detail, verdict }))
)
const results = detailed.filter(Boolean)

// ---- Phase 5: Synthesize the plan doc ----
phase('Synthesize')
log(`Synthesizing the 0.8.0 implementation plan from ${results.length} verified slices`)

const dossier = results.map(({ detail, verdict }) => `## Slice ${detail.number} — ${detail.title}  [${detail.workType}]
Objective: ${detail.objective}
Governing agent (orchestrator): ${detail.governingAgent}
Subagents (≤1 level): ${(detail.subagents||[]).map((x)=>`${x.id} [${x.workType}, ${x.agentType}${x.producesWorktree?', worktree':''}] ${x.task} → ${x.output}`).join('\n  - ')}
Orchestration lifecycle: ${detail.orchestrationLifecycle}
Closure: ${detail.closure}
Depends on: ${(detail.dependsOn||[]).join(', ')||'none'} | HITL: ${detail.hitl}
Reserved follow-on (N+1..N+4): ${detail.reservedFollowOn||'—'}
Success: ${(detail.successCriteria||[]).join('; ')}
VERDICT: ${verdict.verdict}${(verdict.refutations||[]).length?' — '+verdict.refutations.join(' || '):''}
${verdict.corrected && verdict.corrected!=='unchanged' ? 'CORRECTED: '+verdict.corrected : ''}`).join('\n\n')

const plan = await agent(
  `${RULES}

${skeletonCtx}

Below is the verified per-slice dossier (spec + adversarial verdict). Write the FULL BODY (Markdown) of the 0.8.0 implementation plan for ${REPO}/dev/plans/0.8.0-implementation.md. Fold in every adversarial CORRECTION (use corrected specs, not originals). Structure:

1. Front matter + a lead "How to read this plan" that states the N-mod-5 numbering scheme + the reserved-gap insertion mechanism, the slice-orchestrator model (governing Slice N agent + ≤1 level of N.a/N.b subagents), and that slices may be research/design/experiment/verify/docs — not only impl.
2. A "Slice sequence at a glance" table: Slice | title | work-type | gaps | depends-on | HITL? | parallelizable.
3. The keystone-gated critical path + parallelization plan + the HITL gate boundaries (as natural §12.7 session boundaries).
4. Per-slice detail: objective, the governing-agent orchestration behavior, the ≤1-level subagents (N.a/N.b with work-type + whether they take a main-thread-owned worktree), the orchestration lifecycle it follows (§1.5 spine + §9 loop, or the adapted loop for non-impl), the closure artifact (§8 output.json or ADR/note/report), success criteria, dependency gates, reserved-follow-on note, and HITL.
5. An "Orchestration rules this plan binds to" section citing orchestration.md §§1,1.5,8,9,6,11,12 — and the STATUS-0.8.0.md board requirement (§12.5, >4 slices). Include a "Per-slice execution discipline" subsection stating the universal loop EVERY slice runs: (1) orchestrator follows/adjusts the plan (recording drift in the plan/STATUS); (2) implementer writes a design + self-checks before coding; (3) TDD RED→GREEN→refactor; (4) codex review; (5) FIX-N until PASS / CONCERN-override, BLOCK→HITL — with the adapted form for non-impl (design/research/experiment/verify/docs) slices.
6. An "Open items / discrepancies" section that MUST include the implementer-vs-general-purpose subagent-type discrepancy (flag, don't silently resolve) and the outstanding HITL sign-offs (supersession ADR + conformance-test rewrite; canonical-identity-substrate ADR).
7. A short "Deferred to 0.8.x / dropped" recap (G4/G5/G6/G7 verbs, F9, F4-gov, F5 deferred; F6, F7 dropped; global tokenizer kept) cross-referencing the triage.

Be rigorous, faithful to the orchestration rules and the triage. Return ONLY the markdown.

DOSSIER:
${dossier}`,
  { label: 'synthesize:plan', phase: 'Synthesize' }
)

return {
  sliceCount: results.length,
  slices: results.map((r) => ({ n: r.detail.number, type: r.detail.workType, verdict: r.verdict.verdict, gaps: r.slice.gaps })),
  document: plan,
}
