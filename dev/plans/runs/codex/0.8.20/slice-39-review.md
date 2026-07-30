# Slice 39 (`R-20-DOC`) — codex §9 review record

Reviewer: `codex --model gpt-5.4 -c model_reasoning_effort=high --sandbox read-only`, invoked through
`dev/agent-tools/codex-nostdin.sh` (TC-86 redaction at capture time; `check-transcript-hygiene.sh` rc=0,
zero redactions applied).

Target: branch `0.8.20-slice-39-doc`. Baseline `3e9d6d12`.
Three rounds, terminal verdict **PASS**. Within the §6 fix-N cap (3 same-finding / 6 total).

| Round | Target | Transcript | Verdict |
| ----- | ------ | ---------- | ------- |
| 1 | `1bbf9b43` (full slice) | `slice-39-20260730T045914Z.log` | CONCERN — 2 findings |
| 2 | `88a96fa7` (fix-1) | `slice-39-fix-1-rereview-20260730T052714Z.log` | CONCERN — both prior findings RESOLVED, 1 new |
| 3 | `0625b8d3` (fix-2) | `slice-39-fix-2-rereview-20260730T053751Z.log` | **PASS** |

Sandbox note: the reviewer could not write these files (read-only sandbox); the orchestrator promoted
the verdicts verbatim from the logs. In all three rounds codex reported that `git log/show/diff` failed
inside its sandbox with `bwrap: loopback: Failed RTM_NEWADDR: Operation not permitted`, so it verified
**current-file truth** rather than re-deriving commit-to-commit deltas. The orchestrator verified the
diff-shaped claims independently from git — see the closure `0.8.20-slice-39-output.json`.

---

## Round 1 — `1bbf9b43`

### Verdict: CONCERN

### 1. [high] Published Python and TypeScript surfaces still document the wrong error class for inverted node-validity windows

Refs: `src/python/fathomdb/_fathomdb.pyi:186`, `src/ts/dist/index.d.ts:523`,
`src/rust/crates/fathomdb-engine/src/lib.rs:3639`, `:3752`, `:16461`

Both shipped binding surfaces still say `valid_from >= valid_until` raises `InvalidArgumentError`, but
the engine now returns the message-less `WriteValidation` unit variant by decision #18. This is exactly
the false publish-facing contract the slice was supposed to eliminate, and it lands in artifacts
consumers actually read: the Python `.pyi` and the emitted TypeScript `.d.ts`.

### 2. [medium] The TypeScript install guide's default-embedder example calls a non-existent entrypoint

Refs: `docs/install/typescript.md:48`, `src/ts/src/index.ts:705`, `src/ts/dist/index.d.ts:500`

The guide uses `engineOpen("mydb.sqlite", { useDefaultEmbedder: true })`, but the public API exposes
`Engine.open(...)`, not a top-level `engineOpen`. As written, the example will fail for a reader
following the docs, which makes it a publish-facing documentation bug rather than a style nit.

### Orchestrator triage — round 1

Both findings verified independently from git before acting; both **substantive**, therefore **not
overridable** under §7 (override is for structural or prompt-induced CONCERNs only). Routed to fix-1.

One citation was re-pointed: `src/ts/dist/index.d.ts` is **untracked build output**
(`.gitignore:54: **/dist/`), regenerated from `src/ts/src/index.ts`. Editing it would have been undone by
the next build. The defect and its consumer impact were real; only the fix location moved.

---

## Round 2 — `88a96fa7` (fix-1)

### Verdict: CONCERN

### 1. [medium] Embedder guide names a non-existent public error class

Refs: `docs/embedder.md:5-6`, `docs/embedder.md:57-58`, `src/ts/src/errors.ts:31`,
`src/python/fathomdb/errors.py:110`, `docs/reference/errors.md:51`

`docs/embedder.md` says vector writes fail/raise `EmbedderNotConfigured`, but the shipped public class
name is `EmbedderNotConfiguredError` in both bindings. That is a user-facing falsehood in the full slice.
Not introduced by fix-1; it remained wrong at `88a96fa7`.

### Addressed (round 2)

- **Prior finding 1 — RESOLVED.** The three corrected binding-surface sites match the real code path.
  The left-alone `InvalidArgument` sites spot-checked remained correct.
- **Prior finding 2 — RESOLVED.** The broken `engineOpen` examples now use `Engine.open(...)` and the
  added TS import is correct. The `CHANGELOG.md:1186` correction "was the right call: it fixes a
  historically false public symbol name rather than rewriting valid release history."

### Orchestrator triage — round 2

New and distinct (a **productive** round). Substantive — a false published contract is precisely what
this slice exists to eliminate — so **not overridable**. Before commissioning fix-2 the orchestrator
independently bounded the finding's class: every shipped error class whose stem is a multi-word compound
(18 of them) was searched for its suffix-less form across all tracked `docs/**/*.md`;
`EmbedderNotConfigured` was the **sole** hit, at exactly two lines. Every shipped error class is
mentioned somewhere in `docs/`. `ImportError` and `RangeError` are language builtins and correct.

---

## Round 3 — `0625b8d3` (fix-2)

### Verdict: PASS

### Addressed (round 3)

Substantively resolved. `EmbedderNotConfiguredError` is the shipped public class in both bindings —
TypeScript `src/ts/src/errors.ts` (`export class EmbedderNotConfiguredError extends EmbedderError {}`),
Python `src/python/fathomdb/errors.py` re-exporting the PyO3 class created at
`src/rust/crates/fathomdb-py/src/lib.rs` (`create_exception!(_fathomdb, EmbedderNotConfiguredError,
EmbedderError);`). The surrounding claims are also correct: both bindings document the fresh/default-open
path as "no embedder configured", and both binding test suites state that with the default embedder off,
vector writes fail with `EmbedderNotConfiguredError`.

### What passed on inspection (cumulative)

- The license work: MIT declared on the shipped path; the npm lockfile change confined to the root `""`
  entry; the added `LICENSE` copies/mechanisms consistent with the stated packaging model; the guard
  script honest about where it uses a wheel proxy rather than real packaging output.
- No scope violations in `.github/**`, manifest versions, schema version, `categories`, or
  `src/conformance/governed-surface-allowlist.json`.
- `PROPOSED / NOT SIGNED` handling for `DenseReadiness` correct, and the mixed signed/unsigned marker
  splits correct.
- CHANGELOG: heading form matches the release-gate regex; schema span documented as 15 → 24; the step-23
  edge-loss warning prominent and accurate; the `SearchHit.id` `u64` → `IdSpace` break described across
  Rust, Python and TypeScript.
- The npm `files` negations are safe relative to `main = dist/index.js` and `types = dist/index.d.ts`.
- No new falsehood introduced by fix-1; the ~20 deliberately-unchanged `InvalidArgument` mentions
  spot-checked as genuinely correct.

### Orchestrator triage — terminal

**PASS accepted; slice landed.** Round 3 verified the two corrected sentences indirectly (its sandbox
could not read the file); the orchestrator confirmed the two-line diff directly from git.
