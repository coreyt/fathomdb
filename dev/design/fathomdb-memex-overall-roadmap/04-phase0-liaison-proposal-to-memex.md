---
title: Phase-0 liaison PROPOSAL bundle — FathomDB → Memex
date: 2026-07-03
status: DRAFT for HITL relay — NOT sent, NOT pushed, NOT appended to any Memex ledger
desc: >
  The single Phase-0 bundle of asks/notes from FathomDB to Memex, per the overall
  cross-product roadmap (01-overall-roadmap.md, "Phase 0 — NOW"). One message, six items.
  All items are proposals; Memex-side actions are never auto-applied. Push scope: fathomdb only.
---

# Phase-0 liaison bundle: FathomDB → Memex

**How to use this file.** It is a *draft* the HITL reviews and relays through the liaison channel.
It is **not** a Memex enum-ledger append (that requires separate HITL agreement) and it is **never**
pushed to the Memex repo. Nothing here is committed on the Memex side by FathomDB.

**Git-verification status (checked on fathomdb `main` @ `83ef13fc`, 2026-07-03):**

- ✅ `1137c572` (0.8.11.2) **is** an ancestor of `main`.
- ✅ A-1 (`$.action_kind` allowlist) is live on `main` as **`9a46611b`** — *not* the `9e0a3459` hash Memex cites (that commit was rebased away).
- ✅ `stable_id` (0.8.11.2) is on `main`.
- ⚠️ Corpus "6→8 source_types" refers to the **cross-source-bench eval corpus** (`entity_ids` + WEC-Eng), **not** fathomdb's `tests/corpus/` retrieval corpus, which remains **locked at 6** (`email, meeting, paper, article, note, todo`; `tests/corpus/README.md:114`). Do not conflate the two.

**Bottom line for Memex:** you are **not** waiting on anything unpublished. Every dependency the
Memex handoff listed as "may not be on origin yet" is already on `origin/main`. The proof-point work
(0.5.1 Phases 5–6) is unblocked by a **re-pin**, not by new FathomDB work.

---

## Item (a) — OPP-12 ratification close-out  [Gate G-3]

**Ask:** post `agree` at **seq-12** of the enum-discussion ledger (or your objections), and apply the
prepared prose-ledger mirror text (`dev/design/record-lifecycle-protocol/OPP-12-leverage-ledger-update.md`).

**What this does / does not do:** it **freezes the lifecycle contract** (the 3 axes, read modes, projection
registry, the +3 verbs / +5 governed types) so both sides' cleanup can key off stable text. It **schedules
nothing** — the OPP-12 *build* is ~90% net-new and gets its own ≥0.9.x scheduling call (FathomDB-0.9.x ↔
Memex-0.5.x breaking pair). "Ratified ≠ scheduled ≠ cheap."

**Cost:** one message. **Blocks:** the M7 verb-consolidation, the C-1 registration-flow checkpoint, and the
§14 purge-timing re-affirmation.

---

## Item (b) — Re-pin unblock (0.5.1 Phases 5–6)  [Gate: none — dependency already met]

**Ask:** re-pin your editable FathomDB build to `origin/main ≥ 1137c572` (current `83ef13fc`), then:

1. Run capability probes and **flip A-1 server-side** — `$.action_kind` predicate-path allowlist for your
   `WMAction` filter (live as `9a46611b`; update any pin referencing the rebased `9e0a3459`).
2. Probe **A-2** — bool-eq server-executable in `read.list` (confirm it lowers server-side, not client-side).
3. Wire **`stable_id`** into your hit-identity path and **delete the `fathom_store.py` hack** it was standing in for.

**Why now:** this is the single highest-leverage unblock in the portfolio — the joint product's proof point
(a real governed-surface consumer) is stalled purely on a delivery signal that was never sent, not on missing
engine work.

