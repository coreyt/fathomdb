---
status: COMPLETE
---

# EARP Slice 1 — gold basis

Design of record for S1 of `dev/plans/earp-foundation.md`. Governed by rulings
D-6 (IR-C reuse tier under four conditions) and D-1/D-2 (developer harness,
never a gate). **Revision 2** — amended after the independent code-grounded
review recorded in § Review.

## Contract

S1 makes the gold trustworthy *before* anything measures against it. It is
pure: no SDK, no database, no network. It answers one question — "may this gold
be used, and under what identity?" — and it answers with either a typed
identity or a typed blocker, never with a partial result or a silent default.

Everything downstream depends on this. Because EARP never gates FathomDB,
nothing downstream will catch a wrong gold basis, so the refusals here are the
only thing standing between a stale cache and a confident false number.

## Inputs and path resolution

| Input | Source | Resolution |
| --- | --- | --- |
| `data_root` | config `corpus.data_root` | As given; gitignored tree |
| gold path | config `gold.path` | Absolute, else relative to `data_root` |
| `gold.sha256` | config | Content pin (D-6.1) |
| `gold.corpus_hash` | config | Declared corpus pin, three-way checked |
| `gold.qrels_version` | config | Declared version, exact-match checked |
| snapshot | config `corpus.snapshot` | Absolute, else relative to **repo root** |
| manifest | config `corpus.manifest` | Absolute, else relative to **repo root** |

The two resolution roots are different and that is deliberate: the gold lives
under a gitignored `data_root`, while `tests/corpus/snapshot.json` and
`tests/corpus/scripts/manifest.json` are committed in-repo. Resolving all three
the same way would send the snapshot lookup into `data_root` and fail. Repo
root is the one `experiments/_lib.py:41` already establishes.

When `gold.path` is relative and no `data_root` is configured, that is a
configuration-resolution error owned by S3, not a runtime blocker: the config
was never executable.

## Behaviour

Checks run in this order, and the first failure returns; later checks are not
attempted against an input already known to be untrustworthy.

| # | Condition | Outcome |
| ---: | --- | --- |
| 1 | `data_root` configured but absent on disk | `corpus_root_absent` |
| 2 | Gold file absent | `gold_missing` |
| 3 | Computed SHA-256 ≠ `gold.sha256` | `gold_hash_mismatch` |
| 4 | Gold bytes are not valid JSON, or not a conforming GoldSet | `gold_malformed` |
| 5 | Snapshot absent, unreadable, or lacking `corpus_hash` | `snapshot_unreadable` |
| 6 | Gold `corpus_hash` ≠ snapshot `corpus_hash` ≠ config `gold.corpus_hash` | `gold_corpus_mismatch` |
| 7 | Gold `qrels_version` ≠ config `gold.qrels_version` | `gold_stale_qrels_version` |
| 8 | All pass | `CorpusIdentity` + `GoldIdentity` + typed `GoldSet` |

Ordering is load-bearing. The hash pin (3) precedes every semantic check
because a file whose bytes are not the pinned bytes cannot have its fields
trusted at all — reading `corpus_hash` out of an unverified file and reporting
a mismatch would name the wrong defect. Parsing (4) sits after the pin for the
same reason, and before the field checks that depend on it.

Check 6 is a **three-way** equality: the config's declared pin, the gold file's
own field, and the snapshot's field must all agree. Comparing the gold's field
to the snapshot's field is the correct comparison — it is the same value the
generator wrote from that source (`build_ir_gold.py:231`) — and hashing the
snapshot *file* would be a weaker, different check. `manifest.json` carries no
`corpus_hash` at all; it is raw-acquisition provenance only.

### Why v1 is refused

`build_ir_gold.py:46` emits `ir-c-reused-v2`; every cached file on disk says
`ir-c-reused-v1`. The refusal is **provenance and version hygiene**: a gold set
must declare the version its committed generator actually emits, so the
identity recorded alongside every number cannot name a version no code
produces.

It is *not* a metric trap, and an earlier revision of this design wrongly said
so. Verified: `evidence_spans` is non-empty on zero of 4,597 source rows, so
regenerating to v2 produces zero span locators. The only differences are the
version string, the tracer renames `_source`/`_answer_type` →
`source`/`answer_type`, and `query_origin: "human_dataset"` — which the
reference already defaults to when absent (`ir_eval.rs:292-296`). Regeneration
is therefore cheap and carries no re-baselining. The blocker message says this,
so an operator is never told to regenerate for a reason that is untrue.

The rule is an **exact match against the declared version**, not membership in
a refused set. A denylist fails open: when the generator moves to v3, a stale
v2 cache would pass silently, reintroducing exactly what D-6.3 exists to
prevent. An exact match fails closed and needs no maintenance.

## Typed load

Beyond identity, S1 returns the gold as typed structures, because S2's metric
port consumes them and an untyped dict would push field-name drift into the
metric layer. Fields fall into three classes:

**Validated against closed vocabularies** — an unknown value is
`gold_malformed`, matching the reference's hard errors:

- `query_class` (`ir_eval.rs:250-252`)
- `necessity` (`ir_eval.rs:301-311`)
- `query_origin` (`ir_eval.rs:292-296`). This one is *not* an inert tracer:
  `templated` marks high lexical-leakage risk held to a higher validation bar
  (`ir_eval.rs:100-116`). Blanket retention would let `"templeted"` through and
  score leaked queries as clean.

