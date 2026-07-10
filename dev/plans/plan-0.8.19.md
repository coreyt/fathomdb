---
title: FathomDB 0.8.19 — Plan (state-machine ladder)
subtitle: OPP-12 record-lifecycle Phase-1 — lifecycle + id (odd micro, LABEL-ONLY)
date: 2026-07-09
status: PROPOSED
target_release: 0.8.19
---

# FathomDB 0.8.19 — Plan (state-machine ladder) · **OPP-12 record-lifecycle Phase-1 — lifecycle + id**

> **▶ Slices 5 + 15 LANDED to `main` (2026-07-09, HITL-approved both landings; `origin/main` @ `c8e2a5b3`).**
> Slice 5 (existence axis + SCHEMA 19→20) `36074f91`+`a6970496`; Slice 15 (C-2 `IdSpace` `SearchHit.id` swap,
> breaking/label-only) `6616db93`+`a704c317`+`51c2c785`; cross-slice compose-fix `c8e2a5b3`. Combined-DoD green
> (`cargo check`/`clippy`/`fmt --check` --workspace = 0; 19→20 migration fresh==upgrade 3/3; id-swap + existence
> axis compose). **X1 py/ts test *execution* deferred to a quiesced-main window** (Rust bindings compile clean;
> isolated wheel build hit the native-import module-name quirk). **Next: Slice 10 (transition/purge), unblocked.**
>
> **▶ Slice 0 CLOSED — HITL-signed 2026-07-09** (codex design review terminal, no residual BLOCK;
> independent adversarial audit). **gap-1 ruling 1a + the 5 unbundled gaps APPROVED.** **1a:**
> anonymous/doc-seeded hits stay `h:`; the anonymous-surrogate `logical_id` mechanism is **deferred to
> Phase-2 (0.8.20)** — so Phase-1's `SearchHit.id` values stay byte-identical to today (no gold-remap), the
> 19→20 migration carries **existence columns only**, and **Slice 15 is parallelizable off Slice 0** (no
> longer gated on Slice-5's surrogate). Reconciled across this plan + master §4 rows + F-23. Gaps:
> purge=cascade-remove · secure_delete+documented-residual · read-without-`ReadView`=complete ·
> `PreparedWrite` state/reason. **Next = fan Slice 5 ∥ 10 ∥ 15** (schema/id-swap landings HITL-gated;
> label-only). See master **F-23**.
>
> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.19-implementation.md`; live state → `runs/STATUS-0.8.19.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (§4 0.8.19 row, F-19/F-20/F-21/F-22). OPP-12 design authority →
> `dev/design/record-lifecycle-protocol/` (`structural-lifecycle-contract.md`, `api-surface.md`,
> `OPP-12-C1-converged-contract.md`). Run via `/goal complete 0.8.19` as an **orchestrator** session.
>
> **Theme.** First half of OPP-12, the record-lifecycle & id protocol pulled into the 0.8.x line
> (F-19/F-20). Phase-1 lands the **existence axis** (`pending/active/deleted/purged`), its
> **`transition`/`purge`** verbs, the **C-2 typed `SearchHit.id` swap** (`write_cursor → logical_id` +
> typed `IdSpace {space, value}`; TC-8) with **surrogate `logical_id`** minting for anonymous nodes, and
> the **`SCHEMA_VERSION` 19→20** migration that carries them. Phase-2 (read-modes / node-validity /
> projection-registry C-1 co-land / `dense_readiness`) is **0.8.20, NOT here**.
>
> **Odd micro — LABEL-ONLY, regardless of content.** `13` is HITL-forbidden and already skipped; this is
> the next odd slot. Per F-19/F-20 the odd line is **OOB label-only** (no manifest bump / `v*` tag /
> publish; manifests stay `0.8.9`). **The coordinated breaking-pair publish is 0.8.20** — Phase-1 builds
> the breaking id surface but publishes nothing. **BUILD-AUTHORIZED (HITL 2026-07-08, F-21); build ≠
> adopt** — any change to shipped **default behavior** (schema migration, default-read exclusion, the id
> reshape as the caller-facing key) is separately **HITL-gated** (§2, §5).
>
> **Footprint.** All IN-LIBRARY (schema + engine + bindings), CPU-only / deterministic. No LLM, no
> network, no GPU on the query path. The existence axis is an indexed enum column; the id swap is a
> surface/type reshape; purge is a destructive-but-local hard-erase. Nothing here touches the embedder or
> the 1-bit vector path.

---

## 1. Goal & scope

Scope is **exactly** the master §4 0.8.19 row — do not exceed. Everything below is grounded in the OPP-12
design docs; Slice-0's ADR finalizes the detailed semantics (the docs are **PROPOSED**, not frozen — see
§9 underspecified-for-Phase-1 flags).

