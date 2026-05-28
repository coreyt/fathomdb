# Corpus creation — extending

Recipes for the most common extensions. Each recipe lists the
files to touch, the invariants to preserve, and the validation
that lets you know the change landed cleanly. Read
[`architecture.md`](./architecture.md) first — these recipes
assume you understand the source vocabulary, document schema,
and license-posture story.

## A. Add a new source

Use case: you want to add another `email` / `note` / `paper` /
etc. provider (e.g. PMC OA, S2ORC, ELITR — all currently
deferred).

**Decision checklist (before any code):**

1. **License posture.** Pin upstream's license at a known
   revision. Decide `commit` vs `cache` posture — see
   `architecture.md` §6. If the upstream chain is murky (QMSum's
   AMI/ICSI heritage is the canonical example), prefer `cache`
   and capture the unresolved chain in the manifest's
   `license_notes` field. **Do not silently flip a deferred
   `cache` source to `commit` without a HITL pass.**
2. **`source_type` mapping.** Pick exactly one of
   `email|meeting|paper|article|note|todo`. **Do not add a 7th
   value** — vec0 partition_key cardinality, ingest harness
   vocabularies, and chain shapes all depend on it.
3. **Native ID stability.** What field on the upstream record do
   you treat as the per-row stable identifier? Pick one such that
   `doc_id(provenance, native_id)` is deterministic across
   re-acquisitions.
4. **Determinism strategy.** What's the document selection order?
   (Sorted-by-key from a parquet, sorted tarball-internal order,
   sorted GitHub tree, ...) Document this in the script comment.
5. **Volume.** Decide a target count. Pick something that doesn't
   blow up the corpus-share fractions in
   `tests/corpus/corpus-card.md`. Real-data sources should land
   between ~200 and ~2,500 docs by default.

**Implementation:**

1. **Write `tests/corpus/scripts/acquire_<name>.py`.** Mirror
   `acquire_cnn_dailymail.py` or `acquire_qmsum.py` depending on
   whether your source is a HuggingFace stream or a GitHub
   archive. The script must:
   - Carry a top-of-file docstring explaining the source, the
     pinned revision, the license posture, and any
     known-quirks-of-the-source.
   - Declare deps via PEP-723 inline metadata (`# /// script ...
     # ///`) — no requirements.txt, no pyproject.toml.
   - Hard-code the upstream revision/SHA as a top-level constant
     AND list it in `manifest.json`. Don't take revision from an
     env var (defeats determinism).
   - Use `corpus_data_dir() / "raw" / "<name>.jsonl"` as the
     output path — no exceptions.
   - Print `wrote {N} docs to {path}` and `sha256 = {hex}` so the
     final lines of a successful run go in the manifest.
2. **Run the script + capture the sha256.**
3. **Add a `manifest.json` entry** with all of: `script`,
   `upstream.{kind,id,revision,last_modified,...}`, `license`,
   `license_notes` (if non-trivial), `distribution`, `output`,
   `doc_count`, `sha256`, `acquired_at`.
4. **Update `tests/corpus/corpus-card.md`:**
   - Add a row to the "Source catalogue + license posture" table.
   - Add a row to the "Upstream checksums" table.
5. **Update `tests/corpus/README.md`** if the source has a
   non-obvious build step (e.g. needs an API key, a large
   download, a click-through license accept).
6. **Update `dev/corpus-creation/architecture.md`:**
   - Bump the Pack-1 count in §1, §5, or §10 if the source
     ships in.
   - Add a §5.x sub-section describing the source.
7. **Re-run the chain generator** so it gets a chance to pick up
   the new docs as chain anchors (only matters if you're adding
   a source type that an existing chain shape uses, OR if you're
   adding a new chain shape — see Recipe B).
8. **Re-run the ingest harness.** Expect node count to climb by
   your target volume; expect edge count to be unchanged unless
   the new source carries `parent_doc_id` references.
9. **Re-run the Pack-4 tests.** Expect green; if any fail, the
   most likely cause is the new source's bodies tripping the FTS
   salient-word picker (rare) or the vector wiring assertions
   (if the body lengths skew way out of the
   single-chunk-ingest-friendly range).
