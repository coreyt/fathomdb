---
title: ADR-0.6.0-subprocess-bridge-deferral
date: 2026-04-27
target_release: 0.6.0
desc: No subprocess bridge in 0.6.0; revisit in 0.8.0 (per HITL adjustment)
blast_radius: bindings.md; future tooling work; design/*.md (no IPC layer)
status: accepted
---

# ADR-0.6.0 — Subprocess bridge: deferred to 0.8.0

**Status:** accepted (HITL 2026-04-27).

Phase 2 #15 architecture ADR. Defers the subprocess-bridge wire format question entirely; revisits in 0.8.0.

## Context

0.5.x rewrite proposal mentioned a possible subprocess bridge for non-Rust language bindings (or operator tools). 0.6.0 has no concrete consumer: TS via napi-rs in-process; Python via PyO3 in-process; CLI is sync; default embedder is in-process. Wire-format design without a forcing function is over-design.

## Decision

- **No subprocess bridge in 0.6.0.**
- **Revisit in 0.8.0** (per HITL adjustment 2026-04-27). 0.7.0 is not the revisit target; the bridge question is far enough out that a release-skip is appropriate.
- If a forcing function emerges before 0.8.0 (e.g. a non-PyO3 Python flavor, a process-isolation requirement for embedders), this ADR is re-opened.

## Options considered

**A — JSON over stdio; line-delimited; versioned envelope `{ "v": 1, ... }`.** Simplest if a bridge is needed; matches operator-config JSON-only posture; no proto deps. Default if revisit picks "ship a bridge."

**B — MessagePack / CBOR.** Tighter binary; faster; adds dep + tooling complexity; harder to debug. Premature.

**C — Defer entirely (chosen).** No 0.6.0 forcing function. Avoids Phase 3 design + interface work for a speculative feature. Revisit 0.8.0.

## Consequences

- `bindings.md` and `interfaces/wire.md` cover only the in-process binding surfaces (Python via PyO3, TS via napi-rs, Rust direct).
- No `interfaces/wire.md` content for IPC. The doc may still exist (per Phase 3 plan) to record "no wire protocol in 0.6.0."
- 0.8.0 milestone should re-evaluate: is there a real consumer? If yes, default to Option A unless the consumer's needs argue otherwise.
- Tracked: `followups.md` FU-WIRE15 (subprocess bridge design — 0.8.0 revisit).

## Citations

- HITL 2026-04-27 (adjustment from "if-needed-default-A" to explicit 0.8.0 revisit).
- Plan.md non-goal: no speculative knobs.
- ADR-0.6.0-no-shims-policy (no within-0.6.x feature bake-out).
