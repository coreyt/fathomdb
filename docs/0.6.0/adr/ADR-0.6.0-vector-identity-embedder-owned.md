---
title: ADR-0.6.0-vector-identity-embedder-owned
date: 2026-04-27
target_release: 0.6.0
desc: Vector identity (model_identity, model_version, dimension, normalization) belongs to the embedder, never to vector configs
blast_radius: vector config schemas; regeneration code path; embedder trait; design/vector.md; design/embedder.md; interfaces/rust.md; interfaces/python.md; interfaces/typescript.md; ADR-0.6.0-embedder-protocol; PR review checklist
status: accepted
---

# ADR-0.6.0 — Vector identity belongs to the embedder

**Status:** accepted (HITL 2026-04-27, decision-recording).

Promoted from critic-3 M-4. The invariant exists today only as a
memory record (`project_vector_identity_invariant`, established
2026-04-14). It is load-bearing for ADR-0.6.0-embedder-protocol
(`Embedder::identity()` is the source of truth) and for any future
vector-config schema. Elevating to ADR makes the rule citable in PR
review and in design docs without a memory dependency.

## Context

Pre-invariant (0.4.x and earlier): `VectorRegenerationConfig` carried
both identity strings (model_identity, model_version, dimension,
normalization_policy) **and** a `generator_command: Vec<String>`
subprocess invocation. The strings declared what model would be used;
the subprocess actually produced the bytes. Two sources of truth =
structural model drift: a config could declare
`"sentence-transformers/all-MiniLM-L6-v2"` but the subprocess could
silently invoke a different binary, producing vectors that didn't
match the declared identity.

The 2026-04-14 fix moved identity to the embedder and removed the
subprocess generator field entirely from fathomdb. Regeneration
takes an `&dyn QueryEmbedder`; configs carry only destination /
chunking / preprocessing.

## Decision

### Rule

Vector identity is the embedder's responsibility. Vector
configuration structs **never** carry:

- model identity strings
- model version strings
- dimension
- normalization policy
- subprocess commands that would compute embeddings

Vector configuration structs **may** carry:

- profile name
- destination table name / column / vector slot
- chunking policy (chunk size, overlap, splitter type)
- input preprocessing (text-side hints only: language hint, strip
  whitespace, choose-which-field-to-embed). These do not change
  vector identity because they affect *which text* is embedded, not
  *how text→vector* is computed.
- batch sizing
- retry behavior

Forbidden in vector configs even though they look "preprocessing":

- Unicode normalization form (NFC / NFKC / NFD / NFKD) — different
  forms produce different embeddings; embedder-owned.
- Casefold / lowercase — embedder-owned (model tokenizer decides).
- Tokenizer-level transforms — embedder-owned by definition.
- Vector L2-normalization toggle — identity-side; embedder always
  returns unit-norm per ADR-0.6.0-embedder-protocol §Invariant 1.

### Authoritative source

`Embedder::identity() -> EmbedderIdentity` is the single source of
truth for `(model_identity, model_version, dimension,
normalization_policy)`. Per ADR-0.6.0-embedder-protocol:

```rust
pub trait Embedder: Send + Sync {
    fn identity(&self) -> EmbedderIdentity;
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError>;
}
```

Any code path that produces or regenerates vectors must take an
`&dyn Embedder` (or equivalent) and read identity from the impl.

### Engine regeneration entry point

Any engine-level regeneration entry point reads the embedder from
open-time engine state and errors if absent. No implicit fallback to
"configure your own embedder per call." Embedder is explicit at open
time, or the operation fails. Specific method names and signatures
live in `design/engine.md` / `interfaces/*.md`.

### Identity-vs-stored-vectors check (open-time)

On `Engine.open` with an embedder argument, for each existing vector
profile recorded in the database:

- Compare the open-time `embedder.identity()` against the identity
  recorded for that profile at first write.
- If identities differ, fail with
  `EngineError::EmbedderIdentityMismatch { profile, recorded,
  supplied }`.
- A user who *intends* to swap embedders passes
  `Engine.open(... accept_identity_change: true)` (or equivalent
  per-binding flag). This permits open; the recorded identity is
  **not** updated until the user runs an explicit `regenerate`
  against that profile.

The DB-recorded identity is a **derived projection** of "the
embedder that was used to write these vectors." It is not a third
source of truth; it is a historical record used to detect drift. The
authoritative identity for new writes is always
`embedder.identity()`. This check distinguishes from rejected Option
C1 (config-declared identity strings — those would be a parallel
declaration; this is a recorded fingerprint).

### Subprocess generator pattern

Removed from fathomdb proper. If a future client needs
subprocess-driven regeneration, they implement a `SubprocessEmbedder`
adapter against the `Embedder` trait in their own code. fathomdb does
not host subprocess-execution machinery as a first-class config
option.

### Extension shape

New ways to compute embeddings (cloud APIs, different frameworks,
fine-tuned variants, multi-embedder A/B harnesses) plug in by
implementing `Embedder`. fathomdb's regeneration code is closed for
modification, open for extension via the trait.

## Options considered

