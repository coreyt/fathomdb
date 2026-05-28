---
title: Embedder Subsystem Design
date: 2026-04-30
last_extended: 2026-05-28
target_release: 0.6.0
extends_for: 0.7.1-EMBEDDER-UNDEFER
desc: Dispatch pool, warmup behavior, timeout handling, identity checks; 0.7.1 mean-centering sub-design and EMB-5 loader sub-design
blast_radius: embedder dispatch; Engine.open warmup; REQ-028*, REQ-033, REQ-044; default-embedder loader (fathomdb-embedder); mean-vec schema column on _fathomdb_embedder_profiles
status: locked
---

# Embedder Design

This file owns dispatch onto the engine-owned embedder pool, eager warmup,
per-call timeout handling, and the runtime mechanics behind
`EmbedderIdentityMismatch`.

The 0.7.1 EMBEDDER-UNDEFER campaign extends this document with:

- §0 — mean-centering sub-design (forward-cited from EU-0 outcome,
  `dev/notes/0.7.1-default-embedder-research.md` §5.3); and
- §§1–10 — the EMB-5 loader sub-design called out by
  `ADR-0.6.0-default-embedder.md:106-109`.

All decisions below are concrete. No `TBD` is permitted: this design is
the gate before EU-3 implementation code lands. The legal basis for the
loader's network surface is `ADR-0.7.1-default-embedder-weight-fetch.md`;
nothing in this design extends that surface beyond what the ADR's
"scope guardrails" section permits.

## `embedder_pool_size` rationale

`embedder_pool_size` remains an engine-level knob in 0.6.0 because embedded
deployments are not uniform. Some hosts run FathomDB beside latency-sensitive
application work and need to cap embedder parallelism; others run heavier local
models or dedicated ingest jobs and need to tune concurrency around actual CPU
and memory pressure. The knob exists to control embedder contention, not to
create binding-specific surface.

---

## §0 — Mean-centering sub-design

### §0.1 Why it exists

EU-0 (`dev/notes/0.7.1-default-embedder-research.md` §2.2, §5.1) measured
that bge-small-en-v1.5's sign-bit quantization loss is bias-correctable:
subtracting the corpus-mean f32 vector from each embedding before the sign
step lifts recall@10 by **+5.0 pp** at K=64 (paired-bootstrap 95% CI
+0.024…+0.078). At the chosen K=128, mean-centering lifts the point
estimate to **0.907** (95% CI 0.877–0.933), clearing the 0.90 recall floor
in expectation. The cost is one f32 vector per workspace and one f32
subtraction per query.

The technique is a sign-bit-quantization **bias correction**, not a
change to the geometry of the f32 space. The f32 rerank step continues to
use the un-centered, L2-normed vector that comes off the embedder.

### §0.2 Schema

One new nullable column on `_fathomdb_embedder_profiles`:

```
mean_vec BLOB NULL
```

- **Encoding**: little-endian f32, length `dim × 4` bytes. At the chosen
  384d this is exactly **1536 bytes** per workspace.
- **Nullable**: pre-mean-centering workspaces (0.6.x, 0.7.0, and the
  pre-pin window described in §0.3) remain readable; `NULL` means "no
  pinned mean — write/query paths fall through to the no-centering
  branch."
- **Migration**: this column is added by **migration step 10** in
  `fathomdb-schema/src/lib.rs:115` (advancing from the current step 9
  added by PVQ). The migration is a single `ALTER TABLE
  _fathomdb_embedder_profiles ADD COLUMN mean_vec BLOB` — pure additive,
  no backfill.
- **Invariant**: when `mean_vec IS NOT NULL`, its byte length MUST equal
  `4 × embedder_identity.dimension`. The engine asserts this on read in
  debug builds (`debug_assert_eq!`). Release builds fall back to
  `EmbedderIdentityMismatch` if the stored length disagrees with the
  embedder's reported dimension, matching the existing fail-closed
  posture from `ADR-0.6.0-vector-identity-embedder-owned`.

### §0.3 Lifecycle

