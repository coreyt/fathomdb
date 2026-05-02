---
title: Scheduler Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Projection job dispatch, backpressure, retry behavior, and shutdown ordering
blast_radius: projection scheduler; REQ-015, REQ-016, REQ-027, REQ-029, REQ-030, REQ-055
status: locked
---

# Scheduler Design

This file owns projection job spawn policy, queue/backpressure behavior, retry
policy, and the ordered shutdown path that cooperates with the writer thread.

## Fixed retry policy

0.6.0 uses one bounded retry policy for projection jobs:

- 3 retries maximum
- backoff schedule `1s`, `4s`, `16s`

These values are engine constants in 0.6.0, not `Engine.open` knobs. Operator
workflow is inspection plus the explicit regenerate path
(`recover --rebuild-projections`), not per-deployment retry tuning.
