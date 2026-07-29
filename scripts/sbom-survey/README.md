# `sbom-survey` — dependency survey tool (Library Sweep #3)

An **isolated Python mini-project** that produces a **CycloneDX 1.6 JSON** SBOM over every
dependency manifest **tracked on `main`**, enumerates the **library↔library** dependency graph, and
diffs **used (locked) versus published (registry latest)** versions.

It mechanizes the manual triage loop in `dev/plans/prompts/LIBRARY-BUMP-STEWARD.md` §1–2 — is the
manifest tracked, is the dependency direct or transitive, is the locked version already at target —
so a Library Sweep is ~90% mechanical instead of a search → review → check → reason cycle per
candidate.

**Spec of record:** `dev/design/0.8.20-slice-31-sbom-survey-tool.md` — requirements, the 24
acceptance criteria, the design, and the answers to every resolved design question. Read that first;
this file is only the operating note.

## Layout

| Path | What it is |
|---|---|
| `pyproject.toml` | the isolated project manifest — dependencies per design §5.7 |
| `tiers.toml` | the **tracked tier/exclusion data** (§5.3); rules are DATA, never code |
| `sbom_survey/discovery.py` | `git ls-files`-derived manifest discovery (§5.1) |
| `sbom_survey/tiers.py` | longest-prefix-wins matching, `UntieredManifestError` (§5.3) |
| `sbom_survey/parse/` | `Cargo.lock`/`Cargo.toml`, `package-lock.json`/`package.json`, `uv.lock`/`pyproject.toml`/`requirements*.txt` (§5.5) |
| `sbom_survey/registry.py` | the injectable published-version seam (§5.4) |
| `sbom_survey/survey.py` | `run_survey` · `classify_status` · the staleness rows (§5.4, §5.8) |
| `sbom_survey/cyclonedx.py` | CycloneDX 1.6 assembly + real schema `validate()` (§5.5, §5.7) |
| `sbom_survey/report.py` | `sbom.cdx.json` · `staleness.json` · `staleness.md` (§5.6, §5.8) |
| `sbom_survey/cli.py` | the `argparse` entry point and its exit codes (§5.9) |

## Status

| Slice | Deliverable | State |
|---|---|---|
| **31** | requirements · acceptance criteria · design · **RED tests** | landed |
| **32** | the code that turns the RED tests GREEN | **landed — 24 passed, 0 failed, 0 skipped, 0 errors** |
| **33** | **runs** the tool and writes the survey findings | not started |

This directory now contains the implementation as well: the `sbom_survey` package, its isolated
`pyproject.toml`, the tracked `tiers.toml` rule data and the CLI. Slice 32 turned all 24 acceptance
criteria GREEN **without weakening any assertion Slice 31 wrote**.

**The count moved 23 → 24 at fix-1**, under HITL ruling `seq-168` (`TC-112` (a)) — the single
occasion the Slice-31 suite has been unfrozen. `AC-SBOM-24`
(`tests/test_cyclonedx.py::test_no_component_carries_a_constraint_its_version_violates`) rules that
**no component may carry a `fathomdb:constraint` that its own version does not satisfy**. It exists
because codex §9 round 1 found, by reading, a defect the other 23 criteria could not see: a
declaration was attached to every locked version sharing its name, so `sha2 0.10.9` carried
`constraint = "0.11"` and `thiserror 2.0.18` was tagged `direct` under `"1"`. `depth` and
`edit_sites` are the two fields Slice 33 decides on. The id is numbered last rather than slotted in,
per the `AC-SBOM-23` precedent in design §4, so every existing id keeps its meaning.

## Running the tool

```bash
sbom-survey --repo PATH [--offline | --online] [--out DIR] [--tiers FILE] [--now ISO8601]
sbom-survey --describe
```