**Compute-once-on-first-ingest, pinned at the first non-trivial commit.**

Concretely:

1. Until **N = 256** documents have been embedded into the workspace,
   `mean_vec` remains `NULL`. The projection worker maintains an
   in-memory streaming accumulator over the f32 vectors it produces
   (`sum: Vec<f64>` of length `dim`, plus a `count: u64`; f64 chosen to
   bound numerical drift over the accumulation window).
2. On the commit that pushes `count ≥ 256`, the worker materializes
   `mean = (sum / count) as f32[dim]`, writes it to `mean_vec` in the
   same transaction that pins the threshold-crossing batch, and from
   that point onward the column is considered pinned.
3. All **subsequent** writes leave `mean_vec` unchanged. There is no
   silent drift; the pinned mean is a workspace constant after pin.
4. Refresh is **only** via an explicit reindex path. That path is **not
   implemented in 0.7.1** — reindex is a separate campaign. Documented
   as a known limitation in this design and in the 0.7.1 release notes.

**Why N = 256.** Same order of magnitude as the model dimension (384) so
the per-axis sample mean has comparable sample-size to its axis count;
small enough that the pre-pin window is a brief warmup window on any
non-trivial workspace; large enough that the materialized mean is not
dominated by a single document's vector. The choice is not load-bearing
to within a factor of 2 — N anywhere in `[128, 1024]` would be defensible
— but it is locked here so that EU-3 and EU-5 have a single number to
implement against. The constant lives in `fathomdb-engine` as
`MEAN_VEC_PIN_THRESHOLD: u64 = 256`.

**Provenance of N=256.** This number is **new in EU-2**. No prior
document (EU-0 research note, EU-1 ADR, handoff §0/§EU-2) pinned a
specific threshold; EU-0 computed its mean over the entire 7,667-doc
corpus at once because it was a one-shot research notebook, not a
workspace lifecycle. The threshold is therefore a fresh EU-2 decision
recorded here, not a re-statement of an earlier number.

### §0.4 Apply rule

**Write path.** In `run_projection_job`
(`fathomdb-engine/src/lib.rs:2507`), immediately before the `sign_quantize`
step:

```
let f32_vec = embedder.embed(text)?;          // unit-norm, un-centered
let bits = if let Some(mean) = pinned_mean {  // SELECT mean_vec FROM ...
    sign_quantize(&subtract(&f32_vec, &mean))
} else {
    sign_quantize(&f32_vec)                   // pre-pin window
};
// f32_vec (un-centered) is the BLOB written for f32 rerank.
```

**Query path.** Synchronously on the caller thread
(`fathomdb-engine/src/lib.rs:1469`), the query vector is embedded once
and then sign-quantized with the same conditional:

```
let q = embedder.embed(query_text)?;
let q_bits = if let Some(mean) = pinned_mean {
    sign_quantize(&subtract(&q, &mean))
} else {
    sign_quantize(&q)
};
// The f32 rerank against the candidate set uses `q` un-centered.
```

**f32 rerank uses the un-centered vectors on both sides.** This is the
geometric invariant: centering is applied only at the sign-quantization
boundary. Stored f32 BLOBs are un-centered; the query f32 is un-centered;
cosine over those un-centered, L2-normed vectors is the canonical
distance.

### §0.5 Pin-commit re-quantize pass (no asymmetry)

The pre-pin window holds the first up-to-255 documents in a brand-new
workspace. During that window, the projection worker has nonetheless
sign-quantized each f32 vector under a placeholder "no-centering" rule
so that vector visibility remains immediate per `design/engine.md:230`.
This would, naively, leave those first ≤255 rows with sign-quant bits
that do not match the rest of the workspace's centered bits.

**That asymmetry is not accepted.** The HITL directive at EU-2 review
required that, at pin time, the workspace transition to a clean,
uniformly-centered state — no row's sign-quant bits should reflect a
different centering than any other row.

**At-pin re-quantize pass.** In the **same transaction** that materializes
`mean_vec` on the threshold-crossing commit, the projection worker also:

