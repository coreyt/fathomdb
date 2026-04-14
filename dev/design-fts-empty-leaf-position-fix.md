# Design: FTS empty-leaf position-map fix (0.3.1)

Status: Phase 1 design gate. Phase 2 implements after orchestrator review.
Target release: FathomDB 0.3.1 (patch on top of 0.3.0).
Scope: pure bug fix in the recursive property-FTS walker. No surface change.

## 1. Problem

In FathomDB 0.3.0, writing a node whose payload contains a zero-length JSON
string leaf followed by a non-empty leaf in the same recursive walk frame
fails loudly with

    UNIQUE constraint failed: fts_node_property_positions.node_logical_id,
    fts_node_property_positions.kind, fts_node_property_positions.start_offset

The transaction rolls back. The bug fires on any kind that registers a
recursive property-FTS path (`PropertyPathMode::Recursive`) over a payload
shape that mixes empty and non-empty string leaves under the same recursion
root. It blocks common production payloads (lists with sparse text fields,
objects with optional text fields). Severity 4/5.

### Trigger matrix

| Shape | Pre-fix | Post-fix |
| --- | --- | --- |
| `{'xs': ['', 'x']}` | FAIL (UNIQUE) | OK |
| `{'xs': ['', '', 'x']}` | FAIL (UNIQUE) | OK |
| `{'a': '', 'b': 'x'}` | FAIL (UNIQUE) | OK |
| `{'inner': {'a': '', 'b': 'x'}}` | FAIL (UNIQUE) | OK |
| `{'a': '', 'b': {'c': 'x'}}` | FAIL (UNIQUE) | OK (descent guard) |
| `{}` | OK (no positions) | OK |
| `{'key': 'value'}` | OK | OK |
| `{'a': ''}` | OK (empty blob, write skipped) | OK (no positions emitted) |
| `{'a': null}` | OK (Null no-op at line 1272) | OK |
| `{'inner': {}}` | OK | OK |
| `{'xs': ['x']}` | OK | OK |
| `{'xs': ['']}` | OK *by accident* | OK (no positions emitted) |
| `{'xs': ['', '']}` | OK *by accident* | OK |
| `{'xs': ['', '', '']}` | OK *by accident* | OK |
| `{'xs': ['x', 'y']}` | OK | OK |
| `{'xs': ['x', '']}` | OK | OK |
| `{'xs': ['x', '', 'y']}` | OK | OK |
| `{'xs': [null, null]}` | OK | OK |
| `{'a': 'x', 'b': 'x'}` | OK | OK |

The "by accident" cases are addressed in section 8.

## 2. Root cause

Two-step interaction in `RecursiveWalker::emit_leaf` at
`crates/fathomdb-engine/src/writer.rs:1306-1335`. Line numbers verified
against base `7111333` and match the prior investigation exactly.

### Step 1 — leading empty leaves all share offset 0

`emit_leaf` (lines 1306-1335) gates the `LEAF_SEPARATOR` push on
`!self.blob.is_empty()` at line 1324:

```text
1324: if !self.blob.is_empty() {
1325:     self.blob.push_str(LEAF_SEPARATOR);
1326: }
1327: let start_offset = self.blob.len();
1328: self.blob.push_str(value);
1329: let end_offset = self.blob.len();
1330: self.positions.push(PositionEntry {
1331:     start_offset,
1332:     end_offset,
1333:     leaf_path: leaf_path.to_owned(),
1334: });
```

When the very first emitted leaf is `Value::String("")`, the separator push
is skipped (blob is empty), `value` is empty, so `start_offset = 0`,
`end_offset = 0`, and the blob remains empty. Position
`(start=0, end=0, leaf_path=$.payload.xs[0])` is pushed.

The next iteration sees `blob.is_empty() == true` again because nothing
was appended. The separator push is still gated off. If that next leaf is
also `""`, another `(0, 0, ...)` row is pushed. This repeats for every
leading empty leaf in the walker frame.

### Step 2 — the first non-empty leaf collides with them

