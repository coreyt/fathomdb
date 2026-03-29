older /tmp/memex-gap-map.md
      1 -# Memex Gap Map Against `fathomdb` Typed Write, Read Execution, and Detailed Supersession
      2 -
      3 -## Scope
      4 -
      5 -This note evaluates `fathomdb` as if I were a potential demanding user:
      6 -**Chief Engineer for Memex's datastore**.
      7 -
      8 -It compares Memex's current storage requirements against:
      9 -
     10 -- `dev/design-typed-write.md`
     11 -- `dev/design-read-execution.md`
     12 -- `dev/design-detailed-supersession.md`
     13 -
     14 -The question is not "is this elegant." The question is:
     15 -
     16 -> What would make `fathomdb` a world-class operational and semantic memory
     17 -> system for local, personal AI agents like Memex?
     18 -
     19 -## Updated Bottom Line
     20 -
     21 -My view of `fathomdb` improved materially after reading
     22 -`design-detailed-supersession.md`.
     23 -
     24 -Before that document, I considered update/supersession/lifecycle semantics one
     25 -of the largest gaps. That is no longer true. The supersession design gives
     26 -`fathomdb` a serious core story for:
     27 -
     28 -- append-oriented history
     29 -- atomic replace and retire operations
     30 -- chunk and FTS cleanup on text-changing replacements
     31 -- explicit runtime-row status transitions for runs/steps/actions
     32 -- transaction ordering that is trying to preserve recoverability rather than
     33 -  merely pass tests
     34 -
     35 -That is the right direction for a real agent datastore.
     36 -
     37 -But as a Memex datastore owner, I would still say:
     38 -
     39 -- `fathomdb` is now **more credible as a future storage substrate**
     40 -- it is still **not yet sufficient for Memex-wide adoption**
     41 -- the remaining gaps are now less about "missing basic update semantics" and
     42 -  more about **operational depth, semantic integrity, richer result models, and
     43 -  broader typed coverage**
     44 -
     45 -## What The Supersession Design Fixes
     46 -
     47 -### 1. Replace and retire are now first-class engine primitives
     48 -
     49 -This is a major improvement.
     50 -
     51 -The supersession design explicitly separates:
     52 -
     53 -- insert
     54 -- replace
     55 -- retire
     56 -
     57 -and makes them typed engine concepts rather than incidental SQL behavior.
     58 -
     59 -Evidence:
     60 -
     61 -- `dev/design-detailed-supersession.md:113-180`
     62 -
     63 -Why this matters for Memex:
     64 -
     65 -- Memex already depends on versioning/history semantics in `goals_history` and
     66 -  `item_versions`
     67 -- Memex also needs safe "remove without destroy-history" behavior for bad data,
     68 -  soft delete, and lifecycle transitions
     69 -
     70 -Evidence:
     71 -
     72 -- `src/memex/migrations.py:40-73`
     73 -- `src/memex/migrations.py:228-235`
     74 -- `src/memex/memory/store.py:401-463`
     75 -
     76 -### 2. Chunk/FTS lifecycle is finally treated as a correctness problem
     77 -
     78 -The supersession doc correctly identifies that replacing a node without handling
     79 -old chunks and FTS rows causes silent stale retrieval.
     80 -
     81 -That is exactly the kind of bug that makes an agent memory system look correct
     82 -in demos and untrustworthy in production.
     83 -
     84 -Evidence:
     85 -
     86 -- `dev/design-detailed-supersession.md:187-299`
     87 -
     88 -This is a strong design move. It upgrades `fathomdb` from "storage engine with
     89 -append semantics" toward "retrieval system that understands projection
     90 -correctness."
     91 -
     92 -### 3. Runtime transitions are becoming real typed writes
     93 -
     94 -Adding `upsert: bool` to `RunInsert`, `StepInsert`, and `ActionInsert` is
     95 -important because operational state transitions are not optional in real agent
     96 -systems.
     97 -
     98 -Evidence:
     99 -
    100 -- `dev/design-detailed-supersession.md:303-367`
    101 -- `dev/design-detailed-supersession.md:585-599`
    102 -
    103 -This is still much narrower than Memex's runtime surface, but it is the first
    104 -credible step beyond node storage.
    105 -
    106 -### 4. Transaction ordering is operationally literate
    107 -
    108 -The ordered sequence of retires first, then replaces/inserts, then chunks, then
    109 -FTS rows is the kind of detail I would expect from a system trying to be
    110 -recoverable under failure.
    111 -
    112 -Evidence:
    113 -
    114 -- `dev/design-detailed-supersession.md:454-486`
    115 -
    116 -That is a positive signal.
    117 -
    118 -## Updated Capability Matrix
    119 -
    120 -| Memex storage need | Evidence in Memex | `fathomdb` planned coverage | Updated assessment |
    121 -|---|---|---|---|
    122 -| SQLite canonical authority | `src/memex/store.py:860-866`, `src/memex/store.py:879-956` | Canonical rows plus derived projections i
         nside one write path: `dev/design-typed-write.md:17-30`, `dev/design-typed-write.md:96-105` | Direct fit |
    123 -| Single-writer discipline with WAL-friendly reads | `src/memex/store.py:862-865` | Explicit in scope for writes and reads: `dev/desi
         gn-typed-write.md:19-29`, `dev/design-read-execution.md:22-30` | Direct fit |
    124 -| Engine-owned FTS derivation and cleanup | Memex maintains SQLite FTS from canonical tables: `src/memex/migrations.py:177-196`, `src
         /memex/migrations.py:375-440` | FTS derived in engine; supersession now includes stale FTS cleanup on replace/retire: `dev/design-typ
         ed-write.md:96-105`, `dev/design-detailed-supersession.md:221-243`, `dev/design-detailed-supersession.md:271-299` | Strong fit |
    125 -| Vector search as derived capability | Memex rebuilds/search projections from SQLite authority: `src/memex/store.py:986-1045`, `src/
         memex/memory/migrate_to_ladybug.py:180-202` | Derived capability, but vector cleanup on chunk delete is deferred: `dev/design-typed-w
         rite.md:107-115`, `dev/design-read-execution.md:123-134`, `dev/design-detailed-supersession.md:541-544` | Partial fit |
    126 -| Explicit provenance for write and excision | Memex stores provenance broadly: `src/memex/migrations.py:147-170`, `src/memex/migrati
         ons.py:795-1253` | Strong write-path `source_ref` discipline: `dev/design-typed-write.md:88-95`, `dev/design-detailed-supersession.md
         :92-100` | Direct fit |
    127 -| Replace / retire / append-oriented history | Memex needs history and lifecycle control: `src/memex/migrations.py:40-73`, `src/memex
         /memory/store.py:440-463` | Now explicitly designed: `dev/design-detailed-supersession.md:113-180`, `dev/design-detailed-supersession
         .md:614-627` | Strong fit |
    128 -| Soft-delete plus restore plus purge lifecycle | Memex supports delete, restore, purge: `src/memex/memory/store.py:401-436` | Retire
          is covered; restore and purge semantics are still absent from the design set | Partial fit |
    129 -| Chunk history across text revisions | Memex keeps version snapshots today: `src/memex/memory/store.py:440-463` | Old chunks are del
         eted on replace; chunk-level history is explicitly deferred: `dev/design-detailed-supersession.md:249-260`, `dev/design-detailed-supe
         rsession.md:533-535` | Missing for forensic-grade memory |
    130 -| Retire/excise reason as durable queryable history | Memex needs auditability and operational attribution: `src/memex/migrations.py:
         99-127` | Retire `source_ref` only drives warnings; no durable retire event table in v1: `dev/design-detailed-supersession.md:419-428
         ` | Missing |
    131 -| Graph integrity under retire/replace | Memex needs relationship truth and repairability | Replace preserves logical edge continuity
         , which is good; retire can leave logically dangling edges, detection deferred to `check_semantics`: `dev/design-detailed-supersessio
         n.md:371-417`, `dev/design-detailed-supersession.md:527-532` | Partial fit with operational risk |
    132 -| Runtime state transitions | Memex persists scheduler/task/meeting/runtime state broadly: `src/memex/migrations.py:293-690`, `src/me
         mex/migrations.py:695-1253` | Runs/steps/actions now get update semantics: `dev/design-detailed-supersession.md:303-367` | Improved,
         but still narrow |
    133 -| Broad durable runtime tables beyond runs/steps/actions | Memex stores scheduler, intake, settings, meetings, notifications, audit,
         and many `wm_*` tables | Typed coverage remains much narrower than Memex's actual state surface | Missing for current plan |
    134 -| Rich heterogeneous read models | Memex reads many non-node records from SQLite-backed stores and APIs | Read execution still return
         s narrow node rows first and defers richer decoders: `dev/design-read-execution.md:63-79`, `dev/design-read-execution.md:107-121`, `d
         ev/design-read-execution.md:198-204` | Missing |
    135 -| Graceful degraded operation when vector capability is absent | Memex often degrades rather than fails: `src/memex/store.py:883-955`
         , `src/memex/store.py:974-984` | Current read plan prefers explicit capability error: `dev/design-read-execution.md:123-134` | Mismat
         ch |
    136 -| Deterministic rebuild and strong admin tooling | Memex relies on rebuild/recovery/admin flows: `src/memex/store.py:986-1045`, `src/
         memex/memory/lbug2sqlite_recovery.py:15-122`, `src/memex/backup.py:26-227` | The design direction helps, but these slices do not yet
         define the needed admin/runtime contract | Directionally aligned, operationally incomplete |
    137 -
    138 -## Revised View By Memex Area
    139 -
    140 -### 1. Knowledge Items, Chunks, FTS, and Search
    141 -
    142 -Updated view:
    143 -
    144 -- This area is now substantially stronger than my prior assessment.
    145 -- `fathomdb` is no longer missing the core "safe text replace" semantics that a
    146 -  real memory system needs.
    147 -
    148 -What is now good:
    149 -
    150 -- append-oriented node history
    151 -- chunk cleanup on replace
    152 -- FTS cleanup on replace and retire
    153 -- explicit caller-visible chunk policy
    154 -
    155 -What still blocks a "world-class" assessment:
    156 -
    157 -- vector cleanup is deferred
    158 -- chunk history is deleted rather than archived
    159 -- confidence is still out of scope
    160 -
    161 -Evidence:
    162 -
    163 -- `dev/design-detailed-supersession.md:201-260`
    164 -- `dev/design-detailed-supersession.md:539-544`
    165 -
    166 -Chief Engineer judgment:
    167 -
    168 -- Good foundation for canonical knowledge storage
    169 -- Not yet world-class semantic memory because projection correctness is still
    170 -  stronger for FTS than for vectors, and historical text reconstruction remains
    171 -  underpowered
    172 -
    173 -### 2. Versioning, Supersession, and Lifecycle
    174 -
    175 -Updated view:
    176 -
    177 -- This moved from "major gap" to "credible core capability."
    178 -
    179 -What is now good:
    180 -
    181 -- explicit replace and retire
    182 -- atomic supersession
    183 -- append-only active/historical invariant
    184 -- runtime upsert path for runs/steps/actions
    185 -
    186 -What still blocks Memex-grade lifecycle completeness:
    187 -
    188 -- no restore operation
    189 -- no purge/compaction policy
    190 -- no durable retire event table
    191 -
    192 -Evidence:
    193 -
    194 -- `dev/design-detailed-supersession.md:71-110`
    195 -- `dev/design-detailed-supersession.md:113-180`
    196 -- `dev/design-detailed-supersession.md:419-428`
    197 -
    198 -Chief Engineer judgment:
    199 -
    200 -- Strong step forward
    201 -- Still incomplete for a local agent system that must support undo,
    202 -  correction-review, operator trust, and retention policy
    203 -
    204 -### 3. Graph Integrity and Semantic Truth
    205 -
    206 -Updated view:
    207 -
    208 -- The replace semantics for edges are better than I expected because joins are on
    209 -  `logical_id`, so persistent relationships naturally follow the active node
    210 -  version on replace.
    211 -
    212 -That is a good design choice.
    213 -
    214 -Evidence:
    215 -
    216 -- `dev/design-detailed-supersession.md:383-401`
    217 -
    218 -But retire semantics remain underpowered:
    219 -
    220 -- retiring a node can leave dangling active edges
    221 -- detection is deferred to integrity tooling
    222 -- the engine does not yet help callers bundle node-retire and related-edge
    223 -  retire/update policy into a safer higher-level operation
    224 -
    225 -Evidence:
    226 -
    227 -- `dev/design-detailed-supersession.md:403-417`
    228 -- `dev/design-detailed-supersession.md:527-532`
    229 -
    230 -Chief Engineer judgment:
    231 -
    232 -- World-class agent memory should make semantic graph damage hard to create and
    233 -  easy to detect.
    234 -- `fathomdb` is on the right path, but retire operations still feel too easy to
    235 -  misuse.
    236 -
    237 -### 4. Runtime and Operational State
    238 -
    239 -Updated view:
    240 -
    241 -- `RunInsert`/`StepInsert`/`ActionInsert` upsert support is good and necessary.
    242 -- It is still far short of Memex's actual runtime surface.
    243 -
    244 -Memex needs durable storage for:
    245 -
    246 -- scheduler and task execution
    247 -- intake/replay
    248 -- notifications
    249 -- settings
    250 -- meetings and meeting intelligence
    251 -- world-model/planning sidecars
    252 -
    253 -Evidence:
    254 -
    255 -- `src/memex/migrations.py:293-690`
    256 -- `src/memex/migrations.py:695-1253`
    257 -
    258 -Chief Engineer judgment:
    259 -
    260 -- `fathomdb` is now beginning to look like a usable engine core.
    261 -- It still does not yet look like a whole datastore for an agent product with
    262 -  Memex's breadth.
    263 -
    264 -### 5. Read Models and Query Surface
    265 -
    266 -Updated view:
    267 -
    268 -- This remains one of the biggest remaining gaps.
    269 -
    270 -No matter how good the write path is, Memex cannot use `fathomdb` as a primary
    271 -datastore if reads are still mostly node-shaped.
    272 -
    273 -Evidence:
    274 -
    275 -- `dev/design-read-execution.md:63-79`
    276 -- `dev/design-read-execution.md:107-121`
    277 -- `dev/design-read-execution.md:198-204`
    278 -
    279 -Chief Engineer judgment:
    280 -
    281 -- For Memex, world-class memory requires first-class read models for:
    282 -  - graph-like knowledge objects
    283 -  - runtime objects
    284 -  - temporal/history views
    285 -  - operator diagnostics
    286 -
    287 -Until `fathomdb` has those, it remains a promising engine core rather than a
    288 -complete agent datastore.
    289 -
    290 -## What I Would Now Ask `fathomdb` To Build Next
    291 -
    292 -These are not generic ideas. These are the next things I would ask for if I
    293 -were evaluating `fathomdb` as the future memory substrate under Memex.
    294 -
    295 -### 1. Finish supersession all the way to operational completeness
    296 -
    297 -Add:
    298 -
    299 -- `restore` semantics for retired rows
    300 -- purge/retention semantics
    301 -- durable retire/excise event records, not just warnings
    302 -- vector projection cleanup alongside chunk/FTS cleanup
    303 -
    304 -Reason:
    305 -
    306 -- world-class operational memory needs reversible correction, not just safe
    307 -  retirement
    308 -
    309 -### 2. Add optional chunk archival, not only chunk deletion
    310 -
    311 -The current chunk delete policy is operationally clean, but it leaves a gap for
    312 -forensic and semantic audit use cases.
    313 -
    314 -I would want:
    315 -
    316 -- `ChunkPolicy::Archive` or equivalent
    317 -- the ability to reconstruct "what text was active at time T"
    318 -
    319 -Reason:
    320 -
    321 -- local personal AI agents are long-lived and trust-sensitive
    322 -- operators will eventually ask "why did it used to believe this?"
    323 -
    324 -### 3. Make semantic integrity a first-class product promise
    325 -
    326 -`check_semantics` is the right direction. Expand it aggressively.
    327 -
    328 -It should detect at minimum:
    329 -
    330 -- dangling active edges after retire
    331 -- stale vector rows after chunk replacement
    332 -- rows retired without usable provenance
    333 -- mismatches between active-node text and search projections
    334 -
    335 -Reason:
    336 -
    337 -- world-class agent memory is not just fast reads and safe writes
    338 -- it is also continuous semantic self-auditing
    339 -
    340 -### 4. Widen the typed runtime surface fast
    341 -
    342 -The most important storage gap for Memex is no longer supersession. It is
    343 -coverage.
    344 -
    345 -I would prioritize typed support for:
    346 -
    347 -- task/scheduler state
    348 -- ingest/replay runs
    349 -- notification/operator artifacts
    350 -- one planning/intent/action lineage slice
    351 -
    352 -Reason:
    353 -
    354 -- that is the minimum path from "knowledge engine" to "agent datastore"
    355 -
    356 -### 5. Widen reads beyond node rows
    357 -
    358 -I would want:
    359 -
    360 -- typed result families for runtime tables
    361 -- history/active views
    362 -- diagnostic/explain surfaces for planners and operators
    363 -
    364 -Reason:
    365 -
    366 -- Memex is not just retrieving knowledge snippets
    367 -- it is continuously reading durable operational and semantic state
    368 -
    369 -### 6. Revisit degraded-mode policy
    370 -
    371 -The current explicit capability-error policy for unavailable vector support is
    372 -clean, but too strict for a local personal agent system.
    373 -
    374 -I would want:
    375 -
    376 -- degrade to structured and FTS retrieval where possible
    377 -- explicit surfaced degraded status
    378 -- hard failure only when the query truly cannot be satisfied otherwise
    379 -
    380 -Reason:
    381 -
    382 -- local agents must stay useful under partial capability loss
    383 -
    384 -## Revised Adoption View
    385 -
    386 -### What Moved From Gap To Strength
    387 -
    388 -- explicit supersession model
    389 -- typed retire operations
    390 -- atomic chunk and FTS cleanup on replace/retire
    391 -- runtime-row upsert path for runs/steps/actions
    392 -- more operationally serious transaction discipline
    393 -
    394 -### What Is Still The Main Reason Memex Would Not Adopt It Yet
    395 -
    396 -- read models are still too narrow
    397 -- runtime typed coverage is still too narrow
    398 -- restore/retention/event-history semantics are incomplete
    399 -- vector projection lifecycle is still behind FTS lifecycle
    400 -- semantic-integrity guarantees still lean too heavily on future tooling
    401 -
    402 -## Final Judgment
    403 -
    404 -As Chief Engineer for Memex's datastore, I would now describe `fathomdb` this
    405 -way:
    406 -
    407 -- It is no longer just an interesting architecture.
    408 -- It is becoming a credible core for a world-class local agent memory system.
    409 -- The supersession design is a real quality inflection point.
    410 -- But to become world-class for local, personal AI agents, it must now focus on:
    411 -  - operational completeness
    412 -  - semantic integrity
    413 -  - richer typed reads
    414 -  - broader runtime state coverage
    415 -  - durable correction and auditability
    416 -
    417 -If `fathomdb` keeps the same level of rigor it showed in
    418 -`design-detailed-supersession.md` and applies it to vector lifecycle, restore
    419 -semantics, integrity auditing, and runtime-table breadth, then it moves from
    420 -"possible future fit for Memex" to "serious candidate for Memex's long-term
    421 -memory substrate."

