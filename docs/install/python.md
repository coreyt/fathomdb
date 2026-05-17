# Install — Python

The `fathomdb` Python SDK is a [PyO3](https://pyo3.rs/) binding over the
native Rust runtime. Wheels for the GA release will be platform-tagged
(no source build required on supported platforms).

## Requirements

- Python **3.10**, **3.11**, or **3.12**
- One of the supported platforms (per `release.yml` matrix):
    - Linux `x86_64-unknown-linux-gnu` (manylinux 2_28)
    - Linux `aarch64-unknown-linux-gnu` (manylinux 2_28)
    - macOS `x86_64-apple-darwin`
    - macOS `aarch64-apple-darwin`
    - Windows `x86_64-pc-windows-msvc`
- SQLite with the [`sqlite-vec`](https://github.com/asg017/sqlite-vec)
  extension available to the loader (statically linked into the wheel
  for supported platforms).

## Install (post-GA)

```bash
pip install fathomdb==0.6.0
```

## Install (pre-GA, from source)

`0.6.0` has not yet been published to PyPI. The current install path is
editable from the `0.6.0-rewrite` branch using
[maturin](https://www.maturin.rs/):

```bash
git clone https://github.com/coreyt/fathomdb
cd fathomdb
git checkout 0.6.0-rewrite
pip install -e src/python/
```

`pip install -e src/python/` invokes maturin against the workspace and
produces the native PyO3 extension `fathomdb._fathomdb`. **Do not run
`cargo build` and copy the `.so` manually.** Editable install is the
only supported native-build path for development.

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
- [Release notes — 0.6.0](../release-notes/0.6.0.md)