When the walker finally hits a non-empty leaf (e.g. `"x"` at index 2 of
`['', '', 'x']`), `blob.is_empty()` is *still* true because no separator
was ever pushed and no prior leaf contributed bytes. So `start_offset` is
again 0, and the position emitted is `(0, 1, $.payload.xs[2])`.

The walker now holds a `positions` vec containing at least:

    (0, 0, $.payload.xs[0])
    (0, 0, $.payload.xs[1])
    (0, 1, $.payload.xs[2])

Three rows, all with `start_offset = 0`.

### Step 3 — the combine step writes them out

`extract_property_fts` at lines 1176-1234 combines scalar parts and the
recursive blob. The relevant branch is lines 1215-1230:

```text
1215: let combined = match (scalar_text, walker.blob.is_empty()) {
1216:     (None, true) => None,
1217:     (None, false) => Some(walker.blob.clone()),
...
1231: };
```

For the trigger shapes, `walker.blob` is `"x"` — non-empty — so the
`(None, false)` branch fires and `combined = Some("x")`. The walker's
`positions` vec is returned untouched. Downstream insert call sites in
`crates/fathomdb-engine/src/projection.rs` lines 197, 266, 320 push the
rows into `fts_node_property_positions`. The UNIQUE constraint on
`(node_logical_id, kind, start_offset)` defined at
`crates/fathomdb-schema/src/bootstrap.rs:464` (v18 migration block at
425-472) rejects the second row, the transaction aborts, and the write
fails.

### Why the all-empty cases pass today (and why that is fragile)

For `{'xs': ['', '']}` the walker emits two `(0, 0, ...)` entries into
`positions` but `walker.blob` stays `""`. The combining branch at line
1215 sees `(None, true)` and returns `combined = None`. The caller treats
"no combined text" as "skip the entire FTS write for this kind", which
includes skipping the position-row inserts. The colliding rows in
`walker.positions` are silently discarded along with the (absent) FTS row.

That is not safe handling — it is a side effect of the empty-blob short
circuit. As soon as a single non-empty leaf appears later in the walk,
the short-circuit no longer fires and the latent collision becomes a
hard write failure.

## 3. Fix

Add a single guard at the top of `emit_leaf`, immediately after the
existing `if self.stopped { return; }` block. The exact predicate to land
in Phase 2:

```rust
if value.is_empty() {
    return;
}
```

Placement: between line 1309 (`}` closing the `stopped` check) and the
existing comment at line 1310 (`// Compute the projected blob size...`).

### Why this is correct and minimal

1. **`Value::Null` is unaffected.** The walker never reaches `emit_leaf`
   for `Value::Null`; it short-circuits at line 1272 (`Value::Null => {}`).
   That behavior is preserved.
2. **Numbers and booleans are never empty.** `n.to_string()` on a
   `serde_json::Number` is always at least one character (`"0"`, `"-1"`,
   `"1.5"`). `b.to_string()` on a `bool` is `"true"` or `"false"`. The
   guard only ever fires for `Value::String("")` reaching line 1269.
3. **Blob content is unchanged for every payload that previously
   produced a non-empty blob.** For the trigger shapes, the pre-fix blob
   was `"x"` (separator never got pushed because the walker thought blob
   was empty); the post-fix blob is also `"x"` for the same reason — the
   first emitted leaf is `"x"`, blob was empty, separator-gate suppresses
   the push, blob becomes `"x"`. Identical text-projection.
4. **Positions vec is now collision-free for trigger shapes.** Empty
   leaves emit nothing. The non-empty leaf gets `(0, 1, ...)`, which is
   the only entry in the vec. UNIQUE no longer fires.
5. **All-empty cases stay no-ops, but for a principled reason.** Pre-fix:
   walker emits N collision-prone `(0, 0, ...)` rows that get discarded
   downstream by an unrelated short-circuit. Post-fix: walker emits zero
   rows and the downstream short-circuit still applies (blob is still
   empty, `combined = None`, no FTS row written). Same observable
   outcome, but the position vec is no longer holding latent UNIQUE
   collisions.
6. **Descent is unchanged.** The guard is inside `emit_leaf`, not
   `walk`. Recursion into objects (lines 1273-1289) and arrays
   (1290-1302) still happens for every node regardless of leaf
   emptiness. Empty siblings cannot block descent into nested non-empty
   subtrees.

