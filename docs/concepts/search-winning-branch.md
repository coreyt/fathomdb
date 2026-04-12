# Search: Winning Branch

When `text_search()` runs, the engine may match the same node through more
than one retrieval branch — e.g. chunk FTS *and* property FTS, or lexical
strict *and* vector. The engine collapses those matches into one `SearchHit`.
The **winning branch** is the single branch whose metadata that surviving
hit carries.

This page defines the rule and walks through its implications.

## The rule

Dedup precedence for selecting a winning branch:

1. **Match mode first.** Strict beats relaxed. A node found by both branches
   is reported as strict, always.
2. **Score within mode.** Among branches in the winning mode, the one with
   the highest (normalized) score wins.
3. **Source tiebreak.** If scores tie, use a fixed source priority —
   `Chunk > Property > Vector` — so cross-language parity tests stay stable.
4. **Branch declaration order** is the final tiebreak, so results are
   deterministic even if two branches produce identical mode, score, and
   source.

(`logical_id` lexicographic tiebreak is only relevant for ordering *across*
hits, not for picking a winning branch within one hit.)

The winning branch determines the values on the returned `SearchHit`:

- `source` = winning branch's source
- `match_mode` = winning branch's mode
- `score` = winning branch's score
- `matched_path` = winning branch's path (only meaningful if the winner was
  a property branch)
- `snippet` = winning branch's snippet (chunk snippet if chunk won;
  property-leaf snippet if property won; nearest-chunk text if vector won)

## Implications

**1. The hit loses information about the losing branches.** If chunk FTS
matched "quarterly docs" in the body *and* property FTS matched it in
`$.payload.title`, the client only sees one of those — whichever won. For UI
that wants to highlight "matched in title *and* body," that information is
gone. If a real use case needs it, the engine can later add
`matched_sources: Set<SearchHitSource>` alongside the scalar `source`. It is
deliberately not in the v1 shape.

**2. Score is not a blended score.** It is the winner's score, period. There
is no "combined relevance" arithmetic. That keeps the contract honest —
BM25 and cosine similarity do not share a scale — but it means a node that
matched weakly in three branches can rank below a node that matched strongly
in one. That is the intended behavior; fused scoring is explicitly a
non-goal.

**3. Vector-only hits are legal and carry vector metadata.** If a node
matched only via vector similarity, the winning branch is the vector branch,
so `source = Vector`, `match_mode` reflects the mode vectors run under, and
`matched_path` is `None`. Clients that want to suppress vector-only hits can
filter on `source`.

**4. Branch precedence is stable and documented.** Any change to precedence
— e.g. promoting `Property > Chunk` because property hits turned out to be
more trustworthy — is a behavior change visible to every caller. It is
pinned in this document and in cross-language parity fixtures. Changing it
is a deliberate, versioned decision, not a tuning knob.

**5. Determinism requires the tiebreak chain to be total.** Two branches
with identical mode, identical normalized score, and identical source should
not happen in practice, but if they do, branch declaration order is the
final decider. Without that, the same query could return different `source`
values on reruns and parity tests would flake.

**6. Dedup is logical-id-scoped, not content-scoped.** Two genuinely distinct
nodes with near-identical text are two hits. Semantic dedup (for fact-memory
or write-time deduplication) is a write-time concern handled by the
specialized dedup surface, not by search.

**7. "Winning branch" is an engine-internal concept that leaks through
`source` and `match_mode`.** Those two fields are the client's entire view
of which branch won. If they are ever insufficient for a real use case, the
right move is to add a structured `match_trace` to the specialized surface —
not to make the default response polymorphic.
