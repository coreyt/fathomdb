# Mermaid renderer

Renders [Mermaid](https://mermaid.js.org/) diagrams (`.mmd`, or ```` ```mermaid ````
blocks in Markdown) to **SVG / PNG / PDF**. It's a thin, agent-friendly wrapper
around [`@mermaid-js/mermaid-cli`](https://github.com/mermaid-js/mermaid-cli)
(`mmdc`) that stays quiet, checks its own dependencies cheaply, and never
installs a ~170MB browser behind your back.

Entry point: **`dev/tools/mermaid/render-mermaid.sh`**

---

## For an agent

Follow this order. It is designed so you spend almost no tokens unless a render
is actually requested, and so you never trigger a large download without human
approval.

### 1. Check (cheap, no download, no browser launch)

```bash
dev/tools/mermaid/render-mermaid.sh check
```

The answer is the **exit code** — `check` prints nothing on stdout, so you
branch on `$?` without reading any output (the same exit-code-as-signal contract
as `dev/agent-tools/ledgerwatch`):

- exit `0` → dependencies present, go straight to render.
- exit `69` (`EX_UNAVAILABLE`) → not installed; a one-line notice is on stderr.
  Do **not** install unattended. Go to step 2.

```bash
if dev/tools/mermaid/render-mermaid.sh check; then :; else
  # exit 69 → ask the user, then run `install`
fi
```

This check only inspects the filesystem (binary present + Chromium resolvable).
It does not launch Chromium or hit the network, so it's safe to call freely.

### 2. Install — only after asking the human

A missing-deps state means a **~170MB Chromium download**. Surface that to the
user and get explicit approval (e.g. via an AskUserQuestion / HITL prompt).
Only then:

```bash
dev/tools/mermaid/render-mermaid.sh install     # progress + result on stderr; exit 0 = ok
```

The render command will itself refuse to run when deps are missing: it exits
`69` (`EX_UNAVAILABLE`) with the same instruction rather than auto-installing.
Treat exit `69` as "stop and ask", never as "install for me".

### 3. Render

```bash
# Inferred output name (-> diagram.svg):
dev/tools/mermaid/render-mermaid.sh diagram.mmd

# Explicit output + options:
dev/tools/mermaid/render-mermaid.sh -i diagram.mmd -o diagram.png -t dark -b transparent

# Rewrite ```mermaid blocks inside a Markdown file in place:
dev/tools/mermaid/render-mermaid.sh -i notes.md -o notes.md

# Batch: every *.mmd under a dir -> sibling *.svg:
dev/tools/mermaid/render-mermaid.sh --all dev/design
```

Success is quiet, and stdout is **payload only**: the resolved output path (one
per line) for inferred/batch renders — where you didn't specify it — and
*nothing* for explicit-`-o` renders, since you already know the path. The exit
code is the success signal either way. Extra flags pass straight through to
`mmdc` (`-t/--theme`, `-b/--backgroundColor`, `-w`, `-H`, `-s/--scale`,
`-f/--pdfFit`; run `mmdc --help` in `node_modules/.bin` for the full list).

### Exit codes (BSD `sysexits.h`)

Standard codes, so you branch on `$?` alone — errors go to stderr, never stdout.

| code | name | meaning | agent action |
|------|------|---------|--------------|
| `0`  | `EX_OK` | success | continue |
| `64` | `EX_USAGE` | bad invocation / no `*.mmd` found / nothing to do | fix the invocation |
| `66` | `EX_NOINPUT` | input `.mmd`/`.md` does not exist | fix the path |
| `69` | `EX_UNAVAILABLE` | dependencies not installed | **ask the user**, then run `install` |
| `70` | `EX_SOFTWARE` | `mmdc`/render failed (bad diagram, write error) | read stderr, fix diagram/flags |

---

## For a human

### What it is

A repo-local Mermaid → image renderer for diagrams in our docs. You give it a
`.mmd` file (or a Markdown file containing ```` ```mermaid ```` blocks) and it
produces an SVG, PNG, or PDF. It bundles its own headless Chromium via
Puppeteer — nothing needs to be installed system-wide.

### How an agent uses it

The wrapper encodes the workflow an agent should follow so it behaves well
unattended:

1. **`check`** — a zero-cost probe of whether the tooling is installed. The agent
   calls this first so it doesn't waste tokens or accidentally start a download.
2. **`install`** — the agent is instructed (here and by the script's own exit-`3`
   message) to get your approval before running this, because it pulls ~170MB.
3. **`render`** — quiet on success so the agent's context stays clean.

In short: an agent checks, asks you before the big download, then renders.

### Manual setup

```bash
cd dev/tools/mermaid
npm install        # or: ./render-mermaid.sh install
./render-mermaid.sh check && echo ready   # check is silent; exit 0 = ready
./render-mermaid.sh diagram.mmd
```

### Why the wrapper exists (the `--no-sandbox` bit)

`mmdc` drives a headless Chromium. On hosts where unprivileged user namespaces
are restricted (Ubuntu 23.10+ with AppArmor — including this dev box), Chromium
refuses to start:

```text
FATAL … No usable sandbox!
```

`puppeteer.json` supplies `--no-sandbox --disable-setuid-sandbox` and the wrapper
always passes it, so you never have to remember. Dropping the sandbox is
acceptable here because the input is trusted local `.mmd` / `.md` files, not
untrusted web content.

---

## Specification & tests

Behavior is specified as requirements + acceptance criteria in
[`REQ-AC.md`](REQ-AC.md) (happy paths, edge cases / failure modes, and the
agentic I/O contract). Each AC has a matching test in `test-render-mermaid.sh`:

```bash
cd dev/tools/mermaid
./render-mermaid.sh install     # deps must be present — tests render for real
./test-render-mermaid.sh        # exit 0 = all ACs green
```

The suite drives real `mmdc` + Chromium renders and reversibly simulates a
deps-missing checkout, so it also proves the "never auto-install" guarantee.

## Files

| file | purpose |
|------|---------|
| `render-mermaid.sh` | the wrapper: `check` / `install` / render, quiet, no auto-install |
| `REQ-AC.md` | requirements + acceptance criteria (the spec the tests assert) |
| `test-render-mermaid.sh` | acceptance test suite, one test per AC |
| `puppeteer.json`    | Chromium launch flags (`--no-sandbox`) |
| `package.json`      | pins `@mermaid-js/mermaid-cli` |
| `package-lock.json` | locked dependency tree |
| `node_modules/`     | git-ignored; created by `install` |
