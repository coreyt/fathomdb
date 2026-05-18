# tools/docs — local mkdocs preview

Self-contained scripts for previewing + validating the `docs/` site
locally. Useful for clients and contributors who want an early look at
the 0.6.0 SDK reference, quickstart, and deferral disclosures before
the GA tag publishes the site to its hosted URL.

## Quick start

```bash
bash tools/docs/serve.sh
# → http://127.0.0.1:8000 with live reload
```

First run creates `tools/docs/.venv/` (gitignored) and installs the
pinned `mkdocs` version (~10s, ~10 MB). Subsequent runs reuse the venv
(~1s startup).

Edit any file under `docs/`; the browser auto-reloads.

## Scripts

| Script    | What                                                                                                                                                                         |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `serve.sh` | Live-reload preview at `http://127.0.0.1:8000`. Accepts mkdocs flags (e.g. `--port 8080`, `--bind 0.0.0.0:8000` for LAN access).                                              |
| `build.sh` | Static site build with `--strict` (no broken links). Output lands in `site/` (gitignored). CI runs the same check; use locally before pushing docs changes.                  |

## What's in here

```text
tools/docs/
├── .gitignore          # ignores .venv/ + __pycache__/
├── README.md           # this file
├── requirements.txt    # mkdocs version pin (1.6.1)
├── serve.sh            # bootstrap venv + mkdocs serve
└── build.sh            # bootstrap venv + mkdocs build --strict
```

The `.venv/` directory created on first run is **gitignored**; never
commit it.

## What clients see

The served site mirrors the post-GA hosted documentation:

- `getting-started/quickstart.md` — five-verb walkthrough (open →
  write → search → counters → close) matching the AC-056 release-gate
  smoke `scripts/release/smoke/smoke-pypi-wheel.sh`.
- `install/{python,typescript,rust}.md` — install paths for each
  ecosystem. Post-GA commands shown; pre-GA editable-install paths
  documented for current source-build clients.
- `reference/{python-api,typescript-api,cli,errors,config}.md` —
  hand-written SDK reference per locked
  `dev/interfaces/{python,typescript,cli}.md` specs.
- `compatibility/index.md` — supported platforms, toolchains,
  two-axis versioning, deferred-perf disclosures, TS-not-yet-Python-
  parity caveat, no-0.5.x-shim policy.
- `concepts/index.md` — engine lifecycle, five-verb surface, vector
  projections, embedder model, recovery surface.
- `release-notes/0.6.0.md` — preview of 0.6.0 release notes with
  full deferred-items disclosures.
- `positions/*.md` — consumer-relevant technical positions (SDK
  parity, recovery surface, tokenizer policy, embedder identity).

## Troubleshooting

- **`python3` not found**: install Python 3.10+ system-wide
  (`apt install python3 python3-venv` on Debian/Ubuntu;
  `brew install python` on macOS).
- **`mkdocs build --strict` fails on first push**: run
  `bash tools/docs/build.sh` locally and fix any broken-link or
  missing-file errors before pushing. CI runs the same check via
  `.github/workflows/ci.yml`.
- **Port already in use**: pass `--port 8080` (or any free port).
- **Want to share preview on LAN**: pass `--bind 0.0.0.0:8000`. mkdocs
  serves on all interfaces; access via your host's LAN IP.

## When this gets replaced

When the GA-tagged site is hosted at a public URL (planned for a
follow-up `12-DX-DEPLOY` slice — likely GitHub Pages on push to
`main`), this `tools/docs/` directory stays for contributors who want
to preview unmerged doc changes locally. Clients consume the hosted
site; contributors use this directory.
