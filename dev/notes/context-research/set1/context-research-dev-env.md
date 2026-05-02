# Context Research — Development Environment

## Scope

How the development environment should be exposed to AI coding agents
(Claude Code, OpenAI Codex CLI, Cursor agents, Aider, Cline, Devin,
OpenHands). Covers sandbox/VM isolation, permission models, compiler /
typechecker / linter feedback, LSP vs shell, package-manager state,
network policy, secrets handling, CI feedback, agent-computer interface
(ACI) tool-design principles, and the cost of round trips on agent
behavior. Synthesizes empirical findings from 2024–2026 research and
production tooling docs to inform fathomdb's internal agent harness
choices.

## Sources

Primary (fetched):

- SWE-agent: Agent-Computer Interfaces Enable Automated Software
  Engineering (NeurIPS 2024), arXiv:2405.15793 —
  <https://arxiv.org/abs/2405.15793>
- OpenAI Codex CLI sandboxing concepts —
  <https://developers.openai.com/codex/concepts/sandboxing>
- Claude Agent SDK permissions reference —
  <https://code.claude.com/docs/en/agent-sdk/permissions>
- Aider linting and testing docs —
  <https://aider.chat/docs/usage/lint-test.html>
- Cursor blog: Implementing a secure sandbox for local agents —
  <https://cursor.com/blog/agent-sandboxing>
- OpenHands runtime architecture docs —
  <https://docs.openhands.dev/openhands/usage/architecture/runtime>
- The Art of Tool Interface Design (arXiv:2503.21036) —
  <https://arxiv.org/html/2503.21036v1>

Secondary / corroborating (search-summarized):

- OpenHands paper (ICLR 2025) arXiv:2407.16741 —
  <https://arxiv.org/pdf/2407.16741>
- Cognition: Devin can now Manage Devins —
  <https://cognition.ai/blog/devin-can-now-manage-devins>
- Cursor changelog 2.5 / Cursor 3.2 — sandbox + async subagents —
  <https://cursor.com/changelog/2-5>
- Claude Code hooks reference —
  <https://code.claude.com/docs/en/hooks>
- Codex sandboxing implementation deepwiki —
  <https://deepwiki.com/openai/codex/5.6-sandboxing-implementation>
- Modal/E2B/Daytona sandbox benchmark 2026 —
  <https://www.superagent.sh/blog/ai-code-sandbox-benchmark-2026>
- Daytona vs E2B 2026 (Northflank) —
  <https://northflank.com/blog/daytona-vs-e2b-ai-code-execution-sandboxes>
- Agent Vault (Infisical) — credential proxy for agents —
  <https://infisical.com/blog/agent-vault-the-open-source-credential-proxy-and-vault-for-agents>
- LSAP (Language Server Agent Protocol) —
  <https://github.com/lsp-client/LSAP>
- "Inside the Agent Harness" (Codex vs Claude Code) —
  <https://medium.com/jonathans-musings/inside-the-agent-harness-how-codex-and-claude-code-actually-work-63593e26c176>
- Cursor sandbox analysis (Agent Safehouse) —
  <https://agent-safehouse.dev/docs/agent-investigations/cursor-agent>

Empirical-vs-opinion notes are flagged inline in each finding.

## Findings

### F1 — Sandbox is a behavior shaper, not just a safety wrapper

**Evidence:**
Cursor's blog reports (empirical, production telemetry) that
"sandboxed agents stop 40% less often than unsandboxed ones" because
the sandbox absorbs the approval prompts that previously interrupted
the agent. About one-third of supported-platform requests now run
sandboxed. OpenAI Codex CLI's default mode is `workspace-write` +
`on-request` approval, deliberately tuned for "low-friction local
work," with `read-only` and `danger-full-access` as the two
extremes. Claude Agent SDK exposes `default`, `acceptEdits`, `plan`,
`bypassPermissions`, `dontAsk`, and `auto` (model-classified) modes,
each shifting the prompt-rate / blast-radius tradeoff. Devin
deliberately gives each session its own VM (not a container) on the
argument that "container-based approaches create a security
vulnerability… one compromised session can access other containers'
filesystems, credentials, and network connections" (Cognition blog,
opinion-grade but reflected in their architecture).

