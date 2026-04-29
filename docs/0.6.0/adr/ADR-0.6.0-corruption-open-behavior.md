---
title: ADR-0.6.0-corruption-open-behavior
date: 2026-04-29
target_release: 0.6.0
desc: Engine.open refuses fail-closed on detected corruption; structured error; recovery is a separate CLI tool
blast_radius: requirements.md; acceptance.md; design/engine.md; design/recovery.md; design/errors.md; ADR-0.6.0-error-taxonomy; FU-VEC13-CORRUPTION; FU-RECOVERY-CORRUPTION-DETECTION
status: accepted
---

# ADR-0.6.0 — Corruption-on-open behavior

**Status:** accepted (HITL 2026-04-29).

Phase 3c-promoted ADR. Resolves FU-VEC13-CORRUPTION + FU-RECOVERY-CORRUPTION-DETECTION (the latter retargeted from 0.6.x to 0.6.0). Settles `Engine.open` behavior on detected corruption.

## Context

`Engine.open` runs at process start. It applies schema migrations (REQ-042), warms the embedder (Invariant D, owned by `embedder`), acquires the SQLite native exclusive WAL lock, and applies PRAGMAs. Any of these stages may surface corruption: WAL replay errors, schema migration failure on integrity-violating rows, header/format mismatch, sqlite-vec shadow-table inconsistency, or PRAGMA integrity-check violations (when run).

What `Engine.open` does on detected corruption was unspecified across 0.5.x. Three plausible behaviors:

1. **Refuse to open.** Surface a structured error; require operator to invoke a separate recovery tool.
2. **Auto-truncate to last consistent state.** Open succeeds; lossy.
3. **Open read-only.** Open succeeds in a degraded mode; allows queries against possibly-inconsistent state.

Silent truncation violates the project reliability principle (memory `feedback_reliability_principles`: no soak; no punt to a future release for bugs that already burned clients). Read-only-degraded mode hides corruption from operators who would otherwise escalate. Refuse-to-open is the only behavior consistent with operator-aware data-loss reasoning.

A separate forcing function: detection cadence ("WHAT do we check, WHEN") is design-owned (design/recovery.md), not ADR-owned. The ADR commits to the user-visible BEHAVIOR on detection; what triggers detection is a separate engineering decision.

## Decision

### 1. Refuse-to-open on detected corruption

`Engine.open` MUST fail-closed when corruption is detected at any stage of the open path. The engine MUST NOT auto-truncate, auto-rebuild, auto-replay-with-skip, or auto-degrade to read-only.

### 2. Structured error variant

Failure surfaces as `EngineOpenError::Corruption(CorruptionDetail)`. This ADR commits to the OUTER variant + the SHAPE of `CorruptionDetail`; the inner enum *variant lists* (`CorruptionKind`, `OpenStage`, `CorruptionLocator`) extend the ADR-0.6.0-error-taxonomy variant table and their authoritative enumeration is owned by `design/errors.md` (which extends the taxonomy variant table per its existing protocol).

```rust
pub enum EngineOpenError {
    // ... existing variants ...
    Corruption(CorruptionDetail),
}

pub struct CorruptionDetail {
    pub kind: CorruptionKind,        // enum; variants in design/errors.md
    pub stage: OpenStage,            // enum; variants in design/errors.md
    pub locator: CorruptionLocator,  // enum; variants in design/errors.md
    pub recovery_hint: RecoveryHint,
}

pub struct RecoveryHint {
    pub code: &'static str,        // stable machine-readable id, e.g. "E_CORRUPT_WAL_REPLAY"
    pub doc_anchor: &'static str,  // relative docs path, e.g. "recovery.md#wal-replay"
}
```

**Shape constraints (this ADR):**

- `CorruptionDetail` MUST carry `kind`, `stage`, `locator`, `recovery_hint`. Adding fields is non-breaking; removing or repurposing fields requires ADR amendment.
- `RecoveryHint` is structured (machine-readable code + doc anchor), NOT a free-text or English string. Bindings (Python, TypeScript) and CLI render the human sentence; tests dispatch on `code`.
- `OpenStage` MUST NOT include `LockAcquisition`. Lock-acquisition failure is a contention/permissions condition surfaced via a separate `EngineOpenError::Locked` variant, not corruption.
- `CorruptionLocator` MUST NOT include a free-form `Unspecified` escape hatch. When SQLite reports an opaque error with no usable locator, the variant carries the SQLite extended code (e.g. `OpaqueSqliteError { sqlite_extended_code: i32 }`) so something is always captured. `design/errors.md` MUST justify every locator variant.

### 3. Recovery is a separate CLI tool

Recovery is invoked exclusively via `fathomdb recover` (CLI surface; consistent with ADR-0.6.0-cli-scope). The runtime SDK MUST NOT expose a recovery verb (consistent with REQ-054, REQ-037).