1. Reads back every previously-written f32 BLOB from the vector index
   for the workspace's vector tables (the f32 BLOBs are stored
   un-centered per §0.4 — they are the canonical, correct
   representation, so no re-embed is required).
2. For each, computes `bits' = sign_quantize(&subtract(&f32_vec, &mean))`.
3. Updates the corresponding sign-bit columns in-place with `bits'`.

The pass is **deterministic** (input = stored un-centered f32, output =
sign-quant under the just-pinned mean) and **bounded** (touches at most
`MEAN_VEC_PIN_THRESHOLD - 1` rows per vector table = at most 255 rows at
N=256). It runs in the projection worker, not on the user's write
thread, preserving the async write-path contract from §async-surface.

**Atomicity.** The re-quantize updates AND the `mean_vec` write AND the
threshold-crossing batch's sign-quant inserts all land in the same
SQLite transaction. Either the workspace ends the transaction in
fully-centered steady state, or no part of the pin transition is
visible — there is no partially-pinned intermediate state observable
across an `Engine.open` boundary.

**Crash recovery.** If the engine dies between the start of the at-pin
re-quantize pass and the transaction commit, the WAL replay rolls the
entire batch back: `mean_vec` is still `NULL`, the sign-bit columns are
unchanged, and the next ingest will simply retry the pin attempt when
its commit pushes `count ≥ 256` again. The recovery surface is
unchanged from any other transactional write.

**Cost.** At N=256 with dim=384, the re-quantize pass reads at most
255 × 1536 bytes = ~384 KB of f32 BLOBs, does 255 × 384 = ~98k f32
subtractions + sign-tests, and writes 255 × 48 bytes (384/8) = ~12 KB
of bit-packed sign columns. This is microseconds of CPU plus one
transactional batch update — invisible against any realistic ingest
rate.

**Alternative considered: defer all sign-quantization until N≥256.**
Queue f32-only writes in a staging table, and on pin, batch-quantize the
backlog with the pinned mean. **Rejected** because it delays the
search-readiness of newly-ingested documents until pin, violating the
implicit "write → search-visible" latency contract from
`design/engine.md:230`. The re-quantize pass above achieves correctness
without that latency penalty.

**Alternative considered: accept the asymmetry (no re-quantize).**
Rejected by HITL at EU-2 review; recorded here so the rejection is not
re-litigated. The cost analysis above shows the correctness fix is
microseconds and one bounded transaction — there is no performance
argument for accepting the asymmetry.

### §0.6 Visibility

`OpenReport` (`fathomdb-engine/src/lib.rs:548-554`) gains two booleans
that together describe the workspace's mean-centering state. They are
deliberately split into a **static identity capability** and a **dynamic
workspace state** so that callers do not have to derive one from the
other:

```rust
pub struct OpenReport {
    // ... existing fields ...

    /// Static: does this embedder identity REQUIRE mean-centering as
    /// part of its sign-quantization pipeline? True iff the identity
    /// recorded in `_fathomdb_embedder_profiles.identity_name` is one
    /// whose pinned protocol uses MC (true for the 0.7.1 default
    /// `fathomdb-bge-small-en-v1.5`; false for `fathomdb-noop` and any
    /// caller-supplied embedder that does not opt in).
    ///
    /// This bool does NOT change over the workspace's lifetime — it is
    /// determined by the embedder identity, which is itself fail-closed
    /// after first profile-pin per ADR-0.6.0-vector-identity-embedder-owned.
    pub embedder_mean_centering_required: bool,

    /// Dynamic: has this workspace materialized the pinned mean vector
    /// yet? True iff `_fathomdb_embedder_profiles.mean_vec IS NOT NULL`.
    /// When `embedder_mean_centering_required == true` AND this is
    /// `false`, the workspace is in the pre-pin window (count < 256);
    /// when both are `true`, the workspace is in steady state. When
    /// `required == false`, this field is always `false` (no mean is
    /// ever pinned for an identity that does not use MC) — callers
    /// inspecting just this bool MUST also read `required` to interpret
    /// it.
    pub embedder_mean_vec_pinned: bool,
}
```

