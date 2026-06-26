# Context-clarity snapshot — `baseline` (25541d88, 2026-06-26T18:16:01Z)

Token counts are estimates: ceil(bytes/4). Re-run `dev/scripts/context-clarity.sh <label>` and diff JSON for deltas.

| Metric | Value |
|---|---|
| dev/ files (ex-caches) | 1282 (169779622 bytes) |
| dev/ .md files | 730 (13997336 bytes, ~3499334 tok) |
| live-path .md (ex archive/) | 728 (13892025 bytes, ~3473007 tok) |
| archive/ .md | 2 (105311 bytes) |
| runs/ zone files | 670 (100993830 bytes); md=193 json=266 log=179 txt=3 |
| DOC-INDEX.md | 59699 bytes, ~14925 tok, 169 table rows |
| cold-start orient set | 32 files, 340747 bytes, ~85187 tok |
| memory MEMORY.md index | 18323 bytes, ~4581 tok, 43 entries |
| memory/ dir (all .md) | 46 files, 192430 bytes |

## Search signal-to-noise (live-path .md files matching; ledger = runs/+prompts/)
| Query | Total files | Ledger-zone | Core |
|---|---|---|---|
| CE-rerank | 25 | 11 | 14 |
| rerank | 194 | 110 | 84 |
| graphrag | 66 | 41 | 25 |
| recall floor | 61 | 36 | 25 |
| logical_id | 100 | 52 | 48 |
| mem0 | 87 | 44 | 43 |
| RRF | 125 | 74 | 51 |
| GraphRAG | 67 | 41 | 26 |