**The tier rules are read from the SURVEYED repository — `<repo>/scripts/sbom-survey/tiers.toml` —
never from the installed package.** They are data *about a repository* (every rule is a path prefix
into the surveyed tree), so a copy baked into a wheel would describe whichever repository the wheel
was built from. It also keeps `AC-SBOM-08` / `AC-SBOM-11` grading the very file the survey consumed.

Consequences worth knowing before Slice 33 runs this:

- `pip install ./scripts/sbom-survey` followed by `sbom-survey --repo <that repo> --offline --out DIR`
  works and exits `0`. (Before fix-2 it exited `1` with a bare `FileNotFoundError`, because the
  default was resolved relative to the *package* and no package data is declared.)
- Surveying a repository that does **not** track `scripts/sbom-survey/tiers.toml` requires an
  explicit `--tiers FILE`. That is deliberate: §5.3 rules there is **no catch-all tier rule**, so
  guessing a rule set for an unknown repository would be the silent mis-tag REQ-4 exists to prevent.
  The tool exits `3` and names both the file it wanted and the `--tiers` override.

**Timestamps are validated, never substituted.** `--now` and `SOURCE_DATE_EPOCH` both pass through
one function (`util.parse_timestamp`), and so do `staleness.json`'s `generated` and the CycloneDX
`metadata.timestamp` — so the two artifacts of a single run cannot disagree about when it happened.
A malformed value is **rejected** rather than quietly replaced by the default epoch: this value is
the provenance of the whole run, and §5.6 makes the findings doc's provenance header load-bearing.

| Exit | Meaning |
|---|---|
| `0` | survey written |
| `2` | a tracked manifest has no tier assignment (an offending path on stderr) |
| `3` | a tracked manifest could not be parsed, or the tier rules could not be read |
| `1` | unexpected internal error |
| `64` | bad command line (`EX_USAGE`) — e.g. a malformed `--now` |

§5.9 rules the first four. `64` is deliberately **outside** that set: a bad argument is none of those
things, and argparse's own default for a usage error is `2`, which would collide head-on with
"untiered manifest" and make `AC-SBOM-21`'s signal ambiguous for anyone reading exit codes.

## Running the suite

The mini-project is deliberately **not installed**: `tests/conftest.py` puts `scripts/sbom-survey/`
on `sys.path` itself (and on `PYTHONPATH` for the CLI subprocess). Only the third-party dependencies
declared in `pyproject.toml` need to be importable by the interpreter running pytest — build a venv
for them **outside the repository tree**, and never install anything into the repo's shared `.venv`.

```bash
python3 -m venv /tmp/sbom-survey-venv
/tmp/sbom-survey-venv/bin/pip install \
  'cyclonedx-python-lib[json-validation]>=8.0,<9.0' 'packageurl-python>=0.15,<1.0' \
  'packaging>=24.0,<26.0' 'semver>=3.0,<4.0' 'pytest>=8.0,<10.0'
/tmp/sbom-survey-venv/bin/python -m pytest scripts/sbom-survey/tests -q
```

Expected: **`24 passed, 0 failed, 0 skipped, 0 errors`** (exit code `0`).

- **No test may skip.** A skip is a vacuous green — an ungraded criterion reporting success.
- **No module-level `import sbom_survey`.** The import happens inside each test body via the
  `require()` helper in `tests/conftest.py`, so a broken package produces 24 attributable FAILEDs
  rather than one collection error that hides 23 of them.
- The suite needs **no network**. The published-version lookup is behind an injectable seam; the
  tests inject `OfflineSource` / `StaticSource`, and one test asserts zero socket I/O.
- **`AC-SBOM-10` grades CycloneDX validity with an INDEPENDENT validator** — the upstream
  `cyclonedx-python-lib[json-validation]` one, plus a known-invalid negative control — never with
  `sbom_survey.cyclonedx.validate()`, which would be self-certification. From Slice 32 that
  distribution must be installed: if it is missing the criterion **FAILS** naming what to install.
  It does **not** skip, because an ungraded criterion is a green that means nothing (design §5.7).