**Truth-table for the two bools:**

| `required` | `pinned` | meaning                                                                |
|------------|----------|------------------------------------------------------------------------|
| false      | false    | Embedder identity does not use MC (e.g. `fathomdb-noop`, custom impl). |
| false      | true     | **Impossible by construction.** Engine asserts in debug; release falls back to treating as `(false, false)`. |
| true       | false    | MC-required identity, workspace in pre-pin window (count < 256).        |
| true       | true     | MC-required identity, steady state. Sign-quant bits centered everywhere.|

The `(false, true)` cell is impossible because pinning `mean_vec` is
only performed by the projection worker after observing an MC-required
identity. EU-5 includes a regression test that asserts this combination
cannot occur via any public API.

A `MeanVecPinned { dim, doc_count }` event is appended to
`embedder_events` (see §7) on the commit that materializes the pinned
mean (and atomically performs the §0.5 re-quantize pass). This is the
only structured visibility into the pin transition.

### §0.7 Invariant summary

- Pinned `mean_vec` length in bytes equals `4 × embedder_identity.dimension`.
- f32 BLOBs stored in the index are un-centered.
- The f32 rerank distance is cosine over un-centered, L2-normed vectors.
- Centering is applied if and only if
  (`embedder_mean_centering_required == true` AND `mean_vec IS NOT NULL`);
  else the sign-quantize path runs on un-centered vectors.
- After the pin commit's atomic re-quantize pass (§0.5), every row in
  the workspace's vector tables has sign-quant bits computed under the
  same pinned mean — no row uses a different centering than any other.

---

## §1 — Loader scope

The loader is a feature-gated component of `fathomdb-embedder`
(`feature = "default-embedder"`) that materializes the pinned default-
embedder weight set into the platform user-cache directory and returns
ready-to-mmap byte buffers to the candle layer. Its surface is exactly
what `ADR-0.7.1-default-embedder-weight-fetch.md` §scope-guardrails
permits and nothing more.

**HF resolve URL pattern**:

```
https://huggingface.co/<repo>/resolve/<revision>/<file>
```

`<repo>` and `<revision>` are compile-time constants in
`fathomdb-embedder`. They are not parameterizable from caller code; this
is structurally enforced by the absence of a public setter (see
ADR-0.7.1 guardrail 1).

**Pinned identity for 0.7.1** (from EU-0 outcome,
`research.md` §6.2):

| field    | value                                                                 |
|----------|-----------------------------------------------------------------------|
| repo     | `BAAI/bge-small-en-v1.5`                                              |
| revision | `5c38ec7c405ec4b44b94cc5a9bb96e735b38267a`                            |
| name     | `fathomdb-bge-small-en-v1.5` (as recorded in `default_embedder_identity()`) |
| dim      | 384                                                                   |

**Files fetched** (and only these — nothing else):

| file               | purpose                                  | format     |
|--------------------|------------------------------------------|------------|
| `config.json`      | BertConfig (hidden_size, n_layers, ...)  | JSON       |
| `tokenizer.json`   | WordPiece tokenizer table                | JSON       |
| `model.safetensors`| BertModel weights                        | safetensors|

`pytorch_model.bin` is explicitly **NOT** fetched. safetensors is the
only acceptable weight format (candle's BertModel expects it; pickle
deserialization is a remote-code-execution risk we will not absorb for
the default embedder).

**Per-file sha256 pins** are exposed as `&'static str` constants in
`fathomdb-embedder`:

```
pub(crate) const BGE_SMALL_CONFIG_SHA256:     &str = "<64 hex>";
pub(crate) const BGE_SMALL_TOKENIZER_SHA256:  &str = "<64 hex>";
pub(crate) const BGE_SMALL_WEIGHTS_SHA256:    &str = "<64 hex>";
```

