---
status: UNREVIEWED
---

# `SendMessage` unavailable to a spawned orchestrator — cause analysis

**Status:** diagnostic only. READ-ONLY investigation; nothing was changed in either repo.
**Date:** 2026-07-27
**Scope:** `/home/coreyt/projects/fathomdb` (cause) · `/home/coreyt/projects/local/unifi-openwrt` (exposure check)
**Verification basis:** repo files + the actual session transcripts + the published Claude Code
documentation at <https://code.claude.com/docs/en/sub-agents.md> (fetched 2026-07-27).

---

## 1. The symptom

Reported from a FathomDB Orchestrator session; traced to the **live 0.8.20 Slice 30**
orchestrator, 2026-07-27 15:00:48Z (see §2.1 for the transcript witness):

```text
● The verification is complete and decisive: zero failing clauses. But it surfaced three
  "permanently-red assertion" landmines the implementer must avoid. Sending those now.
  ⎿  Error: No such tool available: SendMessage. SendMessage exists but is not enabled in
     this context. Use one of the available tools instead.
```

## 2. The cause — mechanism (2), an explicit frontmatter allowlist

The failing agent was a **spawned `orchestrator` subagent**, not the main thread.

Witness — `.claude/agents/orchestrator.md:4`:

```yaml
tools: Read, Bash, Grep, Glob, Agent, Task
```

Per the published documentation, that line is a **strict allowlist**:

- Frontmatter table (`sub-agents.md`, `tools` row): *"Tools the subagent can use. **Inherits
  every tool available to subagents if omitted.**"*
- *"To restrict tools, use the `tools` field as an **allowlist** or the `disallowedTools`
  field as a denylist."*

`SendMessage` is not in that list, so the seat does not receive it. Because the field is
present at all, nothing is inherited — the omission **is** the denial.

### 2.1 The other two candidate mechanisms are refuted, not merely unlikely

| # | Candidate mechanism | Verdict | Evidence |
|---|---|---|---|
| 1 | The tool does not exist in the harness | **REFUTED** | Documented: *"Claude uses the `SendMessage` tool with the agent's ID or name as the `to` field to resume it."* It is also in the list of built-in tools a **background** subagent keeps, so it is unambiguously grantable to a subagent. It is absent from the documented list of tools never available to subagents (`AskUserQuestion`, `EnterPlanMode`, `ScheduleWakeup`, `TaskOutput`, `Workflow`, …). |
| 2 | The tool exists but is not in this agent type's allowlist | **CONFIRMED** | `.claude/agents/orchestrator.md:4` — see above. |
| 3 | A nesting/depth rule strips it from spawned subagents | **REFUTED for this session** | The same subagent successfully invoked `Agent` **7 times** in the same transcript. The documented depth rule withholds `Agent` — not `SendMessage` — and only *at* the limit. The session ran Claude Code **v2.1.220**, whose documented default is *"up to three layers below the main conversation"*; the orchestrator sat at depth 1. Depth was never the binding constraint. |

**Primary witness — the transcript that produced the quoted symptom.** It is the **live
0.8.20 Slice 30 orchestrator**, a sidechain agent of the current Steward session:

```text
~/.claude/projects/-home-coreyt-projects-fathomdb/
  403e8805-0190-41eb-96de-0cfeed758d57/subagents/agent-a80a957fa16300c48.jsonl

  agentId a80a957fa16300c48 · isSidechain: true · Claude Code v2.1.220
  first prompt: "You are the RELEASE ORCHESTRATOR for FathomDB 0.8.20 Slice 30 —
                 R-20-H7, the RUBRIC-H7 can-i-deploy contract-conformance gate."
  tool use:    Bash 76 · Agent 5 · Read 8 · SendMessage 1 (REFUSED, 15:00:48Z)
  target:      to = "abe0c3aec21376829"  (its own already-running implementer)
  summary:     "Three permanently-red assertion hazards to avoid"
```

