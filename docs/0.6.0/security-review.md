---
title: 0.6.0 Security Review
date: 2026-04-24
target_release: 0.6.0
desc: Output of `security-review` skill against locked design set
blast_radius: TBD
status: not-started
---

# Security Review

TBD — run in Phase 3 after requirements + architecture + design + interfaces locked.

Finding format:

```
## SR-NNN: <short title>

**Severity:** critical | high | medium | low
**Affected doc/component:** <path>
**Description:** <what>
**Proposed resolution:** <how>
**Status:** open | resolved
```

Lock bar: zero open findings at severity ≥ medium (HITL may adjust).
Low severity may carry to `followups.md` w/ explicit call-out.
