# Design: 0.4.1 documentation

**Release:** 0.4.1
**Scope item:** Ship gate 4 in `dev/notes/scope-0.4.1.md` —
documentation deliverables
**Grouping:** all docs work for 0.4.1 grouped into one design

## Problem

`dev/notes/scope-0.4.1.md` locks three callouts that must appear
in user-facing docs before 0.4.1 ships:

1. Per-originator `limit` guarantee (with a worked example showing
   skewed distribution).
2. Per-slot result order is **undefined**; callers must sort
   client-side if order matters.
3. Same-kind self-expand at `max_depth > 1` walks blindly with no
   cycle detection — sharp edge.

Plus two deliverables that are not currently in the docs tree:

4. `docs/reference/query.md` does not currently document
   `.expand()` / `.execute_grouped()` at all
   (verified: the file exists but has no grouped-expand entry).
5. Changelog entry for 0.4.1.

And one pre-existing doc that needs updating:

6. `docs/guides/querying.md:870-906` already has a grouped-expand
   section, but it pre-dates the locked semantics and doesn't
   include any of the three callouts above. Also needs a target-
   side filter worked example (Memex use case 3).

## Deliverables

### 1. `docs/reference/query.md` — new `.expand()` / `.execute_grouped()` section

New section covering the full public surface. Must contain:

- **Method signatures** for all three languages (Rust / Python /
  TypeScript). Mirror the style already used elsewhere in the
  reference for `.traverse()`, `.filter()`, etc.
- **Return shape** — `GroupedQueryRows { roots, expansions,
  was_degraded }`, `ExpansionSlotRows { slot, roots }`,
  `ExpansionRootRows { root_logical_id, nodes }` — cited from
  `crates/fathomdb-engine/src/coordinator.rs:216-241`.
- **Per-originator `limit` guarantee** — explicit paragraph.
  Include the worked example:
  > A search returning 50 hits, each with a `.expand(..., limit=20)`
  > slot, returns up to 20 expanded nodes **per hit**, for up to
  > 1000 total — not 20 total. This holds even when the
  > distribution is heavily skewed: a single originator with 500
  > candidates will not starve other originators' budgets.
- **Per-slot order is undefined** — explicit paragraph. Include
  the idiomatic sort-client-side snippet:
  > fathomdb does not guarantee the order of nodes within a slot.
  > Callers that need an ordering (e.g. plan steps by sequence
  > index) must sort the slot's node list client-side:
  > ```python
  > steps = sorted(group.slots["plan_steps"],
  >                key=lambda n: n.properties["sequence_index"])
  > ```
- **`max_depth` semantics** — document that depth>1 works only
  for a single repeated edge label, result is a flat node list
  (no path preservation), and — **sharp edge:** same-kind
  self-expand at depth>1 walks blindly with no cycle detection
  or dedup. Cite the tested behavior from
  `crates/fathomdb/tests/grouped_query_reads.rs` shape 4 (see
  `design-0.4.1-stress-tests.md`) exactly; doc must not diverge
  from what the tests lock.
- **Target-side filter on `.expand()`** — new subsection.
  Document that `.expand(..., filter=...)` accepts the same
  predicate grammar as the main query path. List the supported
  predicates by linking to the existing main-path filter docs.
  Include one worked example using `filter_json_eq`.
- **Fused filter caveat** — if a fused filter is supplied on the
  expanded side and the target kind has no property-FTS schema,
  the call raises `BuilderValidationError::MissingPropertyFtsSchema`.
  Same contract as main-path fused filters.
- **What `.expand()` does not do** — brief "out of scope" list so
  readers don't have to infer: no cross-edge-label multi-hop
  aliases, no engine-side ranking of expanded nodes, no path-
  structure preservation, no global (cross-originator) limit.

### 2. `docs/guides/querying.md` — update grouped-expand section

The existing section at lines 870-906 shows the basic call pattern
but pre-dates the locked semantics. Update to:

- Add a **worked example following Memex use case 1** (goal →
  commitments + provenance_actions + plan_steps). Use
  `WMGoal`-style fixture kinds so the example matches the shape
  Memex will port to.