- **Existence axis (`pending/active/deleted/purged`).** A mutually-exclusive engine enum on one indexed
  `state` column + one `reason` column on `canonical_nodes` (`api-surface.md` C9 — *not* per-transition
  columns; `structural-lifecycle-contract.md` §1). `pending` = present + versioned but not admitted to
  default retrieval (quarantine/promotion gate); `active` = admitted; `deleted` = soft-deleted, retained +
  recoverable, excluded from default reads, stays indexed behind a flag; `purged` = terminal, physically
  erased. `PreparedWrite::Node` gains an initial `state: InitialState ∈ {pending, active}` (default
  `active`) + `reason` (`api-surface.md` C10/S3) — you cannot *create* a `deleted`/`purged` node. **The
  default-read exclusion** (`state = active`) is added to the hot path; since no existing row is non-active
  (only the new verbs mint non-active states), the filter is a **no-op on the shipped corpus** (preserves
  the eu7 no-op basis — see §2 R-GATE and §9).
- **`transition` / `purge` verbs.** `transition(logical_id, to_state: LifecycleState, reason?)` for the
  *reversible* moves (promote `pending→active`, reject `pending→deleted`, soft-delete `active→deleted`,
  **undelete** `deleted→active` — **never** `restore`, a `recovery_denylist` name). The engine enforces
  the legal-transition table; illegal moves return a **typed `IllegalTransitionError { from_state,
  to_state, legal: [...] }`** (fields `from_state`/`to_state`, **not** `from`/`to` — S7: `from` is a Python
  reserved word). `purge(logical_id)` is a **separate** irreversible verb (C2): GDPR-grade hard-erase of
  all versions + FTS/vector rows, `PRAGMA secure_delete=ON`/`VACUUM`, edges-to-purged cascade-removed or
  converted to a content-free referential stub. (`purge` is *not* on the `recovery_denylist`.)
- **C-2 typed `SearchHit.id` swap (TC-8).** `SearchHit.id` moves from the interim positional
  `write_cursor: u64` (verified `lib.rs:1173`) to the stable `logical_id`, exposed as a **typed
  `IdSpace { space, value }` newtype** (C-2 RATIFIED as binding — `l:`/`h:`/`p:` are `IdSpace` variants,
  *not* a magic-prefixed string). `id` becomes **non-null and id-space-total** across all three hit
  classes (`l:<logical_id>` canonical · `h:<content-hash>` doc-seeded/anonymous · `p:<passage>` synthetic
  rerank). The interim `write_cursor` **and** the additive Cause-A `stable_id` field (`lib.rs:1196`) are
  **subsumed into `id`, not dropped** (real-gold keying continues on `id`). **Lifecycle verbs
  (`transition`/`purge`) key on the bare `logical_id` — the `l:` space only**; `h:`/`p:` hits are not
  lifecycle-addressable by design.
