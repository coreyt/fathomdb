# FathomDB — Steward Session Hand-off (2026-07-03)

> **Boot:** run **`/steward`** (reads `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` for the role), then read
> THIS doc for current state, return an orientation paragraph, and **WAIT for HITL** before mutating. You are
> the **Primary Development Steward (PDS)**: keep the schedule-of-record true to git, place cross-cutting
> work, commission + verify, propose-first to the HITL. **Do not implement code or hand-drive a ladder.**

## Git state — read first

- **`main` is clean and pushed** (`origin/main == main`). This session's work landed as a run of docs commits
  ending ~`68a50168` (+ this hand-off commit). No worktrees are mid-flight; the `0.8.12-*` worktrees were
  cleaned earlier. Verify with `git rev-parse --abbrev-ref HEAD` (expect `main`) before any commit.
- **Steward ledger** `dev/steward/steward-ledger.jsonl` @ ~seq 16. Read deltas with
  `python dev/agent-tools/ledgerwatch/ledgerwatch.py dev/steward/steward-ledger.jsonl`; append with `ledgerwrite`.

## PRIMARY IN-FLIGHT: OPP-12 — Memex⇄FathomDB record-lifecycle/liveness convergence

The dominant work of this session. A cross-repo design convergence over a shared JSONL discussion ledger.

- **Ledger:** `/home/coreyt/projects/memex/dev/fathomdb/enum-discussion-ledger.jsonl` (**Memex repo**).
  Protocol: `…/enum-discussion-ledger.README.md`. Fields: `voice` (MEMEX|FATHOM|HITL), `kind`, `epistemic`,
  `status`, `refs`. **It is MODIFIED locally but NOT committed/pushed** (memex-scope rule below).
- **Thread @ seq 8** (`.seq`=8): s1 MEMEX *problem* (liveness has no coherent home; CR-056/057/060; Q1–Q4) →
  s2 HITL *relax* (no migration, breaking-OK both sides) → s3/s4 MEMEX *clarify/note* (the two "superseded"
  layers; CR-060 origin is a logical is-latest bug) → s5/s6 FATHOM *note/option* (the structural contract) →
  s7 MEMEX *response* = **ACCEPT-THE-SHAPE + 10 conditions + 2 HITL decisions** → **s8 FATHOM *response* =
  reconciled, resolves all 10, states the surface delta.** **Status: converging → RATIFIABLE.**
- **Design docs (all on `main`, committed):** `dev/design/record-lifecycle-protocol/` — `README.md`,
  `structural-lifecycle-contract.md`, `projection-registry-and-async-embed.md`, `api-surface.md`,
  `code-grounded-audit.md` (dated audit report). Steward-owned; **PROPOSED** (nothing contracted until HITL).
- **The contract, in one line:** seam = **mechanism/policy**; engine owns 3 orthogonal axes —
  existence/admission `{pending→active→deleted→purged}` × version-currency (the **shipped `superseded_at`**
  G0 index, *not* `is_latest`) × temporal-validity (**edge-only today**; node-validity net-new) — plus an
  engine-owned **projection registry** (async vector via the **existing** projection worker; `dense_readiness`;
  reuse the shipped `drain()`). App owns transitions + reasons + edge/attr vocab & values + view-policy.
  Solves CR-056/057/060 + Q1–Q4. `SearchHit.id` **subsumes** the prefix-tagged `stable_id` (`l:`/`h:`/`p:`,
  non-null) — **GATING, lands-together with the Cause-A id-contract** + anonymous-node surrogate-minting.
- **Surface delta (both SDKs, X1 parity):** **+3 verbs** (16→~19: `transition`/`purge`/`configure_projections`;
  `drain` reused, `read.projections` folded); **+5 governed types** (~29→~34–36: `ReadView`, `LifecycleState`,
  `IllegalTransitionError`, `ProjectionSpec`, `dense_readiness`); `SearchHit` shrinks; **`undelete` not
  `restore`** (`restore` is on the `recovery_denylist`).
