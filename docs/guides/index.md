# Guides

Guides are task-oriented public documents covering the 0.8.20 surface.

- [Working with structured search hits](structured-search-hits.md) — read the
  typed `id` (`IdSpace`), `kind`, `body`, `score`, `branch`, `source_id` and
  `ce_score` fields on each `search()` hit.
- [Hybrid search & filtering](hybrid-search-filtering.md) — RRF ranking and the
  closed `SearchFilter` metadata filter.
- [Retrieve by id](retrieve-by-id.md) — point lookups by `logical_id`
  (`read.get` / `read.get_many`) and paginated op-store read-back
  (`read.collection` / `read.mutations`).

See also [Erasure](../operations/erasure.md) for `purge` / `erase_source`.
