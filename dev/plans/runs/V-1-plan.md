# V-1 plan — keystone EXP-B′ re-run on the live CE engine (DRAFT for HITL)

> **Status: DRAFT for HITL approval. Do NOT run yet.** This pins the concrete, runnable design for
> **V-1**, the keystone of the 0.8.11.2 experiment campaign. Per the plan it is **held at Phase A → V-1**;
> V-3 (OPP-1) and V-7 (OPP-3) consume V-1's output and do **not** start until V-1 lands.
>
> **Ground sources:**
> `dev/plans/plan-0.8.11.2.md` §2 (the V-1 row + the `Phase A → V-1 → V-3 → V-7` sequencing) and §4
> (per-item exit criteria); `dev/plans/runs/0.8.11-handoff-to-0.8.15.md` §2 (the V-1 gate definition),
> §2b (at-power registered experiments), §3/§5 (the recall-bound throughline, MMR/recency unmeasured);
> the prior result `dev/plans/runs/expb-joint-tune.md` + its harness `src/python/eval/expb_joint_tune_run.py`;
> the P0-4 eval-support `src/python/eval/opp3_eval_support.py`; the engine CE path
> `src/rust/crates/fathomdb-engine/src/lib.rs` (`rerank_fused` L5474, TinyBERT-L-2 L5658) and the pyo3
> `search` knobs `src/rust/crates/fathomdb-py/src/lib.rs` (`alpha`/`pool_n` L833-834, defaults L866-867).

---

## 1. Objective + what "improved-recall substrate" means

**Objective.** Re-run **EXP-B′** (the per-intent `candidate_k × pool_n × alpha` joint-tune) on the
**live cross-encoder engine** (`default-reranker` ON), so the per-intent config tuples are measured from
**fresh, real `ce_norm`** instead of the **0.8.3 CE-pass fallback** the 0.8.11 run was forced onto. The
0.8.11 EXP-B′ produced its tuples from a **feature-OFF build** (`rerank_fused` returned identity
passthrough — see the `build_blocker` block in `expb_joint_tune_run.py` L554-566 and the harness's own
degeneracy guard `ce_norm_is_active` L440-447); the handoff therefore tags every EXP-B′ tuple **PROVISIONAL,
V-1-blocking** (`0.8.11-handoff-to-0.8.15.md` §1 row "EXP-B′", §4 table). V-1 discharges that caveat.

Three deltas over the 0.8.11 run (the V-1 gate line, handoff §2):

1. **Live CE** — re-derive the rerank tuples on a `--features default-reranker` build (not 0.8.3 data).
2. **Fill the thin classes** — give `multi_hop` a **measured** tuple (MuSiQue, now carrying
   `question_decomposition` per P0-3) instead of the EXP-0 provisional pin; characterize `global`.
3. **Add MMR + recency** — the two knobs the handoff flags **unmeasured** (`§5` function ledger: MMR
   "unmeasured", recency "unmeasured / not isolated").

**"Improved-recall substrate" (the V-3/V-7 deliverable).** V-1's output is the **re-validated per-intent
config registry + recall envelope** that everything downstream tunes *within*:

- the **per-intent tuples** `{intent → (candidate_k, pool_n, alpha, final_K, mmr, recency, forbidden_ops)}`
  now measured on live CE (§3 tuple format in `expb_joint_tune_run.py:make_tuple` L295-334), with
  `multi_hop` filled;
- the **gold-in-pool recall envelope** per intent at `candidate_k ∈ {10,50,100,200,300,500}` measured
  **fresh** on the current engine's base retrieval (`recall_envelope_by_intent` L183-198);
- the **re-confirmed §II.C crux** (does `alpha=1.0 @ pool_n=50` still drop r@10 vs `pool_n=10`?).

