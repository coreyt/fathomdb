# Context-clarity snapshot — `post` (bb64a2d4, 2026-06-26T19:32:19Z)

Token counts are estimates: ceil(bytes/4). Re-run `scripts/repo-prune/bin/context-clarity.sh <label>` and diff JSON for deltas.

| Metric | Value |
|---|---|
| dev/ files (ex-caches) | 767 (79517567 bytes) |
| dev/ .md files | 622 (7534226 bytes, ~1883557 tok) |
| live-path .md (ex archive/) | 573 (6781069 bytes, ~1695268 tok) |
| archive/ .md | 49 (753157 bytes) |
| runs/ zone files | 168 (13275503 bytes); md=88 json=64 log=0 txt=0 |
| DOC-INDEX.md | 60371 bytes, ~15093 tok, 171 table rows |
| cold-start orient set | 33 files, 346944 bytes, ~86736 tok |
| memory MEMORY.md index | 15084 bytes, ~3771 tok, 35 entries |
| memory/ dir (all .md) | 36 files, 142802 bytes |

## Search signal-to-noise (live-path .md files matching; ledger = runs/+prompts/)
| Query | Total files | Ledger-zone | Core |
|---|---|---|---|
| CE-rerank | 24 | 10 | 14 |
| rerank | 170 | 92 | 78 |
| graphrag | 64 | 39 | 25 |
| recall floor | 51 | 27 | 24 |
| logical_id | 84 | 38 | 46 |
| mem0 | 82 | 39 | 43 |
| RRF | 114 | 62 | 52 |
| GraphRAG | 64 | 39 | 25 |
