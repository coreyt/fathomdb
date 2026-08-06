---
status: PROPOSED
---

# EARP Slice 1 — gold basis

Design of record for S1 of `dev/plans/earp-foundation.md`. Governed by rulings
D-6 (IR-C reuse tier under four conditions) and D-1/D-2 (developer harness,
never a gate).

## Contract

S1 makes the gold trustworthy *before* anything measures against it. It is
pure: no SDK, no database, no network. It answers one question — "may this gold
be used, and under what identity?" — and it answers with either a typed
identity or a typed blocker, never with a partial result or a silent default.

Everything downstream depends on this. Because EARP never gates FathomDB,
nothing downstream will catch a wrong gold basis, so the refusals here are the
only thing standing between a stale cache and a confident false number.

## Inputs

| Input | Source | Purpose |
| --- | --- | --- |
| `data_root` | config `corpus.data_root` | Explicit root for gitignored data |
| gold path | config `gold.path` | Relative to `data_root` when not absolute |
| `gold.sha256` | config | Content pin (D-6.1) |
| `gold.corpus_hash` | config | Cross-check against the snapshot |
| snapshot path | config `corpus.snapshot` | `corpus_hash` is read from its body |
| manifest path | config `corpus.manifest` | Raw-corpus provenance only (D-6.2) |

## Behaviour

Checks run in this order, and the first failure returns; later checks are not
attempted against an input already known to be untrustworthy.

| # | Condition | Outcome |
| ---: | --- | --- |
| 1 | `data_root` configured but absent on disk | `corpus_root_absent` |
| 2 | Gold file absent | `gold_missing` |
| 3 | Computed SHA-256 ≠ `gold.sha256` | `gold_hash_mismatch` |
| 4 | Gold `corpus_hash` ≠ snapshot `corpus_hash` | `gold_corpus_mismatch` |
| 5 | Gold `qrels_version` in the refused set | `gold_stale_qrels_version` |
| 6 | All pass | `GoldIdentity` + typed `GoldSet` |

Ordering is load-bearing. The hash pin (3) precedes the semantic checks (4, 5)
because a file whose bytes are not the pinned bytes cannot have its fields
trusted at all — reading `corpus_hash` out of an unverified file and reporting
a mismatch would name the wrong defect.

`ir-c-reused-v1` is refused because the committed generator emits
`ir-c-reused-v2` with span locators (`build_ir_gold.py:46,106-127`) while the
cached files on disk are v1. Using the cache unvalidated is a silent metric
change, not a missing feature, which is why it is a blocker rather than a
warning. The refusal names the regeneration command.

## Typed load

Beyond identity, S1 returns the gold as typed structures, because S2's metric
port consumes them and an untyped dict would push field-name drift into the
metric layer:

- `EvidenceUnit` — `evidence_id`, `doc_id`, `necessity`, `locator`
- `GoldQuery` — `query_id`, `query`, `query_class`, `required_evidence`,
  `expected_top_k_doc_ids`
- `GoldSet` — `corpus_hash`, `qrels_version`, `queries`

Unknown top-level keys in a gold query are **retained, not rejected**. This is
the one place EARP is deliberately permissive: the real gold carries
`_source`, `_answer_type`, and `relation_type`, IR-B §(b) defines the schema as
an additive superset, and a strict reject here would refuse the very artifact
D-6 authorizes. Strictness belongs on EARP's own configuration, which EARP
owns; the gold schema is owned upstream.

## Non-goals

- No gold generation. S1 verifies and refuses; it never writes gold.
- No regeneration. It names the command and blocks; running
  `build_ir_gold.py` is an operator action against real corpus data.
- No metric computation. That is S2.
- No claim about gold quality. D-6.4 scopes this to reuse-tier,
  document/body-level evidence gold.

## Acceptance criteria

1. Each of the five refusals returns its own blocker code, with a message
   naming the specific mismatch and, where applicable, the remedy.
2. A valid gold returns a `GoldIdentity` carrying path, sha256, corpus_hash,
   qrels_version, and query_count, plus a typed `GoldSet`.
3. Snapshot identity and manifest provenance are recorded as separate fields
   and never merged into one "corpus hash".
4. The check order above is observable: a file that fails both the hash pin
   and the corpus cross-check reports `gold_hash_mismatch`.
5. Every path is exercised by a test that was first observed to fail.

## Review

Pending — an independent code-grounded review is required before
implementation, per the per-slice governance in the plan.
