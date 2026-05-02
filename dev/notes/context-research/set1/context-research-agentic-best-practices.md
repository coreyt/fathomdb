---
title: Agentic Coding — Best Practices for FathomDB
date: 2026-05-01
inputs:
  - dev/tmp/context-research-summary.md   # 6 dimensions, 70 findings
  - dev/tmp2/context-research-summary.md  # 5 dimensions, ~38 findings
  - dev/tmp3/context-research-summary.md  # 6 dimensions, 9 cross-cutting findings
purpose: Synthesize three independent context-engineering research tracks into actionable best practices for the FathomDB 0.6.0 rewrite, ranked by impact × confidence, with explicit mapping to FathomDB artifacts.
---

# Agentic Coding — Best Practices for FathomDB

## 1. Research Goals

Three parallel research tracks investigated _what context to give an AI coding agent (Claude Code / Codex / Cursor / Aider / Cline / Devin) and how to structure it for reliable code production_. Each track partitioned the space differently:

| Track    | Output                                 | Dimensional split                                                                      | Style                                                         |
| -------- | -------------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| **tmp**  | `dev/tmp/context-research-summary.md`  | tech-docs, source-code, comments, tests, dev-env, _other_ (memory/prompts/multi-agent) | Six dimensions, 70 findings, U1–U8 universal themes           |
| **tmp2** | `dev/tmp2/context-research-summary.md` | tech-docs, source-code, tests, dev-env, agent-scaffolding                              | Five dimensions, ~38 findings, T1–T6 themes + C1–C3 conflicts |
| **tmp3** | `dev/tmp3/context-research-summary.md` | tech-docs, source-code, comments, tests, dev-env, _other_                              | Six dimensions, F1–F9 cross-cutting findings, vendor-doc lean |

Unified question this synthesis answers: **What is the smallest, highest-trust set of artifacts and conventions FathomDB should adopt now — at the start of 0.6.0 implementation — so that AI coding agents (Claude Code / Codex / Cursor) can navigate, decide, edit, and verify reliably across Rust + Python + TypeScript surfaces?**

Confidence convention used throughout:

- **HIGH** — ≥2 independent peer-reviewed or vendor-published benchmarks, replicated direction
- **MED** — single benchmark + multiple vendor anchors, OR multiple vendor anchors with direction agreement
- **LOW** — vendor opinion only, weak / contested replication, or anchor measured outside the domain (e.g. web search benchmark applied to coding)

Effect-size convention:

- **XL** — ≥10 percentage-point absolute lift on a coding benchmark, or ≥4× on a relevant cost/latency metric
- **L** — 5–10 pp absolute lift, or 2–4× cost/latency
- **M** — 1–5 pp lift, or 1.2–2× cost/latency
- **S** — <1 pp, or directional only

---

## 2. Findings — universal best practices ranked by impact × confidence

The findings below appear in **at least two of the three summaries** and are ranked by combined effect size and confidence. Each finding cites the originating tracks and dimensions.

**Inclusion-rule exception:** F14 (prompt caching) is single-track (tmp2 only) but is retained because the cost/latency anchor is vendor-published with concrete numbers and is independent of the accuracy-lift findings; readers should treat it as an operational optimization, not an accuracy result. F15 (prescriptive-bullet style) is added per critic review (covered explicitly in tmp2 T4 and indirectly in tmp3 F5).

### F1. Curate context aggressively; more is not better (the "context budget" rule)

- **Tracks:** tmp U1, tmp2 T1, tmp3 F1 — appears in all three.
- **Mechanism:** Lost-in-the-middle / Context-Rot — long-context multi-fact recall ≈90% Claude / ≈60% Gemini at scale; ~24–59% accuracy degradation at ~30k tokens of distractor density.
- **Effect size:** **XL**. Anthropic Contextual Retrieval (BM25 + reranker, _not_ raw long-context) cut retrieval-failure rate by **49–67%** (tmp U1). AutoMCP improved tool-call accuracy from baseline to **99.9%** after a 19-line spec correction (tmp summary line 148) — single-anchor demonstration that focused curation outweighs volume.
- **Confidence:** **HIGH**. Multiple independent benchmarks (lost-in-the-middle paper, Context-Rot, RepoCoder, Anthropic Contextual Retrieval, AutoMCP).

### F2. Externalize durable state to disk; treat the chat window as ephemeral

- **Tracks:** tmp U2, tmp2 T2, tmp3 F3.
- **Mechanism:** Compaction is now first-party (Anthropic `compact-2026-01-12`); ~65% of enterprise AI failures in 2025 were context drift, not exhaustion (tmp2 T2). Files survive compaction; chat does not.
- **Effect size:** **L**. Indirect — measured via avoidance of catastrophic-failure modes rather than direct lift; Cognition Devin postmortem and Anthropic compaction guidance both load-bearing.
- **Confidence:** **HIGH**. Convergent across three tracks plus two vendors.