## 4. What must still work (regression guards)

Phase 2 must keep all of the following passing:

- `{}` — empty object, no positions, no FTS row.
- `{'key': 'value'}` — single leaf, single position, found by search.
- `{'a': ''}` — single empty leaf, blob empty, no FTS row.
- `{'a': null}` — Null short-circuit at line 1272.
- `{'inner': {}}` — empty nested object.
- `{'xs': ['x']}` — single non-empty array leaf.
- `{'xs': ['']}` — *apparent regression guard*: passed pre-fix only
  because the empty-blob short-circuit discarded the latent
  `(0, 0, ...)` row. Phase 2 must verify the *positions vec* is empty
  post-fix, not just that the write succeeds.
- `{'xs': ['', '']}` — same apparent-regression-guard caveat.
- `{'xs': ['', '', '']}` — same apparent-regression-guard caveat.
- `{'xs': ['x', 'y']}` — two non-empty leaves with separator.
- `{'xs': ['x', '']}` — non-empty followed by empty (this case did NOT
  fail pre-fix because the non-empty came first; preserve that).
- `{'xs': ['x', '', 'y']}` — empty in the middle.
- `{'xs': [null, null]}` — Null short-circuit, no positions.
- `{'a': 'x', 'b': 'x'}` — two non-empty leaves under different keys.
- **`{'a': '', 'b': {'c': 'x'}}`** — descent-guard. Proves the walker
  still descends past an empty sibling into a nested non-empty subtree.

## 5. What the fix must NOT change

- The UNIQUE index on `fts_node_property_positions(node_logical_id,
  kind, start_offset)` defined in `fathomdb-schema/src/bootstrap.rs:464`.
  Phase 2 must not touch the migration block 425-472. The fix is in the
  walker, not the schema.
- The `HitAttribution` public surface in
  `crates/fathomdb-engine/src/...` (search-side attribution lookup).
- The `FtsPropertyPathMode` and `FtsPropertyPathSpec` public types
  (Rust crate, Python bindings, TypeScript bindings).
- The walker's traversal-descent behavior. Recursion into objects and
  arrays is unchanged; empty children must not short-circuit walks into
  sibling non-empty subtrees.
- The text-projection blob content for every payload that previously
  produced a non-empty blob. Snippet windows, FTS5 token offsets, and
  attribution byte ranges remain bit-identical for non-trigger payloads
  and for the non-empty leaves in trigger payloads.
- The byte-cap (`MAX_EXTRACTED_BYTES`) and depth-cap
  (`MAX_RECURSIVE_DEPTH`) accounting. The guard is a pure return; it
  does not bump stats counters. Empty leaves should not contribute to
  byte budgets — they could not before (they added zero bytes), and
  they still do not.
- The `ExtractStats` shape and counters.
- `LEAF_SEPARATOR` constant or its gating predicate at line 1324. The
  separator behavior is correct; only the empty-leaf emission was wrong.
- `extract_property_fts` combining logic at lines 1215-1230.
- Position-insert call sites in
  `crates/fathomdb-engine/src/projection.rs` lines 197, 266, 320.

## 6. Test plan

### Rust tests

Location: `crates/fathomdb-engine/src/writer.rs`,
`mod recursive_extraction_tests` at line 7273. Helpers `schema()` (7280)
and `schema_with_excludes()` (7288) already exist; reuse them. Existing
precedent: `recursive_extraction_skips_nulls_and_missing` at line 7355.

These are walker-level tests, not full engine tests. They call
`extract_property_fts` directly with a `PropertyFtsSchema` configured for
recursive mode on `$.payload`, then assert two things:

1. `combined.is_some()` returns the expected text (trigger shapes) or
   `None` (all-empty shapes).
2. `positions` vec contains the expected entries with no duplicate
   `start_offset` values.

The walker-level tests are sufficient to prove the bug at unit level.
Full end-to-end coverage (write + property-FTS search) lives in the
Python suite (next subsection) and exercises the real insert path that
hits the UNIQUE constraint.

