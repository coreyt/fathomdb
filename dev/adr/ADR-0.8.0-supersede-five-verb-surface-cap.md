---
title: ADR-0.8.0-supersede-five-verb-surface-cap
date: 2026-06-01
target_release: 0.8.0
desc: Supersede AC-057a's five-verb *scope cap* (a development scaffolding device) with a governed, open-but-curated SDK surface. Preserve and re-home the three load-bearing guarantees AC-057a bundled (SDK parity, recovery-unreachability, typed boundary). Unblocks the gated read verbs G2/G3/G4/G5/G7.
blast_radius: dev/acceptance.md (AC-057a supersede + new AC for governed surface); dev/requirements.md (REQ-053 amend); dev/design/bindings.md § 1 (surface-set parity invariant rewrite); src/python/tests/test_surface.py + src/ts/tests/surface.test.ts (set-equality → allowlist+parity); src/python/tests/test_no_recovery_surface.py + src/ts/tests/no-recovery-surface.test.ts (PRESERVED, unchanged); dev/design/0.8.0-agent-memory-fit.md §7 (read-verb HITL question resolved); ADR-0.8.0-agent-memory-retrieval-and-identity.md (Q1/Q2 surface interaction); dev/traceability.md
status: draft, HITL-required
origin: HITL direction 2026-06-01 ("AC-057a … was used to manage scope during development … supersede it thoughtfully"); dev/design/0.8.0-agent-memory-fit.md §7 Q1/Q2; dev/profiling/v05-lineage.md (v0.5.x had the broader surface)
supersedes: AC-057a (five-verb application-runtime SDK surface, REQ-053)
---

# ADR-0.8.0 — Supersede the five-verb surface cap

**Status:** draft, HITL-required.

AC-057a fixed the SDK application surface at **exactly five verbs**
(`Engine.open`, `admin.configure`, `write`, `search`, `close`) and gated it with
a conformance test asserting **set equality** — any sixth public command fails
CI. This ADR proposes to **supersede that cap**. HITL has stated the cap "was
used to manage scope during development" and asked to retire it thoughtfully.

"Thoughtfully" is the whole point: AC-057a is **not one rule, it is a scope cap
welded to three load-bearing guarantees.** Deleting the test would silently drop
the guarantees. This ADR separates them — retire the cap, **preserve and re-home
the guarantees**, and replace the closed surface with a *governed, open* one.

## What AC-057a actually bundles (verified)

| # | Element | Nature | Disposition |
|---|---|---|---|
| 1 | **Scope cap** — "exactly five, no sixth verb" (`dev/acceptance.md:882`, set-equality assertion; `bindings.md` § 1) | **Scaffolding** — bounded dev scope, prevented surface sprawl before the substrate was ready | **RETIRE** |
| 2 | **SDK parity** — a verb appears in *every* SDK binding or none; no per-binding drift (`bindings.md` § 1, REQ-053) | **Load-bearing** — Python/TS must stay in lockstep | **PRESERVE + re-home** |
| 3 | **Recovery-unreachability** — SDK MUST NOT expose `{recover, restore, repair, fix, rebuild, doctor}`; recovery is CLI-only (REQ-037 / REQ-054 / REQ-031d; `bindings.md` § 10; AC-058) | **Load-bearing** — a safety/contract boundary, *independent of verb count* | **PRESERVE, untouched** |
| 4 | **Typed-write boundary** — no raw SQL from clients; `PreparedWrite` is the only write shape (ADR-0.6.0-typed-write-boundary) | **Load-bearing** — independent of verb count | **PRESERVE, untouched** |

Only element 1 is scaffolding. The supersession must touch *only* element 1 and
explicitly carry 2–4 forward.

### Where the cap is enforced today (must all be updated together)

- `dev/acceptance.md:882-890` (AC-057a definition, **set-equality** assertion).
- `dev/requirements.md` REQ-053.
- `dev/design/bindings.md` § 1 (surface-set parity invariant).
- `src/python/tests/test_surface.py` + `src/ts/tests/surface.test.ts` — assert the
  command set; currently `subset`/explicit-verb checks (not literally rejecting a
  sixth today, but the *spec* they bind does). These become allowlist+parity tests.
- `src/python/tests/test_no_recovery_surface.py` +
  `src/ts/tests/no-recovery-surface.test.ts` — these bind element 3 and are
  **preserved unchanged**.
- `dev/traceability.md`, `dev/interfaces/{python,typescript,cli,rust}.md`.

## Why retire the cap now

1. **It did its job.** The substrate + perf are landed (0.6.0/0.7.x); the cap's
   purpose — prevent surface sprawl while the engine was being built — is spent.
2. **It is now the binding constraint on the consumers.** Memex / Hermes /
   OpenClaw need by-id read (G2) as table-stakes; the world-model trajectory needs
   list/neighbors/history (G4/G5/G7). All are gated *solely* by this cap
   (`0.8.0-agent-memory-fit.md` §7; `dev/profiling/v05-lineage.md`).