**Observations:**
The sandbox model directly determines how often the agent is
interrupted, and interruption frequency dominates wall-clock latency
of long-horizon tasks. The platforms converge on the same
three-level pattern (read-only / workspace-write / full-access),
which is now an effective de-facto standard. macOS implementations
all use Seatbelt (`sandbox-exec` + dynamically generated SBPL).
Linux implementations use Landlock + seccomp, sometimes wrapped by
bubblewrap (Codex CLI, Cursor). WSL2 is the standard fallback on
Windows because native Windows sandbox primitives are weaker for
dev tooling (Cursor blog, empirical).

**Recommendations:**
Adopt the three-tier convention (read-only / workspace-write /
full-access) for any internal harness. Default to
workspace-write+on-request approval; require explicit opt-in to full
access. On Linux hosts, prefer Landlock+seccomp with bwrap as
opposed to chroot or rolling your own filter set; that path is
already battle-tested by both Codex and Cursor. Treat
"interruption rate" as a first-class operational metric — cutting it
correlates directly with task throughput.

**Impact on agent LLM:** HIGH — sandbox choice determines the
prompt-rate, which determines whether the agent can complete
multi-step tasks autonomously. Anything that forces a prompt every
30 seconds destroys long-horizon performance regardless of model
quality.

### F2 — ACI design dominates raw model capability for SWE tasks

**Evidence:**
SWE-agent (NeurIPS 2024, arXiv:2405.15793) is the canonical empirical
result: "careful ACI design can substantially improve LM agent
performance without modifying the underlying LM's weights." The
authors built a custom file viewer, structured editor with linter
echo, search/navigation tools, and context manager on top of bash,
and reported pass@1 of 12.5% on SWE-bench and 87.7% on HumanEvalFix
— "far exceeding the previous state-of-the-art achieved with
non-interactive LMs" using the same underlying model. The paper's
key reframing: "LM agents are a new category of end users with their
own needs and abilities," requiring tool surfaces tuned for those
abilities rather than reused human-CLI affordances. The Art of Tool
Interface Design (arXiv:2503.21036) reports a 10.3% success-rate
gain on Llama-3.1 405B from delegating sub-reasoning to a focused
tool rather than driving it from the main loop.

**Observations:**
Two independent empirical results agree: tool design produces
double-digit accuracy swings on the same model. The directional
finding is consistent across SWE-agent, OpenHands, and the
customer-service paper. Specific design moves that consistently
help: structured edit with immediate validation, viewer with
pagination + ranges instead of `cat`, search that returns
locations + context windows instead of raw grep dumps, error
messages that explain *what to do next* rather than echoing libc
errno text.

**Recommendations:**
For fathomdb's harness, invest in tool design before model upgrades.
Specifically: (a) replace any "run this raw command, parse stdout"
pattern with structured tools that return JSON observations, (b)
make every error message include a "next step" hint, (c) page large
outputs and let the agent request more rather than dumping. Treat
this as TDD: fix a tool's interface only when an evaluation
regression demonstrates the need.

**Impact on agent LLM:** HIGH — ACI design is the single largest
empirical lever after model choice. Worth more than swapping models.

### F3 — Compiler / typechecker / linter feedback is the highest-signal context

**Evidence:**
Aider's docs (primary): "When the linter reports a violation, Aider
reads the message, modifies the code to satisfy the rule, and
re-runs until the linter is happy or it gives up after a sensible
number of tries." Aider auto-lints any edited file by default,
supports `--lint-cmd`, `--auto-test`, and `--test-cmd`, and treats
pre-commit hook failure as just another feedback signal. SWE-agent's
editor explicitly runs a syntax check after each edit and
re-prompts the agent on failure. Anthropic shipped native LSP
support in Claude Code (Dec 2025, secondary), providing
"automatic diagnostics after every file edit." LSP-skill / LSAP
projects generalize this pattern into a protocol layer (secondary,
opinion-grade but converging direction).

