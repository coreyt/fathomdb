# Context-clarity snapshot — `post` (fe2734e9, 2026-06-26T18:45:17Z)

Token counts are estimates: ceil(bytes/4). Re-run `dev/scripts/context-clarity.sh <label>` and diff JSON for deltas.

| Metric | Value |
|---|---|
| dev/ files (ex-caches) | 774 (79574378 bytes) |
| dev/ .md files | 627 (7581165 bytes, ~1895292 tok) |
| live-path .md (ex archive/) | 578 (6828008 bytes, ~1707002 tok) |
| archive/ .md | 49 (753157 bytes) |
| runs/ zone files | 168 (13275503 bytes); md=88 json=64 log=0 txt=0 |
| DOC-INDEX.md | 60371 bytes, ~15093 tok, 171 table rows |
| cold-start orient set | 33 files, 346944 bytes, ~86736 tok |
| memory MEMORY.md index | 18323 bytes, ~4581 tok, 43 entries |
| memory/ dir (all .md) | 46 files, 192430 bytes |

## Search signal-to-noise (live-path .md files matching; ledger = runs/+prompts/)
| Query | Total files | Ledger-zone | Core |
|---|---|---|---|
| CE-rerank | 28 | 10 | 18 |
| rerank | 175 | 92 | 83 |
| graphrag | 67 | 39 | 28 |
| recall floor | 53 | 27 | 26 |
| logical_id | 86 | 38 | 48 |
| mem0 | 84 | 39 | 45 |
| RRF | 115 | 62 | 53 |
| GraphRAG | 68 | 39 | 29 |