**Source of the sha values.** For LFS-tracked files (the safetensors
blob), HF's `resolve` endpoint returns the file's git-LFS SHA256 in the
`X-Linked-Etag` (or, for some CDN paths, `Etag`) header. The pinned
constant is populated during EU-3 by:

1. A one-time `curl -I` against the `resolve` URL at the pinned revision.
2. Cross-check against a locally-computed `sha256sum` on the downloaded
   blob, on at least one developer machine and one CI runner, to confirm
   the LFS-SHA equals the bytewise SHA.
3. The value is then frozen into the constant. Subsequent revision bumps
   are a separate `fathomdb-embedder` release with a new constant set.

For non-LFS files (`config.json`, `tokenizer.json`), step 1 is skipped
and the sha is computed locally and pinned the same way.

---

## §2 — HTTP transport

**Client.** `ureq` blocking client, justified in
`ADR-0.6.0-default-embedder.md` consequences (the deps F10 thread).
`reqwest` was rejected there for transitive bloat; `hyper` raw was
rejected for not buying anything `ureq` does not already give us at this
scale.

**Redirect handling.** HF's `resolve` URL responds with `302 Found`
pointing at CloudFront for LFS files. `ureq` is configured with
`redirects(N)` where `N ≥ 3` — one hop covers HF → CloudFront; the
extra budget covers occasional CloudFront → regional-edge hops without
opening a tail risk.

**Range resume.** When the loader finds a `<file>.partial` from a
prior interrupted run (see §5), it issues `Range: bytes=<offset>-` where
`<offset>` is the current size of the partial file. Servers responding
with `200` (no range support) cause the loader to discard the partial
and restart from byte 0 — this is correct behavior for any cache the
server refuses to range-serve.

**Timeouts.** Defaults:

| phase            | default | env override                                |
|------------------|---------|---------------------------------------------|
| connect          | 10s     | `FATHOMDB_EMBEDDER_CONNECT_TIMEOUT_S`       |
| read (per-call)  | 60s     | `FATHOMDB_EMBEDDER_READ_TIMEOUT_S`          |

Both overrides parse as `u64` seconds. Invalid values fall back to the
default and emit a warning event (no panic, no `unwrap`).

**Retry / backoff.** Three attempts per file, with exponential backoff
of `1s, 2s, 4s` between attempts. The retry policy applies to:

- Connect failures (DNS, TCP).
- 5xx responses.
- Read timeouts (partial body received, then timeout — resumed via
  Range on the next attempt).

It does **not** apply to:

- 4xx responses other than `408 Request Timeout` and `429 Too Many
  Requests` (those are retried). `401`/`403`/`404` fail fast.
- sha256 mismatch on the completed file (that is fatal per §6, not
  retryable).

On all-attempts-exhausted, the loader returns
`EmbedderLoadError::NetworkUnavailable { source, attempts: 3 }`.

---

## §3 — Auth tokens

The loader reads `HF_TOKEN` from the process environment at load time.
If present and non-empty, every HTTP request adds:

```
Authorization: Bearer <token>
```

If absent, no `Authorization` header is sent. The public
`BAAI/bge-small-en-v1.5` repository does not require auth; this code
path exists solely so callers who mirror weights behind a token-gated HF
proxy can still hit the default-embedder loader without bypassing it.

**No keychain fallback.** macOS Keychain / Windows Credential Manager /
libsecret are not queried. The surface stays narrow.

**No file fallback.** `~/.huggingface/token` is **not** honored. Users
who want the loader to authenticate must set `HF_TOKEN` in the engine's
process environment.

**No persistence.** `HF_TOKEN` is read once at load time, used for the
duration of that load, and never written to disk by fathomdb. This is a
documented contract in the user-facing 0.7.1 docs (EU-8): "fathomdb does
not store `HF_TOKEN` anywhere on disk; it is read from the environment
at engine open time only."

---

## §4 — Cache layout

**Primary path:**

```
<dirs::cache_dir>()/fathomdb/embedders/<model-sha-prefix>/<file>
```

