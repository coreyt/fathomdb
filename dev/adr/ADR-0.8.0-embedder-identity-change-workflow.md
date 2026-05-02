---
title: ADR-0.8.0-embedder-identity-change-workflow
date: 2026-04-30
target_release: 0.8.0
desc: Deferred workflow for intentional embedder identity swaps
status: draft
origin: deferred from 0.6.0 OD-15
---

# ADR-0.8.0 — Intentional embedder identity-change workflow

This file is the new home for the design that was removed from 0.6.0 under
OD-15.

## 0.6.0 baseline

0.6.0 is fail-closed on embedder identity mismatch:

- `Engine.open` compares the supplied embedder identity against the recorded
  profile identity.
- If they differ, open fails with `EmbedderIdentityMismatch`.
- 0.6.0 ships no `accept_identity_change` flag or equivalent bypass.

## 0.8.0 design questions

If FathomDB adds an intentional identity-swap workflow in 0.8.0, this ADR must
settle at least:

1. how an operator declares intent to swap embedders
2. whether the workflow is CLI-only, SDK-visible, or both
3. whether open is still fail-closed until a separate rebuild / regenerate step
   completes
4. when the recorded profile identity is updated
5. what guarantees exist against partial vector/profile drift during the swap

## Promotion threshold

This should not be pulled back into a release until there is a concrete
operator workflow that cannot be handled by the fail-closed 0.6.0 posture.
