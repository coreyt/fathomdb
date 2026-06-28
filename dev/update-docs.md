<!--
  DOCS EPOCH MARKER — update this line at the END of every successful run.
  Docs current as of: 3a09571  (2026-06-14)  [main — initial epoch stamp; established when dev/update-docs.md was authored. Not yet verified against a tree; the first real run sets a verified SHA.]
  (SHA of the commit whose tree the documentation was last verified against.)
-->

# Prompt: Update Repository Documentation Since a Docs Epoch

You are updating the documentation of **FathomDB** so it once again matches the
code. You work from a **documentation epoch** — a git commit that marks "the docs
were accurate as of this tree" — diff the completed work since that epoch, and
bring every affected document up to date. This file is the operational prompt;
read it fully before acting.

> **When to use this prompt.** Per-slice work already maintains docs in its closing
> commit (the DOC-INDEX rule below). Use *this* prompt for the cases that rule does
> not catch: **out-of-band landings** (e.g. the owner-managed corpus line integrated
> at a push), **drift recovery** after a stretch of fast iteration, **release-eve
> sweeps** before a GA gate, or any time you suspect `dev/`↔`docs/`↔code have
> diverged. It is a whole-tree reconciliation, not a substitute for the per-slice
> discipline.

> **Scope guardrail.** This is a documentation task. **Do not modify any source
> code under `src/`.** **Test files are read-only here** (per `AGENTS.md` §5). If the
> diff reveals a code bug or a code/doc contradiction, record it in the owning
> `dev/design/*` doc's "Known limitations", in `dev/learnings.md`, and on the live
> status board (`dev/plans/runs/STATUS-0.8.x.md`); **do not fix it**. Surface it —
> per `AGENTS.md` §1, a wrong doc is worse than its absence, but silently patching
> code from a docs pass is worse than both.

> **The locked-acceptance guardrail (FathomDB-specific).** `dev/acceptance.md` is
> `status:locked` and `dev/requirements.md` IDs are a contract. **Never mint, renumber,
> or withdraw REQ-*/AC-* IDs from this prompt.** New ACs are minted only at gated
> slices (e.g. 25, 40) by the campaign, not by a doc-sync pass. If completed work
> implies a genuinely new requirement or acceptance behaviour, **record the gap** in
> `dev/learnings.md` + the status board and flag it in your summary — do not invent an
> ID. You may re-point existing trace rows in `dev/traceability.md` to reflect ACs that
> the campaign has *already* minted.

---

## 0. Inputs

| Input | How to obtain | Default |
|-------|---------------|---------|
| `$EPOCH` | The "docs current as of" commit. Read the **DOCS EPOCH MARKER** at the top of this file. If a caller supplied a SHA/tag/`git describe`, use that instead. | the SHA in the marker |
| `$HEAD` | The commit to update docs *to*. | `HEAD` |
| `$SCOPE` | Optional path/subsystem filter (e.g. "only the read-path", "only the 0.8.1 graph track"). | whole repo |

If the marker SHA is unreachable (history rewritten), fall back to the most recent
commit whose subject starts with `docs(`/`docs:`, and note the substitution in your
summary.

---

## 1. Documentation structure (the target layout)

Keep documents in these locations and roles. **`dev/` engineering docs are the
build-time source of truth; `docs/` user-facing pages are derived from them.** The
authoritative map of every doc — path · purpose · owning slice/AC · last-touched —
is **`dev/DOC-INDEX.md`**. Read it first; it is your cold-start index and the thing
you must leave accurate.

