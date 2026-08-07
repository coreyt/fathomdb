---
title: Retrieval Result Limits
date: 2026-08-07
target_release: 0.8.22
desc: Bounded, caller-selectable result cardinality for ranked retrieval
blast_radius: engine retrieval; Python SDK; TypeScript SDK; EARP measurement
status: ACTIVE
decision: steward ledger seq-245
---

# Retrieval Result Limits

## Purpose

Ranked retrieval must have an explicit, truthful result cardinality. Before
this contract, hybrid search could return every matching FTS row, text-only
search was unbounded, and projected-text search had an undocumented engine
limit of ten. That makes ordinary callers vulnerable to unexpectedly large
responses and makes an EARP K-ladder above ten unmeasurable through the public
SDKs.

## Contract

The ranked-result limit has one meaning: the maximum number of ranked search
hits returned to the caller. The omitted/default value is **10**. A caller may
request any integer from **1 through 100**, inclusive. Values outside that
range are rejected as a typed invalid-argument error; they are never silently
clamped. Silent clamping would make an experiment that requests `@200` report
a measurement it did not actually run.

The policy applies to these ranked result sets:

- hybrid `search`, including its filter, rerank, explain, and read-view forms;
- `search_text_only`;
- `search_projected_text`; and
- the `search_hits` member returned by `search_expand`.

`search_expand` therefore receives a `search_limit` with the same `10` default
and `100` maximum. Its graph expansion remains a separate traversal contract:
the existing per-root graph-neighbor hard cap is 50, and this slice does not
add a global expansion-result knob or change that traversal cap.

The policy does not apply to point reads, paginated operational-log reads,
`read_list`, `graph_neighbors`, validity-boundary reporting, or caller-supplied
`fused_rerank`. Those APIs have distinct cardinality semantics and remain
subject to their own contracts.

## Engine semantics

The engine validates the requested limit at its public boundary and carries the
validated value through every participating reader request. Each returned
ranked result set is bounded after validity and metadata filtering and after
the applicable fusion, graph-arm, and cross-encoder ranking work. A SQL
candidate limit must not cause invalid or filtered rows to consume the caller's
result budget.

For hybrid retrieval, the vector phase-2 rerank fanout must be at least the
requested result limit. The current `SEARCH_RERANK_LIMIT = 10` is an internal
default/floor, not a public hard ceiling; Slice 18 replaces that hidden behavior
with the caller-visible contract. The test-only `set_search_limit_for_test`
seam remains test-only and is not an SDK configuration path.

For FTS-only and projected-text retrieval, candidate collection must be bounded
without changing the promised top-ranked, post-filter result set. In particular,
an SQL `LIMIT` placed before a Rust-side filter is insufficient unless the
implementation overfetches or otherwise proves that it can still fill the
requested result count with valid hits.

## Binding contract

Python exposes a keyword-only `limit=10` on each direct search method and a
keyword-only `search_limit=10` on `graph.search_expand`. TypeScript exposes the
same optional fields in its public options shape. Rust preserves a no-argument
convenience form returning ten results and adds an explicit-limit form for each
ranked search family. All three layers must reject zero, negative binding input,
and values above 100 with their established invalid-argument taxonomy.

The exact binding spellings are owned by the interface documents. No new
test/evaluation-only setting becomes public.

## Acceptance criteria

- Every in-scope public ranked-search path returns no more than ten hits when
  its limit is omitted.
- Each path returns the requested top `K` for representative `K = 5, 20, 50`
  fixtures, and never more than 100 hits.
- `0`, negative binding values, and `101` produce typed invalid-argument
  failures in Rust, Python, and TypeScript; no call silently changes the
  requested value.
- Hybrid search's vector path can return and report a depth of 20 and 50; the
  old internal value of 10 cannot truncate either measurement.
- Text-only and projected-text paths enforce their public limit after their
  validity and metadata filters, preserving ranking order among retained hits.
- `search_expand.search_hits` obeys `search_limit`; its existing per-root
  expansion cap remains 50 and is covered by a regression assertion.
- The public API documentation, stubs/types, generated binding declarations,
  and conformance surface tests agree on every new argument and error.
- EARP can request `K = 5, 10, 20, 50` through the public Python and TypeScript
  SDKs without using an engine test seam.

## Implementation sequence

1. Add failing engine tests for default, selected, boundary, and post-filter
   cardinality before changing retrieval code.
2. Thread the validated limit through the common hybrid, FTS-only, projected
   FTS, and search-expand paths; then make the Rust tests green.
3. Add failing Python and TypeScript binding tests, expose the arguments and
   typed errors, and make parity tests green.
4. Update the interface documents from the implemented signatures, run the
   complete agent verification gate, and obtain independent review before the
   Slice 18 landing is considered.
