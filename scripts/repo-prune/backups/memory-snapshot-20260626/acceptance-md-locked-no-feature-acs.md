---
name: acceptance-md-locked-no-feature-acs
description: "dev/acceptance.md is status:locked (titled 0.6.0, max AC-073); 0.8.0 implementation slices have NO per-feature ACs — track by G-gap label + TDD test names. Don't fill the SLICE-TEMPLATE {{AC_IDS}} with invented AC ids; new ACs are authored only at gated governance slices (25, 40)."
metadata: 
  node_type: memory
  type: project
  originSessionId: bb10ac3a-25b1-434f-89fc-6db990f1fc79
---

`dev/acceptance.md` is the **locked** canonical acceptance file (`status: locked`,
front-matter still titled "0.6.0", highest id **AC-073**). It has accreted some post-0.6.0
ACs (AC-057a five-verb cap, AC-068a/b FFI validation), but **no per-feature 0.8.0 ACs exist**
(no G0/G1/G8/G9/G10/G12 AC).

**0.8.0 implementation slices are tracked by G-gap label (G8/F10, G9, …) + their `pr_g*` TDD
tests, NOT by AC ids.** Evidence: closed Slices 5/10/15 `output.json` cite only pre-existing
**infra** ACs (AC-037/AC-038/AC-050a/AC-050c from the agent-verify gate, AC-068a/b for FFI) —
never a newly-authored feature AC.

**Why / how to apply:** The `dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md` has an `{{AC_IDS}}`
fill-in. Do **NOT** mechanically fill it with "pin the G<n> acceptance ids from
dev/acceptance.md" for an implementation slice — those ids don't exist and the slice contracts
don't ask for them (Slice 20's header `gaps: 21–24` are reserved-gap *bands*, not ACs; it
"produces no new gate"). Instead bind the **G-gap + the test names**, and record
`ac_ids: "none — tracked by gap label"`. Authoring/superseding ACs in the locked file is a
**gated governance act** owned by **Slice 25** (the governed-surface AC superseding AC-057a)
and the **Slice 40** verification reconciliation — never an additive implementation slice.
This bit the Slice 20 prompt (the agent correctly flagged "no G8 AC ids exist; acceptance.md
is locked at AC-073" rather than inventing one); fixed in prompt commit `1a7b75f`. Watch for
the same trap when authoring the Slice 30 read-verb prompt. See [[fathomdb-080-plan-approved]].
