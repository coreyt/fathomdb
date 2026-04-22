# Pathway To Basic Cypher Support

Date: 2026-03-28  
**Superseded by:** `dev/pathway-to-basic-cypher-2026-04-17.md`

This document's strategic framing remains valid. The 2026-04-17 document extends it with a
concrete implementation plan, full conformance matrix, explicit AST types, translator rules, and
sequencing relative to 0.5.1 and 0.6.0. Read this document for the "why"; read the 2026-04-17
document for the "how".

---

## Recommendation

If the goal is market acceptance, the right move is to add **Cypher as a compatibility layer over the existing FathomDB query engine**, not as a replacement for the AST approach.

The key point is that the current system is SDK/AST-first: the Python surface builds AST payloads and sends them into the Rust core ([python/fathomdb/_query.py](/home/coreyt/projects/fathomdb/python/fathomdb/_query.py), [crates/fathomdb-query/src/lib.rs](/home/coreyt/projects/fathomdb/crates/fathomdb-query/src/lib.rs)). So the clean architecture is:

- keep AST or a new internal IR as the canonical execution model
- add `Cypher -> internal query model` translation
- reuse the existing planner/compiler to SQLite SQL

I would start in this order.

## 1. Define the scoped Cypher contract first

Do not start with “Cypher support” as a slogan. Start with a compatibility matrix.

A realistic first subset is:

- `MATCH` with one root node label
- one relationship hop
- directed edge type
- parameterized equality predicates
- `RETURN` of one node alias
- `LIMIT`

Example supported shape:

```cypher
MATCH (m:Meeting)-[:HAS_TASK]->(t:Task)
WHERE m.logical_id = $id
RETURN t
LIMIT 10
```

That is small enough to ship, document, and test honestly.

## 2. Do not map Cypher directly to SQL

That would fork the planning logic and create two compilers to maintain.

Instead:

- short term: `Cypher subset -> current AST`, where possible
- medium term: introduce a slightly richer internal IR, then compile both AST and Cypher into that IR

This matters because the current AST is too narrow for meaningful Cypher beyond toy support. Today it has only:

- vector search
- text search
- one traversal
- simple filters
- node-centric results

See [crates/fathomdb-query/src/ast.rs](/home/coreyt/projects/fathomdb/crates/fathomdb-query/src/ast.rs) and [crates/fathomdb-query/src/compile.rs](/home/coreyt/projects/fathomdb/crates/fathomdb-query/src/compile.rs).

## 3. Extend the internal model before promising real scoped Cypher

If you want Cypher to feel useful, you likely need a richer internal query model than the current AST.

The first missing pieces are:

- alias-aware predicates
- projection / `RETURN`
- parameter handling as first-class values
- distinction between root-node filters and terminal-node filters
- multiple bound variables in one query shape

Without that, many normal Cypher queries cannot map cleanly.

## 4. Put the first compatibility layer in Python

Given that the Python library is already “sufficient,” the fastest market-validation path is:

- add `db.cypher(query: str, params: dict | None = None)`
- parse a very small subset
- translate it to AST/IR
- execute through the existing engine
- raise a precise `UnsupportedCypherFeature` for anything outside the matrix

That gives you:

- immediate user-facing syntax compatibility
- fast iteration on which Cypher constructs people actually need
- no premature engine-wide protocol work

Once the subset stabilizes, move parser/translator logic into Rust so it is shared across SDKs.

## 5. Do syntax compatibility before protocol compatibility

For adoption, syntax matters earlier than Bolt.

Phase order should be:

1. Cypher text accepted in Python API
2. documented compatibility matrix
3. stable translation to AST/IR
4. cross-language support
5. only then consider Bolt / Neo4j driver compatibility if demand justifies it

Bolt is much more expensive and forces server/protocol decisions. Scoped Cypher strings inside the Python SDK are much cheaper and probably enough to test market pull.

## 6. Keep Fathom-native features outside initial Cypher

Do not try to force vector and multimodal features into generic Cypher immediately.

Instead:

- use Cypher first for graph/document retrieval compatibility
- keep vector/FTS flows on the native AST API at first
- later add explicit Fathom extensions, for example `CALL fathom.vector_search(...)`

That keeps the compatibility story honest.

## 7. Ship with a conformance matrix, not marketing language

You should publish something like:

- Supported: single-hop `MATCH`, equality `WHERE`, single-node `RETURN`, `LIMIT`
- Not yet supported: `WITH`, aggregation, `OPTIONAL MATCH`, writes, path returns, multi-hop variable paths, alias-scoped mixed predicates, Bolt

That is how you avoid “Cypher support” becoming a credibility problem.

## What I would do concretely

- Add `db.cypher(...)` to the Python SDK.
- Implement a tiny parser for a documented subset.
- Translate into the current AST only for cases that fit.
- In parallel, design a richer internal IR so Cypher support can grow without contorting the AST.
- Keep AST as the canonical execution boundary for now.
- Delay Bolt entirely.

## Strategic recommendation

Add scoped Cypher as a compatibility facade in the Python SDK first, but treat it as a translation layer into FathomDB’s native internal model. Do not build a second independent query engine, and do not start with Bolt.