**Observations:**
The pattern is universal across mature harnesses: edit →
validate-locally → feed errors back → retry. The key empirical
property is *fidelity of the error message*. rustc's
"consider adding `&`" hints, tsc's structured diagnostics, and
clippy's machine-applicable suggestions feed agents better than
mypy's positional errors or eslint's plain text. Compiler feedback
is also high *information density per token*: a single rustc error
points the agent at the exact line, the expected vs found type,
and often the fix. Compare to runtime test failure, which often
needs stack-trace digestion.

**Recommendations:**
Make `cargo check` / `tsc --noEmit` / `mypy` / `clippy` the
*first* feedback loop after every edit, before tests. Surface them
as structured diagnostics (file, line, severity, message,
suggestion) rather than raw stderr. For Rust specifically, prefer
`cargo check --message-format=json` and surface
`rendered`+`spans`+`children` to the agent. Run linters in
`--fix`/`--apply` mode where safe so the agent doesn't burn a
turn re-doing trivial corrections.

**Impact on agent LLM:** HIGH — compiler/typechecker feedback is the
densest, most reliable signal an agent gets. Loss of this loop
forces the agent into runtime-test-driven debugging, which is
slower and noisier.

### F4 — LSP gives agents semantic capabilities grep cannot

**Evidence:**
LSP-vs-grep claims (secondary, blog-grade): "Finding all call sites
of a function takes roughly 50ms with LSP versus potentially tens
of seconds with recursive text search." LSP distinguishes a
local `config` from a module-level `config`, can produce hover
types, jump to definition, find references, and rename
symbol-aware. Anthropic shipped LSP into Claude Code in Dec 2025
(secondary, multiple corroborating reports). LSAP and lsp-skill
(secondary, GitHub) are emerging protocols layering "agent-native
cognitive tools" on top of LSP. Caveat: the 50ms vs tens-of-seconds
number is from a practitioner blog, not a benchmark paper —
directional, not load-bearing.

**Observations:**
Shell/grep is *necessary* (works in tmux, on remote servers, in
CI) but not *sufficient*: it cannot disambiguate identifiers, walk
type relationships, or report post-edit diagnostics. LSP+shell is
the dominant pattern: shell for execution and side effects, LSP for
navigation and validation. The CLI-vs-IDE tradeoff (Claude Code as
CLI, Cursor as IDE) is real but resolvable: a CLI agent can still
spawn a headless LSP client.

**Recommendations:**
For any large-codebase agent, run a headless LSP per language and
expose `find_references`, `goto_definition`, `hover_type`,
`workspace_symbols`, and `diagnostics` as first-class tools.
Treat raw grep as a fallback. Be explicit about when an answer is
syntactic (grep) vs semantic (LSP) so the agent picks correctly.
Watch the LSAP/ACP space; if a standard solidifies in 2026, plug
into it rather than building bespoke wrappers.

**Impact on agent LLM:** HIGH on large codebases (>50k LOC),
MEDIUM on small ones. Below ~10k LOC ripgrep is fine; above that,
LSP feedback meaningfully changes which tasks complete.

### F5 — Container vs VM isolation is a non-trivial choice

**Evidence:**
OpenHands (primary docs + ICLR 2025 paper) uses per-task Docker
containers with an Action Execution REST API, layered base image
extension, and bind-mount or named-volume strategies including
overlay (copy-on-write) mode. E2B uses Firecracker microVMs
(per-session kernel). Modal uses gVisor. Daytona uses Docker
containers (shared kernel) and explicitly trades isolation for
cold-start latency (claimed 27ms vs E2B's sub-second). Cognition
Devin chose VM isolation explicitly: "container-based approaches
create a security vulnerability… containerized agents share a
kernel" (opinion-grade rationale; the architectural choice is
empirical).

**Observations:**
The market has bifurcated. Hardware-isolated (microVM, full VM)
runtimes prioritize multi-tenant security — they are the right
choice when running untrusted code submitted by external users.
Container-based runtimes prioritize cold-start latency and
state persistence — they are the right choice for a developer's
own agents acting on their own code on their own machine. The
fathomdb single-developer case sits firmly in the second bucket;
local sandbox (Codex/Cursor model) plus per-task workspace
checkout is sufficient. Multi-tenant cloud agents (Devin, hosted
OpenHands) require the VM model.

