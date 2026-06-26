---
title: Interface Inventory — Option 2 Summary
date: 2026-05-01
target_release: 0.6.0
desc: Public-surface-first inventory: overview, outward-facing risks, and what the method reveals
status: living
---

# Summary

## Overall public interface model

fathomdb 0.6.0 exposes four distinct outward-facing surfaces plus a shared
observability route. The application runtime exposes a single canonical
five-verb SDK surface (`Engine.open`, `admin.configure`, `write`, `search`,
`close`) reproduced in idiomatic form across Python and TypeScript bindings
(`design/bindings.md` § 1; REQ-053 / AC-057a). The Rust public API is the
ground-truth source for these symbols but has no concrete document content yet
(`interfaces/rust.md` is a `not-started` stub). Operator workflows are routed
exclusively through a separate CLI binary with a two-root mutation-class split
(`fathomdb doctor <verb>` bit-preserving; `fathomdb recover --accept-data-loss
<sub-flag>...` lossy) per `design/recovery.md` and `interfaces/cli.md`.
Cross-cutting observability flows through a host-supplied subscriber whose
typed event payload is wire-stable across bindings (`design/lifecycle.md`;
`design/bindings.md` § 8); machine-readable output additionally appears as
JSON on every CLI verb and as the structured `CorruptionDetail` /
counter-snapshot / profile-record / stress-failure-context payloads owned by
`design/lifecycle.md` and `design/errors.md`.

## Main outward-facing consistency risks

Three of the five `interfaces/*.md` files (`rust.md`, `python.md`,
`typescript.md`) are still `not-started` stubs and `interfaces/wire.md` is
also TBD. As a result the per-language symbol names, exception class names,
attribute spellings, and config-object shapes that `design/bindings.md`
and `design/errors.md` defer to those interface docs do not yet exist in
the corpus. The cross-SDK parity claim, the typed-attribute error contract,
and the engine-config knob symmetry rule are all expressed as protocols whose
realized form lives in files that have no content. CLI flag spelling is
locked in `interfaces/cli.md`, but doctor verbs other than `check-integrity`
are described as "shape remains owned here as the draft fills in"
(`design/recovery.md` § Machine-readable output). Migration per-step event
payload, opt-in profile-record transport, host-subscriber registration call,
and the bounded-completion ("drain") verb name are all named-but-unsigned: the
shape is committed, the symbol is not. The `EngineConfig` knob set is
"includes runtime controls such as `embedder_pool_size` and
`scheduler_runtime_threads`" (`design/engine.md`) without a complete
enumeration, leaving the symmetry contract under-specified.

## What the public-surface-first method reveals quickly

Sweeping the `interfaces/*.md` files first, before the design docs, makes the
stub-vs-locked asymmetry unmistakable: the only public contract in the
inventory that has concrete spelling is the CLI surface (locked verbs,
locked flags, locked exit-code classes). Every per-language SDK surface is
specified only by reference (parity claim, error-mapping protocol, knob
symmetry rule) and the actual symbols live in not-started files. This
contrasts sharply with the depth of `design/bindings.md`, `design/lifecycle.md`,
`design/recovery.md`, and `design/errors.md`, which already enumerate enums,
field sets, and machine-readable contracts in detail. The method also
surfaces ownership ambiguity that a subsystem-first walk would obscure: the
host-subscriber transport, migration progress payload, lifecycle phase, and
op-store payload validation all converge on the same caller-visible entry
point (`Engine.open` and the subscriber callback) but are owned by four
different design docs (`bindings`, `lifecycle`, `migrations`, `op-store`)
plus their interface counterparts.
