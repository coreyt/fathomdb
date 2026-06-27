# ADR-0.8.6 — Governed-Verb Coupling Hygiene (OPP-5)

> **Status:** ACCEPTED — HITL-SIGNED 2026-06-26 (Slice-0 gate).
> **Implements at:** 0.8.6 Slice 10 (parity-harden the governed read surface).
> **Why first (program):** HITL ruled the consumer (Memex) migrates onto governed verbs **before**
> OPP-1/2/4 layer on top — it is a correctness prerequisite for 0.8.10.

---

## 1. Context

OPP-5 asks: is the **governed read surface** complete/stable enough to be **the** consumer boundary, so
the Memex agent can migrate off any internal-engine reach? Verifying state from git (Explore sweep):

**The surface is already complete and LIVE in both bindings**, on a single shared allowlist
(`src/conformance/governed-surface-allowlist.json`):

| Verb | Rust (`fathomdb-engine/src/lib.rs`) | Python | TypeScript | Landed |
|------|-------------------------------------|--------|------------|--------|
| `read.get` / `read.get_many` | `:3110` | `read.py:61` | `read.ts:129` | Slice 30 (G2) |
| `read.collection` / `read.mutations` | `:3276` | `read.py:85` | `read.ts:151` | Slice 30 (G3) |
| `read.list` (filter grammar) | `:3334` | `read.py:122` | `read.ts:187` | Slice 35 (G4) |
| `graph.neighbors` | `:3152` | `graph.py:44` | `index.ts:568` | Slice 20 (G5) |
| `graph.search_expand` | `:3200` | `graph.py:82` | `index.ts:592` | Slice 20 (G6) |

Every gap the Memex 0.6.0 note (`dev/memex-note-on-0.6.0.md`) named — by-id read, filtered list, graph
traversal, op-store reads — is **resolved**. The deliberate non-members (recovery verbs
`{recover,restore,repair,fix,rebuild}`, `doctor`, raw SQL/DSL, logical-id purge/restore) are CLI-only **by
design** (`ADR-0.8.0-supersede-five-verb-surface-cap.md`; `recovery-denylist-five-names`) and are **not**
consumer gaps.

## 2. Decision — scope Slice 10 as parity-hardening, not a build

The surface is complete; the **stability proof is not**. Two findings define the smallest sufficient work:

1. **Cross-binding parity gap (the real gap).** The conformance suite anchors Py↔TS equivalence on
   `read.list` only (`test_read_list.py:136`). The other governed read verbs have functional tests **per
   binding** but **no cross-binding equivalence harness** — so Py↔TS behavioral drift on `read.get`,
   `read.get_many`, `read.collection`, `read.mutations`, `graph.neighbors`, `graph.search_expand` is
   currently **undetectable** (the exact failure mode `conformance-rewrite-vacuous-green-trap` warns of).
2. **Consumer-boundary completeness assertion.** There is a membership allowlist (`test_surface.py`), but
   no test that asserts the OPP-5 read paths require **no internal-engine reach** — i.e. that the verbs a
   migrating consumer needs are all *on* the governed surface. R-CH-1 wants this stated as a contract.

**Slice 10 therefore delivers:**

- **X1 cross-binding functional harness** covering **every** governed read verb (not just `read.list`):
  same fixture DB, same inputs, assert Py and TS return equivalent records/order/error envelopes. Modeled
  on the `read.list` anchor; demonstrate the catch in RED (introduce a deliberate Py↔TS divergence, watch
  it fail) per `conformance-rewrite-vacuous-green-trap`.
- **A consumer-boundary conformance test** enumerating the governed verbs a consumer needs and asserting
  each is reachable on the governed surface with no internal-engine call required for the OPP-5 read paths.
- **Doc reconciliation:** `dev/interfaces/{python,typescript}.md` still describe the 0.6.0 five-verb lock
  and predate the read verbs — update them to reflect the live governed surface (X3).

**Out of scope (not gaps):** adding any new verb; touching the recovery denylist; exposing purge/restore
or raw SQL. G7 (`read.history`) stays deferred per `ADR-0.8.0`.

## 3. Acceptance (Slice 10 DoD)

- **R-CH-1:** a conformance test enumerates the governed verbs a consumer needs; no internal-engine reach
  is required for the OPP-5 read paths (asserted, not asserted-by-absence).
- **R-CH-2:** the cross-binding functional harness lands in **both** Py + TS and is **green**, with the
  RED demonstration committed (a forced Py↔TS divergence fails it).
- **X1/X3:** parity harness in both bindings same-slice; interface docs reconciled to the live surface;
  `DOC-INDEX.md` updated in the closing docs commit.

## 4. Consequences

- Memex can migrate onto the governed surface with a **proven** Py↔TS-stable boundary — the prerequisite
  0.8.10 (#6/#7 layered on top) depends on.
- Future surface drift between bindings becomes a CI failure, not a silent consumer break.

## 5. Sources

- Explore sweep of the governed surface (verb table §1); `src/conformance/governed-surface-allowlist.json`.
- `ADR-0.8.0-supersede-five-verb-surface-cap.md`, `ADR-0.8.0-filter-grammar.md`,
  `ADR-0.8.0-graph-traversal-scope.md`; `dev/memex-note-on-0.6.0.md`.
- Memory: `acceptance-md-locked-no-feature-acs`, `recovery-denylist-five-names`,
  `conformance-rewrite-vacuous-green-trap`.