- **How it got here (don't redo):** 4 adversarial *architecture* rounds (Fable 5) → a **code-grounded audit**
  that found the contract was ~90% net-new but read as near-shipped and **contradicted shipped/HITL-signed
  mechanisms** → reconciliation → **2 consolidation reviews** (verb C1–C11, signature S1–S10). All folded.
- **Roadmap placement = TBD** (breaking changes ⇒ likely ≥0.9.x); reconcile into the master only when the HITL
  schedules it. Memex side rides the coordinated 0.5.x↔0.8.x/≥0.9.x breaking pair; 0.5.5-C proceeds decoupled.

## STANDING GUARDRAILS (load-bearing — the HITL enforced these this session)

1. **NO Memex ledger response without HITL discuss-and-agree FIRST.** Draft the FATHOM entry in-conversation →
   HITL explicitly approves the content → *then* `ledgerwrite`. WRITES only; passive **listening is fine**.
   ([[no-memex-ledger-response-without-hitl-agreement]])
2. **NEVER commit/push the memex repo** without a specific per-push directive EACH time. The enum-ledger stays
   local-only. ([[push-scope-fathomdb-only]]) Push scope is **fathomdb-only**.
3. **Shared-ledger discipline:** a voice-filtered `ledgerwatch --select voice=X` HIDES other voices — do a
   **full unfiltered tail read before EVERY append** and check the seq is contiguous.
   ([[shared-ledger-full-tail-read-before-append]])
4. **Verify design against CODE, not just architecture.** Adversarial architecture review ≠ substrate fidelity;
   run a code-grounded exists-vs-net-new pass before drafting/committing a design contract.
   ([[verify-design-against-code-not-just-architecture]])
5. **Say when something is *done* vs *proposed*** — the HITL values this explicitly.
6. Standing steward constraints (unchanged): label-only (manifests `0.8.9`, no tag/publish w/o a HITL cut) ·
   `13` forbidden minor+micro · **V-7 held** · codex §9 gates commissioned code · one-writer-per-worktree ·
   worktrees off `origin/main`, MAIN-tree-only maturin/GPU builds · **background-agent SPEND needs the user's
   OWN direct message** · verify-from-git before narrating.

## Owed / next decisions (HITL)

1. **OPP-12 is ratifiable** — the next move is **HITL ratification → mirror to OPP-12 in the leverage ledger +
   a `dev/design/` decision record**, OR Memex responds to seq 8. (Not the steward's call to ratify.)
2. **The MEMEX listener is NOT currently armed.** Re-arm on request (background poll on the ledger `.seq`, or
   `ledgerwatch --select voice=MEMEX`) if you want to catch Memex's next entry.
3. **Memex enum-ledger is uncommitted** on the memex side — committing it needs a per-push directive (rule 2).
4. Roadmap reconciliation for the lifecycle work when scheduled (rule: master never silently lies).

## Recently completed this session (context, not action)

0.8.12 **CLOSED + merged** (label-only); masked-gate compile break caught+fixed; HNSW ruled **2.x** (F-16);
**maturity/scale ladder** recorded (F-17: pre-1.0=beta; 0.9.0 soft → 0.9.3 stated → 1.0.0 exit-beta → 1.1.0
hard); `#17` verified shipped in 0.8.11 + `plan-0.8.14` reconciled; `#13`-double-allocation struck from
`plan-0.8.18`; market-research doc committed to `dev/research/`; the full OPP-12 convergence above.

## Memory pointers

`[[opp12-record-lifecycle-protocol]]` · `[[verify-design-against-code-not-just-architecture]]` ·
`[[no-memex-ledger-response-without-hitl-agreement]]` · `[[shared-ledger-full-tail-read-before-append]]` ·
`[[push-scope-fathomdb-only]]` · `[[release-dod-requires-full-workspace-gate]]` ·
`[[gpu-not-retrieval-lever-hnsw-at-0.8.20]]` (HNSW=2.x + F-17 ladder) · `[[steward-delegate-dont-hand-do]]`.
