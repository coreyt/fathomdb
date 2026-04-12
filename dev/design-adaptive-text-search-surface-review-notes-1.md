# Review Notes 1: Adaptive Text Search Surface

Review of `dev/design-adaptive-text-search-surface.md` against Memex's memory
needs (`memex/dev/notes/memex-memory-needs.md`) and desired requirements
(`memex/dev/notes/memex-memory-desired-requirements.md`), plus a pass for
correctness, completeness, and clarity.

## Fit against Memex needs

The doc is well-scoped for the **simple text surface**, but Memex's
requirements set a higher bar. Gaps that matter:

1. **Filters are missing.** Memex's "one function, query + filters (type, tag,
   date range, pinned)" is the default shape. The design only takes
   `(query, limit)`. Callers will fall back to chaining `.where(...)` and the
   engine loses the ability to fuse filtered-FTS efficiently. Make filters
   first-class in the search builder.

2. **No snippets.** `SearchHit` has score/source/path but no highlighted
   excerpt. Memex wants "ranked records with snippets." Add
   `snippet: Option<String>` (chunk-scoped) — it's cheap from FTS5 `snippet()`
   and prevents every caller from re-reading bodies.

3. **Provenance on the hit is thin.** Memex need #7 ("show its work") wants
   source, time, supersession. Expose at least `written_at` and the
   originating projection row id so callers can trace without a second query.

4. **Vectors are unscoped.** Memex explicitly expects the provider to subsume
   SQLite FTS + Ladybug vectors behind one surface. The doc says nothing about
   whether `text_search()` will later fuse vector hits or whether vectors live
   on the specialized surface. Add one paragraph: "v1 is lexical only; vector
   fusion lands behind the same `SearchHit` surface with
   `SearchHitSource::Vector`." Without that commitment, Memex keeps branching.

5. **"Find related from a seed"** — Memex calls this out. Acknowledge it
   explicitly as specialized-surface, future work, not silently omitted.

6. **Concurrency guarantee.** Memex requires background writes not to block
   foreground reads. Stress tests are mentioned; the *contract* is not. State
   it.

## Correctness / completeness issues

7. **Score comparability across strict and relaxed is asserted but not
   achievable.** BM25 scores from different query plans are not on the same
   scale. Either (a) normalize per-branch before merging, or (b) drop the
   "comparable" claim and rely purely on branch precedence + per-branch order.
   Pick one; today's wording promises both.

8. **Fallback trigger is under-specified.** Doc offers "zero hits" *and*
   "fewer than N" as options. Decide for v1. Recommend zero-hits-only; "top up
   to limit" needs a score penalty to avoid relaxed hits leapfrogging weak
   strict hits.

9. **Relaxed-branch blowup.** "Break implicit-AND into per-term alternatives"
   on a 6-term query is a 6-way OR over chunk+property. Cap relaxed candidate
   count and document the cap, or Memex's conversation-history table will OOM
   the ranker.

10. **Recursive property extraction needs guardrails.** Memex has 20KB+ audit
    payloads on meeting nodes — unbounded recursion will bloat the FTS index
    and tank rebuild time. Add: max depth, max extracted bytes per node,
    exclude-paths list. This is also the **metadata-bloat fix** the
    requirements doc calls for in a different hat — it deserves a
    cross-reference.

