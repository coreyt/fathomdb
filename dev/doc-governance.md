# Document Governance

**Status:** Current
**Last updated:** 2026-04-22

## Purpose

Define which docs in `dev/` are normative, which are active trackers, and which
are historical design notes.

## Document Classes

### Normative docs

These must match the shipped behavior and current support contract:

- `README.md`
- `docs/`
- `dev/ARCHITECTURE.md`
- `dev/fathomdb-v1-path-to-production-checklist.md`
- `dev/production-acceptance-bar.md`
- `dev/release-policy.md`
- `dev/repair-support-contract.md`
- `dev/security-review.md`
- `dev/test-plan.md`
- `dev/dbim-playbook.md`
- `dev/engine-vs-application-boundary.md`

### Active trackers

These are execution trackers, not historical notes. When the tracked work is
complete, they must be checked off or explicitly retired:

- None currently.

### Historical design notes

These may describe older plans or implementation slices. They are valuable
reference material, but they do not override normative docs once implementation
has moved on. Historical or superseded material belongs under `dev/archive/`.

Completed implementation plans, superseded designs, stale investigations,
baseline audits, old release notes, and historical evidence should be archived
rather than patched in place unless they are still part of the current support
contract.

## Completion Rule

A feature or fix is not complete until:

1. behavior is implemented and tested
2. the relevant normative docs are updated
3. the relevant active tracker is updated or retired

## Automated Enforcement

`scripts/check-doc-hygiene.py` is the minimum enforcement layer.

It currently checks:

- the production-readiness checklist summary sections agree with the readiness
  matrix

This check must run in CI.