Proposed Rust tests, one per trigger shape:

- `recursive_extraction_empty_then_nonempty_in_array`
  Payload `{"xs": ["", "x"]}`. Assert combined = `"x"`, positions = one
  entry with `start_offset=0, end_offset=1, leaf_path="$.payload.xs[1]"`.
- `recursive_extraction_two_empties_then_nonempty_in_array`
  Payload `{"xs": ["", "", "x"]}`. Same shape; only the index-2 leaf
  produces a position.
- `recursive_extraction_empty_then_nonempty_sibling_keys`
  Payload `{"a": "", "b": "x"}`. One position at
  `$.payload.b`.
- `recursive_extraction_nested_empty_then_nonempty_sibling_keys`
  Payload `{"inner": {"a": "", "b": "x"}}`. One position at
  `$.payload.inner.b`.
- `recursive_extraction_descent_past_empty_sibling_into_nested_subtree`
  Payload `{"a": "", "b": {"c": "x"}}`. One position at
  `$.payload.b.c`. Critical descent-guard: proves the walker descends
  into the `b` object even though the `a` sibling was empty.

Regression-guard tests (table-driven is fine):

- `recursive_extraction_all_empty_shapes_emit_no_positions` — table over
  `{}`, `{"a": ""}`, `{"xs": []}`, `{"xs": [""]}`, `{"xs": ["", ""]}`,
  `{"xs": ["", "", ""]}`. Each must produce `combined = None` AND
  `positions.is_empty()`. The empty-positions assertion is the key new
  check that the prior investigation flagged: today the by-accident
  passing cases produce non-empty `positions` containing
  `(0, 0, ...)` collisions that just happen to never get written.
- `recursive_extraction_nonempty_then_empty_then_nonempty` — payload
  `{"xs": ["x", "", "y"]}`. Two positions at indices 0 and 2 of `xs`,
  separated by exactly `LEAF_SEPARATOR.len()` bytes. The empty at
  index 1 contributes neither bytes nor a position.
- `recursive_extraction_null_leaves_unchanged` — payload
  `{"xs": [null, null]}`. `combined = None`, `positions.is_empty()`.

### Python tests

Location: `python/tests/test_text_search_surface.py`. These are full
end-to-end tests through the Python SDK: open an engine, register a
schema with a recursive property-FTS path on `$.payload`, write a node,
issue a property-FTS search for the non-empty token, assert the node is
found.

This layer is what catches the *insert-time* failure — it exercises the
real `projection.rs` insert path at lines 197/266/320 against the real
UNIQUE-constrained sidecar table.

Parallel test functions, one per trigger shape:

- `test_recursive_property_fts_empty_then_nonempty_in_array`
- `test_recursive_property_fts_two_empties_then_nonempty_in_array`
- `test_recursive_property_fts_empty_then_nonempty_sibling_keys`
- `test_recursive_property_fts_nested_empty_then_nonempty_sibling_keys`
- `test_recursive_property_fts_descent_past_empty_sibling_into_nested_subtree`

Each test:
1. Open engine, register a kind with `FtsPropertyPathMode.RECURSIVE` on
   `$.payload`.
2. Write a single node with the trigger payload (containing the literal
   token `"x"` somewhere non-empty).
3. Assert the write succeeds (no `UNIQUE constraint failed`).
4. Issue a property-FTS search for `"x"` against the kind.
5. Assert the node is in the result set.

Plus regression guards for the by-accident passing cases:

- `test_recursive_property_fts_all_empty_payload_writes_succeed` — table
  over the all-empty payloads. Each write must succeed and a search for
  `"x"` must return zero results (the payload contained no `"x"`).

### TypeScript tests