- `<dirs::cache_dir>()` resolves per the `dirs` crate's platform table:
  - Linux: `$XDG_CACHE_HOME` or `~/.cache`
  - macOS: `~/Library/Caches`
  - Windows: `{FOLDERID_LocalAppData}` (typically `C:\Users\<u>\AppData\Local`)
- `<model-sha-prefix>` = first 12 hex chars of
  `sha256("<repo>@<revision>")`. For the pinned 0.7.1 identity
  (`BAAI/bge-small-en-v1.5@5c38ec7c...`) this resolves to a single
  deterministic prefix that EU-3 may hard-code as a test constant. Using
  a content-addressed prefix means revision bumps land in a different
  cache directory cleanly, without overwriting the older one.
- `<file>` is the bare filename from §1 (`config.json`,
  `tokenizer.json`, `model.safetensors`).

**HF-hub compat probe (best-effort, read-only).** Before issuing any
network request, the loader checks:

```
$HF_HOME/hub/models--<repo-encoded>/snapshots/<revision>/<file>
```

where `<repo-encoded>` is the standard HF-hub flattening (`/` → `--`,
e.g. `models--BAAI--bge-small-en-v1.5`) and `$HF_HOME` defaults to
`~/.cache/huggingface` if unset.

If the file exists AND its sha256 matches the pinned constant, the
loader copies (or, on POSIX, hard-links — same filesystem only) the file
into the fathomdb cache. The HF-hub cache is **never written to** by
fathomdb; the probe is strictly read-only.

A `DefaultEmbedderCacheHit { cache_path, sha256_verified: true }` event
is emitted in this case (see §7).

**Per-platform expansion examples** (illustrative, for cross-cite from
EU-3 tests):

| platform | resolved cache path                                                                |
|----------|------------------------------------------------------------------------------------|
| Linux    | `~/.cache/fathomdb/embedders/<prefix>/model.safetensors`                           |
| macOS    | `~/Library/Caches/fathomdb/embedders/<prefix>/model.safetensors`                   |
| Windows  | `%LOCALAPPDATA%\fathomdb\embedders\<prefix>\model.safetensors`                     |

---

## §5 — Atomic write

For each file being fetched:

1. Open `<file>.partial` in the **same directory** as the eventual
   `<file>` (so the rename is intra-directory and atomic on every
   supported filesystem).
2. Stream bytes from the HTTP response into `<file>.partial`. On Range
   resume, the open mode is `OpenOptions::append`; otherwise it is
   `create_new` to refuse to silently overwrite stale partials from a
   crashed prior run that did not match the expected sha — see §6.
3. On EOF / Content-Length reached: `fsync` the partial file
   (`File::sync_all`).
4. Compute sha256 of the partial file (§6). If sha matches, proceed;
   otherwise, delete the partial and surface `ChecksumMismatch`.
5. `rename(<file>.partial, <file>)`. POSIX `rename(2)` is atomic.
   Win32 `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` is atomic for
   files on the same volume; the cache layout in §4 guarantees same-
   volume.
6. After the rename succeeds, `fsync` the **parent directory** (POSIX
   only — Windows journaling already covers this) to make the rename's
   durability survive a power loss between the rename and the next
   `fsync`.

**Crash recovery.** On the next loader run, if `<file>.partial` exists
but `<file>` does not, the loader queries `metadata().len()` on the
partial and resumes via `Range: bytes=<len>-`. If `<file>` already
exists, the loader verifies its sha256 (§6) and short-circuits the
network path entirely.

If both `<file>` and `<file>.partial` exist, the partial is the result
of an interrupted retry after a successful prior fetch; the loader
deletes the partial and trusts the verified `<file>`.

---

## §6 — Verification

After download completes and `fsync` has been issued on
`<file>.partial`, the loader computes `sha256(<file>.partial)` using
the `sha2` crate and compares against the pinned constant from §1.

- **Match** → proceed to atomic rename (§5 step 5).
- **Mismatch** → the partial file is **removed** (`std::fs::remove_file`)
  and the loader returns
  `EmbedderLoadError::ChecksumMismatch { file, expected, actual }`.
  The rename is not performed; the cache directory remains in a state
  where the next loader run will retry from scratch.

