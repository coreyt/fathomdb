---
title: STATUS — 0.8.9.1 markdown-debt cleanup + anti-regression
date: 2026-06-28
desc: Closing status + sign-off ledger for the transitory 0.8.9.1 markdown-hygiene work.
status: complete
---

# 0.8.9.1 — markdown-debt cleanup + anti-regression — CLOSING STATUS

## Outcome (the single success fact)

`bash scripts/agent-lint-md.sh` **exits 0** — `markdownlint-cli2` (repo) clean, `docs/**` lint
clean, `lychee --offline` 0 errors. Markdown debt went **8,054 → 0 findings** across ~250 files
with **zero meaning changes** (verified by a real CommonMark parser), and every markdown
generator + the docs flow now emit gate-compliant output behind guards.

This session did **both**: it built the tooling/infra AND fixed the files.

## Slices + commits (branch `prompt-0891-handoff`, base `e766f477`)

| Slice | What | Commit |
|-------|------|--------|
| 5 | bulk fix 8054→155 via markdownlint-only (no prettier); disable cosmetic rules; AST-verified neutral | `bbc44e23` |
| 10 | residual tail → 0 (MD040 fence-languages ×134 + MD028/046/025/001) via 4 parallel subagents | `99666c7b` |
| 20 | AST-guarded `md-safe-fix` + pre-commit guard + corruption ledger | `cc5b47d4` |
| — | self-healing `install-hooks.sh` so the guard actually activates | `4a196386` |
| 15 | generators emit compliant markdown + anti-regression guards + `docs/**` lint | `5543694b` |
| 40 | lychee config compat + `dev/archive` exclude → link gate green | `54400a57` |
| 40 | §9 review P2 fixes (guard covers HTML blocks; md-safe-fix never leaks non-target edits) | `56148f98` |

## Key decisions (HITL-approved 2026-06-28)

The charter's "push everything through prettier" path **cannot be made non-corrupting** — prettier's
non-configurable `*`→`_` emphasis reflow mangles multi-line / nested / adjacent-to-`code` emphasis
(broken spans, snake_case `_` loss, word-joins that change tokenization). Evidence: prettier
corrupted ~38 emphasis-dense docs; markdownlint-cli2 `--fix` (AST/token-aware) corrupted only 5
(narrow `#`/`*`/host constructs, all hand-fixed).

- **Tooling:** prettier REMOVED from the markdown gate (`scripts/agent-lint-md.sh`, `package.json`).
  Gate = `markdownlint-cli2` + `docs/**` lint + `lychee`. `markdownlint-cli2 --fix` is the sole
  auto-fixer, always run via the AST-guarded `scripts/md-safe-fix.sh`.
- **Rules:** MD049/MD050 (emphasis char) + MD060 (table pipe-style) DISABLED in `.markdownlint.jsonc`
  with §6 justification comments — cosmetic, zero semantic value, ~5,170 findings (64%), and the
  emphasis-char rule was the corruption driver. Both `*` and `_` now accepted.

## Neutrality method + result

`dev/tools/md_neutrality_guard.py` (markdown-it-py CommonMark AST) compares the tokenizer-stable
visible-text + inline-code + link-target + fenced-code + HTML-block streams before↔after. It was
chosen over a hand-rolled regex after the regex masked a real `baseline_sha`→`baselinesha`
corruption that the AST caught. **Result: 0 meaning changes** across all auto-fixed files; the only
guard-flags are 4 intentional hand-fixes (escaping a literal `EU-\*` that fixes a pre-existing
broken render; backticking bare hosts; two deliberate prose rewrites).

## §9 review (codex out of budget → independent Claude reviewer)

**No P1 (blocking).** Reformat verified meaning-neutral across 245+ docs; generators preserve data
(incl. `m1_verdict_run.py` heading-in-blockquote → bold paragraph, words identical). Three P2s:
- HTML-block guard gap → **FIXED** (`56148f98`).
- md-safe-fix repo-wide `--fix` left non-target files unguarded → **FIXED** (`56148f98`).
- "gate not in CI / silent skip" → the markdown gate **is** CI-wired (ci.yml → `agent-verify.sh`
  → `agent-lint.sh:45` → `agent-lint-md.sh`); the skip-on-missing-binary is the repo's existing
  convention (bootstrap installs the binaries). Noted, no change.

Documented guard limitations (low-risk for markdownlint `--fix`, would false-flag this session's
own legitimate fixes if folded in): heading LEVEL and fence INFO-STRING are treated as formatting.

## Sign-off ledger

- Version bump: **NONE** (0.8.9.1 is a label, not a version — all four manifests stay `0.8.0`;
  `set-version.sh` never run). No `v*` tag, no publish.
- F-7 reconciliation: **HITL-blessed + landed via #110** (`e766f477`); orchestrator did **not**
  re-edit F-7 or the §4 row (confirmed present on base).
- Semantic-neutrality guard: **PASS** (AST, 0 meaning changes).
- Generator guard + `docs/**` guard: **wired** (`scripts/tests/test_md_generators.sh`,
  `src/python/tests/test_md_generator_hygiene.py`, `scripts/agent-lint-docs.sh`).
- Gate: `scripts/agent-lint-md.sh` **exits 0**; `mkdocs build --strict` green.
- `.claude/**` carve-out (charter §5/§8): the local skill/workflow generators
  (`fathomdb-capability-status-report`, `fathomdb-v05-feature-triage`, `deep-research`) are
  local-only tooling, **out of scope for this repo branch** — handed off to the local skill owner
  to make gate-compliant; their output is caught by the §4b guard when it lands in a maintained path.
- `dev/DOC-INDEX.md` / board: unchanged by this cleanup (formatting-only; no docs added/removed/retitled).

## Token / $ ledger

- Priced-API spend: **$0.00** — no LLM eval/judge runs; the only spend is orchestration +
  subagent tokens (well within the $40 session budget).
- Subagents used (billed tokens, from transcripts): Slice 10 fence batches ≈ 57k / 63k / 77k / 61k;
  Slice 15 generators ≈ 135k; §9 review ≈ 67k. Plus 4 parallel + 1 + 1 spawns.
- Models: orchestration on Opus; mechanical/investigative work delegated to Sonnet implementers.

## Follow-ups (not blocking the gate)

1. **Activate the pre-commit guard locally:** the installed `.git/hooks/pre-commit` is a stale
   pre-sentinel copy. After this merges to main, run `scripts/install-hooks.sh --force` **once**
   from the main checkout (then refreshes are automatic). Until then the guard is committed but
   dormant locally; CI enforces the gate regardless.
2. **Rebase before merge:** `origin/main` advanced (PR #113 re-planned the version ladders) after
   this branch's base. Only overlapping file is `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`
   (this branch changed only MD026 punctuation there); rebase onto current `origin/main` and
   re-run the guard + markdownlint before merge.
3. **lychee version drift:** `bootstrap.sh` installs unpinned `cargo install lychee`; the config is
   now compatible with current (0.24.x). Consider pinning a lychee version in bootstrap to prevent
   future schema drift.
4. `.claude/**` skill generators → local skill owner (above).
