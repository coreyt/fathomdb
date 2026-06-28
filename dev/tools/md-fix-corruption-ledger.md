---
title: markdownlint --fix corruption ledger
date: 2026-06-28
desc: Known markdown auto-fix corruption patterns, why they happen, and how to resolve them.
status: living
---

# markdownlint `--fix` corruption ledger

Markdown auto-fixers are supposed to change only formatting, never meaning. They don't always.
This ledger records the corruption patterns we have actually observed, so the AST guard
(`dev/tools/md_neutrality_guard.py`, wired into `scripts/md-safe-fix.sh` and the pre-commit hook)
catches them and an author can resolve the **source construct** by hand.

## Tooling decisions (0.8.9.1, HITL 2026-06-28)

- **prettier is NOT used on markdown.** Its emphasis formatter is non-configurable and its
  `*`->`_` reflow corrupts multi-line / nested / adjacent-to-`code` emphasis (broken spans,
  snake_case `_` loss, word-joins that change tokenization). It was removed from
  `scripts/agent-lint-md.sh` and from the `package.json` md scripts.
- **`markdownlint-cli2 --fix` is the sole auto-fixer.** It is AST/token-aware and safe on the
  overwhelming majority of files, but it still corrupts the few constructs below.
- **MD049/MD050 (emphasis/strong char) and MD060 (table pipe-style) are disabled** in
  `.markdownlint.jsonc` — cosmetic, zero semantic value, and MD049 was the corruption driver.
- Always fix markdown via **`scripts/md-safe-fix.sh`** (guards every fix with the AST check),
  never raw `markdownlint-cli2 --fix` or `prettier --write`.

## Corruption patterns observed + how to resolve

| # | Rule | Trigger construct | What --fix does (corruption) | Resolution (in source) |
|---|------|-------------------|------------------------------|------------------------|
| 1 | MD018 | A prose line that **starts with `#<digits>`** (e.g. `#11-full is...`, `#4 is...`) — a PR/issue ref, not a heading | Inserts a space (`# 11-full`), turning the line into an `<h1>`; MD026 then trims its trailing period | Escape the leading hash: `\#11-full`. Renders as literal `#11-full`, no longer a heading. |
| 2 | MD018 | A **multi-line inline code span** whose continuation line starts with `#` (e.g. a `uname -a` string `` `... #13~24.04...` ``) | Reads the `#` line as a heading and breaks the code span | Put the whole code span on **one line** (a soft line break inside `` `code` `` renders as a space, so joining is byte-equivalent). |
| 3 | MD037 | A **literal `*`** used as a wildcard in prose (e.g. `EU-*`), especially inside `**bold**` | Pairs the stray `*` as emphasis, collapses a space (`WRONG* —` -> `WRONG*—`) | Escape it: `EU-\*` (renders literal `*`). Also fixes any pre-existing mis-render that ate the asterisk. |
| 4 | MD034 | A **schemeless host** as a bare token (e.g. `www.cs.cmu.edu`, `qasper-dataset.s3`) | Wraps it in `<...>`, producing a broken autolink that renders the angle brackets literally | Backtick it (`` `www.cs.cmu.edu` ``) or give it a scheme (`<https://www.cs.cmu.edu>`). |

> The general defense: when an auto-fix wants to "fix" something that is actually a literal token
> (a ref, a wildcard, a host, a shell string), make that token **unambiguous to the parser** —
> escape the punctuation, or wrap it in a code span. Never accept a meaning change to clear a lint.

## How the guard uses this

`scripts/md-safe-fix.sh` snapshots each target file, runs `markdownlint-cli2 --fix`, and for every
changed file runs `md_neutrality_guard.py diff`. If meaning changed it **reverts** the file and
points here. The pre-commit hook runs the same path on staged `*.md`, so a corrupting fix can never
silently land. When you hit a NEW pattern, resolve the source construct and **add a row above**.