`fathomdb recover` is the only path that can mutate a corruption-marked database. Its behavior is owned by `design/recovery.md` and out of this ADR's scope, except for one constraint: the recovery tool MUST require an explicit operator opt-in flag (e.g. `--accept-data-loss`) for any path that is not bit-preserving.

### 4. Detection cadence is design-owned

This ADR does NOT mandate which checks run at every open or how expensive they may be. `design/recovery.md` owns the cadence policy: which checks are always-on vs cheap-only vs opt-in (e.g. `PRAGMA integrity_check` is O(N) on a large DB and likely opt-in via env or config). This ADR's commitment is purely behavioral: IF corruption is detected, REFUSE.

`design/recovery.md` MUST enumerate the always-on detection set. Reducing that set is a behavior change requiring an ADR amendment (anti-regression clause).

### 5. No partial state observable post-failure

When `Engine.open` fails with `Corruption`, no `Engine` handle is returned to the caller. The exclusive WAL lock (if acquired) is released. No SQLite connection is retained past the error return; any connection used for diagnosis (e.g. to populate a `CorruptionLocator`) is closed before the error is surfaced. No writer thread is spawned. No background scheduler runs. The process is in the same observable state as a never-attempted open.

## Options considered

**A — Refuse-to-open with structured error (chosen).** Operator-aware; consistent with reliability principles; prevents silent data loss; surfaces to recovery tool with explicit opt-in.

**B — Auto-truncate to last consistent state.** Maximizes availability. Rejected: silent data loss; violates reliability principle; clients cannot reason about commit durability when truncation is opaque.

**C — Open read-only on detected corruption.** Permits queries against possibly-inconsistent state. Rejected: hides escalation signal from operators; queries return possibly-wrong data with no error class to dispatch on.

**D — Auto-attempt recovery in-process at open.** Engine.open invokes recovery internally on detection. Rejected: collapses two operator decisions (open-and-fail vs open-and-recover) into one; recovery may be irreversible (drop sqlite-vec shadow tables, rebuild projections from canonical) and operator must own that choice.

**E — Configurable behavior (env / config flag selecting A vs B vs C).** Rejected: configuration of irreversible-action defaults is a Stop-doing per reliability principles; one default, explicit operator override via separate tool.

**F — Open succeeds; queries return `Corruption` per request (lazy / per-query corruption signal).** Mirrors how SQLite itself surfaces some `SQLITE_CORRUPT` paths. Rejected: collapses open-time vs query-time error class; forces every caller of every verb (`write`, `search`, `close`) to handle `Corruption`; pollutes the five-verb application surface contract (REQ-053) with a cross-cutting failure mode that is more appropriately surfaced once at open. Operators want a single binary signal at process start, not N signals per query.

## Consequences

- **Adds REQ-corruption-refuse-open** to requirements.md (1 REQ; Engine.open MUST refuse on detected corruption with structured error; recovery via separate CLI tool; never auto-truncate / auto-recover).
- **Adds ACs** to acceptance.md (4 ACs as enumerated): (1) refuse-to-open on detected corruption, (2) structured `EngineOpenError::Corruption(CorruptionDetail)` shape inc. `RecoveryHint { code, doc_anchor }`, (3) exclusive WAL lock released + no engine handle returned, (4) recovery reachable only via `fathomdb recover` CLI (SDK unreachability).
- **`design/engine.md` runtime open path** spec must wrap each stage in a corruption-detector that produces `CorruptionDetail` on failure.
- **`design/errors.md` + `design/bindings.md`** must surface `EngineOpenError::Corruption` as a typed binding error (Python: `CorruptionError`; TypeScript: `CorruptionError`); covered transitively by REQ-056 / AC-060a.
- **`design/recovery.md`** owns: which checks run at open (cadence), which are opt-in vs always, what `fathomdb recover` does, and the `--accept-data-loss` opt-in surface.
- **ADR-0.6.0-error-taxonomy** variant table extended with `CorruptionKind` enum (5 variants); error-taxonomy AC-060a measurement enumerates these per the existing protocol.
- **FU-VEC13-CORRUPTION + FU-RECOVERY-CORRUPTION-DETECTION** resolved by this ADR + design/recovery.md ownership.
- Operators who want availability over correctness on a corrupted DB must invoke recovery explicitly. There is no "just open it anyway" surface in 0.6.0.

## Citations

- HITL 2026-04-29.
- Memory `feedback_reliability_principles` (no soak; no punt; net-negative LoC on reliability work; Stop-doing on silent-degrade).
- ADR-0.6.0-error-taxonomy (extends variant table).
- ADR-0.6.0-cli-scope (recovery verbs CLI-only).
- REQ-031b (zero corruption on power-cut — durability side); REQ-031c (recovery time — `Engine.open` after unclean shutdown).
- REQ-037 (SDK unreachability of recovery verbs); REQ-054 (CLI completeness).
- FU-VEC13-CORRUPTION; FU-RECOVERY-CORRUPTION-DETECTION (both resolved here).