V-3 (OPP-1 EXP-ITER-D/-P/-POLICY) iterates **on top of this substrate** — testing whether decomposition
*manufactures* recall above this envelope; V-7 (OPP-3 CE-default-on packaging) records the CE bearing this
re-run establishes. Running iteration on the *unimproved* (0.8.3-fallback) substrate is the exact error the
handoff throughline warns against ("improve the recall functions first, then re-run the router/agent
experiments on the better substrate").

---

## 2. Arm matrix

**Base sweep (re-run of EXP-B′, live CE).** Reuse the frozen grid in `expb_joint_tune_run.py` L65-77
verbatim so the result is comparable to 0.8.11:

| Axis | Grid | Source |
|------|------|--------|
| `candidate_k` (recall stage) | **{200, 300, 500}** | `CANDIDATE_KS` L65 — EXP-A best=200, not saturated, so ≥200 |
| `pool_n` (CE-rerank depth) | **{10, 20, 50, 100, 200}** | `POOL_NS` L69 — spans the §II.C crux at depth |
| `alpha` (CE blend weight) | **{0.0, 0.3, 0.5, 0.7, 1.0}** | `ALPHAS` L71 — 0.0=pure base, 0.3=prod C6 guard, 1.0=pure CE |
| `final_K` | **10** (pinned) | `FINAL_K` L73 |
| bootstrap | 2000× resamples, seed `0xB5`, 95% percentile CI | L80-81 |

`default-reranker` is **ON** for the whole matrix (that is the point of V-1). The engine knobs map directly:
the offline re-blend `alpha*ce_norm + (1-alpha)*minmax(base_score)` over the top-`pool_n` of the
`candidate_k`-truncated pool (`per_query_rerank_metrics` L152-175) **mirrors** the engine
`rerank_fused`/`ce_rerank` path (`lib.rs:rerank_fused` L5474; pyo3 `search(... alpha, pool_n ...)`
`fathomdb-py/src/lib.rs` L833-867). Anchor points to re-confirm: the **measured-parity** `alpha=1.0,
pool_n=10` (py comment L832) and the 0.8.3 per-intent optima below.

**Prior optima to re-confirm (0.8.3 fallback → must reproduce on live CE):**

| intent | candidate_k | pool_n | alpha | r@10 (0.8.3 data) |
|--------|---|---|---|---|
| needle | 200 | 50 | 0.7 | 0.6438 |
| multi_session | 300 | 100 | 1.0 | 0.4667 |
| temporal | 500 | 20 | 1.0 | 0.5133 |

(`expb-joint-tune.md`. §II.C crux on that data: pooled drops True, Δ(50−10)=−0.0413; needle α=1.0
p50−p10 = −0.1307.)

**× MMR (diversity).** Add an MMR re-rank stage as an **offline transform** over the candidate pool's
**bge-small** embeddings (cached: `models--BAAI--bge-small-en-v1.5`), parameterised by
`mmr_lambda ∈ {off, 0.3, 0.5, 0.7}` (λ=1.0 ≡ off / pure relevance). Applied **after** the CE re-blend,
before the `final_K` cut. Primary target classes: **multi_session** and **global** (de-duplication of
near-identical sessions/passages).

**× recency.** Add a recency decay as an **offline transform** on the fused score,
`score' = score * exp(-Δt / half_life)`, `half_life_days ∈ {off, 7, 30, 90}`, keyed on the node
timestamp. Primary target class: **temporal**.

**Arms that actually run (V-1 scope):**

- **A. Base re-run** — full `{candidate_k} × {pool_n} × {alpha}` grid, **−MMR −recency**, on all
  measurable intents (needle, multi_session, temporal, multi_hop). *This is the keystone — it must run.*
- **B. +MMR** — base grid at **each intent's argmax `(candidate_k,pool_n,alpha)`** × `mmr_lambda` (not the
  full cross-product — sweep MMR only at the per-intent optimum to bound cost), on multi_session + global.
- **C. +recency** — same, at each intent's optimum × `half_life_days`, on temporal.
- **D. +MMR +recency together** — only at the temporal/multi_session optima, to check interaction.

Arms B-D are **knob-isolation** sweeps anchored at A's optima (the handoff asks to *measure* MMR/recency,
not re-tune the whole grid against them). **Open design choice (HITL, §7):** MMR/recency are **not in the
engine search path today** (`make_tuple` emits them as disabled placeholders, L313-314/L329-330) — V-1
measures them as **offline transforms** on the persisted candidate pool (no engine code), which keeps V-1
$0 and code-free; productizing either knob is a later, separately-gated engine change.

---

## 3. Corpora × classes (PER-CORPUS, never pooled)

Mapping uses the harness intent map `LME_CLASS_TO_INTENT` (L84-89) and the corpus assignments in
`expb_joint_tune_run.py` (L411-437, `corpora_measured` L584-588). **Per-corpus discipline is enforced by
`opp3_eval_support.decide_per_corpus`** (L210-235), which raises on any reserved cross-corpus pool key
(`{"pooled","all","combined","all_corpora","overall","global"}`, L205-207). Each corpus is its own
`r2_parity_eval` run → its own decision.