- **Anonymous-node surrogate `logical_id` — DEFERRED to Phase-2 (0.8.20), gap-1 ruling 1a (HITL 2026-07-09, master F-23).** Doc-seeded / anonymous nodes (`PreparedWrite::Node { logical_id: None }`, `lib.rs:4805`) **stay `h:<content-hash>`** in Phase-1 — they are **not** minted a surrogate `logical_id` and are **not** lifecycle-addressable. Rationale: Memex's Phase-1 need is closed by the C-2 swap alone (`structural-lifecycle-contract.md §2(ii)` — its records carry `logical_id`s); `h:` is a first-class *terminal* `IdSpace` variant (`api-surface.md §SearchHit`); a surrogate has no stable meaning absent the Phase-2 natural-key/projection registry (C-1 Q6b). **This SPLITS the "lands-together GATING" co-requisite** stated in the OPP-12 authority docs — the C-2 swap is **total via `l:`/`h:`/`p:` without the surrogate**, so it needs no anonymous surrogate to be complete. The anonymous-surrogate minting mechanism + the doc-seeded end-state decision are **0.8.20 (Phase-2)**. Consequence: **`SearchHit.id` values stay byte-identical to today's `stable_id`** → no gold-remap; eu7 keying a true no-op.
- **`SCHEMA_VERSION` 19→20 migration — EXISTENCE COLUMNS ONLY (R-MIG-1 narrowed, 1a).** One migration carries the net-new existence columns (`state`, `reason`) on `canonical_nodes` — **no surrogate backfill** (deferred to Phase-2). Fresh-create and upgrade-from-19 must both land the same shape (one migration per release — I-6 discipline). Verified base: `fathomdb-schema/src/lib.rs:6` `SCHEMA_VERSION = 19` (after 0.8.18 Slice-5 #5).
- **X1 SDK parity.** Every new surface — `LifecycleState`, `IllegalTransitionError`, `transition`/`purge`,
  the typed `IdSpace` `SearchHit.id`, `PreparedWrite::Node.state`/`reason` — threads through **both**
  `fathomdb-py` and `fathomdb-napi` with parity; the SDKs stay thin pass-throughs (no client-side logic).

**Out of Phase-1 scope (→ 0.8.20 Phase-2, NOT here):** `ReadView` / read-mode relax-flags
(`include_deleted`/`include_superseded`/`valid_as_of`/`ignore_validity`), node-validity
(`valid_from`/`valid_until`), the projection registry + `configure_projections` + EAV/property-FTS (C-1
co-land), and `dense_readiness`. Phase-1 adds the `active`-only default filter but **not** the flags to
relax it — surfacing deleted/superseded/as-of rows is Phase-2.

*Prereqs (all met — cite):* EXP-S substrate (0.8.14 ✓) · Cause-A `stable_id` base (✓, `lib.rs:1196` /
`derive_stable_id` `:9455`) · C-1 RATIFIED both sides (✓, `OPP-12-C1-converged-contract.md`) · F9 for the
`rankable` role (0.8.16 ✓, forward-compat only — `rankable` is Phase-2) · #11-full publish machinery
(0.8.18 ✓, the prerequisite for 0.8.20's breaking-pair publish, *not exercised here*).

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-EX-1 | Existence axis: `state` + `reason` columns; `LifecycleState` enum; `InitialState ∈ {pending, active}` on `PreparedWrite::Node` | Write with `state=pending`/`active` round-trips; `deleted`/`purged` are **not** creatable via write (typed rejection); RED test asserts the create-time subset |
| R-EX-2 | Default-read exclusion = active-only (no relax flags in Phase-1) | RED: a `deleted` node is absent from default `search`/`read.*`; GREEN: an `active` node is present; **no-op on the shipped corpus** (no existing row is non-active) |
| R-TR-1 | `transition(logical_id, to_state, reason?)` enforces the legal-transition table | Each legal move (promote/reject/soft-delete/undelete) succeeds; each illegal move (`purged→*`, `active→purged`, …) returns typed `IllegalTransitionError { from_state, to_state, legal }` enumerating legal targets |
| R-TR-2 | `IllegalTransitionError` is a real typed exception with parity-safe field names | pyo3 exception + napi error-with-fields; fields `from_state`/`to_state` (NOT `from`); X1 parity test green |
| R-PG-1 | `purge(logical_id)` hard-erases all versions + FTS/vector rows; edges cascade-removed or content-free stub | RED: purged content is unretrievable in **every** read path (there is none); FTS/vector entries gone; edge-to-purged is a content-free stub or removed |
| R-PG-2 | Purge erasure preconditions honored | `secure_delete=ON` (or post-purge `VACUUM`); surrogate `logical_id` is opaque (never content-hashed) so a retained stub leaks nothing; `purge` requires deleted-first + is idempotent |
| R-ID-1 | C-2 typed `SearchHit.id` swap (TC-8): `write_cursor → logical_id`, typed `IdSpace {space, value}`, non-null + id-space-total | `id` is a typed `IdSpace` across all 3 hit classes (`l:`/`h:`/`p:`); `write_cursor` **and** `stable_id` retired *into* `id` (subsumed, not dropped); RED test asserts `id` totality + type |
| R-ID-2 | Typed `IdSpace` round-trips through all 4 SDK bindings | `SearchHit.id` `{space, value}` round-trips in Rust · pyo3 · napi/TS · (X1 cross-binding harness); lifecycle verbs accept the bare `logical_id` (`l:` space only); `h:`/`p:` are not lifecycle-addressable (typed refusal) |
| R-ID-3 **(narrowed by 1a)** | **Governed** nodes are `l:`-addressable (they already carry a `logical_id`); **anonymous/doc-seeded hits stay `h:`** and are NOT lifecycle-addressable in Phase-1. Anonymous-surrogate minting → **Phase-2 (0.8.20)** | RED: a governed-node hit's `id` is `l:<logical_id>` and `transition`/`purge` accept it; RED: a doc-seeded hit's `id` stays `h:<content-hash>`, `transition(h:…)` raises `NotLifecycleAddressableError`; `SearchHit.id` values unchanged vs today's `stable_id` |
| R-MIG-1 **(HITL-gated; narrowed by 1a)** | `SCHEMA_VERSION` 19→20 migration carries the **existence columns (`state`/`reason`) ONLY** — **no surrogate backfill** (deferred to Phase-2) | Fresh-create at 20 and upgrade-from-19 both land the identical shape; migration test green both paths; **HITL sign-off required before the bump lands** |
| R-X-1 | Py + TS SDK parity for every new surface | X1 cross-binding harness green (`transition`/`purge`/`IdSpace` id/`LifecycleState`/`IllegalTransitionError`/`PreparedWrite` state) |
| R-GATE | All frozen gates hold on the label-only candidate | eu7 ≥ 0.90 (one-sided CI) on the **no-op basis** (default ranking/vector path byte-unchanged; the `state=active` filter is a no-op on the shipped corpus — see §9 for the gold-keying caveat) · AC-012/013/020 latency unaffected · AC-074 governed-surface allowlist updated for the net-new verbs/types (HITL-decided at the gated slice) |

**HITL-gated items (build ≠ adopt, F-21).** (a) **R-MIG-1** — the `SCHEMA_VERSION` 19→20 bump is a
default-behavior change; it lands only on HITL sign-off. (b) **Any adoption-default** — making the id
reshape the *caller-facing default key*, or turning on default-read exclusion as *observable* behavior for
consumers, is a separate adoption arm, HITL-gated. Building the surface (label-only) is authorized;
flipping shipped defaults / publishing is not (publish = 0.8.20 only).

New ACs: candidates at Slice 0 (existence-axis state-machine contract; `IdSpace` id-totality contract) and
Slice 40 (Phase-1 release-readiness / AC-074 surface delta), HITL-decided per §6. No invented AC ids —
track by OPP-12 requirement id (R-EX/R-TR/R-PG/R-ID/R-MIG) + TDD test names per the locked acceptance
policy.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 40        (1, 11, 20, 25, 30, 35 = reserved gaps)
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0 ✅** | **Setup + ADR (X0-gated) — DESIGN CLOSED, HITL-signed 2026-07-09 (codex review terminal; adversarial audit).** Froze Phase-1 semantics: existence-axis state machine + legal-transition table; `LifecycleState`/`IllegalTransitionError`/`InitialState` types; C-2 `IdSpace` newtype; **19→20 migration = existence columns only** (gap-1 **1a**: anonymous-surrogate minting + `h:` end-state → Phase-2); purge = **cascade-remove** edges (gap-3); `secure_delete` PRAGMA + documented residual (gap-4); `active`-only read complete surface (gap-5); `PreparedWrite::Node` state/reason (gap-6); Phase-1/Phase-2 boundary; eu7 no-op basis. Board `runs/STATUS-0.8.19.md` | design-adr | — |
| **5 ✅** | **LANDED to main `36074f91`+`a6970496` (2026-07-09, HITL-approved 19→20 bump; combined-DoD green).** **KEYSTONE — existence axis + SCHEMA 19→20 migration** *(HITL-gated bump)*. `state`/`reason` columns; `InitialState` on `PreparedWrite::Node`; `active`-only default-read exclusion co-located with the 61 `superseded_at IS NULL` sites; the 19→20 migration (**existence columns only, no surrogate backfill** — 1a; fresh + upgrade). **No surrogate minting** (→ Phase-2, 1a) | implementation (schema + write/read path) | 0 |
| **10** | **`transition` / `purge` verbs.** `transition(logical_id, to_state, reason?)` + legal-transition enforcement + typed `IllegalTransitionError`; `purge(logical_id)` hard-erase (secure_delete/VACUUM, FTS/vector removal, **edges-to-purged cascade-removed** — gap-3, no stubs in Phase-1, deleted-first + idempotent) | implementation (engine verbs) | 5 |
| **15 ✅** | **LANDED to main `6616db93`+`a704c317`+`51c2c785` (2026-07-09, HITL-approved breaking surface, label-only; combined-DoD green; compose-fix `c8e2a5b3`).** **KEYSTONE — C-2 typed `SearchHit.id` swap (TC-8).** `write_cursor → logical_id`; typed `IdSpace {space, value}` newtype (`l:`/`h:`/`p:`), non-null + total; retire `write_cursor` + subsume `stable_id` into `id`; lifecycle verbs key on bare `logical_id` (`l:` only); **doc-seeded/anonymous stay `h:`** (1a — no surrogate). Breaking surface — **built label-only** (publish = 0.8.20). **Parallelizable off Slice 0 (1a): no longer needs Slice-5's surrogate** | implementation (surface/type reshape) | 0 |
| **40** | **Verification + Phase-1 release-readiness (label-only close).** X0/X1/X2/X3; R-EX/R-TR/R-PG/R-ID/R-MIG AC gate; migration fresh+upgrade; `IdSpace` round-trip all 4 bindings; eu7 no-op basis + gold-keying caveat recorded; AC-074 surface delta; codex §9. **No `v*` tag / publish — manifests stay `0.8.9`** | verification | 5,10,15 |

**Keystones / hard gates.**

- **Slice 5 is the schema keystone AND a HITL gate** — the 19→20 migration (R-MIG-1) is a default-behavior
  change; it does not land until HITL signs off (§2, §5 X0). It carries the existence columns
  (`state`/`reason`) in **one** migration — **existence columns only, no surrogate backfill** (1a; the
  surrogate is Phase-2). One migration per release (I-6 discipline).
- **Slice 15 is the id-surface keystone** — the C-2 swap is a **breaking** `SearchHit.id` reshape. It is
  **built label-only**; the coordinated breaking-pair publish is 0.8.20 (F-19/F-21). Its RED test is
  two-sided: `id` is total + typed across all three hit classes (`l:` governed · `h:` doc-seeded/anonymous ·
  `p:` synthetic), and `write_cursor`/`stable_id` are retired *into* `id` without losing the real-gold key.
- **Slice 15 is PARALLELIZABLE OFF SLICE 0 (re-derived under 1a)** — with the surrogate deferred to Phase-2,
  the C-2 swap sets `id` = the existing `derive_stable_id` output (`l:`/`h:`/`p:`), which needs **neither**
  Slice-5's existence columns **nor** the transition/purge verbs. So **Slices 5, 10, 15 can all run
  concurrently off Slice 0** (previously Slice 15 was gated on Slice 5's surrogate — no longer true).

**Tracks (parallelizable).** After Slice 0: **{5 ∥ 10 ∥ 15}** (10 after 5 for the verbs' state columns; 15 independent) → converge at Slice 40.

---

## 4. Reserved-gap policy

Carried unchanged (`dev/plans/plan-0.8.1.md` §Numbering). Reserved gaps occupy odd/unused slice numbers
within the ladder. If an unexpected fix is needed mid-release it lands in the next available reserved-gap
slot (1, 11, 20, 25, 30, 35) with its own PR and documented rationale — not as scope creep inside an
existing slice.

---

## 5. Cross-cutting DoD (X0/X1/X2/X3 — bind EVERY slice)

**X0 — Elevated process gate (HITL 2026-07-08; binds every unit that ships code).** BEFORE any
implementation, each unit requires, in order: **(A) written requirements with explicit, RED-testable
acceptance criteria**, then **(B) an independent design review** (mechanism: **codex**, adversarial-subagent
fallback) **→ HITL sign-off**. Only then does **RED/GREEN TDD → codex §9 code review** proceed. This is a
higher bar than 0.8.16 — the *design* is reviewed before code, not just the diff after. Applies to Slice-0
ADRs, Slice 5 (existence axis + migration), Slice 10 (transition/purge), and Slice 15 (C-2 id swap).

> **Gate carries forward from 0.8.18 (Steward proposes; HITL may relax).** X0 was instituted at 0.8.18 §5
> and is carried into 0.8.19 as the default posture — OPP-12 Phase-1 is a breaking, schema-migrating,
> multi-binding surface, exactly the class X0 exists for. The Steward proposes X0 stands; HITL may relax
> it per-slice.

- **X1 — SDK parity.** Every net-new surface lands in **both** `fathomdb-py` + `fathomdb-napi` with the
  cross-binding harness green (the typed `IdSpace` `SearchHit.id`, `transition`/`purge`, `LifecycleState`,
  `IllegalTransitionError` field names, `PreparedWrite::Node.state`/`reason`). SDKs stay thin.
- **X2 — `mkdocs build` green.** Documentation build stays green (release gate). New/changed docs (the
  Phase-1 ADR, the OPP-12 design-doc slot updates) must not break `mkdocs --strict`.
- **X3 — Docs + DOC-INDEX per slice.** Each slice's findings recorded in `runs/STATUS-0.8.19.md` (the live
  state spine, per-slice X column incl. X0 design-review state).

---

## 6. Acceptance-criteria policy

`dev/acceptance.md` is **status: locked** (HITL-blessed). New AC ids are minted only at gated slices
(Slice 0 and Slice 40), HITL-decided. Do NOT invent AC ids for OPP-12 requirements — track by requirement
id (R-EX/R-TR/R-PG/R-ID/R-MIG) + TDD test names. **AC-074 (governed-surface allowlist)** grows for the
net-new verbs/types (`transition`, `purge`, `LifecycleState`, `IllegalTransitionError`, the typed
`IdSpace`, `PreparedWrite` extension) — the delta is proposed to HITL at the Slice-0 or Slice-40 gate
(`api-surface.md` estimates 16 → ~19 verbs; the type/field delta is Phase-1's subset of the ~+5 governed
types, since `ReadView`/`ProjectionSpec`/validity are Phase-2).

---

## 7. Prerequisites

All prerequisites are **satisfied** on `origin/main` at plan date (2026-07-09):

1. **EXP-S substrate — DONE @0.8.14** (`465b43ac`, SCHEMA 15→17). The kind-tagged substrate the lifecycle
   axis rides.
2. **Cause-A `stable_id` base — DONE** (`SearchHit.stable_id`, `lib.rs:1196`; `derive_stable_id`,
   `lib.rs:9455`). The additive id carrier the C-2 swap subsumes into `id`.
3. **C-1 RATIFIED both sides — DONE** (`OPP-12-C1-converged-contract.md`, HITL + Memex, sub-ledger seq
   6/7/8). Grounds the surrogate space/value split (Q6b) the C-2 swap consumes; the projection registry
   itself is Phase-2 (0.8.20).
4. **F9 importance/confidence — DONE @0.8.16** (`8c6b92aa`, SCHEMA 17→18, OPP-12-`rankable`-forward). Only
   forward-compat relevance here — `rankable` projections are Phase-2.
5. **#11-full publish machinery — DONE @0.8.18** (`bce032a0`). The prerequisite for 0.8.20's breaking-pair
   publish; **not exercised at 0.8.19** (Phase-1 is label-only).
6. **`SCHEMA_VERSION = 19`** on `origin/main` (`fathomdb-schema/src/lib.rs:6`, verified) — so Phase-1's
   migration is **19→20**.
7. **`SearchHit.id: u64`** on `origin/main` (`fathomdb-engine/src/lib.rs:1173`, verified) — the interim
   `write_cursor` carrier the C-2 swap targets.

Worktrees off `$(git rev-parse main)`; schema/migration and `maturin develop` on the MAIN tree only; no
`v*` tag (Phase-1 is label-only regardless — `release-publish-gotchas`).

---

## 8. Out-of-band / parallel notes + key callouts

- **BUILD-AUTHORIZED (F-21), build ≠ adopt.** HITL cleared the build gate for Phase-1 (and Phase-2); this
  authorizes *building* the surface. It does **not** authorize adoption arms (flipping shipped defaults) or
  publish. R-MIG-1 (the migration) + any adoption-default are HITL-gated (§2).
- **The C-2 id swap is a breaking surface built label-only.** `SearchHit.id` changes type
  (`u64 → IdSpace`) and meaning (`write_cursor → logical_id`). The **coordinated breaking-pair publish is
  0.8.20** (even, HITL-gated), pairing with a Memex `0.5.x-successor` — **not here**. 0.8.19 ships nothing
  to a registry; manifests stay `0.8.9`.
- **OPP-12 Phase-2 is 0.8.20, NOT here.** `ReadView`/read-modes · node-validity
  (`valid_from`/`valid_until`) · projection registry (C-1 co-land) + EAV/property-FTS · `dense_readiness`
  are all **0.8.20**. Phase-1 must not build them; it only lands the `active`-only default filter (the
  relax-flags that surface excluded rows are the Phase-2 `ReadView`).
- **`13` is HITL-forbidden** as both minor and micro — the odd line skips it by hard constraint (already
  skipped; 0.8.19 is the next odd slot after 0.8.17/0.8.18).
- **Free-threading + #13 benchmark harness re-homed to `plan-0.8.21.md`** (F-19/F-20) — it formerly held
  this filename; do not resurrect it here.
- **Sequencing (F-21).** OPP-12 stays **in-sequence after 0.8.18** (`0.8.16 → 0.8.18 → 0.8.19 → 0.8.20`).
  Building Phase-1 OOB *concurrently* with an in-flight even release risks **two concurrent
  `SCHEMA_VERSION` migrations on the shared tree** — the Steward recommends in-sequence unless HITL elects
  explicit migration-ordering.
- **eu7 basis (F-22).** The eu7 basis decision (no-op vs real re-baseline) is formally **0.8.20's**; at
  0.8.19 the intent is the **no-op basis** (default ranking/vector path byte-unchanged; `state=active` is a
  no-op on the shipped corpus). **Caveat (§9):** the surrogate-`logical_id` backfill changes
  `SearchHit.id` *values* for existing doc-seeded rows — a **real-gold keying** change, not a ranking
  change; eu7 (recall/fidelity) is unaffected but real-gold keying must be re-mapped. Flag at Slice-0.
- **Reference the OPP-12 design docs for detailed semantics** — `structural-lifecycle-contract.md` (axes,
  transition table, purge erasure), `api-surface.md` (verb/type/field delta, the C-fixes), and
  `OPP-12-C1-converged-contract.md` (surrogate space/value split). They are **PROPOSED**; **Slice-0's ADR
  finalizes them** (see §9).

---

## 9. Immediate next slice

**Slice 0 — OPP-12 Phase-1 ADR + DoD freeze (X0-gated).**

Concrete deliverables:

1. **Verify the base from `origin/main`** — `SCHEMA_VERSION = 19` (`fathomdb-schema/src/lib.rs:6`);
   `SearchHit.id: u64` (`fathomdb-engine/src/lib.rs:1173`); Cause-A `stable_id`/`derive_stable_id`
   (`:1196`/`:9455`); anonymous-node `logical_id: None` path (`:7188`). Confirm the swap/mint targets.
2. **Freeze the existence-axis state machine** — the enum, the legal-transition table (promote/reject/
   soft-delete/undelete + `purge`), the `InitialState ∈ {pending, active}` create-time subset, and the
   `IllegalTransitionError { from_state, to_state, legal }` shape (parity-safe field names).
3. **Design the C-2 `IdSpace` newtype** — `{ space, value }` with `l:`/`h:`/`p:` variants; the total-`id`
   contract across all three hit classes; how `write_cursor` + `stable_id` are subsumed; how lifecycle
   verbs enforce `l:`-only addressability.
4. **Plan the ONE 19→20 migration** — existence columns (`state`/`reason`) **and** the surrogate backfill
   in a single migration; fresh-create vs upgrade-from-19 parity; the R-MIG-1 HITL gate.
5. **Freeze the Phase-1/Phase-2 boundary** — no `ReadView`/validity/projections/`dense_readiness` here;
   only the `active`-only default filter; the relax-flags are 0.8.20.
6. **Resolve the underspecified-for-Phase-1 items below** (flag to HITL at the X0 gate).
7. **Stand up `runs/STATUS-0.8.19.md`** — state spine, per-slice X0/X1/X2/X3 columns, Slice 5 → {10 ∥ 15}
   → 40.

Then fan out **Slice 5**, and after it **Slices 10 ∥ 15**.

### Underspecified in the OPP-12 design docs for Phase-1 (flag at Slice-0 — the docs are PROPOSED)

- **Surrogate-`logical_id` ↔ `h:` id-space interaction (LOAD-BEARING).** If every anonymous doc-seeded node
  is minted an opaque surrogate `logical_id`, does `SearchHit.id` for those (the *dominant* corpus hit
  type) become `l:<surrogate>` or stay `h:<content-hash>`? The contract says lifecycle verbs are `l:`-only
  and `h:` is "not lifecycle-addressable" — but minting a surrogate makes doc-nodes addressable, which
  would migrate the dominant hit class `h:`→`l:`. This is unresolved and it drives both real-gold keying
  stability and which hits are lifecycle-addressable. **Slice-0 must rule.**
- **Migration backfill vs real-gold keying.** Backfilling surrogate `logical_id`s (and swapping
  `SearchHit.id` to them) changes the id *values* returned for the existing 18,472-doc corpus → any stored
  real-gold keyed on `write_cursor`/`stable_id` must be re-mapped. The docs assert "subsumed, not dropped"
  but give no migration recipe for existing gold. **Slice-0 must specify** the gold-remap (relates to
  F-8a's earlier gold-id remap at 0.8.14).
- **Purge edge-cascade scope.** `structural-lifecycle-contract.md` §1.2 requires edges-to-purged be
  cascade-removed *or* converted to content-free referential stubs — but the master 0.8.19 row enumerates
  only node-level items. **Is edge-stub conversion in Phase-1 (part of `purge`) or deferred?** The purge
  verb cannot be correct without an answer; likely in-scope, but the docs don't pin it to a phase.
- **`secure_delete` as a fresh-DB vs migration concern.** `PRAGMA secure_delete=ON` (or post-purge
  `VACUUM`) is a purge precondition, but it cannot retroactively scrub freelist pages written before it was
  enabled. **Slice-0 must decide** whether 19→20 sets `secure_delete` at migration and whether pre-existing
  freelist content is a documented residual (the honest-statement posture the docs favor elsewhere).
- **Default-read exclusion without `ReadView`.** Phase-1 adds `state = active` to the hot path but ships no
  relax-flags (Phase-2). Confirm there is **no Phase-1 way to read a `deleted`/`pending` row** (undelete
  still works via `transition`, but *searching* deleted content waits for 0.8.20's `include_deleted`) — and
  that this is acceptable, not a half-built read surface.
- **`PreparedWrite::Node` extension not called out in the master row.** The existence axis needs a
  create-time `state`/`reason` (`api-surface.md` C10/S3), i.e. a write-surface change. It's implied by
  "existence axis" but the master 0.8.19 row doesn't enumerate it — confirm it is in Phase-1 scope (it must
  be, to create `pending` nodes) and inside the X1 parity + AC-074 surface delta.

---

_Authoritative deps/decision record: `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` §4 (0.8.19 row),
F-19/F-20 (OPP-12 pulled into 0.8.x), F-21 (build-authorized; build ≠ adopt), F-22 (TC-8 @0.8.19)._
_OPP-12 design authority: `dev/design/record-lifecycle-protocol/` — `structural-lifecycle-contract.md`,
`api-surface.md`, `OPP-12-C1-converged-contract.md`._
_Verified base (`origin/main`): `SCHEMA_VERSION = 19` (`fathomdb-schema/src/lib.rs:6`); `SearchHit.id: u64`
(`fathomdb-engine/src/lib.rs:1173`)._
_Phase-2 (0.8.20): `plan-0.8.20.md` — read-modes / node-validity / projection-registry C-1 co-land /
`dense_readiness` + the coordinated breaking-pair publish._