`Agent` was used **5 times successfully in the same transcript** — the seat could spawn,
it could not message.

**Corroborating witness — an independent earlier instance, same seat, same mechanism:**

```text
  2e09d060-…/subagents/agent-aa277f08bfc50012b.jsonl
  "You are the orchestrator for 0.8.20 Slice 20c" · v2.1.220
  Bash 193 · Agent 7 · Read 6 · SendMessage 1 (REFUSED)
```

Every occurrence of this error anywhere in every project directory is in a
`*/subagents/agent-*.jsonl` file — **never** in a main-thread transcript. That is exactly
what the mechanism predicts: a `/orchestrate` main-thread session holds the full tool pool
(where `SendMessage` is a deferred tool reachable through `ToolSearch`), and only the
*spawned* seat is narrowed by the frontmatter allowlist. Further occurrences:
`agent-afa7ea102da8d398f` and `agent-a1ca8591c86eeda62` (v2.1.201) and
`agent-a1d7f85e114710bc4` (v2.1.214) — recurrent across ≥3 Claude Code versions, so this
predates any recent nesting-default change and is not a regression.

### 2.2 Ancillary observation on the same line

`Task` in `.claude/agents/orchestrator.md:4` is a legacy alias. Documented: *"In version
2.1.63, the Task tool was renamed to Agent. Existing `Task(...)` references in settings and
agent definitions still work as aliases."* Harmless, not load-bearing, listed only for
completeness.

## 3. The interesting finding — governing docs instruct a tool the seat forbids

Two of the repo's own governing documents direct a role to use `SendMessage` while that
role's seat does not grant it.

**a. The Steward.** Seat `.claude/agents/steward.md:4` is
`tools: Read, Bash, Grep, Glob, Agent, Task` — no `SendMessage`. Its governing spec says:

- `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md:161` — *"when you message it again
  (`SendMessage` by its agentId) it is **resumed from its saved transcript**"*
- `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md:200` — *"never build a file to pass notes
  between agents — `SendMessage` already does it."*

The whole §7 resident-subagent cost model (`:153`–`:200`) is built on a routing decision
(warm reuse vs. fresh spawn) whose cheap branch is only reachable via `SendMessage`.

*Mitigating nuance, and why this has not bitten yet:* the Steward normally runs as the
**main thread** via the `/steward` slash command, where the seat's `tools:` line does not
apply and the full pool is available. The contradiction is latent — it bites only if a
`steward` is ever *spawned* as a subagent, which `.claude/agents/steward.md` makes possible.

**b. The Library-Bump roles.** `dev/plans/prompts/LIBRARY-BUMP-STEWARD.md:21`, `:42`, `:95`
and `dev/plans/prompts/LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md:4`, `:28`, `:35`, `:58` make
`SendMessage` the *primary* LBS↔LBO reporting channel ("Escalate to LBS (`SendMessage`)
when …"). If an LBO is ever spawned onto the `orchestrator` seat, that channel does not
exist.

**Counter-example — the doc that got it right.** `dev/design/orchestration.md:364-368` is
required reading #2 for the orchestrator and states the correct behaviour explicitly:

> Fresh-spawn is the intended mechanism, **not** a fallback for missing conversational
> continuity: per § 12.1 the implementer's state lives on disk (worktree diff +
> output.json + verdict.md), so a fresh subagent reading the verdict has everything it
> needs. Do not wait on / reach for SendMessage.

So the Slice-20c orchestrator was contradicting its own playbook when it reached for the
tool. The harness refusal enforced the documented design. This is a **prompt-adherence
gap on top of a seat/doc mismatch**, not solely a configuration defect.

## 4. Practical consequence — what it costs

The two witnessed instances show **two different fallbacks with different severity**, and
the worse one is the live one.

### 4.1 Slice 30 (primary) — the findings were DROPPED, not re-routed

The target implementer `abe0c3aec21376829` was **already running**. A fresh spawn was not a
substitute, because the point was to steer work in flight. The orchestrator's own words
immediately after the refusal:

> *"`SendMessage` isn't available in this context, so I can't course-correct mid-flight.
> The safety net holds though: any permanently-red assertion makes the implementer's own
> arm-1 (real repo ⇒ exit 0) fail, and it is instructed to stop and report. I'll verify
> those three hazards explicitly at the review gate. Waiting for it to finish."*

It then fell back to a polling `until [ -f …/0.8.20-slice-30-output.json ]; do sleep 30;
done` wait. So the cost here was not a spawn fee — it was that a completed, decisive
verification result (*"zero failing clauses"*, plus three named permanently-red-assertion
hazards) **could not reach the agent that needed it**, and was deferred to post-hoc review.

Slice 30 subsequently went through **three codex §9 fix rounds**, each a fresh implementer
spawn (`aa5c5bc667f41adca` fix-1, `ac9147080f38a773b` fix-2, `ab9ccbfe1c8b10dde` fix-3).
Those hazards are the same class the orchestrator later recorded as `TC-81`. *Inference,
not witnessed:* whether a successful mid-flight correction would have avoided any of those
rounds cannot be established from the transcript.

**The general point: without `SendMessage` an orchestrator has no mechanism to steer a
running subagent at all.** Its only options are wait, or `TaskStop` and re-spawn. That is a
capability gap, not merely a price difference.

### 4.2 Slice 20c (corroborating) — re-routed as a fresh spawn, priced

Here the fallback worked cleanly. The orchestrator wrote:

> *"SendMessage is not available in this context — spawning a fresh implementer with a
> self-contained fix-1 brief."*

and issued `Agent(subagent_type: "implementer", run_in_background: true, …)` carrying the
same codex §9 round-1 `[P2]` text inline. Nothing was lost; the price was the spawn floor.
Per the measured figures at `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md:169-172`:

| Path | Cost | Notes |
|---|---|---|
| Warm reuse via `SendMessage` (unavailable) | **≈ $0.15** (small T) – $0.28 (large T) | The implementer already held the slice context. |
| Fresh spawn actually taken | **≈ $1.77** floor | Plus re-establishing worktree/slice context from the brief. |

≈ **6–12×** on that hop. The recurring exposure is the fix-N loop: the standing cap is 3
rounds on the same finding / 6 per slice, raised to 10 with a Steward check-in at 6 for
engine slices (`dev/design/orchestration.md:373-387`). Every round pays the spawn floor
instead of the reuse price — Slice 30's three fix rounds are a live example.

Correctness cost: none observed in either case — the on-disk state spine (§12.1) is exactly
what makes fresh-spawn viable for *sequential* hand-offs. The gap is specifically
**concurrent** steering, which no on-disk artefact substitutes for.

## 5. Shape of a fix (stated, not recommended — this is a HITL call)

Either add `SendMessage` to the seats whose governing docs assume it, or amend those docs
to the return-path/fresh-spawn model `dev/design/orchestration.md:364-368` already
specifies. The seat and its governing doc must agree; today they do not.

Worth putting in front of the HITL as a genuine design question rather than a config
typo: `orchestration.md:364-368` rules out `SendMessage` on the reasoning that *"the
implementer's state lives on disk … so a fresh subagent reading the verdict has everything
it needs."* That reasoning is sound for **sequential** fix-N hand-offs, which is the case it
was written for. §4.1 shows it does not cover **mid-flight** steering of a running
implementer, where there is no fresh spawn to hand a verdict to. Whether that capability is
wanted at all is a direction call, not an execution one.

---

## 6. unifi-openwrt exposure check — **NOT APPLICABLE**

`/home/coreyt/projects/local/unifi-openwrt` has `.claude/agents/`, `.claude/commands/` and
`.claude/settings.json`, and it does run a real multi-agent ladder
(steward → orchestrator → implementer). Both of its seats carry explicit `tools:`
allowlists that omit `SendMessage`:

- `.claude/agents/orchestrator.md:4` — `tools: Read, Bash, Agent`
- `.claude/agents/implementer.md:4-5` — `tools: Read, Edit, Write, Bash` +
  `disallowedTools: Agent`

**But nothing in that repo ever directs an agent to use `SendMessage`**, so the mismatch
that bit fathomdb cannot occur. Its escalation architecture is deliberately built on the
return path instead:

- `.claude/agents/orchestrator.md:9-11` — *"your final text returns to the steward … you
  have no direct human channel."*
- `.claude/agents/orchestrator.md:65-67` — *"The relay travels up the return path:
  implementer → closure → you → return text → steward → human. That is what §2.5.1
  describes, and it is the channel to use even where `SendMessage` exists."*
- `agents/handoff-protocol-spec.md:739-740` — *"Escalations travel up the return path,
  never sideways (§2.5.1 item 5)."*
- `.claude/commands/steward.md:26-30` — the steward is deliberately a slash command and
  **not** a spawnable seat, *"because a spawned steward would have no channel to the HITL."*

The repo has already investigated this exact area and recorded a **refuted** hypothesis it
forbids reintroducing (`.claude/agents/orchestrator.md:69-75`): naming the deferred
`SendMessage` in `tools:` was *not* the cause of `Agent` being dropped at depth 1 — the
session-wide nesting switch was. That correction is independently consistent with the
documentation, and with what I measured in fathomdb.

**Verdict: not applicable.** Same latent ingredient (explicit allowlists omitting
`SendMessage`), no exposure, because no prompt there asks for it and the coordination
design routes around it by construction. This is a well-hardened configuration, not a risk.

### 6.1 One stale fact noted in passing (reported, NOT corrected — different project)

`agents/handoff-protocol-spec.md:721-722` states the nesting default *"defaulted on in
v2.1.172–2.1.216 and off from v2.1.217"*, measured on v2.1.218. The documentation now adds
a third era: *"v2.1.219 raised the default to three."* The note is therefore stale as
written for v2.1.219+, though its operational advice (pin the env var; re-check after every
upgrade — `:741-744`) remains sound and the explicit `"CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH": "2"`
in `.claude/settings.json:3` still does useful work by *capping* depth so the implementer
stays a leaf. Flagged for that project's owner; not touched.

---

## 7. Confidence and residual uncertainty

**Witnessed (high confidence):** the seat's allowlist; the failing agent being a spawned
subagent; `Agent` working 7× in the same transcript; the fresh-spawn fallback; the doc/seat
contradictions; the documented allowlist semantics, `SendMessage`'s existence and
subagent-grantability, and the depth-limit rules.

**Inference (labelled):** that adding `SendMessage` to the seat would have made the call
succeed. The documentation supports it — the tool is subagent-grantable and the depth limit
touches only `Agent` — but I did not exercise it, and per the constraints of this
investigation I made no change to test it. A definitive settle would be a throwaway probe
seat declaring `SendMessage`, spawned at depth 1, enumerating its own tool schema. Note
`unifi-openwrt` used exactly that instrument (`7ec30c2:.claude/agents/plain-probe.md`) and
records the discipline of deleting it afterwards.

**Not determined:** whether these orchestrator subagents were themselves running in the
background. It matters only marginally — the documented background filter explicitly
*keeps* `SendMessage` among a background subagent's built-in tools — so the outcome is the
same either way, but the transcripts do not record the spawning call's
`run_in_background` value.

**Not determined:** whether a successful mid-flight correction on Slice 30 would have
avoided any of its three codex §9 fix rounds (§4.1). The hazards the orchestrator tried to
send are the same class it later recorded in `TC-81`, but the counterfactual is not
recoverable from the transcript.

**Procedural note.** During this investigation `dev/todos-and-considerations-ledger.jsonl`
and its `.seq` gained entries `TC-79`/`TC-80`/`TC-81` (seq 109–111, 15:28–15:29Z). Those
were written by the **concurrent live Slice-30 orchestrator**, not by this investigation.
This document is the only file it wrote, and it left it untracked.