None. The TypeScript SDK is not yet property-FTS-parity (per the
project's TypeScript SDK milestone-1 note in agent memory). Adding
property-FTS coverage to the TS suite is out of scope for this fix.

## 7. CHANGELOG entry text

To land in a new `## [0.3.1] - <date>` section. Phase 2 inserts the
section between the existing `## [Unreleased]` (line 8) and `## [0.3.0]`
(line 10) blocks. The `## [Unreleased]` block stays empty.

Proposed entry:

```markdown
## [0.3.1] - <date>

### Fixed

- Property-FTS recursive walker no longer crashes on payloads that mix
  empty and non-empty string leaves. Previously, writing a node whose
  recursive property-FTS payload contained a zero-length JSON string
  followed by a non-empty string in the same traversal frame would fail
  with a `UNIQUE constraint failed` error against
  `fts_node_property_positions` and roll back the transaction. Affected
  shapes include arrays such as `{"xs": ["", "x"]}`, sibling object keys
  such as `{"a": "", "b": "x"}`, and any nested combination of the two.
  Empty string leaves are now skipped at extraction time. All-empty
  payloads (such as `{"xs": ["", ""]}`) continue to produce no FTS row,
  and `null` leaves continue to be ignored as before. No schema or API
  change; existing databases benefit immediately on upgrade. No
  rebuild is required because the bug only affected writes that
  previously failed — there is no corrupt data to repair.
```

## 8. Open items / discrepancies

### "Apparent regression guard" cases

Pre-fix, payloads like `{"xs": ["", ""]}` and `{"xs": ["", "", ""]}` do
not raise `UNIQUE constraint failed`. They appear to work. They do not.
The walker emits a `positions` vec containing colliding `(0, 0, ...)`
rows; the rows are silently dropped because the empty blob causes
`extract_property_fts` (line 1215) to return `combined = None`, which
in turn causes the downstream insert path to skip the FTS row entirely
(and with it, the latent position collisions).

Implication for Phase 2: the regression-guard tests for these shapes
must assert *both*:

1. `combined` is `None` (unchanged from pre-fix).
2. `positions.is_empty()` is `true` (changed from pre-fix; this is the
   defense-in-depth assertion).

Without check (2), Phase 2 risks landing a fix that "still works" for
the apparent-regression-guard cases without actually emptying the
positions vec, and the fix would be silently incomplete in a way that
becomes a real bug the next time someone adds a code path that reads
`walker.positions` regardless of `walker.blob` emptiness.

### Verification deltas vs the briefing

All line numbers in the briefing match the worktree at base `7111333`:

- `emit_leaf` at 1306-1335: confirmed.
- Walker dispatch at 1268-1303: confirmed.
- `Value::Null` no-op at 1272: confirmed.
- `LEAF_SEPARATOR` constant at 300: confirmed.
- Combining logic at 1215-1230: confirmed.
- UNIQUE constraint at `fathomdb-schema/src/bootstrap.rs:464`: confirmed.
- v18 migration block at 425-472: confirmed.
- `mod recursive_extraction_tests` at 7273: confirmed.
- `recursive_extraction_skips_nulls_and_missing` at 7355: confirmed.
- `LEAF_SEPARATOR` push at line 1324, gated on `!self.blob.is_empty()`:
  confirmed verbatim. The two-step root-cause analysis is sound.
- CHANGELOG `## [Unreleased]` at line 8, `## [0.3.0]` at line 10:
  confirmed; no `## [0.3.1]` section yet.

No drift. Phase 2 can take the line numbers in this doc as accurate
against base `7111333`.

### Position-insert call sites (`projection.rs` 197/266/320)

Not touched by this fix. Listed here only so Phase 2 knows which
caller paths surface the bug at runtime, in case a Phase 2 reviewer
asks "where does the UNIQUE actually fire?" — those three sites are
the answer.

### Rebuild path

The v18 migration's open-time rebuild guard
(`ExecutionCoordinator::open` empty-positions detection) is unrelated
to this fix and must not be touched. Existing 0.3.0 databases that
already migrated through v18 will not need a rebuild on 0.3.1 upgrade
because the bug only blocked writes that previously failed; there is
no corrupt persisted state for the rebuild to repair.

### Out-of-scope cleanup

While reading `emit_leaf` we noted that the `sep_len` projection at
lines 1313-1318 and the gated push at 1324-1326 duplicate the
"is the blob empty?" check. That's a small redundancy, not a bug.
Phase 2 must NOT refactor it. The fix is the four-line guard and
nothing else.
