# Design Note: Encryption At Rest and In Motion

## Status

Not implemented. This document captures research and design considerations
for future encryption capabilities.

## 1. Encryption At Rest

### Current State

FathomDB stores data in plaintext SQLite database files. Security relies on
filesystem permissions, flock-based exclusive write access, and
operational/procedural controls (safe export with SHA-256 checksums, integrity
checks, provenance tracking).

### SQLite Encryption Options

#### SQLCipher (recommended path)

Open-source fork of SQLite providing 256-bit AES full-database encryption.

- **rusqlite integration**: Direct support via `bundled-sqlcipher` feature.
  The Cargo.toml change would be:
  ```toml
  # Before
  rusqlite = { version = "0.32.1", features = ["bundled", "load_extension", "backup"] }
  # After
  rusqlite = { version = "0.32.1", features = ["bundled-sqlcipher", "load_extension", "backup"] }
  ```
- **Crypto dependency**: Requires linking against OpenSSL/LibreSSL, or use
  `bundled-sqlcipher-vendored-openssl` to vendor OpenSSL.
- **License**: BSD-style.
- **Key management**: After opening a connection, issue `PRAGMA key = '...';`
  before any other operations. The primary code change would be in
  `sqlite.rs:open_connection()` to accept and apply the key.

#### SQLite SEE (official, commercial)

Official encryption extension from the SQLite authors.

- **Cost**: US $2,000 perpetual source code license.
- **Algorithms**: RC4, AES-128 OFB, AES-128 CCM, AES-256 OFB.
- **Rust integration**: No direct rusqlite feature. Requires manual
  compilation and linking against a custom SQLite build.

#### SQLite3 Multiple Ciphers (open-source, multi-algorithm)

Supports six cipher schemes including ChaCha20-Poly1305, a
SQLCipher-compatible mode, and AES variants.

- **License**: LGPL v3+.
- **Rust integration**: No direct rusqlite feature. Manual compile and link.

### Compatibility with Current Extensions

Encryption operates at the SQLite page level, below the virtual table and
extension layer. Extensions operate above it.

| Extension    | SQLCipher Compatible? | Notes                                        |
|--------------|-----------------------|----------------------------------------------|
| FTS5         | Yes                   | SQLCipher compiles with FTS5 support         |
| WAL mode     | Yes                   | Fully supported                              |
| sqlite-vec   | Likely yes            | Standard extension mechanism; needs verified |

**Risk**: The sqlite-vec + SQLCipher combination is not explicitly documented
or tested upstream. A verification spike should precede any commitment.

### Implementation Considerations

- All PRAGMA configuration in `bootstrap.rs:initialize_connection()` must
  follow the `PRAGMA key` statement, not precede it.
- Backup operations (`rusqlite backup` feature) work with SQLCipher but
  require keying both source and destination connections.
- Safe export manifests (SHA-256 checksums) remain valid — checksums are
  computed over the decrypted logical content, not the encrypted pages.
- Key management strategy (environment variable, file, KMS) is an open
  design question.

## 2. Encryption In Motion

### Current State

FathomDB has no network transport layer. All access paths are local:

- **Python to Rust**: PyO3 in-process binding. No wire protocol.
- **Go to Rust**: JSON-over-stdio bridge (`fathomdb-admin-bridge`). Local
  subprocess with stdin/stdout pipes.
- **Direct Rust**: In-process library calls.

### Network-Exposed API (future)

If fathomdb exposes an API server (HTTP/gRPC), encrypted connections would be
handled at the application layer with standard TLS, not a SQLite plugin.

Typical approaches by language:

- **Rust**: `rustls` or `native-tls` with `axum`, `tonic`, or `hyper`.
- **Go**: stdlib `crypto/tls` (the integrity tool could grow an API surface).
- **Python**: ASGI server with TLS termination, or reverse proxy (nginx, caddy).

This is independent of the database layer. FathomDB would serialize query
results and serve them over TLS like any other service. No SQLite plugin
compatibility concerns.

### Node Replication (future)

Replication introduces a genuine network transport requirement.

#### Replication Approaches

| Approach                | How It Works                              | Encryption               |
|-------------------------|-------------------------------------------|--------------------------|
| Litestream              | Streams WAL frames to S3/NFS/SFTP         | TLS to remote storage    |
| LiteFS                  | FUSE-based replication (Fly.io)           | TLS between nodes        |
| cr-sqlite (CRDTs)       | Mergeable changesets synced between nodes  | Application provides TLS |
| mvSQLite                | SQLite on FoundationDB                    | FoundationDB's TLS       |
| Custom WAL shipping     | Ship WAL files between nodes              | TLS on chosen transport  |

#### FathomDB-Specific Replication Considerations

The safe export system (`ExportManifest` with SHA-256 checksums) already
provides a verified snapshot mechanism. A replication strategy could build on
this: ship encrypted, integrity-checked exports over TLS to replicas, which
import and regenerate projections (including vector embeddings) locally.

**Key architectural question**: read replicas only, or multi-writer?

- **Read replicas**: The single-writer design (flock-based exclusive write
  access) works naturally. Ship WAL or export snapshots to read-only replicas.
- **Multi-writer**: Would require fundamental rethinking of the concurrency
  model. CRDT-based approaches (cr-sqlite) could enable this but add
  significant complexity.

#### Projection Regeneration on Replicas

Vector embeddings are derived data, not canonical. Replicas have two options:

1. **Ship embeddings with the snapshot**: Larger transfer, but replicas are
   immediately query-ready.
2. **Regenerate on arrival**: Smaller transfer, but replicas need access to
   the embedding generator and time to re-embed. Consistent with the
   architecture principle that embeddings are recoverable projections.

FTS5 indexes are similarly derived and can be rebuilt from canonical data on
the replica side.

## 3. Combined At-Rest + In-Motion Strategy

For a fully encrypted deployment:

1. **At rest**: SQLCipher encrypts the database file on each node.
2. **In motion**: TLS encrypts data between nodes (replication) and between
   clients and the API server.
3. **Key management**: Separate concerns — database encryption key (per-node
   or shared) and TLS certificates (standard PKI or mutual TLS).

This layered approach means each concern is handled by purpose-built tooling
rather than a single mechanism trying to solve both problems.

## 4. Open Questions

- Key management strategy for SQLCipher (env var, file, KMS integration).
- Whether sqlite-vec works correctly with SQLCipher (needs spike).
- Whether replication should be WAL-based (continuous) or snapshot-based
  (periodic).
- Read-replica vs multi-writer requirements.
- Whether encrypted backups should use SQLCipher's encrypted pages directly
  or decrypt-then-re-encrypt for the destination.
