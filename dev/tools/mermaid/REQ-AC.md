# Mermaid renderer — Requirements & Acceptance Criteria

Scope: `dev/tools/mermaid/render-mermaid.sh`. A small, agent-friendly wrapper
around `mmdc`. Requirements are few; each acceptance criterion (AC) is written so
a test can assert it mechanically. ACs are grouped into **happy paths**, **edge
cases / failure modes**, and the **agentic I/O contract**.

Tests live in `test-render-mermaid.sh`; every AC below has a matching test id.

---

## R1 — Render Mermaid to images (happy paths)

The tool turns a valid Mermaid source into an image, in the common shapes a
consumer needs.

| AC | Given / When | Then |
|----|--------------|------|
| **AC1.1** | a valid `.mmd` and `-o out.svg` | an SVG is written (content begins `<svg`); exit `0` |
| **AC1.2** | `-o out.png` | a PNG is written (PNG magic bytes); exit `0` |
| **AC1.3** | `-o out.pdf` | a PDF is written (PDF magic bytes); exit `0` |
| **AC1.4** | a single bare arg `diagram.mmd` (no `-o`) | sibling `diagram.svg` is created **and its path is printed on stdout**; exit `0` |
| **AC1.5** | a Markdown file containing a ` ```mermaid ` block, `-o out.svg` | the block is extracted and an SVG artefact is produced; exit `0` |
| **AC1.6** | `--all <dir>` over a dir with two `*.mmd` files | each renders to a sibling `*.svg`, **each path printed on stdout**; exit `0` |
| **AC1.7** | a passthrough style flag (`-t dark`) | the render still succeeds; exit `0` |

## R2 — Input handling & failure modes (edge cases)

Bad or awkward input fails **cleanly and distinguishably** — a precise exit code,
a message on stderr, and no half-written artefacts.

| AC | Given / When | Then |
|----|--------------|------|
| **AC2.1** | `-i` points at a file that does not exist | exit `66` (`EX_NOINPUT`); stderr names the missing file; no output file created |
| **AC2.2** | input is not valid Mermaid | exit `70` (`EX_SOFTWARE`); stderr non-empty; **no stale/zero-byte output file left behind** |
| **AC2.3** | `--all <dir>` where `<dir>` has no `*.mmd` | exit `64` (`EX_USAGE`); nothing on stdout |
| **AC2.4** | no command / empty args | exit `64` (`EX_USAGE`) |
| **AC2.5** | `-o` names a file in a directory that does not exist | the parent directory is created and the render succeeds (exit `0`); if the dir cannot be created, exit `73` (`EX_CANTCREAT`) — never a bare `mmdc` crash |
| **AC2.6** | an input path containing a space (bare-arg infer) | handled correctly: sibling `.svg` created and its path printed; exit `0` |

## R3 — Dependency safety & human-in-the-loop (HITL)

Dependencies are a ~170MB Chromium download. The tool must be able to report its
status cheaply and must **never** install unattended.

| AC | Given / When | Then |
|----|--------------|------|
| **AC3.1** | deps installed; `check` | exit `0`; stdout empty (answer is the exit code) |
| **AC3.2** | deps missing; `check` | exit `69` (`EX_UNAVAILABLE`); a terse HITL notice on stderr; stdout empty |
| **AC3.3** | deps missing; a render is requested | exit `69`; **no install is performed** (the `mmdc` binary is still absent afterward) — the tool stops and defers to the human |

## R4 — Agentic I/O contract (token efficiency)

The load-bearing signal is the exit code (BSD `sysexits.h`), mirroring
`dev/agent-tools/ledgerwatch`. stdout is payload only; stderr is advisory only.

| AC | Given / When | Then |
|----|--------------|------|
| **AC4.1** | any success | exit `0` and **nothing but payload** on stdout |
| **AC4.2** | an explicit `-o` render succeeds | stdout is empty (the caller already knows the path); the signal is the exit code |
| **AC4.3** | any failure (AC2.x) | stdout is empty; the message is on stderr; the exit code is the documented `sysexits` value |
| **AC4.4** | `check` (either state) | stdout is empty in both states — a caller branches on `$?` without capturing output |

---

### Exit-code vocabulary (BSD `sysexits.h`)

`0` `EX_OK` · `64` `EX_USAGE` · `66` `EX_NOINPUT` · `69` `EX_UNAVAILABLE` ·
`70` `EX_SOFTWARE` · `73` `EX_CANTCREAT`.