10. **Verify determinism.** Re-run the acquire script and confirm
    sha256 unchanged. Two independent runs against the same
    pinned upstream must produce a bit-identical JSONL.

**Things that go wrong:**

- The upstream changes their format mid-flight. Mitigation: the
  pinned SHA means you're reading a specific snapshot — if their
  current `main` differs, you don't care. If your pinned snapshot
  becomes unavailable (deleted, mirror taken offline), that's a
  real escalation — record in `manifest.json` and pick a
  fallback source.
- License changes upstream. Same posture — your pin is what you
  shipped against. But if a downstream user re-fetches and the
  license has tightened, they need to know. Update
  `corpus-card.md` and consider flipping `distribution` from
  `commit` to `cache`.
- Native IDs collide. Means your `doc_id` collides too — corpus
  has duplicate doc_ids and the load_subset_or_skip dedup will
  silently drop them. Fix: pick a finer-grained native_id.

## B. Add a new chain shape

Use case: you want chains that exercise a relation type or
source-type combination v1 doesn't cover. E.g. a `PAPER →
NOTE → TODO` chain once PMC OA lands.

**Decision checklist:**

1. **Anchor source(s) must exist in the corpus.** A
   `PAPER → ...` shape requires `paper` source_type docs in the
   Pack-1 JSONLs — don't add the chain shape before the
   underlying source.
2. **Relation kinds.** Pick from the locked 7-value vocabulary
   (`replies_to`, `follows_up_on`, `summarizes`, `action_from`,
   `contradicts`, `mentions`, `cites`). Don't extend the
   vocabulary without a HITL pass — the ingest harness, chain
   generator, and validation tests all share it.
3. **Determinism.** Your builder function will be called once per
   chain index assigned to its shape (round-robin across all
   builders). Every `rng` call inside must come from the
   per-chain `chain_rng(chain_id)` you receive — don't sneak in
   `time.time()` or unseeded random.

**Implementation:**

1. **In `tests/corpus/scripts/generate_chain_corpus.py`,** add a
   new `chain_<shape>` function returning a dict shaped like the
   existing builders. Conventions:
   - Pull anchors via `pick(rng, anchors[source_type])`.
   - Build synthetic docs via `synth_doc(...)` — pass an explicit
     `role` ("note", "todo", "email", "meeting") and the right
     `source_type`.
   - Set `parent_doc_id` on each synthetic doc to the immediately-
     preceding doc in the chain. The ingest harness uses this to
     emit `PreparedWrite::Edge` with `kind=<relation>`.
   - Add `relation:<rel>` to each synthetic doc's `extra_tags`
     for the relation that ties it to its parent.
   - Emit 1-3 `ground_truth_queries` per chain. Each query needs
     a `query` string, `expected_top_k_doc_ids` (list of
     doc_ids that must surface), and `relation_type` from the
     locked vocabulary.
   - Return shape: `{"shape": "...", "anchors": [...],
     "synthetic": [CorpusDoc, ...], "chain_ids": [...],
     "queries": [...]}`.
2. **Append the new builder to `CHAIN_BUILDERS`.** This changes
   the rotation (each shape gets `TARGET_CHAINS / len(builders)`
   chains). Re-running the generator will produce a different
   distribution — that's expected and the chain JSONs will
   change. Re-record the chain_connectives sha256 in
   `manifest.json`.
3. **Update `tests/corpus/corpus-card.md`** §"Cross-document
   chains" — bump the shape count and the relation count.
4. **Update `architecture.md` §7.1** — add a row to the chain-
   shapes table.
5. **Run the generator + verify counts.** Expect chains.len() =
   200 still; expect connective-doc count to change slightly.
   Expect every chain JSON to validate against the chain schema
   sketched in `architecture.md` §7.4.
6. **Re-run the ingest harness.** Expect new edges in the
   per-relation breakdown for the relation kind your shape uses.
7. **Re-run the Pack-4 graph test.** It re-validates that every
   chain produces at least one in-chain edge — your new shape
   must satisfy that.

**Things that go wrong:**

- You add a shape but don't add the relation tag → ingest
  harness emits edges with `kind="linked"` (the fallback). Pack-4
  graph test still passes (linked edges count), but the
  per-relation breakdown looks wrong.
- You add a shape whose anchor source doesn't exist → builder
  returns `None`, chain count drops below 200. Generator reports
  "skipped N chains". Fix: ingest the anchor source first.
- Two chains accidentally share a synthetic doc_id (e.g.
  `synth_doc` called twice with the same `chain_id` + `role`).
  Result: ingest sees the duplicate, second write is idempotently
  skipped. Subtle — chain count looks fine but one chain's
  parent_doc_id chain breaks. Detection: chain JSONs that
  reference the same synthetic_doc_id from two different chains.
  Fix: include the chain-iteration role in the native_id.

## C. Add a new Pack-4 validation gate

Use case: you want to assert a new end-to-end property
(metadata-prefilter correctness, partition-pruning behavior,
hybrid scoring, etc.).

**Decision checklist:**

1. **Is this a correctness gate or a perf gate?** Correctness:
   small fixture, runs at default `cargo test` scale, sub-second.
   Perf: under `AGENT_LONG=1`, canonical-CI-verified. Pack-4 is
   correctness; perf belongs in `tests/perf_gates.rs`.
2. **What needs to be in the corpus to support it?** If the
   property is "metadata prefilter X works", the corpus needs
   docs tagged with X. If the corpus doesn't already cover it,
   you may need to extend a source (Recipe A) before adding the
   test.
3. **Public engine API surface?** Pack-4 tests prefer
   `engine.search` / `engine.write` / `engine.trace_source_ref`.
   Direct rusqlite peeks into canonical_* / vector_default are
   acceptable test-only escape hatches but should be flagged in
   the test docstring.

**Implementation:**

1. **Add `src/rust/crates/fathomdb-engine/tests/corpus_<name>.rs`.**
   Mirror `corpus_fts.rs` — a single `#[test]` function, a
   top-of-file `//!` docstring explaining the property + budget.