11. **Tokenizer is unspecified.** "Vague cues" recall (need #3) is 80%
    tokenizer work: unicode folding, stemming, diacritic stripping.
    Strict/relaxed layering can't compensate for a raw tokenizer. Commit to
    one (e.g. `unicode61 remove_diacritics 2` + optional porter) and say so.

12. **Tiebreakers are non-deterministic.** Doc says strict > relaxed, then
    "higher score within same mode." Equal scores across different logical
    IDs? Add logical_id lexicographic tiebreaker — cross-language parity tests
    will flake otherwise.

13. **`matched_path` semantics when match spans multiple leaves** — return
    which? Define: "path of the highest-scoring matched leaf; tiebreak by
    document order."

14. **Schema migration for recursive flag** on existing property FTS schemas
    — is this a forced rebuild or lazy? The rebuild-discipline principle is
    stated; the migration step is not.

15. **Dedup-on-write path** (Memex fact-memory need) has no story here.
    Fallback helper can serve it ("strict-only, no relaxation") but only if
    the helper accepts `relaxed=None`. Make that explicit.

## Doc clarity fixes

16. **Resolve open question #1 in the doc, don't defer it.** Polymorphic
    `execute()` is hostile to TS/Python typing. Cleanest: the FTS-backed
    builder is its own type whose `execute()` statically returns `SearchRows`.
    Write that down and drop the alternative.

17. **"Adaptive" oversells.** The behavior is strict-then-relaxed. Rename to
    **"graceful text search"** or **"two-stage"**; "adaptive" implies
    learned/rewriting behavior this design doesn't do. It will come up in
    review.

18. **Section heading bug.** "Core Architecture Changes" uses `##` then child
    sections also use `##` (`## fathomdb-query`, `## fathomdb-engine`,
    `## Rust Public Facade`). Demote children to `###`.

19. **Redundant coverage.** `### 3. Add narrow explicit helper` under
    "Proposed Public Surface" duplicates the full `fallback_search` section.
    Keep the full one; make the earlier mention a one-liner cross-reference.

20. **Principle the design implies but never states**: *the engine owns
    fusion; the caller owns reranking.* Put it at the top of "Core Product
    Principle." It's the single sentence that most clearly answers Memex's
    "kill the branching."

21. **Done-when list is process-shaped, not product-shaped.** Add
    user-visible acceptance: "a single call with query + filters returns
    ranked hits with snippets and provenance, with no caller-side branch on
    backend."

## The one change that makes this elite for Memex

Fold **filters, snippets, and a slot for vector-source hits** into
`SearchHit`/`SearchRows` *now*, even if vectors ship later. That single shape
change is what lets Memex delete its retrieval branching. Without it, this
design fixes the FTS seam but leaves the fragmentation Memex is actually
paying for.

## Appendix: Grouped by change type

### A. Simple administrative changes to the design doc

- **#16** Resolve open question #1 — commit to FTS builder with statically
  typed `execute() -> SearchRows`.
- **#17** Rename "adaptive" → "graceful" / "two-stage."
- **#18** Fix heading levels under "Core Architecture Changes."
- **#19** Collapse the duplicated `fallback_search` mention in "Proposed
  Public Surface."
- **#20** Add the "engine owns fusion; caller owns reranking" principle line.
- **#21** Rewrite Done-When as product-shaped acceptance.
- **#5** Explicitly note "find related from seed" as specialized-surface
  future work.
- **#4 (doc half)** Add a one-paragraph scope statement: v1 is lexical;
  vector fusion lands behind the same `SearchHit` surface later.

### B. Straightforward FathomDB implementation

- **#2** Add `snippet: Option<String>` to `SearchHit`, wire FTS5 `snippet()`.
- **#3** Add `written_at` + originating projection row id to `SearchHit`.
- **#9** Cap relaxed-branch candidate count; document the cap.
- **#10** Guardrails on recursive property extraction: max depth, max
  extracted bytes per node, exclude-paths list.
- **#12** Add logical_id lexicographic tiebreaker in merge.
- **#13** Define `matched_path` tie rule (highest-scoring leaf, document
  order).
- **#15** Make `fallback_search` accept `relaxed=None` for strict-only dedup
  path.
- **#6** Document the concurrency contract (background writes don't block
  foreground reads) plus one assertion test.

### C. Complex FathomDB code changes

- **#1** First-class filters in the search builder (kind/tag/date-range/
  pinned), with engine-side fusion into filtered FTS plans. Touches query
  AST, compiler, engine execution, and all three SDKs.
- **#4 (code half)** Reserve the `SearchHit` shape and result plumbing for a
  future `SearchHitSource::Vector` without locking in a wire format that will
  need to change.
- **#14** Schema migration path for adding the recursive flag to an existing
  property-FTS schema (rebuild orchestration, restore parity, integrity
  checks).

### D. Needs HITL decision before implementation

- **#7** Score comparability: pick (a) per-branch normalization, or (b) drop
  the "comparable" claim and rely on branch precedence. Affects ranking
  contract and cross-language parity fixtures.
- **#8** Fallback trigger for v1: zero-hits-only vs. top-up-to-limit-with-
  penalty. Shapes the policy machinery.
- **#11** Tokenizer commitment: `unicode61 remove_diacritics 2` alone, with
  porter stemming, or something else. Affects index format and rebuild
  semantics — hard to change later.
