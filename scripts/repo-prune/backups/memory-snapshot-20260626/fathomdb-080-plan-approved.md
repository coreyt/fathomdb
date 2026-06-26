---
name: fathomdb-080-plan-approved
description: HITL approved the 0.8.0 implementation plan on 2026-06-02; Slices 0, 5, 10, 15 (G0 KEYSTONE), 20 (G8), 21 (pyright-zero cleanup), 25 (governed-surface AC-074), 30 (G2/G3 read.* verbs @ 52ceab3), 31 (G0 identity re-scope = logical_id-alone @ b4e90c8), 32 (graph-model ADR @ e1827c4), 27 (Rust-facade governed-surface pin, Q5=BIND-RUST; orig [P1]→fix-1 HITL Option B operator-feature-gate→fix-1 [P1]→fix-2; CLOSED @ e4dcdd1) AND 33 (op-store cursor+limit hardening, step-13 (collection_name,id) index, SCHEMA_VERSION 12→13 @ 58664d9) all CLOSED on local main. Slice 30's lone codex [P2] (read.get ambiguous under multi-kind-per-logical_id) was root-caused to the compound (logical_id,kind) active-unique key, HITL-SIGNED a re-scope to logical_id-alone, executed as Slice 31 (codex clean PASS) — resolving the [P2] with zero read-API change. Slice 32's graph-model ADR (ACCEPTED; H1 neutral-both + H3 reserve-edge-enrichment-additive HITL-SIGNED) CONFIRMED logical_id-alone holds for the graph aspect too. Identity model settled through both lenses. 34 (CLI op-store read-back, `doctor dump-mutations`; implementer skipped §9, orchestrator §9 caught a [P2] pagination-clamp bug → fix-1 → codex PASS @ fix-1 merge `8212f65`, closeout `77c87d0`) CLOSED — reserved-gap band now EXHAUSTED (27·31·32·33·34). Slice 35 **CLOSED 2026-06-06 (HITL-signed; codex §9 PASS, 2×[P3] reconciled)** — HITL-SPLIT from four ADRs to two: shipped graph-traversal-scope (F1) + filter-grammar (G4/F3); F9 confidence + F5 fielded-FTS moved to **deferred post-0.8.0 Slice 46** (experiment-gated framing ADRs; do NOT spawn 46 in the 0.8.0 campaign). **Sign-off qualifications (honored):** (a) **filter-grammar ACCEPTED**, but G4↔G10 unification recorded as NEEDED future work (reserved-gap 37, affects both G4+G10, touches SearchFilter) — not optional; (b) **graph-traversal-scope signed as 0.8.1 ROADMAP DIRECTION not a frozen G-gap ADR** — recorded in NEW `dev/roadmap/0.8.1.md`, revisable when the 0.8.1 graph work opens; **graph work retargeted 0.8.x → 0.8.1**. Merge `a6bae4f`, close `1a585a9`. **Slice 40 (GA verification) RAN Phase A → HALTED to HITL 2026-06-06 (NOT merged; main unmoved @ 5e809e4→405db00 docs only).** Forcing AGENT_LONG=1 exposed 3 RED gates the per-push CI had masked: B2 = REAL FTS5-tokenizer latency regression (Slice-5 porter+remove_diacritics, ac_012 ~3× @100k); B1 = recall-gate mis-calibration (ac_013b asserts 0.90 on a SYNTHETIC embedder ~0.73; the 0.937 is eu7 real-embedder report-only — see [[perf-recall-gates-masked-and-ac013b-conflation]]); B3 = ac_020 runner-pin. HITL ruled: B1 elevate-eu7/demote-ac_013b (mint AC-075, precedent AC-072/073); B2 = **Slice 6 experiment** (tokenizer/query-path/source-accessibility → findings + HITL decision) = critical-path unblock, PROMPTED; Q3 = wire perf+recall to GA (Slice 40 re-scope). **NEXT = USER spawns Slice 6.** B1+Q3 fold into the re-scoped Slice 40 after B2 resolves. AC-037 → wire agent-security into CI at the re-scoped Slice 40. AC-050c removal-detect (was FAILing on the Slice-25 test_surface.py rename residue) RESOLVED by Slice 27 fix-1 — the scanner now scopes tests/ out (test-fn renames aren't public-API removals). See [[g0-identity-scope-logical-id-alone]].
metadata: 
  node_type: memory
  type: project
  originSessionId: 482125fb-352f-45de-a97b-7961c5482408
---

HITL **approved the 0.8.0 implementation plan** on 2026-06-02
(`dev/plans/0.8.0-implementation.md`). An earlier note said a *separate* agent
owned Slice 0; **that was overridden** — on 2026-06-02 the user directly told THIS
thread "kick off Slice 0, use 3961cf7 on main." **Slice 0 is now CLOSED** at commit
**`c99c5ef`** on `main` (docs-only; no code, no worktree).

**Slice 0 outcome (design-adr):** subagents 0.a ∥ 0.b (both spawned as
`subagent_type: implementer`, no fallback) → PASS. Landed: NEW
`dev/adr/ADR-0.8.0-canonical-identity-substrate.md` (G0 substrate — four decisions
settled; verbatim Slice-15 schema delta `logical_id`+`superseded_at` on
canonical_nodes AND canonical_edges, partial-unique-active, folded G4/G5 indexes,
MIGRATION-ACCRETION-EXEMPTION, SCHEMA_VERSION 10→11; in-place additive migration;
`write_cursor`-as-row-id deviation FLAGGED for HITL; shadow vec0/FTS5 reconciliation
named reserved Slice 16); advanced
`ADR-0.8.0-supersede-five-verb-surface-cap.md`→decision-ready (Q1–Q5 =
A1/B1/amend/confirm/SDK-only); NEW `dev/plans/0.8.0-plan.md` (mod-5 ladder),
`dev/plans/runs/STATUS-0.8.0.md` (nine §12.5 sections, the live board),
`dev/DOC-INDEX.md` (X3), reconciled `mkdocs.yml`. Review via independent adversarial
subagent (codex unrunnable here — see [[orchestration-execution-traps]] trap 5).

**PUSHED 2026-06-02:** the 0.8.0 campaign through Slice 5 is now on **`origin/main` @ `c27028b`**
(PR #83, merge commit; local==origin, no longer local-only). The out-of-band **corpus-work line**
(was `origin/main` `83f5156`) was integrated via `merge -X ours` — local campaign authoritative
for all overlapping docs/ADRs/STATUS/code; origin's unique corpus/eval artifacts (`tests/corpus/*`,
`dev/corpus-creation/*`, `dev/notes/0.8.x-corpus-*`, corpus QA prompts, `test_corpus_eval_qa.py`)
preserved + DOC-INDEX rows added. AC-037 no-egress gate CONFIRMED GREEN on windchill3.

**Slice 5 CLOSED 2026-06-02** (G1 structured `SearchHit` + FTS5 tokenizer), originally landed on
local `main` @ `14a7a06` (close commit; slice content merged at `e76d68b`); now on `origin/main`. Slice agent owned its
own worktree + merged to `main` itself (new execution model — orchestrator owns NO
worktree). Delivered `Vec<SearchHit{id=write_cursor,kind,body,score:f64,branch}>`, `Eq`
dropped, step-11 drop+recreate FTS5 tokenizer migration (`SCHEMA_VERSION 10→11`), X1 SDK
functional harnesses stood up; recall floor 1.000/1.000 across the migration. **codex was
the PRIMARY §9 reviewer and IS runnable here** (`codex exec review --base <sha>
--dangerously-bypass-approvals-and-sandbox`; `--base` rejects a custom PROMPT) — this
supersedes the Slice-0 "codex unrunnable" note (that was read-only-sandbox mode). codex
found a real **[P1] crash-safety bug** (tokenizer reindex was gated on the one-time
boundary crossing → forever-empty FTS index on a crash in the step-11/reproject window)
→ orchestrator ruled **BLOCK→fix-1**; fix-1 made it crash-retryable via an atomic
completion marker in `_fathomdb_open_state`; codex re-review PASS.

**Carried HITL item from Slice 5 (environment-only, NOT a code defect):** `agent-verify.sh
STRICT=1` fails AC-037 `netns-deny-egress` (`unshare -rUn` — no rootless userns in this
sandbox); confirm on a userns-capable host.

**Slice 10 CLOSED 2026-06-03** (local `main` @ close commit `2beaec2`; slice content merged
`b4865f6`). G9 RRF (`RRF_K=60.0`, unconditional, no `fusion_mode` knob), G10 `Option<SearchFilter>`
filtered-KNN (`filter=None` byte-identical to 0.7.2; 3-way shape-sentinel), G12-recency (off-by-default
flag). Two justified vec0 deviations flagged: TEXT metadata NOT NULL-able → `status` empty-string
sentinel `''` (population = reserved-gap 13); BIT subtype re-tagged via `vec_bit()` on Pack1→Pack2.
**codex §9 chain: PASS** — R1 2×[P2] (filter strings crossed FFI unvalidated, AC-068a/068b) →
CONCERN→fix-1 (`46ce583`; DEV-1 = TS JS-side surrogate validation accepted, napi-rs maps lone
surrogates to U+FFFD pre-Rust); fix-1 re-review 1×[P1] (dynamic-kwargs test fails pyright) →
BLOCK→fix-2 (`e9f833a`; typed factories); fix-2 re-review clean PASS. 10.b green (recall Δ 0.0000).
**SendMessage is NOT available in this harness** — fix-2 ran as a fresh `implementer` into the
existing fix-1 worktree (the §6 fix-N model), not a continuation. Carried (not gating): pre-existing
baseline pyright (12 errors, 2 unrelated test files).

**Slice 15 CLOSED 2026-06-03** (G0 canonical-identity substrate, the KEYSTONE; local `main` @ close
commit `b0fd051`; slice content merged `--no-ff` `5fa0e9e` of slice tip `c06472c`, net diff
`d51b1b3..main`). Landed verbatim authorized ADR delta: schema `step_id 12` / `SCHEMA_VERSION 11→12` —
`logical_id TEXT` + `superseded_at INTEGER` on BOTH canonical_nodes AND canonical_edges; per-table
partial `UNIQUE INDEX (logical_id,kind) WHERE superseded_at IS NULL` (NULL-safe, legacy NULL rows never
collide); folded G4/G5 read indexes; accretion-exemption marker. Engine: `PreparedWrite::{Node,Edge}.logical_id`,
`validate_write` rejects empty `logical_id`, `commit_batch` tombstone-then-insert supersession (same
txn, idempotent, one active per `(logical_id,kind)`; `logical_id=None` byte-identical to 0.7.x).
`WriteReceipt.row_cursors: Vec<u64>` (1:1 with batch) Py (`list[int]`) + TS (`rowCursors:number[]`)
parity. **codex §9 (base `d51b1b3`): 1 × [P2], no [P0]/[P1]** — read path doesn't filter `superseded_at
IS NULL`, so stale FTS5/vec0 shadow rows can surface a superseded body. That is EXACTLY
`RESERVED-GAP-16`, pre-signed out of scope by the HITL substrate gate (shadows NOT cascaded → deferred
to Slice 16) and NOT an observable regression (no consumer sets `logical_id`) → **orchestrator OVERRIDE
(§7), not fix-N**. Two flagged deviations: `write_cursor`-as-row-id (HITL-accepted, `row_id`+
`restore_provenance` deferred); `pr_g1_tokenizer_recall.rs` forced v10-seed adaptation (head writer now
requires step-12 cols; recall-floor intent preserved). Op-store cascade (Decision 4) needed NO code
change (`commit_batch` already atomic). Carried (not gating): baseline pyright 11 (zero new); AC-037
userns blocker. **Full inline renumber (step 11/10→11 → step 12/11→12) applied to the Slice 15 contract
+ canonical-identity ADR delta in the close commit.**

**Slice 20 (G8) PROMPTED 2026-06-03** — `dev/plans/prompts/0.8.0-slice-20.md` (additive
`WriteReceipt.dangling_edge_endpoints: u64`; cross-row post-insert active-node EXISTS probe INSIDE
`commit_batch`'s open tx, NOT `validate_write`; baseline `5fa0e9e`). Engine anchors re-verified at
`5fa0e9e`: `WriteReceipt` :833 (cursor+row_cursors), `commit_batch` :5727 (returns `Result<()>` → must
return the count), Edge arm :5777, post-loop hook :5839, `write_inner` :1921 + a 2nd receipt ctor :2612.
Probe predicate SETTLED-by-construction (NOT a HITL item): contract framed it as "logical_id-alone vs
edges-carry-kind" (both assume endpoints are node logical_ids), and `canonical_edges` stores only the
edge's OWN kind → edges-carry-kind ruled out → probe = active-node EXISTS by `logical_id` hitting step-12
`canonical_nodes_logical_active_idx`. Consequence (benign, documented): legacy NULL-logical_id nodes
aren't valid endpoints → flag as dangling, but flag-and-count default + no consumer sets logical_id. No
SDK verb → no new gate.

**Slice 20 (G8) CLOSED 2026-06-04** (local `main`; slice `307aeac`, **fix-1 `54e3e93`** final HEAD).
codex §9 (base `57c023c`) = **1×[P1]** (same-batch supersession O(N²) `batch[i+1..]` scan in the
single-writer tx) → **BLOCK→fix-1** (O(N) `HashMap<(logical_id,kind),last_index>` precompute,
byte-identical count, +guard case (g)) → fix-1 re-review clean PASS. Strict-mode rollback → reserved-gap
**band 22** (flagged, not built).

**Slice 21 (pyright-zero cleanup, interim reserved-gap band 21) CLOSED 2026-06-04** (HITL-inserted
2026-06-04 to retire the carried baseline pyright that short-circuited `agent-verify`'s typecheck across
Slices 10/15/20). Merged `main`@`7aaf6f1`, closed @`406c566`. Pyright over `src/python` now **0/0/0** →
typecheck no longer short-circuits (only AC-037 userns remains). Inventory A–F honest/narrow: **A**
`UnknownEmbedderEvent.kind` made required (drop `total=False`; clears A+B real defect); **C/E** targeted
ignores (test-only FFI hooks / optional `hypothesis`); **F** version-guarded `tomllib`/`tomli` (fixes a
**latent py3.10 crash**). **Item D — orchestrator AMENDED the prompt:** its premise (subprocess test does
not assert the line-30 negative) was **FALSE** vs on-disk `test_pyright_flags_unnarrowed_variant_key_access`
(which requires the `bytes` error), so the prescribed ignore/exclude would break a green. Resolved by
relocating the negative-error fixture `tests/ → src/python/_typecheck_fixtures/` (outside pyright
`include`, byte-unchanged) + repointing the test's `_FIXTURE` — decoupling the project run (0/0) from the
explicitly-passed `--project` subprocess run (error still fires). Config-scope mechanism inside the HITL
constraint, NOT a new gate. First implementer correctly STOPPED at D (justified deviation, see
[[oob-creep-vs-justified-deviation]]); a fresh implementer finished D (SendMessage still unavailable, §6
fix-N pattern). **codex §9 (base `802527e`): clean PASS, no findings.** No AC, no feature, no
engine/SDK/schema change; does NOT affect the SIGNED Slice 25 gate. TS parity holds (TS `kind` already
required).

**Slice 25 (governed-surface conformance rewrite) CLOSED 2026-06-04** (merged `d8665fe`, **fix-1 final
`b86ef63`**, closed `67d3980`). AC-057a **superseded by new AC-074** (measurable governed SDK surface:
allowlist-membership + cross-binding parity + recovery-denylist + no-raw-SQL; records Q5=BIND-RUST → Rust
pin at Slice 27; forward pointer, not deleted); REQ-053 **amended in place** (Q3); `bindings.md` §1/§13/§14
rewritten; trace `acceptance.md:1183` + `rust.md` repointed AC-057a→AC-074. **codex §9: R1 (base
`a736106`) = 2 × [P1] BLOCK → fix-1 → clean PASS.** R1 caught that the first rewrite enforced NEITHER
guarantee — the live-surface enumeration was hard-coded (membership passed vacuously) and the "parity"
test compared a same-file duplicate literal (Py↔TS drift undetectable). fix-1 = genuine `dir(Engine)`
introspection (minus a documented non-command exclusion set) in both bindings + a **single shared contract**
`src/conformance/governed-surface-allowlist.json` both suites read. **HITL Q1–Q5 was already SIGNED
2026-06-03 (pre-satisfied), so codex PASS was the only remaining gate.** Byte-freeze held throughout (3
recovery suites + `bindings.md` §10 zero-line diff). Lesson →
[[conformance-rewrite-vacuous-green-trap]]: the 25.b subagent audit PASSED but missed both [P1]s;
independent codex is load-bearing. Carried (not gating): `src/ts/package-lock.json` pre-existing
`0.6.1`-vs-`0.7.2` drift. **Pointer → Slice 30** (read.* verbs go live); the four `read.*` are already
documented-allowlist members, Slice 30 makes them live (now genuinely enforced by the introspective
membership test).

**Pre-Slice-30 supersession gate SIGNED 2026-06-03** (`ADR-0.8.0-supersede-five-verb-surface-cap.md` →
Status SIGNED): **Q1=A1** (supersede AC-057a five-verb cap; ship G1+G2 read.get/get_many+G3
read.collection/mutations this release), **Q2=B1** (`read.*` namespace), **Q3=amend** (REQ-053 in place),
**Q4=confirm** (read.get(logical_id) SDK-allowed; {recover,restore,repair,fix,rebuild} SDK-unreachable;
restore/purge_logical_id CLI-only), **Q5=BIND-RUST — DEVIATION from recommended SDK-only**: HITL bound the
Rust facade (`dev/interfaces/rust.md`) into the governed-surface AC too → **activates reserved-gap Slice
27** (Rust positive-allowlist pin). Slice 25 lands the Py+TS allowlist+parity rewrite (recovery suites
byte-unchanged) AND records the Rust binding; Slice 27 executes the Rust assertion. Q3=amend means Slice
28 (supersede-by-new-REQ trace cascade) is NOT triggered. **This signature is the HARD gate unblocking
Slice 30.** Slice 25 now gate-clear to prompt (carry the as-signed bundle + Rust-binding note).

**Reserved-gap-16 LIVE:** shadow reconciliation / read-visibility (supersession has NO observable read
effect until Slice 16 and/or the G2/Slice 30 read-path `superseded_at IS NULL` join).

**Retrieval-ADR HITL decisions (2026-06-02, `ADR-0.8.0-agent-memory-retrieval-and-identity.md`
→ status accepted):** Q1=**1A** (G9 RRF + G10 filtered-KNN both table-stakes, ship in Slice 10;
G10 uses a CLOSED `SearchFilter` struct, filter-grammar DSL stays Slice 35); Q2=**2A** (substrate
designed bi-temporal-aware, implement single-supersession only); Q3=**documented-only, NO knob** —
RRF is the unconditional ranking, `fusion_mode`/legacy-union path DROPPED (HITL: "do not carry the
overhead"); the Slice 10 contract was reconciled to remove every `fusion_mode` mention; Q4=**edges
too** (canonical_edges carry logical_id+superseded_at, schema-only); Q5=**advisory** (§8d stays
advisory input, not canonical). Q2/Q4 partially satisfy the substrate gate package (gates Slice
15); op-store cascade + forward-migration policy + write_cursor deviation REMAIN open.

**AC-037 (no-egress gate) (2026-06-02):** Ubuntu 24.04's `apparmor_restrict_unprivileged_userns=1`
blocks `unshare -rUn`, so the gate can't run by default and runs in NO CI workflow. **One-time
confirm DONE ✅:** with the sysctl temporarily set to 0 on windchill3 (restored after), the gate
passed ("all connect() syscalls were loopback / AF_UNIX / AF_NETLINK") — the merged Slice 5 +
fix-1 code has no network egress. **Kept:** WIRE `scripts/agent-security.sh` into CI on a
userns-permissive runner (e.g. ubuntu-22.04) as NEW Slice 40 gate (n) for continuous coverage.

**Substrate gate package ✅ FULLY SIGNED (HITL 2026-06-03) — Slice 15 gate CLEARED.** Q2=2A +
Q4=edges-too signed 2026-06-02; the three remaining items signed 2026-06-03: op-store cascade =
**ratify Decision 4 as-is** (atomic same-txn tombstone-then-insert; latest_state updates,
append_only_log accretes; vec0/FTS5 shadows NOT cascaded — deferred to reserved Slice 16);
forward-migration = **in-place additive `ALTER`** (nullable cols, no data migration, no re-open) —
**accepted consequence: legacy pre-0.8.0 rows carry `logical_id = NULL` until rewritten; the engine
must own a documented NULL-on-legacy-rows rule** (G2 `read.get(logical_id)` resolves a legacy row by
logical_id only after it is rewritten); `write_cursor`-as-row-id = **ACCEPTED for 0.8.0, dedicated
`row_id`+`restore_provenance` DEFERRED** to a later additive slice (bounded/reversible — no
cursor-renumber slice and no committed restore_provenance consumer on the roadmap; recover/restore are
SDK-absent). Slice 15 still FLAGS the `write_cursor` deviation in `output.json` but lands the accepted
shape. Recorded: `ADR-0.8.0-canonical-identity-substrate.md` Status → SIGNED. **Next orchestrator
action = instantiate `dev/plans/prompts/0.8.0-slice-15.md` from the SLICE-TEMPLATE** (step_id 12 /
SCHEMA_VERSION 11→12; AUTHORIZED schema delta verbatim). Supersession Q1–Q5 readied now, **finalized
at Slice 25** (hard gate unblocking Slice 30).

**Slice 30 (G2/G3 governed read.* verbs) CLOSED 2026-06-05** (merged `main`@`52ceab3`; slice agent owned
worktree, merged itself). Landed `read.get`/`read.get_many` (active-only by-`logical_id` point lookup, get→get_many,
not-found=None/null) + `read.collection`/`read.mutations` (paginated op-store read-back, mandatory limit
clamped 1M + after-id cursor) via NEW `ReaderRequest::{GetById,ReadCollection}` on the ReaderWorkerPool
DEFERRED-tx path (never `connection.lock()`); per-request typed `respond` channels so Search's
`ReaderResponse` is byte-unchanged (Slice 10 pin green). Py `read.py` + TS `read.ts` lockstep parity; surface
suites extended to genuinely introspect the live `read` namespace (anti-vacuous, demonstrated in RED). **codex
§9: 1×[P2]** — `read.get` lossy+nondeterministic when one `logical_id` has multiple active `kind`s. NOT a
read-API defect → root-caused to the G0 compound `(logical_id,kind)` key → escalated to HITL → resolved by
**Slice 31**, zero read-API change. No new AC/REQ ids.

**Slice 31 (G0 identity re-scope = logical_id-alone) CLOSED 2026-06-05** (merged `main`@`b4e90c8`; slice agent
owned worktree). HITL-SIGNED 2026-06-05. Both partial-unique indexes → `ON canonical_<table>(logical_id) WHERE
superseded_at IS NULL` (dropped `kind`); node+edge supersession `UPDATE`s drop `AND kind`; Slice-20/G8 in-batch
precompute re-keyed to `logical_id`; **step-12 amended IN PLACE, no SCHEMA_VERSION bump (stays 12; local v12 DBs
disposable)**. ADR **Decision 5** added + `(logical_id,kind)`→`logical_id` propagated across parent ADR Q2 /
roadmap / slice-15 design / impl-plan / architecture / Py+TS API docs; parent-ADR Q4 edge-supersession wording
corrected (edges DO supersede). TDD: `s31_node/edge_kind_change_reingest_supersedes` + inverted `s15` collision
RED under compound key → GREEN. **codex §9 clean PASS, 0 findings.** Zero read-API/binding/SDK change; recovery
suites byte-frozen; pyright 0/0; pins green (Slice 10 Search byte-identity, Slice 20 `pr_g8` 8/0, Slice 30
`pr_g2`/`pr_g3`). **⚠️ Edge half flagged for graph-lens review in USER-spawned Slice 32** (a multigraph may make
multiple active `kind`s between one endpoint pair legitimate — the codex consult evaluated edges as opaque
write-receipts, not as a graph). See [[g0-identity-scope-logical-id-alone]].

**Slice 32 (graph-model ADR / design-eval) CLOSED 2026-06-05** (closeout commit `e1827c4`). Resolved
FathomDB's intended graph model: NEW `dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md` (ACCEPTED) =
one **ontology-neutral binary property-graph substrate** first-classing BOTH corpus (GraphRAG) + memory
(Graphiti fact-on-edge) ontologies; **opaque-id edge addressing** for 0.8.0 (natural-key/hybrid = future
write-API ADR); **fact-on-edge** memory end-state w/ fact-on-node escape hatch; edge-enrichment
(`body`/`confidence` + valid-time `t_valid`/`t_invalid`) + edge-projectability + edge-inclusive-G7
**reserved-additive**. **logical_id-alone (signed Slice 31) CONFIRMED for the graph aspect — NOT reopened.**
HITL 2026-06-05: H1 (neutral-both) + H3 (reserve edge-enrichment) SIGNED; H2/H4/H5/H6 deferred 0.8.x.
**Read-only — NO 0.8.0 engine/schema change**; the only substrate-now footprint is an H3 prose reservation
in the signed substrate ADR. **codex §9 (base bf3ecdd): 2×[P2], no [P1]** — both closeout-consistency, both
fixed: (1) valid-time column-name clash → harmonized to `t_valid`/`t_invalid` (substrate ADR = schema-contract
authority; `valid_at`/`invalid_at` = Graphiti alias); (2) Slice-32 STATUS row flipped to CLOSED. Process:
user-directed research agent drafted the ADR + orchestrator reviewed/closed in lieu of a worktree slice
([[dont-dismiss-user-directed-subagents]]). Foundation for the Slice 35 graph ADRs.

**Slice 27 (Rust-facade governed-surface allowlist/parity pin, Q5=BIND-RUST) CLOSED 2026-06-05** (merged
`main`@`485f498`, closeout `f91542c`). Lands the Rust positive-allowlist + Py/TS↔Rust parity pin
(`governed_surface.rs`) and fills the AC-074 Rust measurement clause — the reserved-gap activated by the
supersession gate's Q5 deviation. Parallel sibling of Slice 33 (disjoint code: facade crate + rust.md +
acceptance.md AC-074 vs Slice 33's engine/schema/op-store); Slice 33 rebased onto this and kept both
STATUS/DOC-INDEX rows.

**Slice 33 (op-store read.collection/read.mutations cursor+limit hardening) CLOSED 2026-06-05** (baseline
`e1827c4` → rebased onto Slice 27 `f91542c`; merged `--no-ff` `58664d9`, closure `761ed68`, codex verdict
doc `96a6243`). The G3/F4-READ reserved item (op-store.md:132 "cursor/limit hardening under a genuine
~1M-row log"). Landed **step-13 additive index** `operational_mutations_collection_id_idx ON
operational_mutations(collection_name, id)`, **SCHEMA_VERSION 12→13**, forward-only/no-reshape — pure
`CREATE INDEX` so the accretion guard (CREATE TABLE / ADD COLUMN only) does NOT fire and **no exemption
marker is required**. EXPLAIN before→after: `SEARCH … USING INTEGER PRIMARY KEY (rowid>?)` (id-PK walk,
O(rows-scanned) for a small collection in a large multi-collection log) → `SEARCH … USING INDEX
operational_mutations_collection_id_idx (collection_name=? AND id>?)` (index-driven, **no SCAN, no temp
B-tree for ORDER BY**, O(page)); pinned RED→GREEN in `pr_g3_read_collection.rs`. Cursor hardened: negative
`after_id` clamped to start (`after_id.unwrap_or(0).max(0)`); limit==0/over-MAX/past-end/unknown-collection
pinned. **No SDK signature/binding change.** Justified in-scope: mechanical schema-head pins 12→13 in 4
regression tests, AND `pr_g1_tokenizer_recall.rs::V10_MIGRATIONS` switched from brittle `MIGRATIONS.len()-2`
to absolute prefix length 10 (would've shifted to v11 once any step landed). **codex §9 (base `f91542c`):
clean PASS, 0 findings** (codex independently re-ran the schema/engine regression suites — all green).
Pins green (Slice 10 Search byte-identity, Slice 20 pr_g8, Slice 30 pr_g2/pr_g3, Py/TS functional-retrieve);
recovery suites byte-frozen; pyright 0/0; mkdocs clean. Verdict recorded
`dev/plans/runs/0.8.0-slice-33-codex-review-20260605T213820Z.md`. No push/origin touched.

**Slice 34 (CLI op-store read-back, `fathomdb doctor dump-mutations`, reserved-gap-34 / F4-READ) CLOSED
2026-06-06** (merged `--no-ff` `main`@`11bfd16`, merge-SHA-recorded closeout `32bb3d5`; implementer owned its
own worktree, merged itself). Adds a **CLI-only** read-only `doctor` diagnostic that pages op-store
(`operational_mutations`) rows for one `append_only_log` collection over the **existing** Slice-30
`Engine::read_mutations` seam (Slice-33 index-driven) — `doctor dump-mutations <collection> [--after-id n]
[--limit n] [--json] <db_path>`, default `--limit` 1000 (engine clamps to ~1M). **Scope call (operator
PRE-SIGNED the diagnostic-not-query framing):** this is a `dump-*` diagnostic over the mutation log, NOT
`ADR-0.6.0-cli-scope` Option B (search/get/list application query over canonical_nodes — stays rejected);
landed as an in-place ADR amendment (Status + Consequences bullet, 2026-06-06). `--json` envelope `{ verb,
collection, after_id, limit, count, rows:[{id,collection,record_key,op_kind,payload,schema_id,write_cursor}
ORDER BY id], next_after_id }`; empty/unknown collection → `rows:[]`/exit 0 (normal absence, never 65);
lock-held → 71. **Rows serialized INLINE** (OpStoreRow never named/re-exported) → **facade public-type set
unchanged** (governed_surface.rs/reexports.rs green). **NO engine/schema/SDK/binding change** — git diff vs
baseline touches ONLY fathomdb-cli (src+2 test files) + 5 docs; recovery suites byte-frozen; Py/TS/SDK
byte-unchanged. TDD: parser.rs (2 tests) + operator_cli.rs (4 dedicated + `dump-mutations` added to
DOCTOR_VERBS so the --help/--json loop pins cover it); RED = E0599 no `DumpMutations` variant. clippy/fmt
clean; mkdocs --strict green; cargo test -p fathomdb-cli + -p fathomdb-engine (standalone, per Slice-27
lesson) + pins (pr_g8/pr_g2/pr_g3/pr_g3_read_collection/pr_g1/pr_g10) all green. **The implementer SKIPPED
its codex §9** (declared CLOSED, self-verified only) — the **Slice-27 trap again** ([[orchestration-execution-traps]]:
implementer agents may merge + self-declare CLOSED without running §9; the orchestrator must ALWAYS run §9
independently before accepting any "CLOSED"). Orchestrator ran the missing §9 (`codex exec review --base
e4dcdd1`): **1 × [P2]** — `next_after_id` compared `rows.len()` to the **un-clamped** requested `--limit`, so
a `>1M --limit` on a >1M-row log made a full capped page look exhausted → `next_after_id: null` → silent
pagination truncation while rows remained. Real but bounded → **fix, not override.** **fix-1** (orchestrator-run,
TDD): CLI-side cap mirror `DUMP_MUTATIONS_MAX_LIMIT=1_000_000` + pure `effective_dump_limit()` clamping
`--limit` before BOTH the read and the `next_after_id` decision; pure unit pin (no >1M-row seeding). codex
re-review (`--base 32bb3d5`): **PASS** — [P2] resolved; lone **[P3]** (comment referenced the review/task,
AGENTS.md §6) folded into the GREEN commit. **fix-1 merged `--no-ff` `8212f65`; closeout `77c87d0`.** Verdicts:
`runs/0.8.0-slice-34-codex-review-20260606T141131Z.md` + `…-review-fix1-20260606T141733Z.md`. agent-verify: 0
violations, only the known AC-037 userns blocker (CI-covered @ 40); AC-050c removal-detect PASS. pyright/Py/TS
functional suites NOT re-run locally (toolchain absent) — preserved by the zero-source-diff byte-freeze;
CI-covered. No push/origin touched (local-main only). **Reserved-gap band now EXHAUSTED (27·31·32·33·34 all
CLOSED) → pointer Slice 35 then 40.**

The plan: 9 mod-5 slices (0,5,10,15,20,25,30,35,40), each a slice-orchestrator on
the main thread (+ ≤1 level N.a/N.b subagents) following `dev/design/orchestration.md`.
Per-slice discipline: orchestrate/adjust-plan → design-first+self-check → TDD
RED→GREEN→refactor → codex(=adversarial) review → fix-N until PASS/override
(BLOCK→HITL). Cross-cutting X1/X2/X3 bind every slice: SDK parity+functional
harnesses (Py∥TS write/search/retrieve/admin + cross-binding equivalence), `mkdocs
build` green, per-slice docs + `dev/DOC-INDEX.md` update in the closing commit.
Relates to [[fathomdb-v05-graph-lineage]], [[fathomdb-consumer-agents]],
[[orchestration-execution-traps]].