**A — Embedder owns identity; configs carry destination/chunking
only (chosen).** Pros: structurally prevents model drift; matches
the trait contract already accepted in
ADR-0.6.0-embedder-protocol; eliminates the subprocess-generator
class entirely. Cons: 0.4.x clients that relied on string-based
identity in configs cannot migrate without code change.
Acceptable per ADR-0.6.0-no-shims-policy (no 0.5.x→0.6.0 shims; no
upgrade path for existing users in 0.6.0).

**B — Configs carry identity strings; embedder is a separate
"computation" object.** Pros: declarative configuration, no required
code. Cons: this is the pre-invariant pattern; structural model
drift is back. Rejected — the entire reason for the invariant.

**C1 — Config-declared identity strings, runtime cross-check.**
Pros: keeps strings in configs as documentation while preventing
drift. Cons: still two sources of truth (config + embedder); the
cross-check adds failure modes (typos, version-string drift)
without a real benefit; configs become "declared identity that must
agree with embedder" which is exactly the ambiguity 2026-04-14
removed. **Rejected.**

**C2 — DB-recorded identity check at open-time.** Pros: detects a
swapped embedder.bin / model-cache poisoning between runs. Cons:
none structurally — the recorded identity is a derived projection,
not a parallel declaration. **Accepted** and folded into the
Decision section as the open-time identity check.

**D — Identity stored on the vector-table schema, embedder must
match.** Pros: identity is durable in the database, surviving
embedder swaps. Cons: pushes identity into a third place (config,
embedder, on-disk schema); makes embedder swap ("upgrade to a better
model") a structural-migration event rather than a config change.
Rejected; identity belongs to whatever computes the bytes, not to
where the bytes are stored.

## Consequences

- `design/vector.md` (Phase 3) documents the rule and the schema
  shape: vector config structs carry only profile / destination /
  chunking / preprocessing / batching / retry fields.
- `design/embedder.md` (Phase 3) documents `EmbedderIdentity` as the
  authoritative struct and lists every field that lives there
  (model_identity, model_version, dimension, normalization_policy)
  + when each may be populated (e.g. fine-tuned variants override
  model_version).
- `interfaces/rust.md`, `interfaces/python.md`,
  `interfaces/typescript.md` document the Embedder trait / ABC /
  interface with `identity()` as a required method on every binding.
- `interfaces/python.md` documents that Python embedders must
  implement `identity(self) -> EmbedderIdentity`; binding fails fast
  if absent.
- Engine `Engine.open` records the open-time embedder's identity in
  engine coordinator state; subsequent regeneration calls read from
  there.
- Any vector-config struct in any binding (Rust, Python, TS) that
  contains a field for model identity, model version, dimension, or
  normalization is a **build-blocker** at PR review.
- **Lint enforcement (followup, tracked as EMB-7).** Structural
  check, not grep: a unit test (or CI macro / typegraph walk) that
  asserts no struct reachable from `VectorConfig` references
  `EmbedderIdentity` or any of its fields by type. Grep on field
  names misses aliases (`model_name`, `dim`, `vector_size`,
  `revision`) and cannot detect a `String` field semantically used
  as identity. Concrete crate path + check shape land with EMB-7.
- **Dimension validation when no embedder is open (read-only / query
  paths).** The engine MAY cache `(profile, dimension)` in
  coordinator state at first write, so subsequent read paths can
  validate ADR-0.6.0-zerocopy-blob §Z-4 byte-length without an open
  embedder. This cache is a derived projection of the open-time
  embedder's identity; it is not a third source of truth. Cross-cite
  ADR-0.6.0-zerocopy-blob §Z-4.
- **Memory hygiene.** `project_vector_identity_invariant` memory
  record must be rewritten to a single-line pointer at this ADR in
  the same commit that lands this ADR. The memory's prior rule text
  is now superseded.
- Phase 2 decision-index does not need a separate entry for this
  invariant; the rule is recorded here and cited from design docs.
- Cross-cite ADR-0.6.0-embedder-protocol § Trait shape: the trait's
  `identity()` method is the implementation of this ADR.
- Cross-cite ADR-0.6.0-no-shims-policy: 0.4.x configs with identity
  strings are not accepted under any compat flag.

## Non-consequences (what this ADR does NOT do)

- Does not specify the wire shape of `EmbedderIdentity`
  (design-level; lives in design/embedder.md).
- Does not specify how identity participates in cache keys for
  HF-Hub model artifacts (separate followup design,
  EMB-5 in critic-3 carryover).
- Does not specify how identity is surfaced in `safe_export` / op-store
  rows (covered separately by op-store ADR consequences and OPS-2
  followup).
- Does not forbid vector-config structs from carrying a
  human-readable `display_name` or `description` field; those carry
  no identity semantics.
- Does not prejudge whether identity is hashed or string-typed —
  design decision.

## Citations

- HITL decision 2026-04-27 (M-4 elevation per critic-3).
- Memory `project_vector_identity_invariant` (established
  2026-04-14; superseded by this ADR — memory should now be a
  pointer, not a primary record).
- ADR-0.6.0-embedder-protocol § Trait shape, § Invariants 1–5.
- ADR-0.6.0-no-shims-policy (no 0.4.x compat).
- 0.4.x task #7 / GH #39 "write-time embedder parity" (the original
  forcing-function for the invariant).
- Stop-doing entry: per-item variable embedding (identity leaked
  into vector config).