**Recommendations:**
For local dev agents: Linux Landlock+seccomp on host, no
container needed for security; container only if you want a clean
build environment. For shared/cloud agents acting on customer
code: gVisor or microVM, never raw Docker. Persistence: prefer
ephemeral workspaces with explicit checkpoint/snapshot rather than
long-lived stateful sandboxes — the latter accumulate drift and
defeat reproducibility.

**Impact on agent LLM:** MEDIUM — isolation choice mostly affects
operations and security, not agent reasoning quality. Becomes HIGH
only if cold-start latency forces the agent into batching that
breaks its loop.

### F6 — Network policy must be scoped, not boolean

**Evidence:**
Codex CLI (primary docs): network access is governed by approval
policy (`untrusted` / `on-request` / `never`); `workspace-write`
default disallows network until escalated. Cursor (primary blog):
"agents… only request approval when they need to step outside it,
most often to access the internet"; Cursor 3.2 added "granular
network access controls" per-domain (secondary changelog).
Infisical Agent Vault (secondary, design article): agents route
through a local HTTP proxy that injects credentials at the network
layer; "the agent never sees secrets." OpenHands (primary docs)
file-locks port allocation per sandbox.

**Observations:**
A boolean "network on/off" is a poor fit for real workflows: the
agent legitimately needs `crates.io` / `pypi.org` / `npmjs.com` /
`registry.fedoraproject.org` / `github.com` but not arbitrary
egress. Domain allowlists + a credential-injecting proxy resolves
both the convenience and the secrets-handling problem in one
mechanism. Lockfile state + network policy interact: if the agent
can hit the registry it can quietly mutate `Cargo.lock` /
`package-lock.json` / `uv.lock` — sometimes desired, sometimes
not. Most harnesses (Codex, Cursor) treat lockfile churn as just
another diff for the user to review.

**Recommendations:**
For fathomdb's harness, run an egress proxy (or network namespace
with NetworkPolicy-style allowlist) defaulting to: package
registries for the project's languages, the project's git remote,
and nothing else. Inject API tokens via the proxy, never put them
in env vars visible to the agent process. Surface `Cargo.lock` /
lockfile diffs explicitly so an agent can't smuggle dependency
upgrades.

**Impact on agent LLM:** MEDIUM — incorrect network policy mostly
manifests as "agent gets stuck" rather than "agent makes wrong
decision." HIGH if mishandled, since unscoped egress is the
primary path for secrets exfiltration.

### F7 — Per-tool-call latency budgets shape agent behavior

**Evidence:**
SWE-agent paper (primary): the design of the editor and viewer
explicitly trades flexibility for predictability because each
agent turn costs both tokens and wall-clock. Daytona's marketing
27ms cold-start (secondary, vendor-claim) is positioned against
E2B's sub-second start specifically because per-call latency
multiplies across agent turns. Cursor's 40% interruption-reduction
result (primary blog) is fundamentally a latency claim. The Art of
Tool Interface Design (primary) recommends "removing redundant tool
results after use, compressing distractions from earlier turns" —
these are direct context-token-cost optimizations.

**Observations:**
Latency comes from two places: model-side (round-trip + reasoning
tokens) and environment-side (sandbox start, command exec, network
I/O). Environment latency is more controllable than model latency.
A 200ms LSP query is 50× cheaper than a 10s grep across a large
repo, and the agent makes more queries per task as a result. A
2-second container cold start, multiplied across 20 tool calls in
a complex task, is 40 wasted seconds — and adds enough variance
that the agent loop can stall.

**Recommendations:**
Track per-tool p50/p95 latency. Treat every >500ms tool as a
candidate for caching, batching, or replacement. Keep a long-lived
language-server process per language rather than respawning;
likewise keep `cargo check` warm in a long-lived `cargo watch`-style
process when feasible. Truncate observation outputs at the tool
boundary (return first N + "X more rows; ask for range"); do not
let the agent burn turns scrolling.

