---
title: ADR-0.7.1-default-embedder-weight-fetch
date: 2026-05-28
target_release: 0.7.1
desc: Narrow exception to NEED-017 / REQ-033 — default embedder MAY fetch its pinned weights on first use when caller opts in
blast_radius: dev/needs.md (NEED-017 cross-cite); dev/requirements.md (REQ-033 cross-cite); dev/adr/ADR-0.6.0-default-embedder.md (cross-cite); future fathomdb-embedder loader code (gated EU-3 onward)
status: accepted
---

# ADR-0.7.1 — Default-embedder weight-fetch exception

**Status:** accepted (HITL 2026-05-28)

## Context

NEED-017 (`dev/needs.md:132`) and REQ-033 (`dev/requirements.md:270-274`)
both forbid the engine from downloading model weights or requiring
external services on `Engine.open`. They were written when "caller owns
the embedder" was the only supported posture — a deliberate stance against
surprising the user with implicit network traffic.

The 0.7.1 EMBEDDER-UNDEFER campaign (handoff:
`dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md`) introduces an
opt-in default embedder. EU-0
(`dev/notes/0.7.1-default-embedder-research.md`) chose
`BAAI/bge-small-en-v1.5` at K=128 with mean-centering. Shipping that model
without first-use weight fetch would require either (a) bundling ~133 MB
of weights into every wheel even for users who do not opt in, or (b)
forcing the caller to supply the weights out-of-band — both of which
nullify the "default embedder = on" happy path that ADR-0.6.0-default-embedder
established for 0.6.0+.

The original NEED-017 / REQ-033 rule must therefore admit a tightly-scoped
exception, narrow enough that it cannot be read as license to fetch
arbitrary models or to silently mutate network behaviour for the
caller-owned posture.

## Decision

NEED-017 and REQ-033 are amended with the following exception clause
(carried verbatim into both `needs.md` and `requirements.md` cross-cites):

> **Exception (default embedder only).** When the caller opts into the
> default embedder by passing `use_default_embedder=true` (or by selecting
> it explicitly at construction), the engine MAY download the single
> declared default-embedder weight set from a fixed URL set on first use,
> cache it under the platform user-cache directory, verify by sha256, and
> load it. This exception is scoped to: (a) the single named
> default-embedder identity recorded in the workspace; (b) weight files
> referenced by sha256 in the published `fathomdb-embedder` crate; (c) no
> other model, no arbitrary URL, no user-controllable model name.
> Caller-supplied embedders remain caller-owned per the unchanged rule.
> The first-use download path SHALL emit a structured
> `default_embedder_download` event in `OpenReport.embedder_events`
> describing url + bytes + sha256 + cache-path, so the wire/disk activity
> is visible.

The exception is **opt-in by binding-surface flag** (Python
`use_default_embedder=True`, TypeScript `useDefaultEmbedder: true`); the
default for both bindings is `false`, preserving the
`fathomdb-noop` / caller-owned posture for any caller that does not
explicitly request it. Switching the binding-surface default away from
`false` is itself a separate decision (not made here).

## Options considered

**A. Ship weights in the wheel (no network on open).** Pros: zero
exception to NEED-017. Cons: wheel size +130 MB even for non-opters; no
way to update weights without a wheel respin; complicates per-platform
wheel-size CI gate (EMB-3).

**B. Force the caller to supply weights out-of-band.** Pros: zero
exception. Cons: nullifies the "default embedder = on" happy path; every
caller of `use_default_embedder=true` must also know about and manage
weight file paths; breaks the "single switch" ergonomics that motivated
EU-0 in the first place.

**C. First-use fetch behind an opt-in flag, sha256-verified, visible in
OpenReport (chosen).** Pros: zero cost for non-opters; download is a
one-time event per cache; tamper-resistant (pinned sha); visible to the
caller via `embedder_events`. Cons: introduces a narrow network surface
that must be defended against scope creep (this ADR is the defence).

**D. Bundle a tiny "stub" weight in the wheel and fetch the full set on
opt-in.** Considered but rejected: adds shipping complexity without
removing the network surface for opt-in callers.

## Scope guardrails

The exception is deliberately narrow. The following are **out of scope**
and remain forbidden by NEED-017 / REQ-033 unchanged:

1. **Arbitrary model fetches.** The caller cannot pass a model name, HF
   repo, or URL and have the engine fetch it. There is exactly one
   default-embedder identity recorded in `fathomdb-embedder` as a Rust
   constant, with file-level sha256 pins. Any change to that identity is
   a new release of `fathomdb-embedder` and a new pinned constant set.
2. **Implicit fetches without opt-in.** `use_default_embedder=false` (the
   default) emits no network traffic on `Engine.open`. The fail mode for
   vector writes without an embedder configured is unchanged
   (`EmbedderNotConfigured`).
3. **Trust-on-first-use.** A file whose sha256 does not match the pinned
   constant is removed and the load fails closed
   (`EmbedderLoadError::ChecksumMismatch`). No "first download wins."
4. **Mirroring / proxying user-supplied URLs.** The pinned URL set is a
   compile-time constant. `HF_TOKEN` is honoured if set in the
   environment (for token-gated mirrors), but the URL itself is not
   user-controllable.

## Consequences

- **`dev/needs.md` NEED-017** gains a cross-cite to this ADR and the
  exception-clause language. The base prohibition stands; the exception
  is the narrow carve-out.
- **`dev/requirements.md` REQ-033** likewise gains the cross-cite and the
  exception-clause language.
- **`dev/adr/ADR-0.6.0-default-embedder.md`** gains a forward cross-cite
  to this ADR in its "0.7.1 implementation choice" section (already
  added by EU-0 closeout — this ADR is the named target).
- **EU-2 loader sub-design** (`dev/design/embedder.md`) is the
  implementation-level deliverable that operationalises this exception:
  cache layout, atomic rename, sha verification, concurrent-load file
  lock, failure taxonomy, and the `embedder_events` event shape.
- **EU-3 loader implementation** lands the code; the
  `default-embedder` Cargo feature in `fathomdb-embedder` gates the new
  network surface so callers compiling without the feature get a
  hard compile error rather than a silent network capability.
- **Bindings (EU-6)** expose only the opt-in boolean, not URL or model
  name. The exception's "no arbitrary URL" guarantee is structurally
  enforced by the absence of a parameter for it.

## Audit posture

Any future grep of "implicit download" or "background fetch" in this
codebase that does not reference this ADR should be treated as a bug. The
exception is the only legitimate path; anything else is scope creep.

## Citations

- HITL 2026-05-28: ADR accepted on the basis of the EU-0 outcome (model
  identity resolved, opt-in shape locked).
- `dev/notes/0.7.1-default-embedder-research.md` §5.3 — chosen
  configuration.
- `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-1 — original
  exception-clause wording (carried verbatim above).
- `dev/adr/ADR-0.6.0-default-embedder.md` — the architecture this ADR
  unblocks.