2. **Use the shared helper** at `tests/support/corpus_subset.rs`:
   - `load_subset_or_skip(per_source)` returns `Option<Vec<Doc>>`
     — `None` means corpus absent; tests bail with a `SKIP:`
     line.
   - `load_chains_or_skip(max_chains)` returns
     `Option<Vec<Chain>>` for chain-driven gates.
   - `fixture_engine()` returns a tempdir + engine wired with
     `VaryingEmbedder` and `configure_vector_kind_for_test("doc")`.
   - `ingest(&engine, &docs)` ingests via per-node writes (NOT
     batched — see the §8 caveat in `architecture.md`).
3. **Wire the test up to `#[path = "support/corpus_subset.rs"] mod
   corpus_subset;`** at the top — that's how other tests pick up
   the support module.
4. **Run `cargo test corpus_<name>`.** Expect <1s, expect green.
5. **Update `architecture.md` §9** to add a §9.x subsection for
   your gate.

**Things that go wrong:**

- The gate fails because the engine bug from §8 collapses the
  batched ingest — and you used batched writes thinking you were
  efficient. Mitigation: use `ingest` from the support module
  (per-node writes) until the engine fix lands.
- The gate flakes between runs because you used an unseeded RNG
  for query selection. Mitigation: pull queries / subsets from
  the same sort-by-doc_id order the helpers use.
- The gate passes when the corpus is absent (because it took the
  skip branch and never ran). That's intentional — `cargo test`
  in environments without the corpus needs to stay green. If
  you want to force the gate to actually run in CI, the CI step
  needs to ensure the corpus is restored first.

## D. Bump a source's pinned revision

Use case: upstream had a critical fix and you want the new data.

**Decision checklist:**

1. **Is the new revision license-compatible?** Re-check the
   upstream license — sometimes dataset owners tighten between
   revisions. If license posture changes, update
   `manifest.json` + `corpus-card.md`.
2. **Will the new revision change the corpus shape?** If yes,
   that's a deliberate refresh — update Pack-2 (chain
   generator re-runs against the new anchors) and likely
   Pack-3/Pack-4 sha256s.