**No "trust on first use".** There is no env var, no config knob, no
build flag, and no caller-API surface that disables sha verification.
The only way to ship a new sha is a new release of `fathomdb-embedder`
with new pinned constants. This is `ADR-0.7.1-default-embedder-weight-fetch`
§scope-guardrails item 3, mechanically enforced here.

**Algorithm.** sha256 is computed in a single streaming pass over the
file (no full-buffer read), in chunks of 64 KiB. The `sha2::Sha256`
digester is the canonical implementation; no alternate hash is accepted.

---

## §7 — Cold-load timing

Per `ADR-0.6.0-async-surface.md` Invariant D, embedder warmup runs
**synchronously** in `Engine.open`. The first-use download path is part
of warmup; its wall-time counts toward the existing `embedder_warmup_ms`
field on `OpenReport`.

For caller observability — specifically the
`ADR-0.7.1-default-embedder-weight-fetch` mandate that wire/disk
activity be visible — two new fields are added to `OpenReport`:

```rust
pub struct OpenReport {
    // existing fields ...
    pub embedder_warmup_ms: u64,                       // already exists
    pub embedder_download_ms: Option<u64>,             // NEW
    pub embedder_events: Vec<EmbedderEvent>,           // NEW
    pub embedder_mean_centering_required: bool,        // NEW (§0.6)
    pub embedder_mean_vec_pinned: bool,                // NEW (§0.6)
    // ...
}
```

`embedder_download_ms` is `None` on a cache hit (no download occurred)
and `Some(ms)` on a cold load. The duration includes connect, transfer,
fsync, and sha verification across all files in the pinned set — it is
the network/disk envelope, not just the read.

`embedder_events` is the structured event log:

```rust
pub enum EmbedderEvent {
    DefaultEmbedderDownload {
        url: String,
        bytes: u64,
        sha256: String,
        cache_path: PathBuf,
    },
    DefaultEmbedderCacheHit {
        cache_path: PathBuf,
        sha256_verified: bool,
    },
    MeanVecPinned {
        dim: u32,
        doc_count: u64,
    },
}
```

- `DefaultEmbedderDownload` is appended once per file fetched
  (config + tokenizer + weights = up to 3 events on a fully-cold cache).
- `DefaultEmbedderCacheHit` is appended once per file served from cache
  (post-verification). `sha256_verified: true` is the only legal value
  — the only way to get a false here would be to disable verification,
  which §6 prohibits.
- `MeanVecPinned` is emitted at most once per workspace lifetime, by the
  projection worker when the pin commit lands (§0.3).

Binding surfaces (EU-6) round-trip these fields verbatim; field names in
this design are the contract with EU-3 and EU-5 implementation.

---

## §8 — Endianness

Workspace targets in 0.7.1 are little-endian: `x86_64` and `aarch64` on
all three supported OSes. The safetensors weight format encodes a `dtype`
+ raw byte payload; HF's published bge-small weights are little-endian.

**Invariant.** `CandleBgeEmbedder::new` SHALL include:

```rust
debug_assert!(cfg!(target_endian = "little"),
              "fathomdb-embedder default path requires little-endian target");
```

Release builds document the contract (in the embedder crate's top-level
docstring) and do not byte-swap on load — if a BE platform somehow runs
release code, the resulting f32 vectors will be garbage and the
existing L2-norm assertion (`ADR-0.6.0-embedder-protocol` Invariant 1)
will fire before any vector reaches the store.

**Big-endian support is out of scope for 0.7.1.** Adding it would
require an explicit byte-swap pass during weight load and a CI runner
on a BE platform. That is its own ADR, deferred.

---

## §9 — Failure taxonomy