### F3. Executable feedback dominates prose feedback (tests / types / linters > docstrings / specs)

- **Tracks:** tmp U3, tmp2 T5, tmp3 F2.
- **Mechanism:** Frontier models are now _trained_ on execution feedback (RLEF/RLVR — tmp2 T5). Anthropic: tests-as-oracle is "the single highest-leverage thing you can do." RustAssistant LLM-with-cargo-diagnostics fixed **74%** of the target population vs `cargo fix` (deterministic auto-fixer, qualitatively different system) **<10%** on the same population (tmp2 T5, dev-env) — the headline number is "LLM with structured diagnostics outperforms compiler-only auto-fix by a wide margin," not a clean 8× ratio.
- **Effect size:** **XL**. Engineering Agent **+15.4 pp** from test feedback (tmp); RustAssistant **74%** absolute (no peer system); SWT **2× precision** (tmp).
- **Confidence:** **HIGH**. Multiple benchmarks (Engineering Agent, RustAssistant, SWT 2× precision, AgentCoder).

### F4. Stale context is worse than missing context

- **Tracks:** tmp U4, tmp2 T3, tmp3 F6.
- **Mechanism:** Agents treat written context as authoritative. Stale comments (tmp comments F1/F7), stale ADRs (tech-docs F6), stale CLAUDE.md (tmp other F1), stale memory bank (tmp other F2) all produce **confidently wrong** patches. AGENTbench: human-written context cost 19% more inference for 4% accuracy gain — and that's when fresh.
- **Effect size:** **L** (negative). 1-in-10 multi-fact recall hits in 1M context are confidently wrong (tmp2 T3); test-gaming on impossible-SWEbench dropped from **76% → 1%** with fresh / property-based oracles (tmp2 T3, tests F3).
- **Confidence:** **HIGH**. Three tracks, multiple incidents, replication across vendors.

### F5. Iterative agentic retrieval beats pre-loaded RAG for local code

- **Tracks:** tmp U5, tmp2 C1, tmp3 F4.
- **Mechanism:** RepoCoder iterative retrieval **>+10 pp** over **in-file-only** baselines (tmp source-code F2, tmp3 F4 — note: comparand is in-file-only, not "no retrieval"); Anthropic and Sourcegraph publicly pivoted off vector DBs as the primary retrieval mechanism. Cursor still gets +12.5% from semantic search but **on top of grep**, not replacing it (tmp2 C1).
- **Effect size:** **L–XL boundary** for iterative vs in-file-only (RepoCoder ≈+10 pp, sits exactly at the XL threshold); **M** for hybrid vs grep-only (Cursor +12.5% with caveats).
- **Confidence:** **HIGH** for "iterate not pre-load"; **MED** for "no local vector index" (Cursor data argues for hybrid even locally).

### F6. Tool-surface design (ACI) rivals model upgrade for accuracy

- **Tracks:** tmp U6, tmp2 T4 + Action 4, tmp3 F8.
- **Mechanism:** SWE-agent ACI paper, Tool Interface Design paper: **~10 pp** accuracy delta from tool design alone — comparable to a model-tier swap. Cursor sandbox: **~40%** interruption-rate delta and **84%** prompt-approval reduction (dev-env). Structured `--error-format=json` over raw bash → **~4×** SWE-bench lift.
- **Effect size:** **XL**. ~10 pp from tool design; ~4× from structured diagnostics.
- **Confidence:** **HIGH**. Two papers + vendor measurement.

### F7. Cross-vendor convergence on AGENTS.md as the scaffolding standard

- **Tracks:** tmp2 T6, tmp3 F3 (root-instruction file pattern); tmp implicit (CLAUDE.md ≤200 lines).
- **Mechanism:** AGENTS.md stewarded by Linux Foundation Agentic AI Foundation; **60k+ OSS adopters**; native support in Codex, Cursor, Copilot, Devin, Aider, Zed, Warp, goose, Factory, VS Code. Claude Code reads it natively. Pattern: short root instruction file, scoped subsystem files, link-out to deeper docs.
- **Effect size:** **L** for adoption alone (token efficiency, instruction adherence); compounds with F1 and F2.
- **Confidence:** **HIGH** for the convention's existence and adoption; **MED** for measured per-task lift (no published ablation in any track).

### F8. Subagents pay off only at clean seams (parallel reads / format-strict review); single-agent for shared-state edits

- **Tracks:** tmp U7, tmp2 C3 + Action 7, tmp3 F9.
- **Mechanism:** Anthropic research mode +90.2% over single Opus, but **15× tokens** and _same Anthropic post_: "most coding tasks are not a good fit." Cognition Devin: "writes stay single-threaded." AgentCoder three-agent test loop is the clean positive example. _Note:_ tmp U7 also cites Aider's architect/editor split, but the validating benchmark is single-track (tmp only) and is therefore not load-bearing for this finding's HIGH rating.
- **Effect size:** **XL** for fan-out search/research (+90%); **negative** for shared-state implementation.
- **Confidence:** **HIGH** on the shape; **MED** on exact token cost trade-off.

