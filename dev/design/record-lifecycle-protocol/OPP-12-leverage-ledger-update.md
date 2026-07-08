# OPP-12 — proposed leverage-ledger update (FathomDB → Memex)

> **What this is.** Proposed replacement text for the **OPP-12** section of Memex's
> `dev/fathomdb/LEVERAGE-OPPORTUNITIES-LEDGER.md`. FathomDB prepares it **here** rather than editing the Memex
> repo directly (push-scope: fathomdb-only). **MEMEX applies this to its prose ledger and confirms.**
>
> **Ratification status: RATIFIED (both sides, 2026-07-03).** FathomDB HITL approved (enum seq-11) and **MEMEX
> agreed (enum seq-12)** — MEMEX has applied this mirror to `LEVERAGE-OPPORTUNITIES-LEDGER.md` (OPP-12 section +
> nav row). Ratification schedules nothing / authorizes no build (build ≠ adopt).

---

## Proposed replacement for the OPP-12 section (paste as-is into `LEVERAGE-OPPORTUNITIES-LEDGER.md`)

### OPP-12 — Record liveness / lifecycle: one coherent "is this live?" across retired / deleted / superseded

- **Status:** `RATIFIED` (both sides, 2026-07-03). Converged over the enum-discussion-ledger `seq 1→12`; the
  shape, all 10 seq-8 conditions, and the seq-9 residuals (C-1..C-5) are resolved; FathomDB HITL approved
  (seq-11) and MEMEX agreed (seq-12). The ratified design contract lives at FathomDB `dev/design/record-lifecycle-protocol/`
  (5 docs). **Ratification schedules nothing / authorizes no build (build ≠ adopt).**
- **UPDATE 2026-07-08 (F-19/F-20 resequence, HITL 2026-07-07):** OPP-12 is now **slotted in the FathomDB
  0.8.x line** — **Phase-1 @ 0.8.19** (odd, label-only: existence axis · `transition`/`purge` · C-2 typed
  `SearchHit.id` swap · schema migration · X1) + **Phase-2 @ 0.8.20** (even, publish; breaking-pair with a
  Memex `0.5.x-successor`; the C-1 projection registry co-lands here). This **supersedes** the earlier
  "≥0.9.x / not yet slotted" placement. Build ≠ adopt still holds — 0.8.19/0.8.20 build-authorization is a
  separate pending HITL gate. **NB (cross-repo):** the Memex-side `LEVERAGE-OPPORTUNITIES-LEDGER.md` mirror
  still carries the old "≥0.9.x / not yet slotted" text and needs a **coordinated Memex-side update** — NOT
  applied here (push-scope is fathomdb-only).
- **📒 Live discussion** is the dedicated JSONL ledger `dev/fathomdb/enum-discussion-ledger.jsonl` (append with
  `ledgerwrite`, watch with `ledgerwatch`). This OPP is the **index + ratified-outcome home**.
- **Owner:** MEMEX (raises the need + owns liveness *semantics*) / FATHOM (structure & mechanism).

**Ratified-shape outcome (one line).** Seam = **mechanism/policy**: the engine owns three orthogonal axes —
existence/admission `{pending→active→deleted→purged}` × version-currency (the **shipped `superseded_at`** G0
index, *not* `is_latest`) × temporal-validity (**edge-only today**; node-validity net-new) — plus an
engine-owned **projection registry** (async vector via the **existing** projection worker; `dense_readiness`;
reuse the shipped `drain()`). App owns transitions + reasons + edge/attr vocab & values + view-policy. Solves
**CR-056 / CR-057 / CR-060** + **Q1–Q4**.

**Surface delta (both SDKs, X1 parity).** **+3 verbs** (16→~19: `transition` / `purge` /
`configure_projections`; `drain` reused, `read.projections` folded); **+5 governed types** (`ReadView`,
`LifecycleState`, `IllegalTransitionError`, `ProjectionSpec`, `dense_readiness`); `SearchHit` shrinks;
**`undelete` not `restore`** (`restore` is on the recovery_denylist).

**seq-9 residuals (folded).**

- **C-1 (hard):** one declarative registration flow — a Memex `EntityTypeSpec` **drives** the engine
  `ProjectionSpec`; the app never syncs two registries (ordering / atomicity / idempotent re-registration /
  drift-detection in one place). Elevates the entity-schema-registry ↔ projection co-design from soft to hard;
  seam unchanged (Memex owns type + semantics + declarations; Fathom owns the projection mechanism).
- **C-2 (binding):** `SearchHit.id` is a typed `{ space: IdSpace, value }` newtype (`l:`/`h:`/`p:` → `IdSpace`
  variants), **not** a prefix-parsed string; lifecycle-addressability (`l:` space) is a **type check**. Binding
  id-carrier for the **Cause-A** lands-together id-contract (+ anonymous-node surrogate-minting).
- **C-3:** `purge` stays a **distinct gated verb** — the two-place state machine (`transition()` reversible +
  `purge()` terminal) is deliberate.
- **C-4 / C-5:** `LifecycleState` documented as **one family** (per-verb legal subsets that keep illegal states
  unrepresentable); `ReadView` + `SearchFilter` two-struct split kept (frozen `SearchFilter` preserved; a thin
  `QueryOptions` wrapper is a reserved, non-breaking later option).

**Gating.** The C-2 id-contract **lands-together with Cause-A** (`SearchHit.id` typed carrier + anonymous-node
surrogate-minting).

#### Thread

- `[2026-07-03][MEMEX]` seq 1 — problem + Q1–Q4 (3 orthogonal not-live encodings + relational `superseded` with
  no stored home; CR-056/057/060).
- `[2026-07-03][HITL]` seq 2 — relax (no migration required; breaking-OK both sides).
- seq 3–5 — clarify / note (the two "superseded" layers; the structural contract).
- `[FATHOM]` seq 6 **option** (structural contract) → `[MEMEX]` seq 7 **accept-the-shape** + 10 conditions →
  `[FATHOM]` seq 8 **reconciled** (resolves the 10; states the surface delta) → `[MEMEX]` seq 9 **accept** +
  residuals C-1..C-5 → `[FATHOM]` seq 10 **accept** residuals.
- `[FATHOM]` seq 11 — HITL-approved ratification; this mirror prepared FathomDB-side; awaiting MEMEX `agree`.
- `[MEMEX]` seq 12 — **AGREE → OPP-12 RATIFIED** (both sides); MEMEX applied this mirror to its leverage ledger.
- `[FATHOM]` seq 13 — confirms RATIFIED; thread `agreed` / CLOSED.