**Guardrail (M20):** before you *close* the Phase-6 behavioral-equivalence gate (B-1), the five open cutover
risks must each be cited-as-resolved or put on the next joint-sync agenda — **Python-SDK production-grade,
aarch64/Jetson, append-only-log at scale, single-writer vs your TUI+service two-process architecture, and
join-query expressiveness.** The single-writer/two-process one is architectural and could invalidate the
cutover destination itself — please confirm its status explicitly.

---

## Item (c) — Cause-A Stage-1 sufficiency probe  [Gate G-6]

**Ask:** run a small probe answering: **does shipped `stable_id` (0.8.11.2) suffice for OPP-11 hit-level
data-fitness**, or do you need the full typed `{space,value}` `SearchHit.id` (Cause-A Stage 2)?

**Decision rule (probe-then-retire, never retire-then-probe):** PASS ⇒ FathomDB retires the standalone
Cause-A pico as discharged. FAIL ⇒ the pico stays the live interim vehicle, now evidence-backed, and Stage 2
rides the OPP-12 ≥0.9.x breaking pair (one mechanism, both ledgers).

---

## Item (d) — E-A2 filter-rate telemetry  [Gate G-1 input — needs a named owner]

**Ask:** this is the one **unowned** instrumentation the 2MM premise decision needs. Either name a Memex
owner + slot to emit **query filter-rate telemetry** (what fraction of real queries carry a
`source_type`/time-window/entity filter, and the resulting candidate-set selectivity), **or** tell the HITL
you accept deciding G-1 on corpus-size projections alone.

**Why it matters:** if most queries are filter-scoped, partition-pruned exact scan may cover 1–2M without any
kernel/ANN work at all — it directly changes what FathomDB builds. Without an owner, G-1 defaults to **NO**
(published roadmap stands).

---

## Item (e) — gpu-rerank home is UNDECIDED  [Gate G-5 — do not plan against "0.8.14"]

**Heads-up, not an ask:** the `0.8.14-gpu-rerank` branch (`ce_blend_enabled` flip + `embed_batch_cls`,
rebased/green, unmerged) that your retrieval-quality plan homes at "0.8.14" has **no home in any current
FathomDB plan** — plan-0.8.14 contains zero rerank scope. HITL will decide to either admit it to 0.8.14 or
re-home it to 0.8.16 (ranking-signal/embedder-reach window). **Do not plan CE-blend adoption against a "0.8.14"
availability date until FathomDB confirms which release ships it.** We will tell you the moment G-5 resolves.

---

## Item (f) — Corpus / WEC-Eng origin visibility CONFIRMED  [enables #30/#31 pinning]

**Note:** the cross-source-bench corpus schema (`source_type` 6→8, `entity_ids` join-key) and the WEC-Eng
acquisition are on fathomdb `origin/main`. Your **#30/#31 (QID entity-linking + re-probe)** may pin
`manifest_sha256` against `origin/main` now — no wait. (Reminder: this is the *eval* corpus; the retrieval
`tests/corpus/` vocabulary is unchanged at 6.)

---

### Summary of asks

| # | Item | Type | Gate | Memex effort |
|---|---|---|---|---|
| a | OPP-12 `agree` @ seq-12 + apply ledger text | ratify | G-3 | 1 message |
| b | Re-pin to origin/main; probe A-1/A-2/`stable_id`; flip A-1; wire `stable_id` | consume | — | small; unblocks 0.5.1 |
| c | Cause-A Stage-1 sufficiency probe | probe | G-6 | small |
| d | Name an owner for E-A2 filter-rate telemetry (or accept projection-only G-1) | own/decide | G-1 | decision |
| e | Acknowledge gpu-rerank home is undecided; don't plan against "0.8.14" | ack | G-5 | none |
| f | Pin #30/#31 against origin/main | consume | — | none blocking |

*All items are PROPOSALS for the Memex HITL. FathomDB applies nothing on the Memex side and pushes nothing to the Memex repo.*
