# Install — Python

The `fathomdb` Python SDK is a [PyO3](https://pyo3.rs/) binding over the
native Rust runtime. Published wheels are platform-tagged (no source build
required on supported platforms).

> **0.8.21 is published to PyPI** for Linux x86_64/glibc and Linux
> AArch64/glibc. Other hosts must use the from-source path below. FathomDB is
> pre-1.0 and the surface is **beta**.

## Requirements

- Python **3.10**, **3.11**, or **3.12**
- One of the supported platforms (per `release.yml` matrix):
  - Linux `x86_64-unknown-linux-gnu` (manylinux 2_28)
  - Linux `aarch64-unknown-linux-gnu` (manylinux 2_28)
- SQLite with the [`sqlite-vec`](https://github.com/asg017/sqlite-vec)
  extension available to the loader (statically linked into the wheel
  for supported platforms).

## Install the published wheel

```bash
pip install fathomdb==0.8.21
```

## Install (current path — from source)

Editable from `main` using [maturin](https://www.maturin.rs/):

```bash
git clone https://github.com/coreyt/fathomdb
cd fathomdb
pip install -e src/python/
```

`pip install -e src/python/` invokes maturin against the workspace and
produces the native PyO3 extension `fathomdb._fathomdb`. **Do not run
`cargo build` and copy the `.so` manually.** Editable install is the
only supported native-build path for development.

## Default embedder

The published wheel already includes the default embedder; there is no
`default-embedder` Python extra. Opt in at `open`:

```python
engine = Engine.open("mydb.sqlite", use_default_embedder=True)
```

This pulls in the in-process `bge-small-en-v1.5` model and, on first use,
downloads + sha256-verifies ~133 MB of weights into your platform cache
(visible in `engine.open_report().embedder_events`). The flag defaults to
`False`. See the [Default Embedder guide](../embedder.md) for the opt-in
contract, offline/`HF_TOKEN` notes, caveats, and migration.

## Verify

```python
from fathomdb import Engine

engine = Engine.open("./hello.fdb")
engine.write([])
engine.search("hello")
engine.close()
print("ok")
```

Expected output: `ok`. See [Quickstart](../getting-started/quickstart.md)
for a richer walkthrough.

## Troubleshooting

- **`ImportError: libsqlite3 ...`** — your system SQLite is older than
  the version the wheel was built against. Install via your package
  manager (`apt install libsqlite3-dev`, `brew install sqlite`, etc.)
  or upgrade.
- **`OSError: sqlite-vec extension not found`** — `sqlite-vec` is
  statically linked into the official wheels. If you are building from
  source, ensure `sqlite-vec` is installed and discoverable by the
  build script.
- **`pip install -e src/python/` fails on `maturin`** — install maturin
  explicitly (`pip install maturin`) and retry. The build also requires
  a stable Rust toolchain (`rustup default stable`).
- **`fathomdb.errors.DatabaseLockedError`** — another process holds an
  exclusive lock on the DB file. See
  [errors reference](../reference/errors.md).

## See also

- [Reference — Python API](../reference/python-api.md)
- [Reference — config](../reference/config.md)
- [Compatibility](../compatibility/index.md)
- [CHANGELOG](https://github.com/coreyt/fathomdb/blob/main/CHANGELOG.md)