**Typed but optional** — accepted under the v2 names with the `_`-prefixed
legacy fallback the reference implements (`ir_eval.rs:284-291`):
`relation_type`, `chain_shape`, `source`, `answer_type`, and the GoldSet-level
`note`, which names the generator and reuse tier and is exactly the provenance
this slice exists to preserve.

**Genuinely unknown** — retained in a named `extra: Mapping[str, Any]` field.
Slice 0's dataclasses are frozen with fixed fields, so retention needs an
explicit slot; it does not happen by itself.

This is the one place EARP is deliberately permissive, and the asymmetry is
justified: IR-B defines the gold schema as an additive superset
(`ir_eval.rs:174-177`) and it is owned upstream, whereas EARP's own
configuration is owned by EARP and stays strict.

`query_id` is optional in the reference (`ir_eval.rs:178`) but is the pairing
key for comparisons (`earp.per-query.v1.schema.json`). All 4,597 real queries
carry one. S1 **requires** it and refuses a gold set with any query lacking
one, rather than letting S8's pairing degrade silently.

Shapes follow the reference: `locator` is optional (`ir_eval.rs:170`) and
`Locator.spans` is optional, even though today every unit is
`{"kind": "whole_body"}` with no spans.

`query_count` is `len(queries)` — the total, including the 125 negatives.

S1 loads exactly the one file named by `gold.path`. The three per-source files
(`enronqa` 710, `qaconv` 2303, `qmsum` 1584, same corpus hash and version) are
not read; per-source stratification derives from each query's `source` field,
which is what S7 depends on.

## Non-goals

- No gold generation. S1 verifies and refuses; it never writes gold.
- No regeneration. It names the command and blocks; running
  `build_ir_gold.py` is an operator action against real corpus data.
- No metric computation. That is S2.
- No claim about gold quality. D-6.4 scopes this to reuse-tier,
  document/body-level evidence gold.

## S0 lock amendment carried by this slice

`BlockerCode` is a closed vocabulary, so the two outcomes added above cannot be
improvised at implementation time. This slice amends the Slice 0 lock to add
`gold_malformed` and `snapshot_unreadable`, in both
`schema/models.py` and the `blocker` enum of
`schema/earp.result.v1.schema.json`. The amendment is reviewed as part of this
design rather than slipped in during implementation.

## Acceptance criteria

1. Each of the seven refusals returns its own blocker code, with a message
   naming the specific mismatch and, where applicable, the remedy.
2. A valid gold returns `CorpusIdentity`, `GoldIdentity`, and a typed
   `GoldSet`.
3. Snapshot identity and manifest provenance are recorded as separate fields
   and never merged into one "corpus hash". `snapshot_sha256` and
   `manifest_sha256` are hashes of those *files*, distinct from the
   `corpus_hash` *field* read out of the snapshot body.
4. The check order is observable: a file failing both the hash pin and the
   corpus cross-check reports `gold_hash_mismatch`.
5. An unknown `query_class`, `necessity`, or `query_origin` value is refused,
   not retained.
6. Every path is exercised by a test that was first observed to fail.

## Review

Independent code-grounded review, 2026-08-06. Verdict: **proceed with specified
revisions**; the slice's structure was sound and no rework was warranted. All
findings are resolved in this revision.

| # | Severity | Finding | Resolution |
| ---: | --- | --- | --- |
| 1 | BLOCKER | The v1-refusal rationale was factually false — `evidence_spans` is non-empty on zero of 4,597 source rows, so v2 is semantically identical to v1 and no span locator or metric change appears | Rationale rewritten as provenance/version hygiene; measured evidence recorded; refusal retained |
| 2 | BLOCKER | No typed outcome for gold that will not parse, or an unreadable snapshot, against a deliberately closed blocker vocabulary | `gold_malformed` and `snapshot_unreadable` added as a reviewed S0 amendment; both placed in the ordering table |
| 3 | MAJOR | `gold.corpus_hash` and `gold.qrels_version` were required by the config schema but consumed nowhere, contradicting S3's unused-key rejection | Both now consumed: three-way corpus equality, exact version match |
| 4 | MAJOR | A refused-*set* fails open at v3 | Replaced with exact match against the declared version |
| 5 | MAJOR | The permissive-gold rationale cited v1 key names, and blanket retention would swallow `query_origin`, a leakage-provenance field the reference hard-errors on | Split into validated / typed-optional / `extra`; v2 names used; `query_origin` validated |
| 6 | MAJOR | The declared return could not satisfy AC-3, since neither `GoldIdentity` nor `GoldSet` carries snapshot or manifest identity | `CorpusIdentity` added to the return; file-hash vs body-field distinction stated |
| 7 | MAJOR | Path resolution was stated only for the gold path; snapshot and manifest must *not* resolve against `data_root` | Three explicit resolution rules; unconfigured-root case assigned to S3 |
| 8 | MINOR | `query_count` ambiguous; per-source files unaddressed | Defined as `len(queries)`; per-source stratification derives from the `source` field |
| 9 | MINOR | Type drift: `note` dropped, `locator` optionality, `query_id` optional in reference but load-bearing for pairing | `note` retained; optionality matched; `query_id` required with explicit refusal |

Confirmed correct by the review, with no change required: the snapshot
cross-check is the right comparison and passes today; the hash-pin-first
ordering argument is sound; `GoldIdentity` matches the S0 lock and the JSON
Schema with no drift.