### F9. Prompt position matters: invariants front-loaded, task end-loaded

- **Tracks:** tmp U8, tmp2 T1 (implicit via budget rule), tmp3 F1 (layered context).
- **Mechanism:** Lost-in-the-middle implies position discipline (tmp U8, source-code F4). Convergent template: invariants near start, transient task near end, retrievable bulk middle accessed on demand.
- **Effect size:** **M**. Inferred from lost-in-the-middle gradient; no clean isolated ablation in any track.
- **Confidence:** **MED**. Direction is solid (lost-in-the-middle is HIGH), specific position effect for code agents under-measured.

### F10. Structured artifacts and machine-readable contracts beat prose

- **Tracks:** tmp U3 (subset), tmp2 T4, tmp3 F5.
- **Mechanism:** AST chunks **+2.67 SWE-bench Pass@1** over line-based (tmp2 T4, source-code F3). Property-based tests (Hypothesis) **+23–37% relative pass@1** over example-based TDD (tmp2 T4, tests F5). OpenAPI / JSON Schema / typed examples reduce parameter hallucination (tmp3 F5).
- **Effect size:** **L**. Property-based tests are the strongest single anchor here.
- **Confidence:** **HIGH**.

### F11. Comments encode intent, invariants, and "why" — not "what"

- **Tracks:** tmp U3 + per-dim comments, tmp3 F7 (tmp2 doesn't include comments dimension).
- **Mechanism:** Public-API docstrings carry weight (~1–3% lift class-level generation); internal-helper docstrings do not. AGENTbench: prose comments cost ~19% more inference for ~4% accuracy gain. Stale comments are net-negative (F4).
- **Effect size:** **S–M**. Direction-positive but small; mostly a _cost-avoidance_ finding (stale comments hurt more than fresh comments help).
- **Confidence:** **MED**. Two tracks; comments dimension absent from tmp2.

### F12. Verification stack ordering by latency: lint → typecheck → unit-test → integration

- **Tracks:** tmp dev-env F + tests reconciliation, tmp2 Action 4, tmp3 F8.
- **Mechanism:** Latency-ordered post-edit hooks. Test feedback granularity should match the budget; compiler/linter is the primary loop, tests come second.
- **Effect size:** **L**. Compounds with F3 and F6.
- **Confidence:** **HIGH** on ordering; **MED** on isolated effect-size attribution.

### F13. Three-tier permission model + disposable workspaces + egress allowlist

- **Tracks:** tmp dev-env, tmp2 Action 4, tmp3 F8.
- **Mechanism:** Read-only / workspace-write / full-access tiers with explicit escalation. Worktree or microVM per task. Cursor sandbox **84% prompt-approval reduction** without measured task-success regression.
- **Effect size:** **L**. Operational: large UX improvement; task-success effect untested.
- **Confidence:** **MED**. Strong vendor data on UX; no public ablation isolating task-success delta.

### F14. Prompt caching is a force multiplier (cost + latency) — _single-track, retained as cost/latency anchor_

- **Tracks:** tmp2 Action 2 only (tmp / tmp3 do not call this out). Inclusion-rule exception per Section 2 intro: this is a cost/latency optimization, not an accuracy lift, and is independent of the F1–F13 backbone.
- **Mechanism:** Cache stable prefix (system prompt + tool defs + scaffolding). 1-hour TTL for long sessions. Reported **5–10× input-cost reduction**, **~85% latency reduction** on cached portions.
- **Effect size:** **XL** on cost/latency; no direct task-accuracy effect.
- **Confidence:** **MED**. Vendor-reported, no peer-reviewed replication; numbers themselves are well-trusted as Anthropic-published.

### F15. Prescriptive bullets outperform narrative for instruction docs

- **Tracks:** tmp2 T4 (explicit), tmp3 F5 (indirect — structured artifacts > prose), tmp comments dim (related: why-not-what).
- **Mechanism:** Bullet-form "Use X" / "Never Y" survives compaction and prompt-position drift better than paragraphs. Distinct from F10 (machine-readable contracts) — F15 is style discipline for human-language instructions, F10 is the contract surface itself.
- **Effect size:** **M**. No isolated ablation; argued by tmp2 as a writing convention with strong direction agreement.
- **Confidence:** **MED**. Two tracks; one explicit, one indirect.

---

## 3. Comparisons

### 3.1 Convergence matrix

Rows = best practices F1–F14. Columns = the three research tracks. ✓ = explicitly named; ◐ = implied or partial; ✗ = absent.

```text
                                                          tmp   tmp2  tmp3
F1  Curate; context budget                                 ✓     ✓     ✓
F2  Durable state to disk                                  ✓     ✓     ✓
F3  Executable feedback > prose                            ✓     ✓     ✓
F4  Stale > missing as failure mode                        ✓     ✓     ✓
F5  Iterative retrieval > pre-load RAG                     ✓     ✓     ✓
F6  Tool-surface design (ACI) ≈ model upgrade              ✓     ✓     ✓
F7  AGENTS.md as scaffolding standard                      ◐     ✓     ✓
F8  Subagents at clean seams only                          ✓     ✓     ✓
F9  Prompt position discipline                             ✓     ◐     ◐
F10 Structured artifacts > prose contracts                 ◐     ✓     ✓
F11 Comments = why/invariants only                         ✓     ✗     ✓
F12 Verification ordering lint→type→test                   ✓     ✓     ◐
F13 3-tier perms + sandbox + egress allowlist              ✓     ✓     ✓
F14 Prompt caching*                                        ✗     ✓     ✗
F15 Prescriptive bullets > narrative                       ◐     ✓     ◐
```

\*F14 is single-track (inclusion-rule exception — see Section 2 intro).

### 3.2 Where the tracks disagree, and how to read it

- **C1: Embedding RAG vs agentic grep.** _tmp_ says "do not make embedding the substrate"; _tmp2_ says "no local vector index"; _tmp3_ says "hybrid: lexical + semantic + structural." Resolution: _for a single local repo on a typed Rust + Python + TS codebase, grep + glob + read + tree-sitter map is sufficient and matches Anthropic's published direction. Embeddings become useful at multi-repo / cloud scale or for pure-prose corpora._ For FathomDB now: skip vector index; revisit if cross-repo agent search becomes a goal.

- **C2: Doc dumps vs distilled summaries.** _tmp_ tech-docs splits on whether to pre-load architecture docs or only index them. Resolution: index by path; distill load-bearing invariants into the AGENTS.md ≤200 lines; full ADRs reachable but not preloaded. _tmp2_ and _tmp3_ both already converge on this.

- **C3: Multi-agent for coding.** Anthropic's research-mode 90.2% lift cited by _tmp2_ and Anthropic's own caveat ("most coding tasks are not a good fit") cited by all three. Resolution: subagents for fan-out search and format-strict review; single-agent for any edit on shared state. Already encoded in `feedback_orchestrate_releases.md` (worktree implementer + code-reviewer).

- **C4: Docstring value.** _tmp_ finds 1–3% lift from docstrings; _tmp2_ doesn't measure (no comments dim); _tmp3_ favors "why/invariant" comments and warns against restating syntax. Resolution: public-API docstrings yes; internal-helper docstrings no. Module-header invariants when load-bearing.

- **C5: Test feedback granularity.** _tmp_ tests dim says single tests; _tmp_ dev-env says lint/typecheck first. _tmp2_ T5 reconciles via latency budget (lint < typecheck < unit < integration). All three end up at the same place via different paths.

### 3.3 Effect-size visualization (top practices, normalized to best-known empirical anchor)

ASCII bar chart. Bar units are illustrative and _not_ comparable across rows (each row's anchor is a different metric — accuracy lift, cost reduction, etc.); they show within-row magnitude relative to the strongest claim.

```text
F3  Executable feedback (RustAssistant 74% vs <10%)         ████████████████████  XL
F6  Tool-surface design (~4× SWE-bench from json diags)     ████████████████████  XL
F1  Curation (Anthropic 49–67% retrieval-failure cut)       █████████████████░░░  XL
F8  Subagents on clean seams (+90.2% accuracy / 15× cost)   █████████████████░░░  XL accuracy, fan-out only
F14 Prompt caching (5–10× input cost, ~85% latency)         █████████████████░░░  XL cost/latency only — not accuracy
F5  Iterative retrieval (RepoCoder +10 pp)                  █████████████░░░░░░░  XL
F10 Structured artifacts (Hypothesis +23–37% pass@1)        ████████████░░░░░░░░  L
F4  Stale-context cost (test-gaming 76%→1%)                 ████████████░░░░░░░░  L (negative→positive)
F12 Verification ordering (compounds with F3 + F6)          █████████░░░░░░░░░░░  L
F2  Durable state to disk (avoids drift; no clean ablation) █████████░░░░░░░░░░░  L
F13 Sandbox / 3-tier perms (Cursor 84% prompt cut)          ████████░░░░░░░░░░░░  L
F7  AGENTS.md adoption (60k+ OSS, no per-task ablation)     ███████░░░░░░░░░░░░░  L
F9  Prompt position (lost-in-middle gradient)               █████░░░░░░░░░░░░░░░  M
F11 Comments = why/invariants (1–3% lift, stale-cost neg)   ████░░░░░░░░░░░░░░░░  S–M
```

### 3.4 Impact × Confidence quadrant

```text
                               IMPACT
                  ┌──────────────────┬──────────────────┐
                  │     LOW IMPACT   │   HIGH IMPACT    │
        HIGH   ┌──┤                  │ F1, F3, F4, F5,  │
        CONF.  │  │      F11         │ F6†, F8†, F12†,  │
        ───    │  │                  │ F2, F10          │  ← ship-now stack
               ├──┼──────────────────┼──────────────────┤
               │  │                  │ F7, F9, F13,     │
        MED    │  │       —          │ F14*, F15        │
        CONF.  │  │                  │  ← ship-now;     │
               │  │                  │     measure      │
               ├──┼──────────────────┼──────────────────┤
               │  │                  │                  │
        LOW    │  │       —          │       —          │
        CONF.  │  │                  │                  │
                  └──────────────────┴──────────────────┘
```

Reading: every practice in the top-right cell is supported by ≥2 independent benchmarks plus convergent vendor guidance — adopt without measurement. The middle-right cell is also worth adopting, but instrument so you can measure whether the published numbers carry over to FathomDB's specific stack.

†Split-confidence findings — placed at the higher rating because the _direction_ is HIGH-confidence even where the isolated effect-size attribution is MED:

- **F6** ACI: HIGH that tool-surface design matters; MED on the exact +10pp magnitude.
- **F8** subagents: HIGH on the shape (fan-out yes, shared-state edits no); MED on the exact 15× token cost trade-off.
- **F12** verification ordering: HIGH on lint→type→test order; MED on isolated effect attribution.

\*F14 prompt caching is single-track (tmp2 only) and is a cost/latency optimization, not an accuracy lift — placement reflects measured cost reduction, not measured task-success change.

---

## 4. Relation to FathomDB

FathomDB shape (verified 2026-05-01):

- Cargo workspace with 7 Rust crates under `src/rust/crates/` (`fathomdb`, `fathomdb-cli`, `fathomdb-engine`, `fathomdb-query`, `fathomdb-schema`, `fathomdb-embedder`, `fathomdb-embedder-api`).
- Python bindings under `src/python/`; TypeScript under `src/ts/`.
- 36 ADRs in `dev/adr/` covering 0.6.0 design decisions; one 0.8.0 ADR.
- Interface specs in `dev/interfaces/` (cli.md, python.md, rust.md, typescript.md, wire.md).
- `AGENTS.md` exists at root but is **empty**.
- `CLAUDE.md` does **not** exist.
- 0.6.0-rewrite branch is the active implementation branch; main has scaffolded structure, no production code yet.
- Workspace metadata declares `public_docs = ["docs"]`, `internal_docs = ["dev"]`.
- TDD already mandatory (`feedback_tdd.md`).
- Implementer/code-reviewer subagent split already encoded (`feedback_orchestrate_releases.md`).

### 4.1 Action map — best practice → FathomDB artifact

| #   | Practice                                                                                                                                                                                                                                                                                   | Artifact / location                                                                                                                                                                                                                                                                                                       | Status                            | Effort             |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- | ------------------ |
| 1   | **AGENTS.md** ≤300 lines at root (F7, F1, F2) — sources disagree (tmp ≤200, tmp2 ≤300); pick ≤300 for the root because it must carry build/test commands across three language surfaces, and explicitly link out (not inline) deeper docs. Per-scope files stay ≤100 lines (see action 4). | `AGENTS.md` (root)                                                                                                                                                                                                                                                                                                        | empty — fill                      | LOW                |
| 2   | Symlink `CLAUDE.md → AGENTS.md` (F7)                                                                                                                                                                                                                                                       | `CLAUDE.md`                                                                                                                                                                                                                                                                                                               | missing — create                  | LOW                |
| 3   | ADR index by path, not inline (F1, F4)                                                                                                                                                                                                                                                     | `dev/adr/ADR-0.6.0-decision-index.md` already exists; AGENTS.md should _link_ it, not inline ADR content                                                                                                                                                                                                                  | mostly in place                   | LOW                |
| 4   | Workspace-scoped instruction files (F1) — **deferred per finding F4 (stale > missing)**: create per-crate AGENTS.md only when the crate's first non-scaffold PR lands, not pre-emptively for stub crates.                                                                                  | `src/rust/crates/<crate>/AGENTS.md` (per crate, on first real PR)                                                                                                                                                                                                                                                         | deferred                          | LOW–MED, on demand |
| 5   | Tests-as-oracle, failing-test-first (F3, F4)                                                                                                                                                                                                                                               | Already mandated by `feedback_tdd.md`; needs: test files marked read-only during fix-to-spec; cap retry-budget at ~2 same-issue corrections then `/clear`                                                                                                                                                                 | partial                           | MED                |
| 6   | Property-based tests for codecs / projection invariants / round-trips (F10)                                                                                                                                                                                                                | Rust: `proptest`; Python: `hypothesis`. Add to test scaffolding now, before code lands.                                                                                                                                                                                                                                   | not yet                           | MED                |
| 7   | Structured diagnostics passthrough (F3, F6)                                                                                                                                                                                                                                                | `cargo --message-format=json`; `cargo clippy --message-format=json`; `pyright --outputjson`; `ruff --output-format=json`; `tsc --pretty false` — wired into agent dev-loop scripts under `scripts/`                                                                                                                       | not yet                           | MED                |
| 8   | Typed dev-loop verbs (F6)                                                                                                                                                                                                                                                                  | `scripts/agent-build.sh`, `scripts/agent-lint.sh`, `scripts/agent-test.sh`, `scripts/agent-typecheck.sh` — each: structured output, ~200-line cap with spill-to-file                                                                                                                                                      | not yet                           | MED                |
| 9   | Verification ordering lint → type → test (F12)                                                                                                                                                                                                                                             | `scripts/agent-verify.sh` runs the four in latency order, short-circuits on first failure                                                                                                                                                                                                                                 | not yet                           | LOW                |
| 10  | Iterative agentic retrieval; **no** local vector index (F5)                                                                                                                                                                                                                                | Default tools: grep, glob, read-with-line-range; `rust-analyzer` headless LSP; `pyright`; `tsserver`                                                                                                                                                                                                                      | partial — agents already use grep | LOW                |
| 11  | Tree-sitter / LSP-derived signature map per task (F1, F5)                                                                                                                                                                                                                                  | Optional `scripts/repo-map.sh` regenerates a ≤2k-token map of public symbols across the workspace                                                                                                                                                                                                                         | not yet                           | MED                |
| 12  | Subagent shape: implementer (worktree) + code-reviewer; **single-agent for edits** (F8)                                                                                                                                                                                                    | Already in `feedback_orchestrate_releases.md` and `feedback_orchestrator_thread.md`                                                                                                                                                                                                                                       | in place                          | —                  |
| 13  | 3-tier permission model + worktree per task + egress allowlist (F13)                                                                                                                                                                                                                       | `.claude/settings.json` permission tiers; worktree pattern already in use; egress: `cargo registry`, `pypi`, `npm`, github.com only                                                                                                                                                                                       | partial                           | MED                |
| 14  | Prompt-caching enablement on stable prefix (F14)                                                                                                                                                                                                                                           | Anthropic API: cache the system + AGENTS.md + tool defs prefix; track cache-hit rate. Applies to any internal Claude API integration FathomDB ships (e.g. embedder fallback).                                                                                                                                             | not yet                           | LOW                |
| 15  | Comments: why / invariants / hazards only — public-API docstrings carry weight (F11)                                                                                                                                                                                                       | Codify in AGENTS.md "comment policy" line. CI: clippy `missing_docs` for public APIs only.                                                                                                                                                                                                                                | not yet                           | LOW                |
| 16  | Front-load invariants in AGENTS.md, end-load task in prompt (F9)                                                                                                                                                                                                                           | Doc-style convention; encoded in AGENTS.md + agent prompt templates                                                                                                                                                                                                                                                       | not yet                           | LOW                |
| 17  | Externalize plan/decision/progress as files; treat chat as ephemeral (F2)                                                                                                                                                                                                                  | Existing pattern: `dev/adr/`, `dev/interfaces/`, `dev/tmp/` research notes. **Synthesis-author proposal (not directly cited; grounded in tmp2 T2's "SPEC.md + progress logs" pattern):** add `dev/progress/<release>.md` for multi-session work logs.                                                                     | mostly in place                   | LOW                |
| 18  | Stale-context discipline (F4)                                                                                                                                                                                                                                                              | Co-locate decision text with the code it governs; mark superseded ADRs with explicit "superseded by …" notes (already practiced — see the `ADR-0.6.0-vector-identity-embedder-owned.md` ↔ predecessor relationship). Add freshness check: any `dev/adr/` doc whose dependent code path no longer exists is flagged in CI. | partial                           | MED                |

### 4.2 Specific FathomDB risks the research flags

- **Rust diagnostics are best-in-class — pass them through unparaphrased.** _tmp2_ explicitly calls this out for the 0.6.0 rewrite. Any agent harness that shortens or summarizes `cargo` / `clippy` output before the model sees it is leaving a 4× lift on the table.
- **Property-based tests for the codec layer.** ADR-0.6.0-typed-write-boundary, ADR-0.6.0-prepared-write-shape, ADR-0.6.0-zerocopy-blob, ADR-0.6.0-recovery-rank-correlation all describe round-trip / invariant properties that are perfect targets for `proptest` / `hypothesis`. Pinning example-based tests here would give up the 23–37% pass@1 lift and the test-gaming protection.
- **Three language surfaces (Rust + Python + TypeScript) multiply the AGENTS.md complexity.** Per-crate / per-package scoped AGENTS.md is more important here than for a single-language repo. ADR-0.6.0-python-api-shape and ADR-0.6.0-typescript-api-shape define the surface contracts; link them, don't inline them.
- **0.6.0-rewrite is a clean canvas** — adopting these conventions before code lands costs near-zero. Retro-fitting them to a populated workspace is much more expensive (per the comments-track AGENTbench data on the cost of stale prose).

---

## 5. Conclusion — recommended FathomDB baseline

Adopt these practices **before** substantial 0.6.0 implementation lands. They are ordered by impact × effort; the first six are the highest leverage for the lowest effort. _Section 5 is a condensation of the 18-row Section 4.1 action map: items 11, 14, 15, 16 fold into items 1, 5, 1, 1 respectively below; item 4 (per-crate AGENTS.md) is intentionally deferred per F4._

1. **Fill `AGENTS.md`** at the root: ≤300 lines, bullet form (per F15), build/test commands across Rust/Python/TS, architecture entrypoints (link to `dev/adr/ADR-0.6.0-decision-index.md` and `dev/interfaces/` — do **not** inline ADR content), forbidden patterns (rooted in `feedback_*.md` items already in MEMORY), comment policy (public-API docstrings + why/invariants only), verification ordering, invariants front-loaded. Symlink `CLAUDE.md → AGENTS.md`. _(F1, F2, F4, F7, F9, F11, F15.)_
2. **Wire structured diagnostics + typed dev-loop verbs** under `scripts/`: `agent-build.sh`, `agent-lint.sh`, `agent-typecheck.sh`, `agent-test.sh`, plus a `agent-verify.sh` that runs them in lint → type → test order. Each emits structured JSON or capped text (~200-line cap with spill-to-file). _(F3, F6, F12.)_
3. **Mandate property-based tests** for codec, projection, recovery, and round-trip invariants. Add `proptest` (Rust) and `hypothesis` (Python) to the test scaffolding now, while there is no production code to retro-fit. _(F3, F10.)_
4. **Codify subagent rules** in AGENTS.md, mirroring `feedback_orchestrator_thread.md`: main thread is the orchestrator; spawn implementer (worktree) + code-reviewer; single-agent for any edit on shared state; subagents for fan-out search/review only. _(F8.)_
5. **Enable prompt caching** on any internal Anthropic API integration (e.g. an embedder using Claude). 1-hour TTL; track cache-hit rate. Document the cache-aware prefix shape in AGENTS.md so contributors don't break it accidentally. _(F14.)_
6. **Three-tier permission model + per-task worktree + egress allowlist** in `.claude/settings.json`. Restrict egress to crates.io, pypi.org, npmjs.com, github.com. _(F13.)_
7. **Tree-sitter / LSP signature map** generator under `scripts/repo-map.sh` — regenerated per task, ≤2k tokens, public-symbol summary. _(F1, F5.)_
8. **Plan / decision / progress externalization**: synthesis-author proposal — add `dev/progress/<release>.md` for multi-session work logs (grounded in tmp2 T2 SPEC.md pattern, not directly named in any track); `dev/adr/` already covers decisions; `dev/interfaces/` already covers contracts. _(F2.)_

**Deferred until code lands** (per F4 — stale > missing):

1. **Per-crate / per-package AGENTS.md** under `src/rust/crates/<crate>/` and `src/python/`, `src/ts/`. Each ≤100 lines, scope-specific. Create on the crate's first non-scaffold PR, not before. _(F1, F7.)_
2. **Stale-context CI check**: flag `dev/adr/` documents whose referenced symbols or paths no longer exist. Useful only once there are referenced symbols. _(F4.)_

### What we are _not_ doing (explicit non-actions)

- **No local vector index.** Anthropic and Sourcegraph have publicly pivoted off this; for a single typed repo, grep + LSP wins (F5, C1). Revisit only if cross-repo agent search becomes a goal.
- **No long-form per-method docstrings on internal helpers.** Public-API docstrings only. (F11, AGENTbench.)
- **No agent-driven multi-edit on shared state.** Single-agent for edits; subagents only at clean seams. (F8, C3.)
- **No reliance on long-context windows in lieu of curation.** Multi-fact recall at 1M is still ≈90% Claude / 60% Gemini with 1250× cost penalty. (F1, C2.)

---

## 6. Future Work

### 6.1 Open questions (no clean evidence yet, worth measuring once we have implementation)

- **LSP-on vs LSP-off task-success delta** for FathomDB-specific edits. tmp2 calls this out as a publicly under-measured area; FathomDB's three-language surface is a useful test environment.
- **Repo-map isolated contribution.** No public ablation isolates the map's win from agentic search + planning. Worth A/B-testing internally once the workspace is populated.
- **Plan-mode / progress-file quantitative effect.** Vendor guidance is concrete; benchmark numbers are absent. Easy to instrument.
- **Sandbox capability cost at task-success granularity.** Cursor's 84% prompt-reduction is measured; whether task-success regresses under tighter sandboxes is asserted-no-cost but unmeasured.
- **100-call tool-loop plateau.** Borrowed from BrowseComp (web search), not SWE-bench. The premature-termination shape clearly transfers; the specific number may not.

### 6.2 Things to revisit if the landscape moves

- **Long-context reliability ≥1M.** Current evidence says no, but if multi-fact recall crosses ~99% reliably, some curation discipline can relax.
- **Async subagent trees.** Currently IDE-bound; if they generalize to headless workflows, the "single-agent for edits" rule may need a refinement.
- **Per-edit semantic-diff vs textual-diff.** Aider unified-diff is the current SOTA; semantic-diff approaches are promising but not yet shipped.
- **Standardization of agent protocols (LSAP / Agent Client Protocol).** If one wins, FathomDB should align rather than building a bespoke harness.
- **Local vector retrieval if the repo balloons.** Reconsider F5/C1 once the workspace is large enough that grep is no longer fast enough — likely never for a single-product codebase, but worth a pre-registered threshold (e.g. >1M LoC).

### 6.3 Measurement plan

If we want to publish or internally validate any of this, the cheapest experiments are:

- A/B AGENTS.md present / absent on a fixed set of agent tasks; measure same-task pass@1.
- A/B structured-diagnostics on / off on a fixed set of cargo-error-driven repair tasks.
- A/B property-based vs example-based tests for the codec layer.
- Track prompt cache-hit rate over a release cycle.

These map directly to the four highest-confidence findings (F1/F2, F3/F6, F10, F14).

---

## Appendix A — Source crosswalk

| Best practice                 | tmp anchor                                   | tmp2 anchor       | tmp3 anchor   |
| ----------------------------- | -------------------------------------------- | ----------------- | ------------- |
| F1 Curate / context budget    | U1, source-code F4, AutoMCP 99.9% (line 148) | T1, Action 1      | F1            |
| F2 Durable state to disk      | U2                                           | T2                | F3            |
| F3 Executable feedback        | U3, tests                                    | T5, dev-env F2    | F2            |
| F4 Stale > missing            | U4                                           | T3                | F6            |
| F5 Iterative retrieval        | U5, source-code F2                           | C1, Action 5      | F4            |
| F6 ACI / tool design          | U6, dev-env                                  | T4, dev-env F2/F3 | F8            |
| F7 AGENTS.md standard         | implicit (CLAUDE.md ≤200)                    | T6, Action 1      | F3            |
| F8 Subagents at clean seams   | U7, other F10                                | C3, Action 7      | F9            |
| F9 Prompt position            | U8                                           | implicit T1       | implicit F1   |
| F10 Structured artifacts      | U3 (subset)                                  | T4                | F5            |
| F11 Comments = why/invariants | comments dim                                 | (no comments dim) | F7            |
| F12 Verification ordering     | dev-env / tests reconciliation               | Action 4          | F8 (impl)     |
| F13 3-tier perms / sandbox    | dev-env                                      | Action 4          | F8            |
| F14 Prompt caching\*          | (absent)                                     | Action 2          | (absent)      |
| F15 Prescriptive bullets      | comments dim (related: why-not-what)         | T4 (explicit)     | F5 (indirect) |

## Appendix B — Confidence rationale per finding

| #   | Confidence                                          | Why                                                                                                                     |
| --- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| F1  | HIGH                                                | 4+ independent anchors (lost-in-the-middle paper, Context-Rot, RepoCoder, Anthropic published)                          |
| F2  | HIGH                                                | All three tracks; two vendor postmortems (Cognition Devin, Anthropic compaction)                                        |
| F3  | HIGH                                                | RustAssistant + Engineering Agent + AgentCoder + SWT; all peer-reviewed or published with numbers                       |
| F4  | HIGH                                                | Test-gaming 76%→1% (impossible-SWEbench); recurrence across all three tracks; AGENTbench data                           |
| F5  | HIGH for "iterate"; MED for "no local vector index" | RepoCoder peer-reviewed; "no local index" is direction-correct but Cursor data argues hybrid still helps                |
| F6  | HIGH                                                | SWE-agent ACI paper + Tool Interface Design paper + Cursor sandbox + RustAssistant structured diagnostics               |
| F7  | HIGH on adoption; MED on per-task lift              | 60k+ OSS repos verified; no public ablation isolates AGENTS.md effect                                                   |
| F8  | HIGH on shape; MED on cost trade-off                | Anthropic published numbers + Cognition postmortem + AgentCoder + Aider; exact token cost varies                        |
| F9  | MED                                                 | Lost-in-the-middle is HIGH; isolated position-effect for code agents under-measured                                     |
| F10 | HIGH                                                | Hypothesis +23–37%, AST +2.67 SWE-bench, OpenAPI grounding measurable                                                   |
| F11 | MED                                                 | tmp + tmp3 only; small effect size; mostly cost-avoidance evidence                                                      |
| F12 | HIGH on order; MED on isolated effect               | Latency ordering is uncontested; clean ablation absent                                                                  |
| F13 | MED                                                 | Cursor vendor data on UX is solid; task-success effect untested in public                                               |
| F14 | MED                                                 | Anthropic-published; well-trusted but not peer-reviewed externally; _single-track inclusion exception_                  |
| F15 | MED                                                 | Two tracks (one explicit, one indirect); writing-style convention with strong direction agreement; no isolated ablation |
