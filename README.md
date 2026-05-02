# FathomDB

FathomDB is a rewrite-in-progress centered on the `0.6.0` design corpus.

Repository layout:

- `docs/` contains public MkDocs source and client-facing technical positions.
- `dev/` contains internal engineering material: requirements, architecture,
  ADRs, subsystem design, interface contracts, and planning notes.
- `src/` contains implementation roots and unit-test-adjacent code.
- `test/` contains cross-language, smoke, fixture, and performance assets that
  are not package-local unit tests.

Implementation roots:

- Rust workspace members live under `src/rust/crates/`
- Python package root lives under `src/python/`
- TypeScript package root lives under `src/ts/`

Start here:

- Public docs: `docs/index.md`
- Internal docs index: `dev/README.md`
- Workspace checks: `scripts/check.sh`

Common commands:

```bash
cargo check --workspace
pip install -e src/python/
cd src/ts && npm install
mkdocs build --strict
```
