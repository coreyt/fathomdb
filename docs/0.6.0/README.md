---
title: 0.6.0 Design Docs — Index
date: 2026-04-24
target_release: 0.6.0
desc: Entry point + status table for all 0.6.0 design docs
blast_radius: TBD
status: draft
---

# 0.6.0 Design Docs

Single source of truth for the 0.6.0 rewrite. Code PRs gated on `0.6.0-design-frozen` tag.

Start here:
- [plan.md](plan.md) — phased plan for reaching design freeze
- [adr/](adr/) — architectural decisions
- [learnings.md](learnings.md) — keep-doing / stop-doing from prior releases

## Doc status

| Doc | Status | Notes |
|-----|--------|-------|
| plan.md | living | this plan |
| requirements.md | not-started | |
| acceptance.md | not-started | |
| architecture.md | not-started | |
| test-plan.md | not-started | |
| security-review.md | not-started | |
| learnings.md | not-started | living during 0.6.0 |
| followups.md | not-started | write-mostly; do not read unless told |
| deps/ | not-started | one file per dep; living folder |
| design/bindings.md | not-started | written first (may fill distinct role vs. interfaces/) |
| design/* (rest) | pending | architect agent proposes list post-architecture.md |
| interfaces/rust.md | not-started | |
| interfaces/python.md | not-started | |
| interfaces/typescript.md | not-started | |
| interfaces/cli.md | not-started | |
| interfaces/wire.md | not-started | short OK |
| adr/ADR-0.6.0-decision-index.md | not-started | triage list, not a decision |

Lifecycle states: `not-started → draft → review → locked` (most docs);
`not-started → draft → review → proposed → critic-reviewed → accepted | rejected | superseded` (ADRs);
`living` (plan, learnings, followups, deps folder).
