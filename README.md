# FathomDB 0.6.0 Rewrite Scaffold

This branch is the clean active workspace for the `0.6.0` rewrite.
The design corpus under `docs/0.6.0/` is the source of truth for
crate boundaries, binding shapes, and release scope.

Active roots:

- `crates/` for the Rust workspace scaffold
- `python/` for the Python package scaffold
- `ts/` for the TypeScript package scaffold
- `tests/` for rewrite-era integration scaffolding
- `.github/workflows/` and `scripts/` for minimal rewrite checks

Start with:

- `docs/0.6.0/README.md`
- `docs/0.6.0/architecture.md`
- `docs/0.6.0/design/bindings.md`

Local checks:

```bash
scripts/check.sh
```

Binding bootstrap entry points:

```bash
pip install -e python/
cd ts && npm install
```
