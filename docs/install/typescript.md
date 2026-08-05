# Install — TypeScript / Node.js

The `fathomdb` npm package is a [napi-rs](https://napi.rs/) binding
over the native Rust runtime. The published package selects a
platform-tagged `.node` binary at load time.

> **0.8.21 is published on npm's `next` dist-tag.** It ships Linux
> x86_64/glibc and Linux AArch64/glibc native packages; other hosts must build
> from source.
> FathomDB is pre-1.0 and the surface is **beta**.
>
> **TS SDK parity caveat.** Both bindings expose the same governed command
> surface (`src/conformance/governed-surface-allowlist.json`) and the same
> 27-class error taxonomy, but Python remains the more heavily exercised
> binding. For production pilots, prefer Python. See
> [SDK parity](../positions/sdk-parity.md).

## Requirements

- Node **18** or later (release.yml runs CI on Node 25.9.0).
- The published `0.8.21` npm package supports Linux
  `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`. Other hosts
  must build from source.
- SQLite + `sqlite-vec` (statically linked into the platform binary).

## Install the published package

```bash
npm install fathomdb@next
```

## Install (current path — build from source)

```bash
git clone https://github.com/coreyt/fathomdb
cd fathomdb/src/ts
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
import { Engine } from "fathomdb";

const engine = await Engine.open("mydb.sqlite", { useDefaultEmbedder: true });
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
- [CHANGELOG](https://github.com/coreyt/fathomdb/blob/main/CHANGELOG.md)
