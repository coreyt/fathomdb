---
name: recovery-denylist-five-names
description: "0.8.0 spec decision (HITL, 2026-06-02): the SDK recovery-name denylist is FIVE names {recover,restore,repair,fix,rebuild}; `doctor` is SDK-absent via the positive verb allowlist, NOT this denylist. Enforcement (tests/ACs) is canonical at five; prose was corrected down to match."
metadata: 
  node_type: memory
  type: project
  originSessionId: 40ea6169-1c1c-40ea-8159-b9ea4d47e0a5
---

On 2026-06-02 HITL settled a 5-vs-6 split that ran through the 0.8.0 spec corpus.

**The split (pre-existing):** every *executable/measurable* enforcement point asserts
**five** recovery names — `src/python/tests/test_no_recovery_surface.py` `FORBIDDEN`,
the `.ts`/`.rs` mirrors, and `acceptance.md` AC-035d measurement all use
`{recover, restore, repair, fix, rebuild}`. But the *prose* (bindings.md §1/§10,
the supersede ADR's element-3 table, interface-inventory, the v05-feature-triage doc)
claimed **six**, adding `doctor`.

**DECISION — "five everywhere":** `doctor` is a CLI **diagnostic** namespace
(`fathomdb doctor <verb>`), not a recovery-mutation verb. It is kept off the SDK by
the **positive verb allowlist** (it is never added to the SDK surface), **NOT** by the
recovery-name denylist. So the denylist is canonically **five** names; the five-name
enforcement artifacts were always right and stay **byte-unchanged** (the supersede
ADR mandates a zero-line git diff on them through Slice 25). The prose was corrected
*down* to five, each spot carrying a note that doctor's SDK-absence rests on the
allowlist. Do NOT add `doctor` to the denylist tests/ACs — that would contradict this.

**Why it matters for [[fathomdb-080-plan-approved]]:** Slice 25 rewrites the SDK
conformance tests from the supersede-ADR sign-off text. The denylist clause it
generates MUST be the five-name set; an SDK verb literally named `doctor` is barred
by allowlist-membership, not by the denylist.

**Provenance:** surfaced by the real codex reviewer (see [[orchestration-execution-traps]]
trap #5) across three review rounds on the uncommitted 0.8.0 doc fixes — it caught the
narrowing, then the test-vs-prose contradiction, including a no-spaces
`{recover,restore,repair,fix,rebuild,doctor}` instance a spaced grep missed. Six prose
spots fixed: `bindings.md:46`, supersede ADR (element-3 table, §"preserves" verdict,
Q4, Slice-15 guarantee #2), `interface-inventory/option2.../interfaces.md` (×2),
`dev/design/0.8.0-v05-feature-triage.md` (×2).