3. **Should other sources bump too?** Sometimes a dataset
   bumps to fix a problem that exists in the same form in a
   different dataset. Check.

**Implementation:**

1. **Update the pinned constant in `acquire_<name>.py`.** One
   line.
2. **Re-run the script + capture new sha256.**
3. **Update `manifest.json`** — `upstream.revision`,
   `upstream.last_modified`, `sha256`, `acquired_at` (today's
   date — important so future readers can date the snapshot).
4. **Update `corpus-card.md` §"Upstream checksums"** — new
   sha256.
5. **Re-run the chain generator + re-run the ingest harness +
   re-run the Pack-4 tests.** All should still pass; chain
   sha256 will likely change because the chain generator picks
   different anchors when the upstream order shifts.
6. **Verify determinism on the new revision.** Two
   independent runs against the new pin must produce the same
   sha256.

## E. Move a `cache`-only source to `commit`

Use case: an upstream license was clarified / a HITL pass
resolved the chain.

**Decision checklist:**

1. **Has HITL actually signed off?** This needs a documented
   decision — see the existing 2026-05-27 lock in
   `dev/plans/0.7.0-HITL-recommendations.md` for the pattern.
2. **Is the corpus shape still committable in the abstract?**
   E.g. does the new license allow redistribution in a
   public repo at all, or just internal use?

**Implementation:**

1. **Update `manifest.json`** — `distribution`: `cache` →
   `commit`.
2. **Update `corpus-card.md` §"Source catalogue"** —
   "License posture" column.
3. **Note: the JSONL is still gitignored under
   `data/corpus-data/`.** Currently NO source is shipped in-tree
   regardless of `distribution` field — the field is a license-
   eligibility marker, not a "is this committed?" flag. To
   actually start tracking the JSONL in git, that's a separate
   layout change (move data back into `tests/corpus/raw/` for
   that one source, partially undo the b9ed101 reorg). Don't
   make that change without HITL — the
   "scripts in git, data out of git" pattern is the current
   default for good reason.

## F. Investigate a corpus reproducibility regression

Use case: someone ran the build and got a different sha256 than
the manifest expects.

**Triage steps:**

1. **What changed?** Diff `git status` for the script + the
   manifest. If the script changed, that's the regression —
   either the script is buggy or the manifest needs updating.
   If neither changed, the upstream changed under us.
2. **Is the upstream snapshot still there?** `curl -I` against
   the upstream URL (HuggingFace, GitHub archive, CMU). For a
   pinned revision the content should be byte-identical; if
   it isn't, that's a serious upstream issue (revision
   rewritten?).
3. **Re-run the script** and compare byte-for-byte against the
   committed JSONL (if any) or the prior sha256 you trusted.
4. **Bisect by row.** If the sha256 differs, walk the JSONL row
   by row to find the first divergent line. That usually points
   straight at the rule that changed (date parsing, field
   ordering, tokenization).

**Common root causes:**

- A `dict.items()` or set iteration in a generator that's
  insertion-order-dependent in newer Python but used to land in
  the same order by accident.
- `datetime.now()` slipping into a script that should be using
  a deterministic anchor + offset (see CNN/DM's
  `synthesize_created_at` for the right pattern).
- A `random.Random()` constructor without an explicit seed.
- Upstream re-encoded its parquet files (rare but happens) —
  the byte stream changes but content is "equivalent". For
  HuggingFace, pin to a specific commit SHA, not `main`.

## G. Run the corpus build in CI

The intended CI pattern (not yet wired):

1. Cache key: SHA-256 of `tests/corpus/scripts/manifest.json`.
   Any change to the manifest invalidates the cache.
2. Cache hit: restore `data/corpus-data/` from cache. Ingest +
   tests run as normal.
3. Cache miss: run every `acquire_*.py` + `generate_*.py` step
   in sequence. ~5 minutes on a clean machine including the
   443 MB Enron download. Cache the result.

The acquisition scripts are intentionally **not parallel-safe**
against the same `data/corpus-data/` directory — but they
write to disjoint output files, so parallel runs on disjoint
working dirs are fine.
