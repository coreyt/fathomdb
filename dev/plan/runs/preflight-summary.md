# Phase 9 Pack 5 — Pre-flight summary

UTC date: 2026-05-03. Branch: `0.6.0-rewrite`. HEAD at preflight: `da9ae05`
(four docs commits landed on top of `b4a3261` baseline noted in handoff §3
during/before this session; all four are docs-only — Pack 5 plan, whitepaper
notes, markdownlint, prettier — and do not affect production code).

Tools:

- `claude` 2.1.126 at `/home/coreyt/.local/bin/claude`
- `codex-cli` 0.128.0 at `/home/coreyt/.nvm/versions/node/v24.15.0/bin/codex`

Plan §0.2 mandates: do not proceed past pre-flight on partial PASS. All seven
checks below recorded PASS (one with documented limitation; see check 2).

---

## Check 1 — model pin (claude)

Command shape:

```bash
echo "Identify your model id. Print only the model identifier on a single line, nothing else." \
  | claude -p --model <MODEL> --output-format json --tools ""
```

| Sub | Model             | Log                                            | Result              | Verdict |
| --- | ----------------- | ---------------------------------------------- | ------------------- | ------- |
| 1a  | claude-sonnet-4-6 | `preflight-check1-sonnet-20260503T201931Z.log` | `claude-sonnet-4-6` | PASS    |
| 1b  | claude-opus-4-7   | `preflight-check1-opus-20260503T201945Z.log`   | `claude-opus-4-7`   | PASS    |

Both runs report matching `model` in `modelUsage`; result string equals the
pinned model id verbatim. Exit 0.

Notes:

- `--bare` rejected because nested `claude -p` lacks keychain auth path; drop
  it for headless spawns.
- `--tools ""` is variadic and consumes the prompt positional; pass prompt via
  stdin instead.

---

## Check 2 — effort tag (claude)

Command shape:

```bash
echo "Respond with literal token: OK" \
  | claude -p --model claude-opus-4-7 --effort xhigh --output-format json --tools ""
```

| Sub | What                            | Log                                                 | Result                                         | Verdict |
| --- | ------------------------------- | --------------------------------------------------- | ---------------------------------------------- | ------- |
| 2a  | `/effort xhigh` slash in prompt | `preflight-check2-effort-20260503T202048Z.log`      | `/effort isn't available in this environment.` | N/A     |
| 2b  | `--effort xhigh` CLI flag       | `preflight-check2-effort-flag-20260503T202058Z.log` | Exit 0, `result=OK`                            | PASS    |

Limitation recorded: headless `claude -p` accepts `--effort <low|medium|high|xhigh|max>`
but the JSON result envelope does not surface effort level back. Behavioral
effect on the model is not directly observable from the run log.

Decision per plan §0.2 instruction: downgrade §0 effort tags to **intent only**.
Subagent prompts will pass `--effort` on the CLI; we cannot assert from the
JSON that the model honored it. Slash-form (`/effort` in body) is invalid in
headless `-p` and must not be used in pre-written prompt files.

---

## Check 3 — input delivery (claude)

Built `/tmp/preflight-input-20260503T202131Z.md` (20085 bytes; 200 markdown
bullets + marker line + 80 paragraphs). Piped via stdin behind a leading
prompt and `---` separator. Asked for verbatim marker echo.

Log: `preflight-check3-input-20260503T202131Z.log`. Exit 0.
Result: `MARKER_TOKEN: 7af3b1c9-input-delivery-ok` (verbatim).

PASS — stdin handles >4KB Markdown intact.

---

## Check 4 — instruction-following (claude)

Command:

```bash
claude -p --model claude-sonnet-4-6 --add-dir /tmp \
  --allowedTools Write --permission-mode bypassPermissions \
  --output-format json
```

Prompt (via stdin) instructed write of literal `FOO\n` to
`/tmp/preflight-20260503T202204Z.txt` and forbade touching anything else.

Log: `preflight-check4-instruct-20260503T202204Z.log`. Exit 0.

- Target file exists, content `FOO\n`, 4 bytes.
- Sentinel scratch dir `/tmp/preflight-scratch-20260503T202204Z` unchanged.
- `permission_denials` empty.

PASS. Note: `--cwd` mentioned in plan §0.1 does not exist on `claude`; use
`--add-dir <path>` (and start the spawning shell at the desired worktree)
instead. Plan amendment noted; will fold into Phase A.0 prompt Update log.

---

## Check 5 — output capture (claude)

Two probes:

| Sub | What                           | Log                                                                                             | Result                                                 | Verdict |
| --- | ------------------------------ | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------ | ------- |
| 5a  | stdout / stderr split routing  | `preflight-check5-stdout-20260503T202237Z.log` + `preflight-check5-stderr-20260503T202237Z.log` | stdout=10 bytes (`STDOUT_OK`), stderr=0 bytes, exit 0  | PASS    |
| 5b  | bad-flag stderr + nonzero exit | `preflight-check5-badflag-20260503T202237Z.log`                                                 | `error: unknown option '--no-such-flag-xyzzy'`, exit 1 | PASS    |

