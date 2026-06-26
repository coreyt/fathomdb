---
name: dont-dismiss-user-directed-subagents
description: "Don't dismiss subagent output as over-scoped — the user may be directing the subagent directly"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 5e309f70-21d0-432b-af9b-9e5bc364b025
---

When a background subagent returns output that looks broader than the prompt I
gave it (e.g. produced a full design + review when I only asked a yes/no
question), do NOT characterize it as "off the rails / over-scoped / disregarding"
before checking whether the USER directed it. The user can drive a spawned
subagent directly via follow-up messages, so extra/expanded results may be
exactly what they asked for.

**Why:** On 2026-05-30 a subagent I launched to answer "do we have ANN + IR recall
mechanisms" returned three escalating results (answer -> IR-recall design ->
ANN-safety review). I dismissed the latter two as over-scope; the user had in fact
been sending the subagent those tasks directly ("I asked it to create a design and
implementation approach for an IR recall function"). My dismissal was wrong.

**How to apply:** Treat unexpected-but-coherent subagent output as potentially
user-directed. Acknowledge it as a real deliverable, ask how to use it, and offer
to capture it to disk (the user values capturing findings to disk) rather than
setting it aside.