| class | corpus → gold | present? | how filled |
|-------|---------------|----------|------------|
| **needle** | LME factoid + knowledge_update; gold `0.8.3-d0a-memory-gold.json` | ✅ (`longmemeval-cleaned` HF cache + gold file) | measured (as 0.8.11) |
| **multi_session** | LME multi_session (+ LOCOMO corroboration) | ✅ LME; LOCOMO **needs acquire** (`acquire_locomo.py` present, raw not built) | measured on LME; LOCOMO is a $0 corroboration add (handoff §2 V-5 corpus), optional for V-1 |
| **temporal** | LME temporal (+ LOCOMO) | ✅ LME; LOCOMO as above | measured on LME; recency knob targets this class |
| **multi_hop** | **MuSiQue** (2,417 answerable, `question_decomposition` retained per P0-3) **+ HotpotQA** | ✅ MuSiQue (`data/corpus-data/raw/musique_dev.jsonl`, P0-3 verified: 2,417 rows carry `question_decomposition`); **HotpotQA ABSENT** | **fill on MuSiQue for V-1** (replaces the EXP-0 provisional pin with a measured tuple); **HotpotQA deferred — see blocker B3** |
| **global** | AP-News BenchmarkQED, **win-rate / `decide_084` axis** | acquire script present (`acquire_apnews_benchmarkqed.py`), raw not built | **NO node-level retrieval gold by design** — global has no r@10 axis; **stays provisional in V-1** (see flag below) |

**How `multi_hop` gets filled.** MuSiQue is present and P0-3-complete (verified: `answerable` subset =
2,417, all carry `question_decomposition`). V-1 runs a **fresh fused+CE pass** on MuSiQue (the 0.8.11 run
deferred it — "no prior MuSiQue CE-pass exists → pinned provisional", `expb_joint_tune_run.py` L567-571)
and emits a **measured** `multi_hop` tuple. HotpotQA is the intended 2nd multi_hop corpus (handoff §2b
EXP-ITER-D power target) but is **not acquirable today** (blocker B3) — V-1 lands multi_hop on MuSiQue
alone and flags HotpotQA for V-3/V-5.

**Flag — `global` has no adequate corpus for the V-1 $0 axis.** `global` (sensemaking) is scored on the
**win-rate / `decide_084`** axis with an **LLM judge**, *not* node-level retrieval gold
(`expb_joint_tune_run.py` L572-574, `make_tuple` keeps it provisional L321-334). Filling it properly is a
**priced** pass (draws the $75 pool) and belongs to **OPP-6 / V-7**, not the $0 V-1 keystone. V-1 keeps
`global` **provisional-pinned** (EXP-0-global tuple `alpha=0.3, pool_n=10, candidate_k=200`, L98-100) and
records that as a known gap. MMR may still be *measured* on global if a retrieval proxy gold is available;
otherwise global's MMR row is reported as N/A.

---

## 4. Metrics + decision rule

**Per-corpus / per-class metrics** (all $0, computed from ranked ids/scores the engine already returns):

- **r@final_K (r@10)** — strict recall@10, the primary maximand (`strict_recall_at_k`, harness L173).
- **recall@k / gold-in-pool envelope** — at `k ∈ {10,50,100,200,300,500}`, base order, alpha-invariant
  (`recall_envelope_by_intent` L183-198) — the recall substrate V-3 reads.
- **MRR** — reciprocal first-gold rank (`per_query_rerank_metrics` L175).
- **nDCG@10** — **NEW for V-1** (not in the current harness; small additive metric fn) — adds graded-rank
  sensitivity the binary r@10 misses, and is the comparison metric for the BEIR-class V-2/V-6 follow-ons.
- **P0-4 margins** — `top_gap`, `gold_rival_margin`, and their `_ce` variants
  (`opp3_eval_support.Margins` L243-261, `margins_from_search_result` L324-345), computed from the fused
  `score` + per-candidate `ce_score` already on `SearchHit`. These quantify **how decisively** gold beats
  its nearest rival on the live CE — the signal OPP-3/V-7 needs and that the 0.8.11 run never had (no live
  CE). Optionally stress-test robustness with `inject_distractors` / `demote_gold` (L54-194).

