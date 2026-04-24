---
title: 0.6.0 Followups
date: 2026-04-24
target_release: 0.6.0
desc: Items deferred beyond 0.6.0; write-only during 0.6.0 doc phase
blast_radius: TBD
status: living
---

# Followups

**Read-discipline:** this file is **write-mostly** during 0.6.0. Working agents
append items but MUST NOT read this file unless explicitly told. Keeps working
context clean.

Item format:

```
## FU-NNN: <title>

**Origin:** <who/when/why>
**Target release:** 0.6.1 | 0.7.0 | TBD
**Notes:**
```

Seeded:
- **Upgrade path for 0.5.x users** — deferred from 0.6.0. Design in later release.
