# DOC-INDEX detail — `docs/`

> Long-form per-doc notes for this area of the doc tree. The thin map at
> `dev/DOC-INDEX.md` links here; **every path listed below also has its own row in
> `dev/DOC-INDEX.md`** (path + a ≤120-char purpose clause) — this file carries the
> full slice-history / decision-record prose that DOC-INDEX.md compresses away.
> Update this file (not DOC-INDEX.md) when you want to add narrative detail; update
> BOTH files when you add/rename/materially change a doc (DOC-INDEX.md row +
> this file's row, same closing commit — mirrors DOC-INDEX.md's own rule).

## `docs/` — user-facing documentation (mkdocs, `nav` in `mkdocs.yml`)

| Path | Purpose | Owning slice / AC | Last-touched |
|------|---------|-------------------|--------------|
| `docs/index.md` | Docs home | X2 (nav) | 2026-05-17 |
| `docs/getting-started/index.md` | Getting-started overview | — | 2026-05-17 |
| `docs/getting-started/quickstart.md` | Quickstart (five-operation contract) | 5/30 (new surface examples) | 2026-05-17 |
| `docs/install/python.md` | Python install | — | 2026-05-30 |
| `docs/install/typescript.md` | TypeScript install | — | 2026-05-30 |
| `docs/install/rust.md` | Rust install | — | 2026-05-17 |
| `docs/reference/index.md` | API-reference overview | — | 2026-05-17 |
| `docs/reference/python-api.md` | Python API reference, including governed `read.*`, projection configuration/derived readiness, and the pure `read.projection_status` candidate API | Slice 21/22 locally complete pending landing; earlier public API history remains in Git | 2026-08-08 |
| `docs/reference/typescript-api.md` | TypeScript API reference, including governed `read.*`, projection configuration/derived readiness, and the pure `read.projectionStatus` candidate API | Slice 21/22 locally complete pending landing; earlier public API history remains in Git | 2026-08-08 |
| `docs/reference/cli.md` | CLI reference (recovery verbs CLI-only); 34 documents the `doctor dump-mutations` op-store read-back diagnostic + `--json` example; 0.8.20 Slice 5d adds `doctor orphan-provenance` (read-only per-`source_id` census; exit 65 on `unerasable_rows > 0`) | 34 (dump-mutations); 0.8.20 Slice 5d (R-20-E8) | 2026-07-19 |
| `docs/reference/errors.md` | Error reference (taxonomy) | per-binding error-class adds | 2026-05-17 |
| `docs/reference/config.md` | Config reference | — | 2026-05-17 |
| `docs/concepts/index.md` | Concepts overview; identifies the unpublished 0.8.22 candidate versus published 0.8.21 | 0.8.22 documentation correctness | 2026-08-08 |
| `docs/embedder.md` | Default embedder | — | 2026-06-01 |
| `docs/compatibility/index.md` | Compatibility matrix | 40 (compat events) | 2026-05-17 |
| `docs/operations/index.md` | Operations guide | — | 2026-07-19 |
| `docs/operations/erasure.md` | **Erasure boundary** — what `erase_source`/`purge` guarantee and, explicitly, what they do NOT (copies, SQLite free pages absent `VACUUM`, CoW/snapshotted filesystems, backups); the retention-exempt erasure-audit record; the non-PII `source_id` rule with its CORRECTED rationale (design defect D-A: v4 §3.6's "audit retains `source_id` permanently" premise was verified FALSE and is SUPERSEDED IN PART — the real basis is an unswept audit row, now made durable) + the `derive_logical_id` case-folding note (D-C); `doctor orphan-provenance` usage | 0.8.20 Slice 5d (R-20-E4/E8, design §4 item 12) | 2026-07-19 |
| `docs/guides/index.md` | Guides hub (structured-hit / retrieve examples land here) | 5/30 add examples | 2026-06-04 |
| `docs/guides/structured-search-hits.md` | Structured `SearchHit` usage guide (id/kind/body/score/branch; Py + TS) | 5 (G1); 10 (score → RRF) | 2026-06-03 |
| `docs/guides/retrieve-by-id.md` | Retrieve-by-id guide — `read.get`/`read.get_many` point lookup by `logical_id` (active-only) + `read.collection`/`read.mutations` paginated op-store read-back (mandatory limit + after-id cursor); Py + TS | 30 (G2/G3) | 2026-06-04 |
| `docs/guides/hybrid-search-filtering.md` | Hybrid search guide — G9 RRF ranking (documented behavior-compat event) + G10 `SearchFilter` metadata filtering, Py + TS examples | 10 (G9/G10) | 2026-06-03 |
| `docs/positions/index.md` | Positions hub | — | 2026-05-01 |
| `docs/positions/sdk-parity.md` | Position: SDK parity (guarantee carried forward by 25) | 25 | 2026-05-01 |
| `docs/positions/recovery-surface.md` | Position: recovery surface (denylist, CLI-only) | preserved by 25/30 | 2026-05-01 |
| `docs/positions/tokenizer-policy.md` | Position: tokenizer policy | 5 (FTS5 default upgrade) | 2026-05-01 |
| `docs/positions/embedder-identity.md` | Position: embedder identity | — | 2026-05-01 |
| `docs/release-notes/0.6.0.md` | 0.6.0 historical release notes; points to published 0.8.21 as current | 0.8.22 documentation correctness | 2026-08-08 |
| `docs/release-notes/0.6.1.md` | 0.6.1 historical release notes; points to published 0.8.21 as current | 0.8.22 documentation correctness | 2026-08-08 |
| `docs/release-notes/0.8.0.md` | 0.8.0 historical release notes; points to published 0.8.21 as current | 0.8.22 documentation correctness | 2026-08-08 |
| `dev/releases/0.8.0.md` | **0.8.0 internal release record** — engineering companion to the user notes: behavior-compat events, AC-075/076 gate restructure (◆ B-1), CI split, verification posture; every claim traces to a measured Slice-40 result or signed ADR | 40/GA-2 | 2026-06-08 |