All reported **per-corpus, never pooled** via `decide_per_corpus`; the per-CLASS `"pooled"` view *inside a
single corpus* (LME's 3 classes) is a class pool, not a corpus pool, and remains allowed (L21-24, L201-207).

**Decision rule (what V-1 must show to unblock V-3/V-7):**

1. **Tuple re-validation (blocking).** Re-run the KILL check (`kill_check`, L525-545): do the per-intent
   optima **diverge** on live CE (distinct `(candidate_k,pool_n,alpha)` signatures beyond
   `DIVERGENCE_EPS=0.02`)? **GO** = diverge (config-carrying router retains measured value — confirms the
   0.8.11 GO on real CE); **KILL** = collapse to one global config (router ships pinned to the global
   tuple). Either way the tuples are now **live-CE-measured, not provisional**.
2. **§II.C crux re-confirmation.** Reproduce the needle `alpha=1.0` `pool_n=50 < pool_n=10` r@10 drop on
   the live engine (`crux_check` L265-287). If the live CE *removes* the drop, that itself is a finding
   that feeds the V-7 pool_n default.
3. **multi_hop filled.** MuSiQue yields a **measured** `multi_hop` tuple with a bootstrap CI (replaces the
   EXP-0 pin). This is the recall baseline V-3's decomposition arm must beat.
4. **MMR/recency dispositioned.** For each knob, apply the OPP-6 exit bar (plan §4): a knob is a lever for
   a class iff **Δ(r@10) CI-lower-bound > +0.04** vs that class's no-knob optimum; otherwise it is
   recorded as "no measured lift" and left off-default. (recency→temporal, MMR→multi_session/global.)
5. **Recall envelope re-measured** fresh on the current engine per intent.

**Gate:** V-1 **lands** (and unblocks V-3/V-7) when items 1-3 + 5 produce committed verdicts/artifacts
(R-U-1: a result doc + repro, not an `AGREED`) on live CE, and item 4 has a disposition for each knob.
`global` staying provisional is an **acknowledged carry**, not a blocker, since V-3 (OPP-1) operates on
needle/multi_hop and V-7 records the CE bearing the measured classes establish.

---

## 5. $0-vs-priced

**V-1 is $0 / fully local — it draws NOTHING from the $75 pool.** Every metric (r@10, recall envelope,
MRR, nDCG@10, P0-4 margins) is a **retrieval-metric over existing node-level gold** — no LLM answerer is
invoked. The two models are local CPU: the **TinyBERT-L-2 cross-encoder** (`lib.rs` L5658-5661, Candle/CPU)
and the **bge-small** embedder (cached). This matches the 0.8.11 EXP-B′ cost line exactly
(`cost_usd: 0.0`, `expb_joint_tune_run.py` L551-553; `expb-joint-tune.md` "cost: $0.00").

**The one priced thing V-1 deliberately does NOT do:** fill `global` on the `decide_084` win-rate axis
(LLM judge). That is excluded from V-1 to keep it $0; it is OPP-6 / V-7 priced work under the pooled
envelope. So: **V-1 = $0, no pool draw, no LLM, no network egress** (other than the one-time CE model
fetch — see B1).

---

## 6. Prerequisite status + runnability

**Phase-0 prerequisites (per plan §0):**

| Prereq | Status | Evidence |
|--------|--------|----------|
| **P0-3** MuSiQue re-pull retains `question_decomposition` | ✅ DONE | commit `41c7b49a`; `musique_dev.jsonl` present; **verified 2,417 answerable rows carry `question_decomposition`** |
| **P0-4** eval-support (margins + distractor/rank knobs + per-corpus decide) | ✅ DONE | commit `94ab2417`; `src/python/eval/opp3_eval_support.py` + `test_opp3_eval_support.py` |
| **P0-5** Memex 0.5.2 value-test harness confirmed | ✅ (not on V-1 path) | V-1 is an **academic arm** — runs on FathomDB `decide_08x`, not the Memex harness (plan §0.5) |
| **Cause-A** | VERIFIED ✅ / CUT IN PROGRESS | `CAUSE-A-sizing.md` GO (additive-only); **not a V-1 dependency** (gates only real-gold *adoption* arms, plan §4) |

**Runnability of V-1 in THIS eval environment — verified present:**

- ✅ **LME** corpus (`longmemeval-cleaned` HF cache) + gold (`dev/plans/runs/0.8.3-d0a-memory-gold.json`).
- ✅ **MuSiQue** corpus with `question_decomposition` (P0-3, 2,417 rows).
- ✅ **bge-small** embedder (`models--BAAI--bge-small-en-v1.5`).
- ✅ The **harness** (`expb_joint_tune_run.py`) and **P0-4 eval-support** are landed and importable.

**BLOCKERS / flags to clear before V-1 can actually run (the prompt asked to CHECK — these are real):**

- **B1 — CE model NOT cached (BLOCKER).** `cross-encoder/ms-marco-TinyBERT-L-2` is **absent** from the HF
  hub cache (checked `~/.cache/huggingface/hub` — only `bge-small`, `musique`, `longmemeval-cleaned`,
  BEIR sets present). The whole premise of V-1 is **live CE**, so this model **must be fetched** first
  (one-time network pull). Without it `rerank_fused` cannot produce real `ce_norm`.
- **B2 — live build almost certainly has `default-reranker` OFF (BLOCKER).** The agent build is
  `pip install -e src/python` with **no `--features`** (`scripts/agent-build.sh` L22), and the 0.8.11 run
  explicitly hit `rerank_fused` gated off → identity passthrough (`expb_joint_tune_run.py` L554-566). V-1
  **requires a rebuild with `--features default-reranker,default-embedder,test-hooks`**. The harness's own
  guard **`ce_norm_is_active` (L440-447) will HARD-STOP** if pointed at a feature-off pass — so a bad build
  fails loudly, not silently. Note the worktree constraint: **`maturin develop` must run on the MAIN tree,
  not this worktree** (`scripts/preflight.sh` L100; memory: worktree maturin breaks the `.venv` binding).
  This rebuild **is itself the V-7 packaging question** (shipped wheel has `default-reranker` OFF) — V-1
  surfaces it first.
- **B3 — HotpotQA absent (partial, non-blocking for V-1).** No HF cache, **no acquire script** in
  `tests/corpus/scripts/` (only musique/locomo/apnews exist). V-1 lands `multi_hop` on **MuSiQue alone**;
  HotpotQA needs an acquire script and is deferred to V-3/V-5.
- **B4 — LOCOMO / AP-News raw not built (non-blocking).** Acquire scripts present
  (`acquire_locomo.py`, `acquire_apnews_benchmarkqed.py`); LOCOMO corroboration is an optional $0 add,
  AP-News/global is priced-judge (out of V-1 $0 scope).
- **B5 — `global` has no node-level retrieval gold (inherent).** Stays provisional in V-1 (§3).

---

## 7. Execution steps + open questions

**Execution (once HITL approves + B1/B2 cleared):**

1. **Clear B1/B2** on the **main tree**: fetch `cross-encoder/ms-marco-TinyBERT-L-2`; rebuild
   `maturin develop --features default-reranker,default-embedder,test-hooks`; confirm via a smoke that
   `ce_norm_is_active` returns True on a tiny pass (the built-in degeneracy guard).
2. **Base re-run (arm A)** — point the EXP-B′ harness at the live CE build, regenerating the **real-CE
   pass** instead of the 0.8.3 fallback:
   `python -m eval.expb_joint_tune_run --rerank-ce-pass <fresh-live-CE-pass> --out-json
   dev/plans/runs/V-1-output.json --out-md dev/plans/runs/V-1-results.md`
   (the `--recall-pool-ckpt` / `--rerank-ce-pass` checkpoints L455-460 give the priced-run-style
   incremental resume; V-1 is $0 but the CE pass is the slow part, so checkpoint it).
3. **MuSiQue multi_hop pass** — run the fused+CE pass on the 2,417 answerable rows; emit the measured
   `multi_hop` tuple + CI.
4. **Arms B-D (MMR/recency)** — offline transforms at each intent's optimum; apply the +0.04 CI-lo bar.
5. **Per-corpus decide** — wrap every corpus's resolution through `decide_per_corpus` (never pooled);
   compute nDCG@10 + the P0-4 margins per corpus.
6. **Commit** `V-1-output.json` + `V-1-results.md`; update `runs/STATUS-0.8.11.2.md` V-1 row from
   "not started" → the landed verdict; only then unblock V-3.

**Open questions for HITL:**

1. **Build vehicle (B2).** Confirm V-1 rebuilds `default-reranker` on the **main tree** (worktree maturin
   is forbidden). Is a transient feature-on build for eval acceptable given the shipped wheel ships it OFF
   (the V-7 question)? Who owns the rebuild?
2. **CE model fetch (B1).** Is one-time network access to pull `cross-encoder/ms-marco-TinyBERT-L-2`
   authorized in this eval env?
3. **multi_hop scope.** Accept landing `multi_hop` on **MuSiQue alone** for V-1, deferring HotpotQA (needs
   a new acquire script) to V-3/V-5? Or block V-1 until HotpotQA is acquirable?
4. **global.** Accept `global` **staying provisional** in V-1 (its proper fill is the priced `decide_084`
   axis = OPP-6/V-7), keeping V-1 strictly $0?
5. **MMR/recency mechanism.** Approve measuring MMR/recency as **offline transforms** (no engine code,
   keeps V-1 $0/code-free), with productization deferred to a separately-gated engine change? Confirm the
   target classes (recency→temporal, MMR→multi_session/global) and the **+0.04 CI-lo** lever bar.
6. **LOCOMO corroboration.** Include the $0 LOCOMO corroboration of multi_session/temporal in V-1, or
   defer to V-5 (multi-corpus)?
