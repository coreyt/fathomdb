# ASH-B fix-2 Review — codex gpt-5.4 (§ 9 gate), round 3 — TERMINAL

- **Slice:** ASH-B fix-2 · **Branch:** `agent-seat-hardening` · **HEAD reviewed:** `21fd0d74`
  (fix-2 diff `06b7418f..21fd0d74`).
- **Reviewer:** `codex exec --model gpt-5.4 -c model_reasoning_effort=high --sandbox read-only`
  via `dev/agent-tools/codex-nostdin.sh`.
- **Fix-N rounds consumed:** **2 of 6** total. Same-finding count on the comment-exactness class:
  **3rd consecutive appearance** (round 1 finding 2 → round 2 finding 2 → round 3 finding 1).
- **Terminal transcript (TC-RUBRIC-7):**
  `dev/plans/runs/codex/agent-seat-hardening/ASH-B-fix-2-20260727T185914Z.log`

## Verdict: CONCERN — **ACCEPTED BY ORCHESTRATOR OVERRIDE**

### 1. [low] Redirection table still misstates which arm covers `2>|` allow behaviour

Refs: `.claude/hooks/seat-path-guard.sh:356`, `:358`;
`scripts/tests/test_seat_path_guard.sh:642`, `:645`

The header table says the `2>|` row has "allow half: arm 55", but arm 55 exercises plain
`>|dev/plans/note.md`, not `2>|`. The **runtime handling itself is correct**; the citation is wrong.

## What passed on inspection

- **E1 is CLOSED.** `skip_as_flag()` implements a coherent `--` / bare-`-` / option split, and the
  reviewer verified by inspection that *every* branch which previously relied on the
  "starts-with-`-` ⇒ skip" assumption now routes through it: `tee`, `sed|gsed|perl|ruby`,
  `cp|mv|install|rsync|ln`, `rm|shred|unlink|touch|truncate|mkfifo`, and `git`.
- **THE DANGEROUS DIRECTION IS CLEAN.** No surviving silent bypass found in any of the four
  newly-fixed areas: `sed`/`perl` pending-program handling, `git -C` / `-c` global-option handling,
  `consider()` on dash-led paths, and `-t` / `--target-directory` destinations. The new arms from 57
  onward assert the right direction in each case.
- **No regression.** Deny on `Edit`/`Write`/`Bash` into protected paths; read-only *mentions*
  allowed; test-source precedence over `scripts/**`; fail-open posture; silence-on-allow;
  `implementer` never blocked; unknown / non-guarded seats never blocked.

## Reviewer process notes

The reviewer confirms it did not open any `*-review-*.md` (the guard added after round 2's void
attempt). `git diff` was unreliable in the codex sandbox, so the fix-2 file-scope check was only
partially verifiable there — the **orchestrator verified it independently** from `git log` in the
real checkout: no commit anywhere on this branch touches any off-limits file.

## Orchestrator override — 2026-07-27: CONCERN accepted

**Orchestrator override 2026-07-27: CONCERN accepted.** Rationale, stated so the Steward can reverse
it:

1. **Severity is [low] and the runtime is affirmatively correct.** The reviewer verified in the same
   breath that `2>|` is handled properly at runtime. The defect is a wrong cross-reference inside a
   comment, not a behaviour.
2. **This is the THIRD consecutive appearance of one finding class** — "a comment claims test
   coverage that the cited arm does not deliver." It was raised at round 1, patched; raised again at
   round 2, swept header-wide; raised again now. § 6's own stated rationale for the same-finding
   bound is that *"three attempts at one defect means the **design** is wrong, not the patch."* That
   applies literally here: **a comment table that hand-cites arm numbers living in a different file
   is structurally guaranteed to drift**, and no third hand-patch changes that. Under § 7 that makes
   the finding *structural*, which is the category an orchestrator may accept.
3. **Nothing live depends on it.** The hook ships UNWIRED; the comment governs no behaviour today.
4. **Authorising a third same-finding round is the exact action the circuit-breaker exists to
   prevent.** Spending it on a comment citation would be the breaker firing on the least valuable
   possible target.

**I flag the self-serving shape of this call in the open:** an orchestrator that overrides a finding
*partly because* fixing it would trip a breaker is reasoning close to its own convenience. I judged
[low] + verified-correct-runtime + unwired + structural-recurrence to outweigh that, but the Steward
should treat this as a **reversible judgement call**, not a settled one.

**Carried up as a durable follow-up rather than fixed here:** replace the hand-maintained arm-number
citations in `seat-path-guard.sh`'s redirection table with something mechanically checkable — either
a suite arm that parses the table and asserts every cited arm exists and exercises the form claimed,
or drop the arm numbers from the comment entirely and let the suite be the record. That closes the
class instead of its third instance. This is a Phase-2 / follow-up item, not part of ASH-B.

**ASH-B is CLOSED** at `21fd0d74` on the strength of: E1 closed, no silent bypass, no regression, and
all gates independently re-run by the orchestrator (113 PASS / 0 FAIL; every gate `rc=0`).