- Add a **worked example with target-side filter** following
  Memex use case 3 (`discussed_in` + `action_kind` partition).
  This is the canonical demo of why target-side filter exists.
- Add a **per-originator limit callout box** with the skewed
  distribution intuition (same content as the reference, but in
  guide-style prose).
- Add a **pointer to the reference doc** for the full semantics
  list — don't duplicate the sharp-edge callouts in both places;
  guide links to reference.

Do not touch the Python/TypeScript examples at
`docs/guides/querying.md:870-906` beyond what's needed for the
above — they still demonstrate the happy-path call shape and are
already correct.

### 3. Changelog entry

Add to `CHANGELOG.md` under a new 0.4.1 section. Cover:

- **New:** `.expand(...)` and `.execute_grouped()` on
  `SearchBuilder` (not just `NodeQueryBuilder`) — full
  `.nodes(kind).search(...).expand(...).execute_grouped()` chain
  now compiles in Rust. (Python/TypeScript: confirm whether the
  chain already worked before 0.4.1 and phrase accordingly — see
  `design-0.4.1-searchbuilder-expand-chain.md` open question.)
- **New:** target-side filter on `.expand(...)`, accepting the
  same predicate grammar as main-path filters, including 0.4.0
  named fused filters.
- **Clarified:** per-slot result order is explicitly undefined.
  Any caller that was depending on an incidental order should
  sort client-side. (Behavior unchanged; contract is what
  changed.)
- **Sharp edge documented:** same-kind self-expand at `max_depth >
  1` walks blindly. No cycle detection. Depth=1 unaffected.
- **Link** to the Memex search-latency crisis resolution notes
  for context on why grouped expand is the 0.4.1 headline.

Keep it factual. Do not frame 0.4.1 as "fixing" the 44s
`canonical_outcome_hits` latency — that's a Memex-side caller fix
unlocked by 0.4.1, not a fathomdb engine perf change.

### 4. Release notes entry (optional, if the repo has one)

Short 0.4.1 release-notes blurb pointing users at the new
reference section and the updated guide section. Skip if release
notes are auto-generated from changelog.

## Deliverables NOT in this design

- The "target-side filter" surface design itself — covered in
  `design-0.4.1-expand-target-filter.md`. This design only
  covers **documenting** it.
- Memex's adoption-guide content (their migration from N+1 to
  grouped expand). That's a Memex-side doc, not fathomdb's.
- Per-binding API reference auto-generation. If the repo has a
  rustdoc/sphinx/typedoc pipeline, those regenerate from code
  comments — out of scope for this design.

## Acceptance

1. `docs/reference/query.md` contains the new `.expand()` /
   `.execute_grouped()` section with all seven content items
   listed above.
2. `docs/guides/querying.md` grouped-expand section updated with
   both worked examples (use case 1 and use case 3) and the
   per-originator limit callout.
3. `CHANGELOG.md` has a 0.4.1 section with the entries above.
4. Sharp-edge text in the reference matches the tested behavior
   locked by `design-0.4.1-stress-tests.md` shape 4 exactly — no
   divergence between docs and tests.
5. Docs link out correctly to main-path filter grammar
   (target-side filter doc refers to existing filter reference
   instead of duplicating).

## Out of scope

- Architectural rationale docs for why grouped expand exists —
  that lives in `dev/notes/scope-0.4.1.md` and the Memex
  roadmap doc, not in user-facing docs.
- Redocumenting `.traverse()` — unchanged in 0.4.1.
- Migration guide for users moving off `.traverse()` to
  `.expand()`. Not a migration; they're distinct primitives with
  different use cases. Document `.expand()` as a new capability,
  not a replacement.

## Risks

- **Docs drifting from tested behavior on shape 4.** Mitigation:
  write the reference section's sharp-edge paragraph *after* the
  shape 4 test lands, and copy the observed behavior verbatim.
  Don't write it from the design-intent doc.
- **Changelog framing around latency.** Resist the urge to take
  credit for Memex's caller-side latency win. The engine does
  the same work; the win is fewer round trips on the caller's
  side. Frame accordingly.