---

## Part II — What a fix would involve, and what would change

**Still description only. Nothing below was implemented.** The ordering is a proposal for
the HITL to rule on, not a work item.

## 8. A prior question that must be settled first (fathomdb)

**The seat allowlist only bites because practice diverged from the playbook, and the
playbook was never updated.** This is a *fifth* contradiction, and it is upstream of the
`SendMessage` question:

| Source | Says |
|---|---|
| `dev/design/orchestration.md:39` (§1 role table) | Orchestrator = *"Main thread (you). **Always.**"* |
| `dev/design/orchestration.md:47-48` (§1 anti-patterns) | *"**Do not spawn an 'orchestrator' subagent.** The main thread IS the orchestrator."* |
| `dev/design/orchestration.md:483` (§10 rule 1) | *"Main thread orchestrates. No orchestrator subagent."* |
| `.claude/agents/orchestrator.md:3` | *"**The main thread plays this role**"* |
| `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md:240-244` (§9 mechanism 2) | *"**A Steward-spawned background orchestrator** — `Agent`, `subagent_type: orchestrator` … Ledger precedent seq-51 (0.8.16), seq-72 (0.8.19), seq-81 (0.8.20 Slice-0)."* |

Both witnessed failures are §9-mechanism-2 spawns. If §10 rule 1 were actually honoured the
`orchestrator` seat file would never be exercised and its `tools:` line would be inert —
`SendMessage` would always be present, because a main thread holds the full pool.

So the question *"should the orchestrator seat carry `SendMessage`?"* is unanswerable until
the HITL rules on *"is a spawned orchestrator a sanctioned shape?"* Today the Steward
hand-off says yes with three ledger precedents and `orchestration.md` says no. **Settle
that first; the tool grant follows from it, not the other way round.**

## 9. fathomdb (A) — ordered fix description

*Conditional on §8 resolving in favour of spawned orchestrators. If it resolves the other
way, the correct fix is the opposite: delete `.claude/agents/orchestrator.md`'s spawnability
premise, amend §9 mechanism 2 out of the Steward hand-off, and no tool grant is needed.*

**Step 1 — seat files (2 lines).**

- `.claude/agents/orchestrator.md:4` — add `SendMessage` to the `tools:` list.
- `.claude/agents/steward.md:4` — same, so a *spawned* steward matches its own §7.
- `.claude/agents/implementer.md:4` — **deliberately leave it out.** The implementer is a
  leaf. Granting it would enable lateral implementer↔implementer traffic, against the spirit
  of `orchestration.md:49-53` (*"Do not chain subagents to each other"*). Optionally mirror
  `unifi-openwrt`'s belt-and-braces and add `disallowedTools: Agent` (docs: `disallowedTools`
  is applied first, then `tools` resolves against the remainder).

**Step 2 — reconcile `dev/design/orchestration.md:364-368`. SCOPE it; do not overturn it.**

Its reasoning — *"the implementer's state lives on disk (worktree diff + output.json +
verdict.md), so a fresh subagent reading the verdict has everything it needs"* — is
**correct and should survive** for the case it was written for: a **sequential fix-N
hand-off to a subagent that has already exited.** There, fresh-spawn genuinely is the
intended mechanism, not a workaround, and §4.2 shows it working.

What it does not cover is **steering a subagent that is still running** (§4.1). There is no
fresh spawn to hand a verdict to; the choice is steer, wait, or `TaskStop` and restart.
Reconciled guidance should therefore say, in substance:

> Fresh-spawn remains the intended mechanism for fix-N and every hand-off across a
> subagent boundary that has already closed — on-disk state is the contract, and a warm
> resident is never a *substitute* for the artifact. `SendMessage` is for the case
> on-disk state cannot serve: correcting or halting a subagent that is still running.
> A mid-flight steer never replaces the brief, the `output.json`, or the codex §9 gate.