3. **The capability is proven.** v0.5.x already shipped a broad
   read/traverse/filter surface; this is un-stripping a deliberate scope reset,
   not inventing risk.
4. **The cap punishes the wrong thing.** It blocks *reads* (which don't touch the
   write/projection/durability invariants) using a rule designed to keep the
   write surface small. Reads and the cap are orthogonal.

## What replaces it — a governed, open surface

Retire "exactly five" → adopt **"a curated, parity-enforced surface partitioned
into a write/admin core and an additive read surface, with a permanent
recovery-name denylist."** Concretely:

- **Core (unchanged):** `Engine.open`, `admin.configure`, `write`, `search`,
  `close` — still the canonical write/lifecycle surface, still typed-boundary.
- **Read surface (new, additive):** read verbs may be added, **but governed**:
  - **Parity (element 2):** any read verb appears in Python *and* TypeScript or
    neither — enforced by the rewritten conformance test (allowlist membership +
    cross-binding equality), not by a count.
  - **Recovery denylist (element 3):** the surface MUST NOT contain any name in
    `{recover, restore, repair, fix, rebuild, doctor}` — preserved verbatim;
    `restore_logical_id`/`purge_logical_id` stay **CLI-only** (`recover --*-logical-id`).
  - **Typed boundary (element 4):** reads take typed args + a small fixed filter
    grammar (equality + range over body-JSON), **never raw SQL** — the same line
    `0.8.0-agent-memory-fit.md` §5 and the impl-strategy already draw.
  - **Namespace:** read verbs land under a dedicated **`read.*`** namespace (or
    `admin.*`); see Decision Q-B. Either keeps the write/lifecycle core legible.
  - **Additive evolution:** adding a read verb is a deliberate, documented event
    (release note + parity test update + one error class per binding per
    AC-060a), not silent drift.

The conformance test flips from **"the command set == {5}"** to **"every public
command is in the governed allowlist; the allowlist is identical across bindings;
no denylisted name appears; no raw-SQL entrypoint exists."** That is *stronger*
governance than a count — it survives the surface growing.

## Options

### Q-A — How far to open in 0.8.0

- **A1 — Supersede the cap; ship G1+G2+G3 read verbs in 0.8.0; G4/G5/G7 land
  incrementally behind the same governance.** (Recommended.) The cap is gone, but
  the surface grows by *demonstrated need*, smallest-blast-radius first (by-id is
  the universal table-stakes gap). G5/G6 (graph) follow once the substrate +
  traversal baseline (profiling slice) are in.
  - *For:* unblocks the consumers immediately; keeps each verb a reviewable
    increment; governance (parity/denylist/typed) holds throughout.
  - *Against:* the surface is now a living thing requiring ongoing curation (that's
    the intended trade — governance replaces a frozen count).
- **A2 — Supersede the cap as policy now; ship no new verbs until the
  knowledge-store schema lands.** Retire AC-057a, defer all read verbs.
  - *For:* cleanest separation of the *policy* decision from the *feature* work.
  - *Against:* leaves the consumer table-stakes gap (G2) open another cycle for no
    governance reason.
- **A3 — Keep the cap; raise it to a specific higher number.** e.g. "exactly N."
  - *For:* minimal change to the test shape.
  - *Against:* reproduces the exact problem — a frozen count that the *next*
    consumer need will re-litigate. A count is the wrong abstraction; parity +
    denylist + typed-boundary is the right one. **Reject.**

### Q-B — Read-verb namespace

- **B1 — New `read.*` namespace** (`read.get`, `read.list`, `read.neighbors`,
  `read.history`, `read.collection`). (Recommended.) Keeps the write/lifecycle
  core (`Engine.*` + `admin.*`) legible; makes "this is a read" self-documenting;
  clean parity surface to enumerate.
- **B2 — Under existing `admin.*`** (`admin.get`, `admin.read_collection`).
  - *For:* `admin.*` already exists; smaller conceptual delta.
  - *Against:* conflates *operator config* (`admin.configure`) with *application
    reads*; `admin.configure` is a counted standalone verb, not a namespace today —
    overloading it muddies the boundary the recovery-denylist relies on.
- **B3 — Top-level `Engine` methods** (`engine.get`, `engine.list`).
  - *For:* most idiomatic for callers.
  - *Against:* largest blast radius on the "core five" mental model; harder to keep
    the read/write distinction crisp for the denylist + typed-boundary checks.

## Recommendation

