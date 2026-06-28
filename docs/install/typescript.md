# Install — TypeScript / Node.js

The `fathomdb` npm package is a [napi-rs](https://napi.rs/) binding
over the native Rust runtime. The published package selects a
platform-tagged `.node` binary at load time.

> **TS SDK parity caveat.** The 0.6.0 TS SDK is not yet at feature
> parity with the Python SDK. Both bindings cover the same five-verb
> surface and the same error taxonomy, but the TS package shipped its
> first working slice on 2026-04-07 and remains the less-mature option.
> For production pilots, prefer Python. See
> [release notes § TypeScript SDK parity](../release-notes/0.6.0.md).

## Requirements

- Node **18** or later (release.yml runs CI on Node 22).
- One of the supported platforms (per `release.yml` matrix):
  - Linux `x86_64-unknown-linux-gnu`
  - macOS `x86_64-apple-darwin`
  - macOS `aarch64-apple-darwin`
  - Windows `x86_64-pc-windows-msvc`
- SQLite + `sqlite-vec` (statically linked into the platform binary).

## Install (post-GA)

```bash
npm install fathomdb@0.6.0
```

## Install (pre-GA, build from source)

```bash
git clone https://github.com/coreyt/fathomdb
cd fathomdb
git checkout 0.6.0-rewrite
cd src/ts
npm install
npm run build
```

`npm run build` invokes `napi build` against the workspace Rust crate
`fathomdb-napi` and emits `fathomdb.<platform>-<arch>.node` plus the
TypeScript output in `dist/`.

## Default embedder (optional)

To let FathomDB embed documents for you, use the embedder-enabled native binary
and opt in at open:

```ts
const engine = await engineOpen("mydb.sqlite", { useDefaultEmbedder: true });
```

This enables the in-process `bge-small-en-v1.5` model and, on first use,
downloads + sha256-verifies ~133 MB of weights into your platform cache
(visible in `engine.openReport().embedderEvents`). The flag defaults to
`false`, and the embedder-enabled binary is larger. See the
[Default Embedder guide](../embedder.md) for the opt-in contract,
offline/`HF_TOKEN` notes, caveats, and migration.

## Verify

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("./hello.fdb");
await engine.write([]);
await engine.search("hello");
await engine.close();
console.log("ok");
```

Expected output: `ok`. See [Quickstart](../getting-started/quickstart.md).

## Troubleshooting

- **`Error: Cannot find module 'fathomdb.<platform>-<arch>.node'`** —
  no platform binary matched your runtime. Confirm your platform is on
  the supported matrix above. For source builds, ensure
  `npm run build` completed before `node` resolves the package.
- **`FathomDbError`** — every native error is rethrown as a typed
  subclass of `FathomDbError`. See [errors reference](../reference/errors.md).

## See also

- [Reference — TypeScript API](../reference/typescript-api.md)
- [Reference — config](../reference/config.md)
- [Compatibility](../compatibility/index.md)
- [Release notes — 0.6.0](../release-notes/0.6.0.md)