```rust
#[derive(Debug, thiserror::Error)]
pub enum EmbedderLoadError {
    #[error("network unavailable after {attempts} attempts")]
    NetworkUnavailable {
        #[source] source: ureq::Error,
        attempts: u32,
    },
    #[error("checksum mismatch on {file:?}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        file: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("cache I/O error on {path:?}")]
    CacheIoError {
        path: PathBuf,
        #[source] source: std::io::Error,
    },
    #[error("safetensors deserialization failed")]
    ModelDeserialize {
        #[source] source: candle_core::Error,
    },
    #[error("tokenizer load failed")]
    TokenizerLoad {
        #[source] source: tokenizers::Error,
    },
    #[error("lock acquisition timed out on {lock_path:?} after {waited_s}s")]
    LockTimeout {
        lock_path: PathBuf,
        waited_s: u64,
    },
}
```

**Mapping to engine open-error** (per `ADR-0.6.0-error-taxonomy` — top
level open returns a typed error, never `panic!`):

| LoaderError              | Engine surface                | Retryable? | User action                                   |
|--------------------------|-------------------------------|------------|-----------------------------------------------|
| `NetworkUnavailable`     | `EngineOpenError::Embedder`   | YES        | Re-open; check network; consider HF mirror    |
| `ChecksumMismatch`       | `EngineOpenError::Embedder`   | NO         | Investigate compromised cache/mirror; file bug|
| `CacheIoError`           | `EngineOpenError::Embedder`   | NO         | Inspect disk; permissions; full disk          |
| `ModelDeserialize`       | `EngineOpenError::Embedder`   | NO         | Cache corruption or weights/code skew         |
| `TokenizerLoad`          | `EngineOpenError::Embedder`   | NO         | Same                                          |
| `LockTimeout`            | `EngineOpenError::Embedder`   | YES        | Re-open after the holder completes            |

"Retryable by re-opening" means: a fresh `Engine.open` call may succeed
without operator intervention (e.g. network came back, the other process
finished its download). Non-retryable errors require the operator to
look at logs or filesystem state before re-opening will help.

---

## §10 — Concurrency

**Cross-process file lock.** A `<cache_dir>/.lock` sentinel file
co-located with the per-model cache directory is opened with
`fs2::FileExt::lock_exclusive`. The lock is held during the
download + verification + rename window. Cache-hit reads (sha verify
against an already-renamed `<file>`) do **not** acquire the lock; they
operate against the post-rename atomic file directly.

**Timeout.** Default `120s`, configurable via
`FATHOMDB_EMBEDDER_LOCK_TIMEOUT_S` (parsed as `u64` seconds, invalid →
default). On timeout, the loader returns
`EmbedderLoadError::LockTimeout { lock_path, waited_s: 120 }`.

The 120s default is sized to cover a worst-case cold download of the
~133 MB bge-small weight set on a typical home broadband link (~10–20
Mbps gives a ~60–100s download; pad for hash + slow start). CI on
faster pipes will rarely hit the timeout; on slower or contested links,
the operator override is the escape hatch.

**Scope.** This serializes:

- Concurrent `Engine.open` calls in the same process that both opt into
  the default embedder on a cold cache.
- Concurrent `Engine.open` calls **across processes** on the same host
  on a cold cache (e.g. parallel test runners, parallel CLI users).

**Performance implication.** On a fresh cache (CI image without
warm-up), `N` parallel test processes will serialize their first
default-embedder loads. After the first process completes, the rest hit
the cache and skip the lock entirely. The mitigation is documented in
the EU-3 cache-warmup note: CI pre-warms via
`cargo run --bin warm-embedder-cache` (or equivalent) before parallel
test launch. Without warm-up, parallel test runtime ≈ single-process
download + N × cache-hit cost. With warm-up, parallel test runtime ≈ N
× cache-hit cost. Both are acceptable; warm-up is the optimisation, not
a correctness requirement.

**Held-resource invariant.** The lock is released on:

- successful completion of download + verification + rename, OR
- any error return (RAII via `fs2::FileExt::unlock` on drop), OR
- process death (POSIX advisory locks and Win32 `LockFileEx` both
  release on close/death; the kernel cleans up).

A process holding the lock past its own death cannot wedge other
processes — this is a hard guarantee of both supported lock backends.
