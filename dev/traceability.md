---
title: 0.6.0 Traceability Matrix
date: 2026-05-02
target_release: 0.6.0
desc: Needs to requirements to design/interfaces to test-plan traceability for the 0.6.0 rewrite
blast_radius: dev/needs.md; dev/requirements.md; dev/acceptance.md; dev/design/*; dev/interfaces/*; dev/test-plan.md
status: draft
---

# Traceability Matrix

This file proves end-to-end traceability for the 0.6.0 rewrite:

`needs.md` -> `requirements.md` -> `design/*` and `interfaces/*` -> `test-plan.md`.

Rules:

- Every `NEED-*` maps to at least one `REQ-*`.
- Every `REQ-*` maps to at least one `AC-*` and therefore at least one `T-*`
  (per `dev/test-plan.md` ID convention).
- Every planned test exists to satisfy one or more `REQ-*` (and therefore one
  or more `NEED-*`).

This matrix is intentionally coarse-grained: it ties needs to the owning
requirements and the owning design/interface docs, and points to test-plan suite
coverage for execution identity.

## Matrix

| NEED      | Need summary                          | Key REQs                                                               | Design owners                                                                | Interface owners                                          | Test-plan coverage                                 |
| --------- | ------------------------------------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------- | --------------------------------------------------------- | -------------------------------------------------- |
| NEED-001  | trustworthy canonical storage         | REQ-021, REQ-022, REQ-023, REQ-031d, REQ-042                           | design/engine.md, design/migrations.md, design/errors.md, design/recovery.md | interfaces/{python,typescript,rust}.md, interfaces/cli.md | AC-021..AC-025, AC-034a..AC-035c, AC-046a..AC-050c |
| NEED-002  | local-first privacy/offline           | REQ-033, REQ-054, REQ-037                                              | design/embedder.md, design/recovery.md                                       | interfaces/{python,typescript,rust}.md, interfaces/cli.md | AC-035d..AC-045, AC-041, AC-058                    |
| NEED-003  | low-ceremony embedded deploy          | REQ-053, REQ-024, REQ-021                                              | design/bindings.md, design/engine.md                                         | interfaces/{python,typescript,rust,cli}.md                | AC-057a..AC-060b, AC-023a/b                        |
| NEED-004  | backup/export + recovery              | REQ-012, REQ-035, REQ-036, REQ-037, REQ-038, REQ-039, REQ-040, REQ-054 | design/recovery.md, design/errors.md                                         | interfaces/cli.md                                         | AC-026..AC-028c, AC-039..AC-045, AC-058..AC-063c   |
| NEED-004a | backup minimizes service interruption | REQ-012, REQ-024                                                       | design/recovery.md, design/engine.md                                         | interfaces/cli.md                                         | AC-035d..AC-045                                    |
| NEED-005  | permissive licensing posture          | REQ-047, REQ-048, REQ-049, REQ-050, REQ-052                            | design/release.md                                                            | (n/a)                                                     | AC-051a..AC-056                                    |
| NEED-006  | database-quality releases             | REQ-047, REQ-048, REQ-049, REQ-050, REQ-052                            | design/release.md                                                            | (n/a)                                                     | AC-051a..AC-056                                    |
| NEED-010  | small learnable SDK                   | REQ-053, REQ-056                                                       | design/bindings.md, design/errors.md                                         | interfaces/{python,typescript,rust}.md                    | AC-057a..AC-060b                                   |
| NEED-011  | SDK vs operator boundary              | REQ-037, REQ-054                                                       | design/recovery.md, design/bindings.md                                       | interfaces/cli.md, interfaces/{python,typescript,rust}.md | AC-041, AC-054, AC-057a, AC-058                    |
| NEED-011a | dedicated operator surface            | REQ-035, REQ-036, REQ-037, REQ-038, REQ-039, REQ-040, REQ-054          | design/recovery.md                                                           | interfaces/cli.md                                         | AC-026..AC-028c, AC-039..AC-045, AC-058..AC-063c   |
| NEED-012  | hybrid retrieval                      | REQ-010, REQ-011, REQ-017, REQ-018, REQ-029, REQ-034                   | design/retrieval.md, design/vector.md, design/projections.md                 | interfaces/{python,typescript,rust}.md                    | AC-011a..AC-020                                    |
| NEED-013  | graph-oriented model                  | REQ-053 (typed writes), REQ-057..REQ-059                               | design/engine.md, design/op-store.md                                         | interfaces/{python,typescript,rust}.md                    | AC-061a..AC-063c                                   |
| NEED-014  | explicit catch-up barrier             | REQ-030, REQ-016, REQ-008                                              | design/scheduler.md, design/projections.md, design/bindings.md               | interfaces/{python,typescript,rust}.md                    | AC-029..AC-033                                     |
| NEED-015  | migrations-on-open reporting          | REQ-042, REQ-046a/b                                                    | design/migrations.md, design/engine.md, design/errors.md                     | interfaces/{python,typescript,rust}.md                    | AC-046a..AC-050c                                   |
| NEED-016  | cross-platform support                | REQ-048, REQ-052                                                       | design/release.md                                                            | (n/a)                                                     | AC-051a..AC-056                                    |
| NEED-017  | no implicit model download            | REQ-033                                                                | design/embedder.md                                                           | interfaces/{python,typescript,rust}.md                    | AC-035d..AC-045                                    |
| NEED-020  | bounded shutdown/no deadlocks         | REQ-021, REQ-023, REQ-024, REQ-030                                     | design/engine.md, design/lifecycle.md, design/scheduler.md                   | interfaces/{python,typescript,rust}.md                    | AC-021..AC-025, AC-029..AC-033                     |
| NEED-020a | concurrent queries stay low-latency   | REQ-010, REQ-011, REQ-018, REQ-022a                                    | design/retrieval.md, design/engine.md                                        | interfaces/{python,typescript,rust}.md                    | AC-011a..AC-020, AC-021..AC-025                    |
| NEED-021  | corruption detection/diagnostics      | REQ-031d, REQ-038, REQ-039, REQ-040                                    | design/recovery.md, design/errors.md, design/engine.md                       | interfaces/cli.md, interfaces/{python,typescript,rust}.md | AC-035d..AC-045, AC-043a/b/c                       |
| NEED-022  | interactive performance               | REQ-009a/b, REQ-010, REQ-011                                           | design/retrieval.md, design/vector.md                                        | interfaces/{python,typescript,rust}.md                    | AC-011a..AC-020                                    |
| NEED-023  | usable without DB expertise           | REQ-001..REQ-008, REQ-042, REQ-056                                     | design/lifecycle.md, design/errors.md, design/engine.md                      | interfaces/{python,typescript,rust,cli}.md                | AC-001..AC-010, AC-057a..AC-060b                   |
| NEED-024  | operational transparency              | REQ-001..REQ-008                                                       | design/lifecycle.md                                                          | interfaces/{python,typescript,rust}.md                    | AC-001..AC-010                                     |
| NEED-025  | minimal migrations/upgrade complexity | REQ-046a/b, REQ-047..REQ-050                                           | design/migrations.md, design/release.md                                      | (n/a)                                                     | AC-046a..AC-056                                    |

## Gaps (Must Resolve Before Lock)

This matrix is a first draft. Before `needs.md` and this file can flip to
`locked`, verify the following:

- Every `NEED-*` row maps to concrete `REQ-*` ids that exist in
  `dev/requirements.md`.
- Every referenced design/interface file exists and remains the owner for the
  cited behavior.
- For each `REQ-*`, confirm there is at least one REQ->AC mapping in
  `dev/acceptance.md` and therefore test coverage in `dev/test-plan.md`.

If a `NEED-*` lacks any REQ linkage, either:

1. Add or amend a requirement (and acceptance/test coverage), or
2. Downgrade the need from a 0.6.0 must-have, or
3. Reword the need so it accurately reflects what 0.6.0 commits to ship.