**A1 + B1.** Supersede AC-057a's cap now; replace it with the governed-surface
acceptance criteria (parity + recovery-denylist + typed-boundary + allowlist),
preserving elements 2–4 intact; ship read verbs additively under a new `read.*`
namespace, starting with **G1 (structured hits) + G2 (`read.get`/`get_many`) +
G3 (`read.collection`)** in 0.8.0 (the table-stakes set), with **G4/G5/G7**
following behind the same governance as the substrate + profiling baselines land.

This is the smallest change that (a) does what HITL asked — retire the scope
device — while (b) losing none of the guarantees the device was carrying.

## Read verbs landing under the governed surface (v0.5.x triage)

Per `dev/design/0.8.0-v05-feature-triage.md` (2026-06-01):

**0.8.0** (parity-enforced Py+TS, recovery-denylist-clean, typed args + small
fixed filter grammar):
- `read.get(logical_id)` / `read.get_many([logical_id])` — G2 (after G0).
- `read.collection(name, key?, filter?)` / `read.mutations(collection)` — G3
  READ subset (after this ADR's HITL sign-off; engine seam dormant-shippable now).

**DEFER 0.8.x** (governance path clear, sequencing pending):
- `read.list(kind, filter?, limit)` — G4, closed typed equality+range enum.
- `read.neighbors(id, edge_type?, depth<=3)` + `search(expand=)` — G5/G6.
- `read.history(id)` — G7.

**NOT on the SDK (CLI-only, recovery boundary preserved):**
`purge_logical_id`, `restore_logical_id`, `regenerate_vector_embeddings`,
`rebuild_projections`, `safe_export` (F7). Note: the name-denylist test catches
only `restore`/`rebuild` substrings; the others are excluded by the broader
recovery-CLI-only + typed-mutation boundary (REQ-037/054), not the name test.

**No new verb:** G1 structured hits (enriches `search()`); confidence (F9) and
recency/importance (G12) are scoring signals, governance-free.

## What this ADR explicitly preserves (do not drop)

- **Recovery-unreachability** — `test_no_recovery_surface` (py + ts) stays GREEN
  and unchanged; the `{recover,restore,repair,fix,rebuild,doctor}` denylist
  becomes a permanent clause of the new governed-surface AC. AC-058 (recovery
  CLI-reachable) unchanged.
- **SDK parity** — re-expressed as allowlist-equality across Python + TS.
- **Typed-write boundary** — no raw SQL; reads get a fixed filter grammar, not a DSL.
- **The five core verbs** — unchanged in name, shape, and semantics.

## Consequences if accepted

- `dev/acceptance.md`: AC-057a marked **superseded by AC-0NN** (new:
  "Governed SDK surface — parity + recovery-denylist + typed-boundary + read
  allowlist"). REQ-053 amended (or superseded by a REQ for the governed surface).
- `dev/design/bindings.md` § 1: the "surface-set parity invariant (exactly five)"
  rewritten as the governed-surface invariant; § 10 (recovery-unreachability)
  unchanged.
- `test_surface.{py,ts}`: set-equality → allowlist+parity. `test_no_recovery_surface.{py,ts}`:
  unchanged.
- `ADR-0.8.0-agent-memory-retrieval-and-identity.md`: its open AC-057a interaction
  (Q1/Q2) resolves to "governed surface, `read.*` namespace."
- `0.8.0-agent-memory-fit.md` §7 Q1/Q2 answered; the gated read verbs (G2/G3/G4/
  G5/G7) move from "blocked on HITL" to "additive under governance."
- `dev/roadmap/0.8.0.md` retrieval-anchor scope gains the read verbs explicitly.

## Open questions for HITL

1. Accept A1 (supersede + ship G1/G2/G3 in 0.8.0) vs A2 (supersede policy only,
   defer verbs)?
2. Accept B1 (`read.*` namespace) vs B2 (`admin.*`) vs B3 (top-level)?
3. Should REQ-053 be **amended** (re-scoped to the governed surface) or
   **superseded** by a new REQ id, for traceability cleanliness?
4. Logical-id reads: confirm `restore`/`purge` stay **CLI-only** (denylist) while
   a *non-destructive* `read.get(logical_id)` is allowed on the SDK — i.e. the
   denylist is about *recovery/mutation* names, not about reading by id.
5. Does the governed-surface AC also bind the **Rust facade** (`interfaces/rust.md`),
   or stay SDK-only (Python/TS) as AC-057a did? (Rust was never in the parity set.)

## Non-goals

- Not relaxing the typed-write boundary, the recovery-CLI-only boundary, or the
  single-writer/durability invariants.
- Not adding raw-SQL or a query DSL (the filter grammar stays small + fixed).
- Not deciding the individual read-verb signatures (owned by the impl-strategy
  slices + the agent-memory ADR); this ADR decides the *governance*, not the APIs.
- Not re-introducing v0.5.x wholesale (grouped/aggregation queries, FTS property
  schemas, etc. remain out unless a consumer need promotes them — see
  `dev/profiling/v05-lineage.md`).