```
dev/   (engineering docs — source of truth; NOT shipped in the wheel)
├── DOC-INDEX.md           THE agentic doc map — one row per doc; YOU keep it accurate
├── update-docs.md         THIS prompt + the docs-epoch marker
├── README.md              entry map for the dev/ tree (update when files added/moved)
├── needs.md               product/consumer needs (NEED-*)
├── requirements.md        REQ-* — contract; never renumber
├── acceptance.md          AC-* — status:LOCKED; never mint/renumber here
├── traceability.md        REQ ↔ AC ↔ test matrix (re-point only; flag orphans)
├── interfaces/            PUBLIC SURFACE CONTRACT (AGENTS.md §1)
│   ├── rust.md            Rust-visible symbol spelling + governed-facade surface
│   └── cli.md             CLI flag spelling, root paths, exit-code classes, --json
├── architecture.md        engine / projections / reader-pool / surface map
├── test-plan.md           test strategy + tiers
├── security-review.md     SR-*
├── learnings.md           cross-phase learnings + the gap/bug log this prompt writes to
├── adr/                   architecture decision records (authoritative; propose successors, don't edit accepted)
│   ├── ADR-0.6.0-decision-index.md   the ADR index
│   └── ADR-<rel>-<slug>.md
├── design/                design notes + ADR-adjacent specs (per-slice memos)
│   └── README.md          design-notes index
├── plans/                 plans + live state
│   ├── 0.8.x-implementation.md   authoritative slice contracts
│   ├── 0.8.x-plan.md             mod-5 ladder + Immediate-Next-Slice pointer
│   ├── runs/STATUS-0.8.x.md      LIVE state board (record gaps/bugs here)
│   └── prompts/                  per-slice + orchestrator prompts
├── progress/              compaction-safe persistence ledger
├── releases/             internal release records (engineering companion to user notes)
├── roadmap/              forward-looking release direction (revisable)
└── archive/              historical/superseded material (banner + manifest)

docs/   (user-facing; mkdocs — nav in tools/docs/mkdocs.yml; plain language; link to dev/ for depth)
├── index.md                       docs home + map
├── getting-started/               overview + quickstart (five-operation contract)
├── install/{python,typescript,rust}.md
├── reference/
│   ├── python-api.md   MUST match the Python SDK surface
│   ├── typescript-api.md MUST match the TS SDK surface (parity with Python)
│   ├── cli.md          MUST match dev/interfaces/cli.md + the CLI --help
│   ├── errors.md       error taxonomy
│   └── config.md
├── concepts/ · guides/ · operations/ · positions/ · compatibility/
└── release-notes/<rel>.md         user-facing release notes
```

### Per-slice design-memo shape (`dev/design/slice-*-design.md`)

Objective · approach + exact SQL/migration step (with `SCHEMA_VERSION` deltas where
relevant) · the gap(s) it closes · SDK shapes (Python + TypeScript parity) · test
plan · open issues. Re-verify every `file:line`, symbol, default, and `SCHEMA_VERSION`
against the live tree at `$HEAD`.

### Interface-doc shape (`dev/interfaces/*.md`) — CONTRACT

Concrete symbol/flag spelling · governed-surface allowlist + parity statement ·
exit-code classes (CLI) · what is feature-gated (e.g. the `operator` cargo feature).
A change to public surface needs an ADR or an interface-doc update **in the same PR**
(`AGENTS.md` §1) — if the diff changed surface without one, that is a gap to flag.

---

## 2. Procedure (follow in order — later steps depend on earlier ones)

### Step 1 — Establish the epoch and read the diff

```
git log --oneline $EPOCH..$HEAD
git diff --stat $EPOCH..$HEAD -- src/ Cargo.toml Cargo.lock src/python/pyproject.toml src/ts/package.json
git diff --name-status $EPOCH..$HEAD -- src/
```

Note new/removed modules, schema-migration steps (`grep` for `SCHEMA_VERSION`), new
SDK methods, new/changed CLI flags, and any new crate/dependency.

### Step 2 — Classify the completed work

For each change decide which docs it touches:

| Change observed in `$EPOCH..$HEAD` | Docs to update |
|------------------------------------|----------------|
| **New `src/` module / engine subsystem** | `dev/architecture.md`; the owning `dev/design/*` memo; add/refresh its `dev/DOC-INDEX.md` row |
| **New / changed SDK verb or shape** (Python/TS/Rust) | `dev/interfaces/rust.md` (Rust); `docs/reference/{python,typescript}-api.md` — **assert Python↔TS parity**; the owning design memo |
| **Changed algorithm / fusion / strategy** (RRF, rerank, BFS, scoring) | the owning `dev/design/*` memo; `dev/design/retrieval.md`; user guide under `docs/guides/` |
| **Changed default / threshold / cap / weight** | every doc quoting that value (grep it); `docs/reference/config.md` / `cli.md` if surfaced |
| **New / renamed / removed CLI flag or verb** | `dev/interfaces/cli.md`; `docs/reference/cli.md`; confirm against `--help` |
| **Schema migration step / `SCHEMA_VERSION` bump** | `dev/design/migrations.md`; the owning design memo; `dev/architecture.md` |
| **New crate / dependency** (`Cargo.toml`, `pyproject.toml`, `package.json`) | `dev/architecture.md`; owning design memo citations |
| **New ADR landed / ADR amended** | `dev/adr/ADR-0.6.0-decision-index.md`; cross-links from affected design docs |
| **New / changed tests** | "test plan" / "test coverage" of the owning design memo; `dev/traceability.md` test column |
| **Behaviour-compat event** (documented behaviour change) | `docs/release-notes/<rel>.md`; the relevant `docs/positions/*` page |
| **Completed work implying a new requirement/AC** | **DO NOT mint an ID** — log the gap in `dev/learnings.md` + status board; flag in summary |
| **Superseded spec / research** | move to `dev/archive/` with a SUPERSEDED banner; update the archive manifest |
| **Out-of-band / owner-managed landing** (e.g. corpus line) | reconcile its rows in the dedicated `dev/DOC-INDEX.md` section; do not claim campaign ownership |

If `$SCOPE` is set, restrict to docs matching that subsystem.

### Step 3 — Update developer docs FIRST (dependency root)

Order within `dev/` matters because user docs and traces derive from it:

1. **`requirements.md` / `acceptance.md` / `needs.md`** — read-only for IDs (locked). Adjust only *prose* describing an already-shipped, already-IDed behaviour. New need? → log it, don't ID it.
2. **`interfaces/rust.md`, `interfaces/cli.md`** — the public-surface contract. Re-verify every symbol/flag/exit-code against `$HEAD`.
3. **`architecture.md`** — module map, data flow, reader-pool/projection/surface, dependency list, `SCHEMA_VERSION`.
4. **`design/`** — the owning per-slice memos and the cross-cutting specs (`retrieval.md`, `migrations.md`, `op-store.md`, `vector.md`, …). Re-verify every `file:line`, symbol, default, SQL step.
5. **`adr/`** — do not edit an **accepted** ADR; if the diff contradicts one, propose a successor ADR and flag it. Keep the decision index current.
6. **`test-plan.md`, `learnings.md`** — only if tiers/strategy changed or you logged a gap/bug.

### Step 4 — Update user docs (derived from `dev/`)

After `dev/` is correct:

- `docs/reference/python-api.md` ⟵ the Python SDK surface.
- `docs/reference/typescript-api.md` ⟵ the TS SDK surface — **confirm parity with Python** (SDK-parity is a shipped position; `docs/positions/sdk-parity.md`).
- `docs/reference/cli.md` ⟵ `dev/interfaces/cli.md`; confirm against `--help`.
- `docs/reference/errors.md`, `config.md` ⟵ taxonomy / config surface.
- `docs/guides/*`, `getting-started/*`, `concepts/*` as needed (keep examples runnable).
- `docs/release-notes/<rel>.md` + `dev/releases/<rel>.md` for behaviour-compat events.
- Root `README.md` stays a slim overview + doc map; push detail into `docs/`.

### Step 5 — Update cross-cutting indexes & traceability (depends on Steps 3–4)