**Impact on agent LLM:** MEDIUM — directly affects throughput; only
indirectly affects correctness (slow loops force fewer iterations,
which means worse final outputs).

### F8 — Secrets must never enter the agent's context window

**Evidence:**
Aider (primary docs): supports `.env` for credential loading but
defaults to `--no-verify` on git commits, intentionally bypassing
hooks that might leak commit-signing keys. Claude Agent SDK
(primary docs): hooks evaluate before `bypassPermissions` — so a
deny-rule on `Bash(env)` or pre-tool hook can scrub the
environment. Infisical Agent Vault (secondary): credential proxy
pattern, agent calls `https://internal-api/...` and the proxy
injects the bearer token at the network layer. Render / Cloudflare
Zero Trust (secondary): "workload identity" via OAuth + runtime
attestation. Cognition Devin (secondary): credentials live in the
VM environment but specifically isolated per-session.

**Observations:**
Three failure modes recur: (a) secrets in environment variables an
agent can `printenv` and quote in chat, (b) secrets in
`~/.netrc`/`~/.aws/credentials` an agent can `cat`, (c) secrets in
git config / commit signatures the agent unintentionally exposes
in diffs. (a) is the most common; the agent rarely intentionally
exfiltrates, but it cheerfully echoes them in error messages or
debug output. The proxy pattern (Agent Vault, Cloudflare AI
Gateway) is the cleanest fix because it keeps secrets out of the
agent's process entirely.

**Recommendations:**
For fathomdb: do not put long-lived API keys in the agent's
environment. Use a local proxy that injects auth headers, and a
deny-rule on tools that can echo env (`env`, `printenv`, raw
`Bash` without filtering). On commit signing, avoid `--no-verify`
patterns; if signing is required, run signing in a wrapper the
agent calls but cannot read. Audit any error path that includes
environment variables in messages.

**Impact on agent LLM:** LOW on reasoning quality, HIGH on
operational safety. Bad secrets handling does not make the agent
dumber, it just makes a leak inevitable.

### F9 — Hooks beat memory for enforced behavior

**Evidence:**
Claude Agent SDK (primary): permission evaluation runs hooks
*first*, before allow/deny rules and permission mode. Hooks can
allow, deny, or modify a tool call. As of 2026 Claude Code has 21
lifecycle events and 4 handler types (command/http/prompt/agent).
Aider (primary): `--git-commit-verify` opts into pre-commit hooks;
hooks can be a feedback signal, not just a guardrail. Cursor
(secondary): updated shell tool descriptions and error renderings
to surface sandbox constraints "to prevent agents from repeatedly
retrying identical commands" — i.e. environment-side hooks
encoding policy that memory alone could not enforce.

**Observations:**
Memory ("from now on, always run X") is unreliable because the
agent's instruction-following degrades under context pressure.
Hooks are reliable because they are executed by the harness, not
by the model. The fathomdb auto-memory note "Workflow validation
= actionlint" is exactly this pattern: configure the gate, do not
just describe it. Cursor's finding that surfacing the sandbox
constraint in the error message changes agent behavior is the same
phenomenon at the observation layer: the harness shapes the
message the agent sees, and that determines what it tries next.

**Recommendations:**
Encode invariants as hooks/gates, not as memory entries.
Specifically: post-edit linter, post-edit typechecker, pre-commit
test gate, deny-rules on dangerous commands, and an error-renderer
that explains *why* a sandbox or policy denied an action. Reserve
memory/system-prompt for taste and high-level priorities;
everything load-bearing belongs in the harness.

**Impact on agent LLM:** HIGH — hooks reliably eliminate whole
classes of failure that prompt engineering only mitigates.

### F10 — Reproducibility requires explicit workspace lifecycle

**Evidence:**
OpenHands (primary docs): each task gets a fresh container from a
versioned base image; bind-mounts use overlay copy-on-write so the
host workspace is not mutated until explicitly synced. Devin
(secondary): "vectorised snapshots of the code base plus a full
replay timeline of every command, file diff, and browser tab." E2B
microVMs are explicitly disposable. Daytona explicitly contrasts
"long-lived workspaces" against "disposable execution
environments" (secondary, vendor blog) — these are opposite
philosophies.

