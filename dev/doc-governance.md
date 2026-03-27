# Document Governance

## Purpose

Define which docs in `dev/` are normative, which are active trackers, and which
are historical design notes.

## Document Classes

### Normative docs

These must match the shipped behavior and current support contract:

- `README.md`
- `dev/ARCHITECTURE.md`
- `dev/0.1_IMPLEMENTATION_PLAN.md`
- `dev/production-readiness-checklist.md`
- `dev/production-acceptance-bar.md`
- `dev/release-policy.md`
- `dev/repair-support-contract.md`

### Active trackers

These are execution trackers, not historical notes. When the tracked work is
complete, they must be checked off or explicitly retired:

- `dev/TODO-response-cycle-feedback.md`

### Historical design notes

These may describe older plans or implementation slices. They are valuable
reference material, but they do not override normative docs once implementation
has moved on.

## Completion Rule

A feature or fix is not complete until:

1. behavior is implemented and tested
2. the relevant normative docs are updated
3. the relevant active tracker is updated or retired

## Automated Enforcement

`scripts/check-doc-hygiene.py` is the minimum enforcement layer.

It currently checks:

- active tracker docs do not remain fully unchecked after completion
- the production-readiness checklist summary sections agree with the readiness
  matrix

This check must run in CI.
