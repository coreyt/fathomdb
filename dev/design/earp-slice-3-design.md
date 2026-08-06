---
status: PROPOSED
---

# EARP Slice 3 — strict resolver and knob catalog

Design of record for S3 of `dev/plans/earp-foundation.md`. Depends on S2
(`check_depth`, eligibility) and on the S0 lock.

## Contract

S3 makes it impossible to express a run that cannot be honestly executed. A
configuration either resolves into a typed, fully-consumed scenario, or it is
rejected with a message naming the offending key. There is no partial
resolution and no defaulting-away of a bad value.

Pure: no SDK, no database, no network, no filesystem beyond reading the config
file itself.

## Four rejection classes

`earp.v1` is strict in four distinct ways, and they are not the same check:

| Class | Meaning | Example |
| --- | --- | --- |
| **unknown** | a key the schema does not define | `corpuss:` |
| **missing** | a required key absent | `gold` without `sha256` |
| **invalid** | a defined key with an out-of-domain value | `campaign: benchmark` |
| **unused** | a declared key no code path consumes | see below |

**Unused is the one that needs a mechanism, not just a rule.** S1's review
found exactly this defect in the shipped schema: `gold.corpus_hash` and
`gold.qrels_version` were *required* by `earp.config.v1.schema.json` yet
consumed nowhere, so a config could declare a pin that silently did nothing.
Prose cannot catch that; a resolver that records which keys it read can.

The resolver therefore tracks consumed key paths during resolution and, at the
end, refuses any declared path it never touched. This makes "unused" a
mechanical property of the code rather than a promise, and it means adding a
config key without wiring it is a test failure rather than a latent lie.

## The two-representation problem

`earp.config.v1.schema.json` is the declarative lock. The resolver is
hand-written Python. Two representations of one contract will drift.

They cannot simply be collapsed: `jsonschema` is **not** a declared dependency
of `src/python/pyproject.toml`, and the repo has already been bitten by a
harness importing an undeclared package — the numpy declaration there carries a
codex §9 [P1] note recording that a clean `pip install -e src/python[dev]`
failed with `ModuleNotFoundError`. Adding a dependency so a *developer harness*
can validate its own config is a poor trade.

So the resolver is dependency-free, and drift is closed by a test rather than a
library: the test parses the JSON Schema as plain JSON and asserts that the
resolver's own tables agree with it on required keys, enum domains, and
`additionalProperties: false` coverage. The schema stays the reviewable
artifact; the resolver stays importable anywhere; neither can drift without a
red test.

## Resolution order

1. Parse the file (YAML is a JSON superset; `pyyaml` is already a declared dep).
2. Structural validation — unknown, missing, invalid, per the schema tables.
3. Semantic validation, which needs cross-field knowledge:
   - **Depth.** Every `metrics.evidence_recall_k` entry is checked against the
     declared retrieval mode via `earp.depth.check_depth` (S2). A config asking
     for @20 on hybrid is refused *here*, at validation time, rather than
     producing three copies of the @10 number at run time.
   - **Campaign coherence.** `comparison` requires a `comparison` block;
     `characterization` must not carry one. A retrieval-quality campaign
     requires `gold`; `diagnostic` does not.
   - **Decision rule (D-4).** Optional. When present it is predeclared and
     persisted; when absent the resolved scenario records that no better-than
     claim may be made. Its `metric` must name a metric the campaign will
     actually emit — a rule keyed on a metric that resolves `not_applicable` is
     refused rather than silently never evaluated.
   - **Budget (D-3).** `budget.estimated_usd` is required for any campaign
     declaring a priced arm, and forbidden otherwise.
4. Consumption check — refuse unused declared paths.

Depth is checked at step 3 and again by the runner before retrieval. That is
deliberate duplication of the *call*, not of the *rule*: both call the single
`check_depth` predicate, so there is one source of truth and two enforcement
points.

## The knob catalog

Keyed on **whether a concrete SDK call path exists**, never on dataclass
membership — the correction S1's sibling review forced on the design of record.
Each entry carries a classification, a call path, a witness name, and a reason.

Verified entries for the current SDK:

| Knob | Class | Call path | Note |
| --- | --- | --- | --- |
| `use_default_embedder` | semantic | `Engine.open(use_default_embedder=)` | the only `EngineConfig`-adjacent setting reaching native open |
| `slow_threshold_ms` | runtime | `Engine.set_slow_threshold_ms` | an `EngineConfig` field `Engine.open` never forwards, yet independently supported |
| `profiling` | observability | `Engine.set_profiling` | sibling of the above |
| `embedder_pool_size` | unsupported | — | `EngineConfig` field, never forwarded, no independent path |
| `scheduler_runtime_threads` | unsupported | — | as above |
| `provenance_row_cap` | unsupported | — | as above |
| `embedder_call_timeout_ms` | unsupported | — | as above |
| `rerank_depth`, `use_graph_arm`, `alpha`, `pool_n` | semantic | `Engine.search(...)` | real search parameters |

`pool_n` is classified `semantic` but carries an explicit note that it is the
CE-rerank pool size and **not** a result-depth control — the confusion that
would otherwise look like a workaround for the depth cap.

### Completeness without reflection

The design forbids reflection over public Python parameters as a proxy for a
usable configuration surface. But a maintained list can also silently omit
things, so the completeness test is bounded and targeted rather than open:

- Every field of `EngineConfig` must appear in the catalog with an individual
  verdict. This *is* introspection, of one known type, asserting coverage — not
  reflection defining the surface. It is what would have caught the original
  design's blanket "all `EngineConfig` fields are unsupported" claim.
- Every `supported` entry (`semantic`, `indexing`, `runtime`, `observability`)
  must carry a non-empty `call_path` and `witness`.
- Every `unsupported` / `held_constant` entry must carry a reason and no call
  path.

## `earp validate`

A CLI entry point that resolves a config and prints either the resolved
scenario or the rejection. Exit 0 on success, non-zero on rejection. It is the
only user-facing surface in this slice, and it performs no I/O beyond reading
the named file — in particular it does **not** touch gold, corpus, or an
engine, so it runs anywhere including a worktree with no data.

## Non-goals

- No gold verification. That is S1, and `earp validate` deliberately does not
  call it: a config can be *well-formed* while its data is absent, and
  conflating the two would make validation impossible without the gitignored
  corpus.
- No run execution, no artifact writing, no engine.
- No defaulting of absent optional blocks into synthesised values — an absent
  `decision_rule` stays absent and is recorded as such.

## Acceptance criteria

1. Unknown, missing, invalid, and unused keys are each refused, with distinct
   messages naming the offending path.
2. A config declaring a key no resolver path consumes is refused, demonstrated
   by a test that adds a schema key without wiring it.
3. The resolver's tables and `earp.config.v1.schema.json` agree on required
   keys, enum domains, and closed-object coverage, asserted without importing
   `jsonschema`.
4. `evidence_recall_k` entries beyond a mode's measurable depth are refused at
   validation time, via `check_depth`.
5. A `decision_rule` naming a metric the campaign cannot emit is refused.
6. Campaign coherence is enforced in both directions: a comparison without a
   `comparison` block, and a characterization carrying one, are both refused.
7. Every `EngineConfig` field appears in the catalog with an individual
   verdict; every supported entry has a call path and a witness.
8. `earp validate` exits 0 on a good config and non-zero on each rejection
   class, touching no gold, corpus, or engine.
9. Every path is exercised by a test that was first observed to fail.

## Review

Pending — an independent code-grounded review is required before
implementation, per the per-slice governance in the plan.