**Observations:**
Long-lived agent workspaces drift. Lockfiles update, caches
populate, build artifacts accumulate, env vars get added by
side-effect commands. Drift is invisible to the agent and to the
user until a "works on my machine" failure surfaces. The
disposable-workspace pattern (E2B-style, OpenHands per-task) sheds
this drift but requires the harness to checkpoint anything worth
keeping. The replay-timeline pattern (Devin) is a hybrid — long
lifetime, but every change is recorded and reversible.

**Recommendations:**
Default to disposable workspaces per task, with the project
checkout treated as a clean immutable starting point. Promote a
workspace to "persistent" only on explicit user action. Capture
the diff at task end as the unit of work to review/commit. Avoid
agents that live inside the developer's actual working tree —
that's how lockfile churn and sneaky toolchain upgrades happen.

**Impact on agent LLM:** MEDIUM — drift mostly causes operational
pain, not reasoning errors. Becomes HIGH when a long-lived
workspace silently accumulates a state the agent cannot reason
about.

## Synthesis

A coherent picture emerges across SWE-agent, OpenHands, Codex CLI,
Claude Code, Cursor, Aider, and Devin: **the development environment
is not infrastructure under the agent; it is a co-equal participant
that shapes what the agent will do**. The empirical results are
consistent — sandbox choice changes interruption rate by ~40%
(Cursor), tool design changes accuracy by ~10pp (Tool Interface
Design paper), and ACI design alone produced state-of-the-art
SWE-bench results without changing the underlying model
(SWE-agent).

The convergent patterns worth adopting:

1. **Three-tier permission model** (read-only / workspace-write /
   full-access) is now standard. fathomdb should match this contract
   so users have one mental model across tools.
2. **Compiler/typechecker/linter as the primary feedback loop**, not
   tests. Tests are slower, noisier, and hit later in the loop.
   Aider's pattern — auto-lint on edit, retry to convergence — is
   directly transplantable.
3. **LSP for navigation, shell for execution.** Once a codebase is
   non-trivial, grep alone is slow enough to change agent behavior
   for the worse. Run a headless LSP per language as a long-lived
   process; expose semantic queries as tools.
4. **Network policy is allowlist + credential proxy**, not on/off.
   This collapses the secrets-handling and the package-install
   problem into a single mechanism.
5. **Hooks over memory** for any invariant. The Claude Code
   permission flow (hooks → deny → mode → allow → callback) is the
   right ordering, and the same pattern applies to local fathomdb
   gates: actionlint, cargo check, pre-publish smoke.
6. **Disposable workspaces with explicit promotion.** Drift is the
   silent killer of reproducibility; per-task overlay or microVM
   checkouts (OpenHands / E2B model) eliminate it by construction.
7. **Tool design before model upgrade.** This is the highest-leverage
   point. Two independent empirical results agree that the same
   model performs measurably better against a well-designed tool
   surface, and the gain is comparable to a model-tier swap.

For fathomdb specifically (single-developer, local-first, Rust +
Python + TypeScript): the right baseline is Codex/Cursor-style local
sandbox (Landlock+seccomp on Linux, Seatbelt on macOS) at
workspace-write level, headless LSP per language exposed as semantic
tools, post-edit `cargo check` / `tsc` / `mypy` / `clippy` /
`actionlint` as hooks, an egress proxy with allowlist for
`crates.io` + `pypi.org` + `npmjs.com` + the project's git remote,
disposable per-task workspaces (worktree-per-task fits naturally),
and zero secrets in agent env. This composition is achievable today
and matches the empirical state-of-the-art.

What remains genuinely uncertain (do not over-design here): whether
LSAP / Agent Client Protocol will solidify as a real standard in
2026 (currently emerging, opinion-grade), whether async subagent
trees (Cursor 3.2's `/multitask`) generalize beyond IDE-bound
workflows, and whether per-edit semantic diff (vs textual diff) is
worth the implementation cost. Defer those decisions; do not bake
them into the harness yet.