Alongside it: `orchestration.md:39` (§1 role table) and `:483` (§10 rule 1) must be updated
in the same commit, or §8's contradiction survives the fix.

**Step 3 — the Library-Bump pair.** `LIBRARY-BUMP-STEWARD.md:21,42,95` and
`LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md:4,28,35,58` make `SendMessage` the primary LBS↔LBO
channel. If Step 1 lands, these become true as written and need no edit — *provided* an LBO
is spawned onto the `orchestrator` seat. **Verify which seat an LBO actually uses**; neither
doc names one. *(Inference: the templates predate the seat files and assume a main-thread
LBS, which would have had the tool all along.)*

**Step 4 — `0.8.x-STEWARD-HANDOFF.md` §7.** No text change needed if Step 1 lands; `:161`
and `:200` become accurate. If Step 1 does *not* land, §7's entire decision procedure
(`:174-182`) needs a caveat saying its cheap branch is unreachable from a spawned steward.

**Step 5 — verification (nobody has empirically confirmed the fix works).**

1. **Probe the grant.** Throwaway seat declaring `SendMessage`, spawned at depth 1, that
   enumerates its own tool schema and attempts one real send to a live sibling. Precedent
   and discipline to copy: `unifi-openwrt` used `7ec30c2:.claude/agents/plain-probe.md` and
   **deleted it afterwards** — *"a live spawnable probe in the agent registry misleads every
   later reader of that directory"* (`agents/handoff-protocol-spec.md:700-705`).
2. **Confirm the seat still launches.** Docs: if *nothing* in `tools:` resolves, the subagent
   usually fails to launch with an error naming the entries. Adding a valid name cannot
   trigger that, but the launch should be observed once regardless.
3. **Check `Grep`/`Glob` in the same probe — a likely second latent seat defect.**
   `.claude/agents/orchestrator.md:4` grants them, yet both witnessed orchestrators did all
   searching through `Bash` (Slice 30: `Bash 76`, zero `Grep`/`Glob`; Slice 20c: `Bash 193`,
   zero). `unifi-openwrt` measured that *"`Grep`, `Glob` and `Task` do not resolve in this
   session"* (`agents/handoff-protocol-spec.md:732-733`) and that unresolved names are
   dropped silently without voiding the rest of the list (`:727-731`). *Inference, worth one
   cheap check:* fathomdb's orchestrator seat may be silently running two tools short.
4. **`Task` is a legacy alias**, harmless — docs: renamed to `Agent` in v2.1.63, still
   accepted. No action.
5. **Re-check after every Claude Code upgrade.** Adopt `unifi-openwrt`'s standing discipline
   (`agents/handoff-protocol-spec.md:741-744`): have an orchestrator enumerate its schema and
   exercise both `Agent` and `SendMessage` on a no-op.

## 10. fathomdb (B) — what actually changes in behaviour

### Edge 1 · Steward → orchestrator

| | Today (witnessed) | After |
|---|---|---|
| Mid-run status probe | Impossible. The spawn *"returns once and does NOT notify on stall"* (`STEWARD-HANDOFF:242-244`); the Steward polls git. One orchestrator stalled **36 h unnoticed** (memory `background-agent-silent-death-proactive-check`). | Possible — the Steward can ask a running orchestrator directly. |
| Delivering a HITL ruling that lands mid-run | Impossible. Wait for return, or kill and re-commission with a new brief. | Possible. |
| Re-scoping / halting in flight | Impossible. | Possible. |
| A **completed** orchestrator | Dead to the Steward; any follow-up is a fresh spawn. | Docs: *"A completed subagent that receives a `SendMessage` auto-resumes in the background without a new `Agent` invocation."* Warm resume becomes available. |

**Does not change:** polling from git stays mandatory — `SendMessage` adds a probe, not a
notification. The anti-stall directive stays required.

### Edge 2 · Orchestrator → implementer