PASS — stdout / stderr / exit-code all reach separate destinations parseable
either as JSON (when `--output-format json`) or raw text.

---

## Check 6 — worktree isolation (claude)

```bash
git worktree add /tmp/preflight-wt-<ts> -b preflight-tmp-<ts>
( cd /tmp/preflight-wt-<ts> && echo "..." | claude -p --model claude-sonnet-4-6 \
    --output-format text --tools "" > <log> 2>&1 )
git worktree remove --force /tmp/preflight-wt-<ts>
git branch -D preflight-tmp-<ts>
```

Log: `preflight-check6-worktree-20260503T202354Z.log` +
`preflight-check6-claude-20260503T202354Z.log`.

- Parent worktree `git status --porcelain` sha unchanged before/after.
- Parent `HEAD` unchanged (`da9ae05`).
- claude inside worktree exit 0; result `WORKTREE_OK`.
- Throwaway worktree + branch removed cleanly.

PASS. First attempt used a relative log path that broke once we `cd` into the
worktree; second attempt used an absolute log path. Future spawns must pass
absolute paths for any cross-worktree artifact.

---

## Check 7 — codex parallel

Codex behavior notes:

- `codex exec` reads stdin even when prompt provided as positional arg; pass
  `< /dev/null` to avoid blocking when no input is intended.
- Default model: `gpt-5.4` (per `~/.codex/config.toml`). `gpt-5` flag is
  rejected on a ChatGPT account (`The 'gpt-5' model is not supported when
using Codex with a ChatGPT account.`). Planned reviewer model: **gpt-5.4**.
- Reasoning-effort flag observed name: **`-c model_reasoning_effort=<low|medium|high>`**.
  No `--reasoning-effort` short flag exists.

| Sub | What                                            | Log                                                             | Result                                                          | Verdict |
| --- | ----------------------------------------------- | --------------------------------------------------------------- | --------------------------------------------------------------- | ------- |
| 7a  | smoke (default model, prompt-only)              | `preflight-check7a-codex-smoke-20260503T205240Z.log`            | `agent_message: CODEX_OK`                                       | PASS    |
| 7b  | model pin gpt-5 → expected fail                 | `preflight-check7-codex-model-20260503T205300Z.log`             | 400 invalid_request_error (documented; use gpt-5.4)             | NOTE    |
| 7c  | input >4KB + reasoning-effort low               | `preflight-check7-codex-input-effort-low-20260503T205354Z.log`  | 17838-byte fixture; marker echoed; `reasoning_output_tokens=11` | PASS    |
| 7d  | input >4KB + reasoning-effort high              | `preflight-check7-codex-input-effort-high-20260503T205409Z.log` | same fixture-shape; marker echoed; `reasoning_output_tokens=22` | PASS    |
| 7e  | output capture (JSONL stream → log + exit code) | implicit across 7a/7c/7d                                        | parseable JSONL + correct exit code                             | PASS    |
| 7f  | instruction-following (string discipline)       | implicit via 7c/7d                                              | agent*message contains \_only* the marker line                  | PASS    |
| 7g  | instruction-following (file write)              | not run                                                         | reviewer role is read-only per §0.1; deliberate skip            | N/A     |

Reasoning-effort flag effect: reasoning_output_tokens roughly doubled
(11 → 22) at otherwise-fixed prompt + input. Flag is honored by the CLI and
visibly affects the model's reasoning budget.

---

## Plan amendments produced by pre-flight

These deltas must be carried into the per-phase prompt files when they are
pre-written (next step):

1. **claude headless**: prompt body via stdin, not positional, when
   `--tools ""` (or any variadic) is also in the command.
2. **No `--bare`** for nested claude spawns — keychain-only OAuth path
   requires the standard path.
3. **No `--cwd`** on claude; use `--add-dir <path>` and shell-side
   `cd` to the worktree.
4. **Effort tags are intent-only** in headless `-p`; CLI accepts the flag
   but JSON envelope does not surface it. Do not embed `/effort …` slash
   commands in headless prompt bodies.
5. **Codex reviewer model** is `gpt-5.4`, not `gpt-5`. Update §0.1
   prompt-file table reviewer column to `gpt-5.4`.
6. **Codex stdin**: always close with `< /dev/null` unless deliberately
   piping a content block.
7. **Codex reasoning-effort flag name**: `-c model_reasoning_effort=high`.
   Flag is honored — observed reasoning-token roughly doubles low→high.
8. **Cross-worktree artifact paths**: pass absolute paths whenever a
   subagent will `cd` into a worktree.

---

## Verdict

**ALL CHECKS PASS** (check 2 with documented limitation; check 7g
deliberately skipped because reviewer role is read-only). Plan §0.2
exit gate satisfied; safe to proceed to §10 step 1 (pre-write all
phase prompt files).