- **TC-97.** The only pytest *settings* in this repository live in `src/python/pyproject.toml`, whose
  `pythonpath = ["."]` shadows an installed wheel. That file is **not** an ancestor of
  `scripts/sbom-survey/tests`, so this suite can never inherit it. Since Slice 32 the header does
  print `configfile: pyproject.toml`, pointing at **this directory's own** `pyproject.toml`: from
  pytest 8.1 a `pyproject.toml` found while walking up from the test arguments becomes the rootdir
  anchor even when it carries no `[tool.pytest.ini_options]` table, in which case the applied
  settings are the empty dict (verified: `config.inicfg == {}`, `config.getini("pythonpath") == []`).
  **Do not add a `[tool.pytest.ini_options]` table here, and do not add a `pyproject.toml`,
  `pytest.ini`, `tox.ini` or `setup.cfg` above this directory** — either would start applying real
  settings to this suite.

## Install-then-run smoke (TC-115)

`scripts/sbom-survey/smoke-install-run.sh` guards the **install path**, which the suite above does
not reach: **installing is not verifying an install — invoking what was installed is.** It builds a
throwaway venv **outside** the repo, `pip install`s this project, runs the **installed console
script**, then re-runs the same survey from the source tree and asserts the two agree.

```bash
bash scripts/sbom-survey/smoke-install-run.sh     # exit 0 = PASS
```

It asserts: the console-script file exists and is executable · both runs exit `0` · a **provenance**
check that the source-tree run really is the tree (not the still-installed copy) · identical artifact
**sets** · byte-identical `sbom.cdx.json` / `staleness.json` / `staleness.md` · and a **vacuity
guard** (`summary.components > 0`, non-empty `rows`) — two empty files are byte-identical. Only the
`pip install` needs network; both surveys run `--offline`.

**It is deliberately NOT CI-wired** (steward `seq-172` ruled wiring out, not deferred) — run it by
hand, and do not add it to `agent-test.sh` or `ci.yml`.

## Deliberately NOT wired into CI

This tool is **recurring by design and NOT CI-gating** — it is **informational**
(`plan-0.8.20.md` §3a, HITL 2026-07-29, steward `seq-153`).

- It is **not** registered in `scripts/agent-test.sh`.
- It is **not** referenced by `.github/workflows/ci.yml`.
- It is **not** part of `scripts/agent-verify.sh`, `scripts/check.sh`, or any lint/typecheck scope
  (`ruff` and `pyright` are scoped to `src/python`).

Do not wire it in. **The suite being GREEN does not authorize wiring**: the standing HITL ruling
(`seq-166`) is that the suite may be wired only when it is green **AND** the HITL has approved —
both required, neither sufficient. `tests/test_cli.py::test_tool_declares_non_ci_gating_and_is_absent_from_ci_wiring`
is the standing guard: it greps both wiring files and fails if either grows a reference, so it stays
green precisely **because the wiring is absent**.

## Isolation

The mini-project's own `pyproject.toml` (a Slice 32 artifact) is standalone: not a Cargo workspace
member, not referenced by `src/python/pyproject.toml`, not a dependency of the root `package.json`.
It can never enlarge the published dependency graph or the advisory backlog. Its own dependencies
are surveyed by the tool and tagged `dev-tooling` — the tool appears in its own SBOM, by design.

Generated reports go to `scripts/sbom-survey/out/`, which Slice 32 added to `.gitignore`. Slice 33's
**findings** have a separate tracked home:
`dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md` — the house convention for a dated run
report, weighed against `dev/design/` and `dev/deps/` in design §5.6.

## Scope guard

The tool **never** applies a dependency bump and **never** edits a manifest or a lockfile. Its only
write path is its own gitignored output directory. The survey is an **input to 0.8.22**, which owns
the actual upgrades.
