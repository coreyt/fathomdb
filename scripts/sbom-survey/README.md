# `sbom-survey` — dependency survey tool (Library Sweep #3)

An **isolated Python mini-project** that produces a **CycloneDX 1.6 JSON** SBOM over every
dependency manifest **tracked on `main`**, enumerates the **library↔library** dependency graph, and
diffs **used (locked) versus published (registry latest)** versions.

It mechanizes the manual triage loop in `dev/plans/prompts/LIBRARY-BUMP-STEWARD.md` §1–2 — is the
manifest tracked, is the dependency direct or transitive, is the locked version already at target —
so a Library Sweep is ~90% mechanical instead of a search → review → check → reason cycle per
candidate.

**Spec of record:** `dev/design/0.8.20-slice-31-sbom-survey-tool.md` — requirements, the 22
acceptance criteria, the design, and the answers to every resolved design question. Read that first;
this file is only the operating note.

## Status

| Slice | Deliverable | State |
|---|---|---|
| **31** | requirements · acceptance criteria · design · **RED tests** | **this directory today** |
| **32** | the code that turns the RED tests GREEN | not started |
| **33** | **runs** the tool and writes the survey findings | not started |

At Slice 31 this directory contains **only** this README and the RED acceptance suite. There is no
`sbom_survey` package, no `pyproject.toml`, no `tiers.toml` and no CLI yet — that is Slice 32. The
suite is therefore **RED by construction**: 22 tests, 22 failures, one per acceptance criterion.

## Running the suite

From the repository root, with a neutral working directory:

```bash
python3 -m pytest scripts/sbom-survey/tests
```

Expected at Slice 31: **`22 failed, 0 passed, 0 skipped, 0 errors`** (exit code `1`).

- **No test may skip and no test may pass** before Slice 32. A skip is a vacuous green.
- **No module-level `import sbom_survey`.** The import happens inside each test body via the
  `require()` helper in `tests/conftest.py`, so a missing package produces 22 attributable FAILEDs
  rather than one collection error that hides 21 of them.
- The suite needs **no network**. The published-version lookup is behind an injectable seam; the
  tests inject `OfflineSource` / `StaticSource`, and one test asserts zero socket I/O.
- **`AC-SBOM-10` grades CycloneDX validity with an INDEPENDENT validator** — the upstream
  `cyclonedx-python-lib[json-validation]` one, plus a known-invalid negative control — never with
  `sbom_survey.cyclonedx.validate()`, which would be self-certification. From Slice 32 that
  distribution must be installed: if it is missing the criterion **FAILS** naming what to install.
  It does **not** skip, because an ungraded criterion is a green that means nothing (design §5.7).
- **TC-97.** The only pytest configuration in this repository is `src/python/pyproject.toml`, whose
  `pythonpath = ["."]` shadows an installed wheel. It is **not** an ancestor of
  `scripts/sbom-survey/tests`, so pytest runs this suite with **no config file** and cannot inherit
  it. Do not add a `pyproject.toml` above this directory.

## Deliberately NOT wired into CI

This tool is **recurring by design and NOT CI-gating** — it is **informational**
(`plan-0.8.20.md` §3a, HITL 2026-07-29, steward `seq-153`).

- It is **not** registered in `scripts/agent-test.sh`.
- It is **not** referenced by `.github/workflows/ci.yml`.
- It is **not** part of `scripts/agent-verify.sh`, `scripts/check.sh`, or any lint/typecheck scope
  (`ruff` and `pyright` are scoped to `src/python`).

Do not wire it in. Beyond the standing ruling, the acceptance suite is red until Slice 32 lands, so
registering it would turn `main` red. `tests/test_cli.py::test_tool_declares_non_ci_gating_and_is_absent_from_ci_wiring`
is the standing guard: it greps both wiring files and fails if either grows a reference.

## Isolation

The mini-project's own `pyproject.toml` (a Slice 32 artifact) is standalone: not a Cargo workspace
member, not referenced by `src/python/pyproject.toml`, not a dependency of the root `package.json`.
It can never enlarge the published dependency graph or the advisory backlog. Its own dependencies
are surveyed by the tool and tagged `dev-tooling` — the tool appears in its own SBOM, by design.

Generated reports go to `scripts/sbom-survey/out/`, which Slice 32 adds to `.gitignore`. Slice 33's
**findings** have a separate tracked home:
`dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md` — the house convention for a dated run
report, weighed against `dev/design/` and `dev/deps/` in design §5.6.

## Scope guard

The tool **never** applies a dependency bump and **never** edits a manifest or a lockfile. Its only
write path is its own gitignored output directory. The survey is an **input to 0.8.22**, which owns
the actual upgrades.