- **`dev/DOC-INDEX.md`** — the keystone of this step. Every doc you added/renamed/materially changed gets its row refreshed (path · purpose · owning slice/AC · `last-touched` = today). A stale or missing row is the **Slice-40 gate-m** failure condition; leave the index a true map of the shipped surface.
- **`dev/traceability.md`** — re-chain REQ↔AC↔test for shipped work; flag orphans/dangling honestly (`ORPHAN`, `PARTIAL`). Re-point only; do not invent IDs.
- **`dev/README.md`, `dev/design/README.md`, `dev/adr/...decision-index.md`, `dev/archive/README.md`, `docs/index.md`** — list only files that exist.
- **`tools/docs/mkdocs.yml`** `nav` — every new `docs/` page is in nav; no orphaned pages.

### Step 6 — Verify (gate before declaring done)

- **Factual:** every `file:line`, symbol, flag, default, `SCHEMA_VERSION`, and citation matches `$HEAD` source; dependency claims match `Cargo.toml`/`pyproject.toml`/`package.json`.
- **Parity:** Python and TypeScript API references describe the same surface; the governed-surface allowlist in `dev/interfaces/rust.md` still matches the code.
- **CLI parity:** the FathomDB CLI `--help` flags/verbs == `dev/interfaces/cli.md` == `docs/reference/cli.md`.
- **Links & nav:** intra-doc links resolve; `dev/DOC-INDEX.md` references only existing files; `mkdocs.yml` nav has no orphans.
- **No code touched:** `git status --porcelain src/` is empty (aside from pre-existing changes); no test file modified.
- **Build green:** docs build via `./tools/docs/build.sh` (or `./scripts/check.sh`, which adds the mkdocs build); markdown lint via `./scripts/agent-lint.sh` (auto-fix: `npm run format:md` + `markdownlint-cli2 --fix`). Run `./scripts/agent-verify.sh` as a sanity check that nothing leaked into source.

### Step 7 — Stamp the new epoch

Update the **DOCS EPOCH MARKER** at the top of this file to `$HEAD`'s short SHA and
today's date, with a one-line note on what the run reconciled. Commit the docs as a
single `docs(...)` commit (per the repo's conventional-commit + one-docs-commit-per-
transition style). Summarize what changed and list every gap/bug/surface-without-ADR
you flagged (with where you logged it).

---

## 3. Conventions

- **Cite code as `` `src/relative/path:line` ``** (clickable). Names, flags, and defaults must be exact — re-read the source, do not recall.
- **Stale > missing** (`AGENTS.md` §1): if you cannot make a doc correct, delete it or banner it rather than leave it wrong.
- **Prefer editing** an existing doc over creating a parallel one; fold superseded material into `dev/archive/` with a banner instead of leaving stale duplicates.
- **ADRs are authoritative** (`dev/adr/`, index `ADR-0.6.0-decision-index.md`). Do not contradict an accepted ADR; propose a successor.
- **Public surface is contract**: `dev/interfaces/*` and `pub` Rust APIs change only with an ADR or interface-doc update in the same PR — if the diff broke this, flag it, don't paper over it.
- **No invented requirement/AC IDs** (acceptance is locked) — log gaps, never mint.
- **No `CLAUDE.md`** — `AGENTS.md` is the canonical agent-instruction file.

## 4. Execution model (optional, for large diffs)

A small diff is a single pass. For a large diff, fan out: one context-bounded subagent
per affected doc (or per subsystem) — give each only its target source module(s), the
relevant `dev/design/*` memo, the matching interface contract, and the section of
`dev/DOC-INDEX.md` it owns — then run a **verify** agent (facts / `file:line` / parity
/ links) and a **review** agent (completeness vs the Step-2 classification table) and
dispatch targeted fix agents for any blocking findings, before Steps 5–7. Keep `dev/`
ahead of `docs/` in the sequence regardless of parallelism, and let exactly one writer
own `dev/DOC-INDEX.md` to avoid row races.