This is where the measured pain is.

- **Mid-flight steering.** Today impossible: §4.1: a completed verification (*zero failing
  clauses* + three named hazards) was **dropped**, and the orchestrator fell back to
  `until [ -f …output.json ]; do sleep 30; done`. After: deliverable in flight.
- **Fix-N economics.** Today every round is a fresh spawn — Slice 30 spent **three**
  (`aa5c5bc667f41adca`, `ac9147080f38a773b`, `ab9ccbfe1c8b10dde`). At the §7 figures
  (`STEWARD-HANDOFF:169-172`) that is ≈$1.77 × 3 versus ≈$0.15–0.28 warm.
- **Does the K=2 crossover guidance change? No — but it becomes *applicable* one tier
  lower.** §7's decision procedure (`:174-182`) already assumes `SendMessage`; it was written
  for Steward-side residents. Nothing in the rule changes; the orchestrator tier simply stops
  being excluded from it.
- **Honest caveat against a naive "always reuse" reading.** §7 also measures that waking a
  **cold, large** resident costs ~$0.40 (small T) to **~$1.93 at T≈60k** — *"a cold, large
  resident can cost more to wake than a fresh spawn costs to do the whole job"*
  (`:170-172`). An implementer that just finished a long slice is precisely a large-T
  resident. So fix-N after a long slice may still be cheaper as a fresh spawn. The ≈$0.15
  figure is **warm and small**, not a general fix-N price.

### Edge 3 · Return paths and the authority model

**The authority model is untouched. A new channel is not new authority.** State this
plainly — the rule was written *for* `SendMessage` and already anticipates it:

- `.claude/agents/steward.md:78-80` — *"**You cannot launder authority downward.** A message
  to a commissioned orchestrator or resident is peer-level, not the HITL's authority;
  anything HITL-gated stays with you and escalates to coreyt."*
- `0.8.x-STEWARD-HANDOFF.md:130-131` — *"A message you send a subagent is **peer-level, not
  user authority**."*
- `0.8.x-STEWARD-HANDOFF.md:260-261` — *"the brief must name where the orchestrator STOPS —
  HITL gates never travel down to it."*

Unchanged in every respect: escalation still travels up the return path; codex §9 remains
the merge gate; publish, manifest bump, tag, direction/record changes, AC sign-off all stay
HITL-gated; `/code-review` stays the codex fallback. A steward that can message an
orchestrator mid-run still cannot *authorize* it to do anything the HITL has not.

### New failure modes the capability introduces

Named honestly; these are the cost side of the ledger.

1. **The fix-N round cap can be made non-binding — the most serious risk.** The breaker is
   *"3 fix-N rounds on the SAME finding, or 6 rounds total on a slice"*, engine-amended to 10
   with a mandatory Steward check-in at 6 (`orchestration.md:373-387`). It counts **rounds**,
   and a round is a discrete briefed spawn. **Mid-flight steers are not rounds.** An
   orchestrator could drip-feed N corrections inside one round and never trip the cap. This
   matters concretely: on Slice 20c the same-finding rule never fired and *"the binding
   constraint was the total"* (`:392-395`) — so the count is load-bearing, not decorative.
   Any adoption must rule explicitly whether a mid-flight steer counts against the cap.
2. **Erosion of "verify from git, not narration."** The program's spine is that state lives
   on disk (`orchestration.md` §1.5, §12.1: *"if it must survive a `/compact` or new session,
   it goes on disk"*; `.claude/agents/orchestrator.md:61-64`). A brief and an `output.json`
   are artifacts; a mid-flight instruction exists only in a transcript. Steering that changes
   what gets built would leave **no on-disk witness** — the Steward's verification would
   silently become less complete without anything looking different.
3. **Scope creep past the brief.** §10 rule 3 requires per-spawn facts in the Agent prompt
   *"Always"*; the brief is the contract. Mid-flight messages can widen scope with no
   re-brief and no record — the exact pattern memory `oob-creep-vs-justified-deviation`
   forbids.
4. **Worktree collision — indirect, not direct.** §10 rule 10 (*"One writer per checkout"*)
   is not breached by `SendMessage`, which grants no filesystem access. But it makes it
   easier to *believe* two agents own one worktree — e.g. steering implementer A in worktree
   W while spawning implementer B into W. Today's fire-and-forget shape makes that unlikely.
   *Inference, not witnessed.*
5. **Cold-resident cost inversion** — see Edge 2 caveat. A "reuse is always cheaper" reading
   loses money.
6. **Name-collision refusal.** Docs: `SendMessage` verifies a name still refers to the same
   agent and refuses if a newer agent took it — a real operational gotcha for re-spawned
   background agents that reuse names. Address by agent **ID**, not name.
7. **Leaf discipline must hold.** If the implementer ever gains `SendMessage`, lateral
   implementer↔implementer traffic becomes possible. Keep it omitted (Step 1).

## 11. unifi-openwrt (A) — no fix is needed

**Plainly: this project already solved the problem, by a different and arguably better
route. There is no work to do and none should be manufactured.**

Its seats omit `SendMessage` exactly as fathomdb's do (`.claude/agents/orchestrator.md:4` =
`Read, Bash, Agent`; `implementer.md:4-5` = `Read, Edit, Write, Bash` +
`disallowedTools: Agent`) — but **no document anywhere in the repo directs any agent to use
it**, so the instruct-but-don't-grant mismatch cannot arise. Coordination is designed around
the return path instead: `orchestrator.md:9-11`, `:65-67`,
`agents/handoff-protocol-spec.md:739-740`, and the steward is deliberately a slash command
rather than a spawnable seat *"because a spawned steward would have no channel to the HITL"*
(`.claude/commands/steward.md:26-30`).

**No latent exposure found.** The one stale fact is unrelated to `SendMessage`: the nesting
default note at `agents/handoff-protocol-spec.md:721-722` predates *"v2.1.219 raised the
default to three."* Its operational advice still holds and its explicit
`CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH: "2"` still does real work by *capping* depth so the
implementer stays a leaf.

**If they ever did want it**, the cost is unusually high for a two-word frontmatter edit,
because it is a design reversal rather than an addition:

1. It would contradict a **dated, measured, explicitly-do-not-reintroduce** correction note
   (`orchestrator.md:69-75`).
2. It would contradict a stated design *principle*, not just a default:
   `orchestrator.md:66-67` says the return path is *"the channel to use **even where
   `SendMessage` exists**"* — they already considered and rejected it.
3. It would require amending the §2.5.1 relay contract and the §6.8 topology
   (`handoff-protocol-spec.md:739-740`), which are load-bearing for their verbatim
   escalation-relay guarantee.
4. The Steward edge would gain nothing: a main-thread steward already holds the full tool
   pool, so `.claude/commands/steward.md:26-30` is unaffected either way.

## 12. unifi-openwrt (B) — what would change: almost nothing

Only **one** edge would move: orchestrator → implementer mid-flight steering. The
Steward↔orchestrator edge is already a return-path design by deliberate choice, and the
implementer is a hard leaf by a doubled guard the spec forbids widening
(`orchestrator.md:84-88`).

The marginal value there is **lower** than in fathomdb and the marginal risk **higher**,
because their protocol is more artifact-centric, not less: closure artifacts, a
`sanitisation-witness`, **both traceability walks**, and an external non-author review gate
(`orchestrator.md:44-49`). Risk #2 from §10 — steering that leaves no on-disk witness — cuts
directly against the property their whole spec exists to guarantee. Their hard boundaries
(no write to the W1700K, no DHCP on the live LAN, no capture values into committed files —
`orchestrator.md:79-83`) are human-approval gates and would be **unaffected**: as in
fathomdb, a new channel is not new authority.

**Verdict: not applicable, and adopting it would be a net negative for this project as
designed.**
