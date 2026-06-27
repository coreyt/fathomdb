# Free-threaded Python (PEP 703) for FathomDB — value, lift, and experiment plan

> **Status:** analysis / experiment proposal (pre-decision). Owner: TBD. Raised out of the 0.8.8
> pyo3 `0.24.1 → 0.29.0` bump, where pyo3 0.28+ makes `#[pymodule]` free-threaded **by default** and
> forces an explicit `gil_used` declaration.
>
> **Scheduling (master §6 F-5):** the EXP-FT ladder (FT-1…5) is folded into **0.8.15** (the forks/overflow
> odd slot — `$0` eval, gated only on the 0.8.8 pyo3 0.29 landing), kept **inside 0.8.x**. Productization
> (`gil_used = false` + the non-abi3 `*t` wheels) is a **0.8.15-readout contingency** → 0.8.16 #11-full or
> a net-new 0.8.17. "Pre-decision" here is about the *outcome* (V1/V2, the GIL flip), not the slot.
>
> **Scope.** Should FathomDB's `fathomdb-py` binding *support* free-threaded CPython (declare
> `#[pymodule(gil_used = false)]`), rather than merely *tolerate* it by pinning the GIL back on
> (`gil_used = true`)? This doc sizes the value, sizes the lift, and proposes the experiments that turn
> both into objective numbers. It does **not** decide; it gives the steward + HITL the data plan.

---

## 0. TL;DR / provisional recommendation

- **0.8.8 ships `gil_used = true`** (the safe move — preserve today's GIL semantics on a patched binding).
  This is correct and unblocks the security bump. Nothing below changes that.
- **Free-threaded *support* is a separate, later, data-gated campaign.** The dominant value is **not**
  FathomDB running faster — the engine already does all real work with the GIL released, so the direct
  throughput ceiling is bounded by the small GIL-held marshalling fraction (to be measured, EXP-FT-1).
  The dominant value is **ecosystem citizenship**: under free-threaded CPython, importing a module
  declared `gil_used = true` **re-enables the GIL process-wide**, so FathomDB would silently *poison*
  free-threading for its own consumers (Memex / Hermes / OpenClaw — hot-path agent-memory apps). As
  free-threaded builds go mainstream (3.13t experimental → 3.14t improving), a hot-path memory library
  that forces the GIL back on becomes an ecosystem liability.
- **The lift is lower than "support no-GIL" usually implies**, because the risky part — shared mutable
  state protected only by the GIL — **does not exist in FathomDB's engine**. The engine is already
  thread-safe via real Rust primitives and is already called with the GIL released. The FT-safety risk
  surface is narrowly the binding's thin marshalling layer + packaging (free-threaded wheels are not
  abi3-compatible). See §3, §5.
- **Recommendation:** run EXP-FT-1 (cheap, ~hours) and EXP-FT-3 (the poisoning cost) first; they decide
  whether the value is "nice-to-have throughput" or "forward-looking citizenship requirement." Gate any
  `gil_used = false` claim on EXP-FT-4 (concurrency-safety, a hard gate).

---

## 1. Background — what changed and why it matters

**PEP 703 / free-threaded CPython.** CPython 3.13 introduced an experimental build (`python3.13t`,
wheel tag `cp313t`) that removes the Global Interpreter Lock, allowing Python threads to execute
bytecode truly in parallel. 3.14 hardens it. Adoption among native extensions is the gating factor for
real-world use.

**The pyo3 0.28 default + the `gil_used` flag.** From pyo3 0.28, `#[pymodule]` modules are assumed to
support free-threading unless told otherwise. The declaration is a **correctness claim** to the
interpreter, with a sharp runtime consequence:

- `#[pymodule(gil_used = false)]` — "this module is safe without the GIL." On a free-threaded
  interpreter, the module loads and the process stays free-threaded.
- `#[pymodule(gil_used = true)]` — "this module needs the GIL." On a free-threaded interpreter, CPython
  **re-enables the GIL for the entire process** (emitting a `RuntimeWarning`) unless the user forces
  `PYTHON_GIL=0`. One such module anywhere in the import graph collapses free-threading for *all* of it.

So the choice is not local. `gil_used = true` is safe for *us* but externalizes a cost onto every
consumer who opted into free-threading. That externality — not FathomDB's own speed — is the crux of the
value question.

---

## 2. Why the value question is *empirical*, not obvious

The naive pitch ("no GIL = FathomDB scales across threads") is mostly **already true today** and
therefore mostly **not** the FT value-add. The binding releases the GIL around every engine call:

- `call_engine` wraps every engine method in `py.allow_threads(|| catch_unwind(...))`
  (`fathomdb-py/src/lib.rs` — the single chokepoint all of `write`/`search`/`read_*`/`drain`/`embed`/
  `ingest_with_extractor` route through). `open` and `rerank` release the GIL directly too.
- Therefore, **today, on stock GIL CPython, N Python threads calling `search()` already run their engine
  work in parallel** — the GIL is held only during the thin marshalling at entry/exit (string
  validation, dict construction, `extract`/`downcast`).

What free-threading additionally unlocks is exactly the part that is *still* serialized today:

1. The **GIL-held marshalling fraction** of each call (string validation `validate_ffi_string`, building
   `PyDict`s in `rerank`/`embedder_event_to_py`, `extract`/`cast` of inputs).
2. **Python-side application code** running concurrently *with* and *between* FathomDB calls (the agent
   loop's own token-wrangling, JSON, orchestration).

(1) is plausibly small for FathomDB-bound workloads → small direct win. (2) is real but accrues to the
*application*, and is exactly what `gil_used = true` would destroy process-wide. Both must be measured,
not assumed. Hence the experiments.

---

## 3. Architecture facts that bound the lift (grounded in the code)

The reason FT-support is *cheaper* for FathomDB than for a typical C/Cython extension: **the GIL was
never the thing protecting FathomDB's mutable state.** The engine is a Rust core called with the GIL
released, already concurrency-safe by construction:

- **Concurrent reads already happen in Rust.** Reads ride the `ReaderWorkerPool` DEFERRED-tx path
  (`fathomdb-engine/src/lib.rs:380`, `:3104`), independent of the GIL.
- **Shared state uses real Rust sync primitives, not GIL-implicit serialization.** Engine event buffer
  is `Mutex<Vec<EmbedderEvent>>` (`:289`); the CE reranker lazy-init is `std::sync::OnceLock`
  (`:5086`, *not* a `GILOnceCell`). The embedder is an `Arc<dyn Embedder>` "invoked by at most one
  worker at a time" (`:221`) — the engine serializes it itself, GIL or no GIL.
- **`Arc<dyn Embedder>` requires `Send + Sync`** — the engine is already designed to be shared across OS
  threads. Removing the GIL does not introduce a new race class into the engine; if the engine were
  GIL-dependent it would already be unsound under the existing `allow_threads` calls.
- **The binding's `#[pyclass]` payloads are `frozen`** (`PyWriteReceipt`, `PySearchHit`, … all
  `#[pyclass(frozen, get_all)]`) — immutable, FT-trivially-safe. `PyEngine` holds two `Arc`s and exposes
  `&self` methods that only clone Arcs.
- **The logging bridge is presently a no-op stub** — `attach_logging_subscriber` ignores its `logger`
  (`:816`). The historical pyo3-log GIL-deadlock hazard (`dev/learnings.md`) is therefore **not live**
  in the binding today; FT-support must keep it that way (or, when the subscriber is wired in a later
  slice, design it FT-safe from the start).

**Implication:** the FT-safety audit is confined to the binding's marshalling layer + a few Python-object
touchpoints (`setattr` on exception values under `Python::with_gil`/`attach`; `PyDict` building), plus
packaging. It is *not* an engine-core rewrite. This is the single biggest input to the lift estimate.

---

## 4. Value analysis — three streams, honestly sized

| # | Value stream | Mechanism | A-priori size | Decided by |
|---|--------------|-----------|---------------|-----------|
| V1 | **Ecosystem non-poisoning** | `gil_used = true` re-enables the GIL process-wide on FT interpreters → kills FT for the *whole* consumer app | **Potentially large & growing**; this is the real case | EXP-FT-3 (+ adoption trajectory) |
| V2 | **Direct multi-thread throughput** of FathomDB calls | FT parallelizes the GIL-held marshalling fraction that `allow_threads` can't | **Likely small** (engine already GIL-free) | EXP-FT-1, EXP-FT-2 |
| V3 | **Future surface** (Python-callback embedders, in-process telemetry fan-out, batch ingest from threads) | New features that *would* hold the GIL longer benefit more from FT | Speculative / deferred | revisit when those land |

**Framing for HITL.** If EXP-FT-1 shows the GIL-held fraction is a few percent and EXP-FT-2 shows GIL
CPython already scales (because engine calls release the GIL), then **V2 ≈ 0** and the entire case rests
on **V1**. V1 is a *citizenship / forward-compatibility* argument, not a benchmark win — appropriate to
weigh against FathomDB's stated "function over footprint, user-controlled spend" posture and its identity
as a hot-path library inside other people's free-threaded agents.

---

## 5. Lift analysis

Three layers, smallest to largest assurance cost.

### 5.1 Mechanical (trivial)
- Flip the module marker to `#[pymodule(gil_used = false)]` (one line). This is the *only* code change to
  *claim* support — everything else is verification + packaging.
- (Already required by the 0.8.8 bump regardless: the `with_gil→attach`, `allow_threads→detach`,
  `downcast→cast`, `PyObject→Py<PyAny>` renames. Those are orthogonal to FT and land in 0.8.8 Slice 1.)

### 5.2 Correctness / assurance (the real lift — MEDIUM)
The claim `gil_used = false` is only honest if verified. Required work:
1. **Build a free-threaded CPython toolchain** (`python3.13t` / `3.14t`) into a dev + CI lane.
2. **Run the full existing Py suite (67 tests) under `3.13t`** — must be green.
3. **Author a concurrency stress suite** (new): N threads (N=2..16) sharing one `Engine`, issuing mixed
   `write`/`search`/`read_*`/`drain`, asserting: byte-stability of results, write-cursor monotonicity,
   no panics surfacing as aborts, **no deadlocks** (re-validate the multi-instance + logging hazard).
4. **Run under sanitizers** where feasible — ThreadSanitizer on the Rust side; pyo3 debug reference-count
   checks; CPython's FT race assertions.
5. **Audit the marshalling layer** for any remaining GIL-implicit assumption (exception `setattr`
   sequences, `Python::attach` regions, dict building). Per §3 this surface is small.
6. **Re-clear the existing gates under FT** — byte-stability + eu7 recall must hold on the FT build.

Risk note: the engine core is believed FT-safe by construction (§3), but "believed" is not "tested under
a no-GIL interpreter." EXP-FT-4 converts belief into evidence and is the **hard gate**.

### 5.3 Packaging (MEDIUM — easy to under-estimate)
- **Free-threaded wheels are not abi3.** The binding ships `abi3-py310` (one wheel, many versions). The
  free-threaded ABI is version-specific (`cp313t`, `cp314t`, …) and *not* covered by the abi3 wheel. FT
  support therefore means **adding per-version `*t` wheels to the build matrix** alongside the abi3 wheel
  — more CI build minutes, more artifacts, a maturin/cibuildwheel matrix change.
- Decision sub-question: ship FT wheels for which interpreters, and from which release? This may itself
  gate FT support to a later release purely on packaging readiness, independent of correctness.

### 5.4 Lift summary
| Layer | Size | Notes |
|-------|------|-------|
| Mechanical (claim) | **XS** | one line |
| Correctness/assurance | **M** | FT toolchain + stress suite + sanitizer run; engine core low-risk per §3 |
| Packaging | **M** | non-abi3 per-version wheel matrix; CI cost |
| **Total to ship FT support** | **MEDIUM** | dominated by FT CI lane + wheel matrix, not by code |

---

## 6. Proposed experiments — objective data + evaluation criteria

All experiments build FathomDB twice: **(A) stock CPython 3.13, `gil_used = true`** and **(B)
free-threaded CPython 3.13t, `gil_used = false`**, off the same engine commit. Local-only, deterministic
where possible; no network.

### EXP-FT-1 — GIL-held fraction profiling (cheapest; run first)
- **Question:** what fraction of each op's wall-time is spent GIL-held (marshalling) vs GIL-released
  (engine)? This is the ceiling on V2.
- **Method:** instrument the binding to timestamp around the `detach`/`attach` boundary for a
  representative op mix: small `write`, large `write` batch (marshalling-heavy), `search`,
  `search`+rerank (`PyDict` fan-out heavy), `read_list`, `rerank` over a large passage list,
  `ingest_with_extractor`. Report GIL-held µs and % of total per op (median over K reps).
- **Objective data:** per-op `{total_us, gil_held_us, gil_held_pct}`.
- **Eval criterion:** if **max GIL-held % across ops < 5%**, V2 is negligible → value rests on V1, and
  the FT case is "citizenship, not speed." If **any hot op > 20% GIL-held** (suspect: large-batch
  `write` marshalling, `rerank` dict construction), V2 is real *there* → quantify in EXP-FT-2.

### EXP-FT-2 — Multi-thread throughput scaling (A vs B)
- **Question:** does FT actually raise aggregate throughput, beyond what GIL CPython already gets from
  `allow_threads`?
- **Method:** fixed harness, K Python threads (K ∈ {1,2,4,8,16}) sharing one `Engine`, each driving a
  stream of mixed `search`/`write`/`read` ops against a fixed corpus. Measure aggregate QPS and
  p50/p99 latency for builds A and B. Pin CPU count; record core count.
- **Objective data:** throughput(K) and p99(K) curves for A and B; the B/A multiplier at each K.
- **Eval criterion:** report **B/A throughput multiplier at K=8**. Interpretation:
  - multiplier ≈ 1.0 → GIL CPython already scales (engine releases GIL); FT adds ~nothing → V2 ≈ 0.
  - multiplier ≥ ~1.3 at K≥4 → FT unlocks real marshalling/contention-bound throughput → V2 is material.
  Cross-check against EXP-FT-1: a high multiplier should be explained by a high GIL-held fraction.

### EXP-FT-3 — The GIL-poisoning cost (the V1 number)
- **Question:** how much *application-level* parallelism does FathomDB's `gil_used = true` destroy on an
  FT interpreter?
- **Method:** synthetic agent-loop workload on **3.13t**: P threads doing realistic Python-side work
  (JSON/token wrangling, embedding orchestration in pure Python) interleaved with FathomDB calls. Run
  with (a) FathomDB `gil_used = true` (forces the GIL back on process-wide) vs (b) FathomDB
  `gil_used = false` (or a stub module declaring false). Measure the Python-side work's aggregate
  throughput and the end-to-end loop QPS.
- **Objective data:** app-throughput(a) vs app-throughput(b) at P ∈ {2,4,8}; the parallelism lost ratio
  `1 − tput(a)/tput(b)`.
- **Eval criterion:** **parallelism-lost ratio** quantifies V1 independent of FathomDB's own speed. A
  large ratio (e.g., >40% of app parallelism lost) makes `gil_used = true` an active ecosystem harm and
  argues for prioritizing FT support; a small ratio weakens V1.

### EXP-FT-4 — Concurrency safety under no-GIL (HARD GATE for any `gil_used = false`)
- **Question:** is the binding+engine actually race/deadlock-free without the GIL?
- **Method:** on **3.13t**, run the full Py suite + the new stress suite (§5.2.3): M iterations of
  N-thread mixed `write`/`search`/`read`/`drain` on a shared `Engine`, plus a multi-`Engine`-instance
  variant (the historical deadlock shape). Run Rust under ThreadSanitizer; enable pyo3/CPython FT debug
  assertions.
- **Objective data:** pass/fail; count of TSan races, deadlocks, aborts, refcount errors over M runs.
- **Eval criterion:** **zero races / zero deadlocks / zero aborts across M ≥ (TBD, e.g. 1000) iterations
  is mandatory** before `gil_used = false` may ship. Any failure → BLOCK → root-cause → HITL. This gate
  is where §3's "FT-safe by construction" belief is empirically confirmed or refuted.

### EXP-FT-5 — Wheel-matrix feasibility (packaging)
- **Question:** what does shipping FT wheels actually cost, and is abi3 reuse possible?
- **Method:** prototype a maturin/cibuildwheel matrix adding `cp313t`(/`cp314t`) targets beside the
  abi3-py310 wheel; build them; measure added CI wall-time, artifact count, and whether the abi3 wheel
  can serve FT interpreters (expected: no).
- **Objective data:** added CI minutes, wheel count delta, abi3-on-FT load success/failure.
- **Eval criterion:** packaging lift is **acceptable** if it fits the existing release CI budget without
  a structural overhaul; otherwise FT support is **packaging-gated to a later release** even if §6
  correctness passes.

---

## 7. Decision rule (how the data resolves the question)

```
run EXP-FT-1 (hours) and EXP-FT-3 (day):
  if V2 (EXP-FT-1/2) small AND V1 (EXP-FT-3) small:
      → keep gil_used = true indefinitely; revisit only when a V3 feature lands or FT adoption forces it.
  if V1 large (EXP-FT-3 parallelism-lost high) OR consumer (Memex/Hermes/OpenClaw) ships on 3.1Xt:
      → FT support becomes a citizenship requirement → schedule the campaign:
           gate on EXP-FT-4 (zero races) → resolve EXP-FT-5 (wheels) → ship gil_used = false.
  if V2 large (rare, EXP-FT-2 multiplier high):
      → FT support also pays for itself directly; same campaign, stronger justification.
in all cases: 0.8.8 ships gil_used = true now. FT support is never in the 0.8.8 critical path.
```

**Working hypothesis (to be falsified):** V2 ≈ small (engine already GIL-free), V1 = the real and growing
value, engine core is FT-safe by construction so EXP-FT-4 passes, and the binding gate becomes a
**packaging + CI** decision (EXP-FT-5) more than a code-safety one.

---

## 8. Open questions / risks

- **FT adoption timing among consumers.** V1's weight scales with how soon Memex / Hermes / OpenClaw run
  on free-threaded CPython. Worth a direct read of their roadmaps before committing the campaign.
- **abi3 vs FT wheels** — confirm there is genuinely no abi3 path for FT (expected) and quantify the
  matrix blow-up (EXP-FT-5). This may dominate the decision.
- **GPU embedder path under FT** (0.8.7 device seam) — concurrent threads dispatching to one CUDA device;
  the engine already serializes the embedder (§3), but verify the device seam holds N-thread access.
- **Logging subscriber, when wired** — the currently-stub `attach_logging_subscriber` must be designed
  FT-safe (no GIL-implicit serialization, no re-entrant pyo3-log deadlock) if/when it goes live, or it
  reintroduces the historical hazard precisely where FT removes the GIL that masked it.
- **`gil_used = false` is a promise.** Shipping it and later discovering a race is worse than never
  claiming it. EXP-FT-4's hardness is deliberate.

---

## 9. Relationship to 0.8.8

0.8.8 Slice 1 (the pyo3 `0.24.1 → 0.29.0` bump) ships **`gil_used = true`** — see
`dev/plans/plan-0.8.8.md` §1 (Migration approach) and R-SEC-1. This document is the forward-looking case
for *revisiting* that default; it imposes **no** change on 0.8.8. The earliest the FT campaign could
begin is post-0.8.8, gated on the EXP-FT data above and an explicit HITL go.

---

## Appendix A — Survey: free-threaded Python in the pyo3 ecosystem (web research, 2026-06)

This appendix gathers how other pyo3 / Rust-backed projects have fared adopting free-threaded (FT)
CPython, what broke, and whether the expected benefits materialized. The headline for FathomDB: **its
architecture matches the projects that succeeded (Rust core + real locks + GIL released) and avoids the
shape of the projects that struggled (concurrent access to Python objects / mutable `#[pyclass]`).**

### A.1 Landscape & timeline
- **pyo3** first grew real FT support in **0.23.3 (Dec 2024)**; **0.28** flipped the default to
  "FT-assumed" with `#[pymodule(gil_used = true)]` as the opt-out (the exact knob driving this doc).
- **CPython:** 3.13t was experimental (2024); **3.14 (Oct 2025) made the free-threaded build a supported
  option** and re-enabled the specializing interpreter, cutting the single-thread penalty from **~40%
  (3.13t) to ~5–10% (3.14t)** (~9% on Linux x86_64 in 3.15 alpha). Per-object memory roughly doubles
  (`PyObject` 16 → ~32 bytes: adds `ob_tid`, `ob_mutex`, `ob_gc_bits`, ref-count fields). [PyO3 guide;
  CPython howto; Nandann 2026; danilchenko 3.14]
- **The GIL-poisoning effect is real and observed in the wild**, not theoretical: "importing a C
  extension without `Py_mod_gil` support silently enables the GIL … your free-threaded Python becomes
  GIL-enabled the moment you import an unsupported package." Matplotlib and SQLAlchemy are cited as
  causing partial/full process-wide GIL re-enablement. **This directly validates V1 / EXP-FT-3.**
  [danilchenko; py-free-threading porting guide]

### A.2 Project case studies
| Project | Stack | FT status (2026) | Lesson for FathomDB |
|---|---|---|---|
| **HF `tokenizers`** | pyo3 / Rust core | **Ships `cp314t` wheels; declares `Py_MOD_GIL_NOT_USED`; inner state in `std::sync::RwLock`, concurrent `encode` takes a read guard** | **Near-exact analog.** Same recipe FathomDB already follows (Rust core, `ReaderWorkerPool`, `Mutex`/`OnceLock`, GIL released). Strongest evidence the pattern works. |
| **pydantic-core** | pyo3 / Rust | FT import **segfaulted** pre-fix; resolved by pyo3 0.23; now ships FT wheels (2.29+) | "`gil_used=false` is a promise" — an unverified claim crashes. Reinforces EXP-FT-4 as a **hard gate**. |
| **polars** | Rust/C bindings, heavy Python-object interplay | **In progress / cautioned**: bindings "designed under the assumption the GIL exists" → forcing FT risks "non-deterministic crashes, deadlocks, or silent memory corruption" | The **anti-pattern**. FathomDB differs precisely here: engine does **not** touch Python refcounts/objects under load (GIL released; marshalling is thin). Why FathomDB ≈ tokenizers, not polars. |
| **cryptography (46+), orjson, rpds-py (0.22.3+)** | pyo3 / Rust | Shipping FT wheels | Broad Rust-backed adoption is now normal, not bleeding-edge. |
| **PyTorch + custom inference** (Trent Nelson) | C++/native + pyo3 deps | Realized **parallel transformer inference for the first time**; **no deadlocks/races** in the workload; main pain = ecosystem (had to manually rebuild `tiktoken` bumping pyo3 0.22.2→0.23.3; pydantic/Jupyter incompatible) | The dominant cost was the **dependency/packaging lift**, not own-code safety — matches our §5.3 framing. |

### A.3 Were the benefits realized? (and where they were *not*)
- **CPU-parallel Python work scales near-linearly:** danilchenko's 3.14 benchmarks show **~3.5× on 4
  cores** for prime counting and SHA-256, **3.28×** for matmul, while the **GIL build gained ~6% from
  threads (i.e. nothing).** PyTorch saw real parallel-inference gains.
- **But the benefit does NOT appear for work already off the GIL:** the same sources note **"I/O-bound
  code and existing NumPy-heavy workloads see no benefit."** Native calls that already release the GIL
  (NumPy, and by direct analogy **FathomDB's `detach`-wrapped engine calls**) gain little *directly*
  from FT. **This is the empirical confirmation of our V2 ≈ small hypothesis:** FT's payoff is in
  parallelizing *Python-level* compute and the GIL-held marshalling slice — exactly what EXP-FT-1/2
  measure — not the engine work FathomDB already parallelizes today.

### A.4 Documented pyo3 footguns, mapped to FathomDB's actual exposure
| Footgun (from pyo3 guide / issue #4738 / LWN) | FathomDB exposure |
|---|---|
| **Deadlock on 3.13t** when a thread re-attaches to the interpreter while another holds it (stop-the-world to immortalize objects on first background thread); fix = `allow_threads`/`detach` around `join()`/`barrier.wait()` | **Low.** FathomDB already wraps every engine call in `detach`; `ingest_with_extractor` spawns a **subprocess**, not a Python thread. EXP-FT-4 must still stress the multi-`Engine` + `drain`/`join` paths. |
| **Runtime borrow-check panic** when two threads mutably borrow the same `#[pyclass]` (pyo3 0.23+ enforces at runtime) | **None.** FathomDB's result pyclasses are `#[pyclass(frozen)]` (immutable); `PyEngine` exposes `&self` + `Arc` clones only. |
| **Mutable `#[pyclass]` shared across threads = the biggest problem source** | **Avoided by design** (frozen + `Arc<RustEngine>`). |
| **GIL-implicit single-init** (`GILOnceCell`) breaks; use `PyOnceLock`/`PyMutex` (detach while blocked, can't deadlock vs GIL) | FathomDB's engine uses **`std::sync::OnceLock`/`Mutex`** held only in Rust with the GIL released — not GIL-coupled. Audit point in EXP-FT-4, but no `GILOnceCell`/`GILProtected` present. |
| **Single-thread penalty (~5–10%) + ~2× per-object memory** if you run on the FT interpreter | Mitigated by shipping **both** abi3 + FT wheels — users opt in; no cost imposed on default GIL users. |

### A.5 What the survey implies for FathomDB
1. **The success pattern is FathomDB's existing pattern.** `tokenizers` (Rust core, `RwLock`, GIL
   released, `Py_MOD_GIL_NOT_USED`) shipped FT support without drama. FathomDB's engine
   (`ReaderWorkerPool` + `Mutex`/`OnceLock`, every call under `detach`, `frozen` pyclasses) is the same
   shape. This raises confidence that **EXP-FT-4 passes** and the lift is dominated by CI + packaging,
   per §5.
2. **The failure pattern is one FathomDB structurally avoids.** polars-style "GIL-assumed bindings with
   concurrent Python-object access" is the danger zone; FathomDB keeps Python objects on the thin
   marshalling layer only.
3. **Benefits caveat is confirmed.** Don't sell FT as "FathomDB gets faster" — the ecosystem data shows
   already-GIL-free native work gains little. Sell it as **V1 (non-poisoning)** + enabling the
   *consumer's* Python-parallel agent loop, which is exactly what EXP-FT-3 quantifies.
4. **The real-world cost center is the dependency/packaging lift** (PyTorch author's experience;
   non-abi3 `*t` wheels) — i.e. EXP-FT-5, not code safety.
5. **"`gil_used=false` is a promise."** pydantic-core's pre-fix segfault is the cautionary tale behind
   making EXP-FT-4 a hard, high-iteration gate rather than a smoke test.

### A.6 Sources
- PyO3 user guide — *Supporting Free-Threaded Python* — https://pyo3.rs/main/free-threading
- PyO3 — *Thread Safety and Free-Threading* (DeepWiki) — https://deepwiki.com/PyO3/pyo3/5.5-thread-safety-and-free-threading
- PyO3 issue #4265 — *Tracking issue for no-gil/freethreaded work* — https://github.com/PyO3/pyo3/issues/4265
- PyO3 discussion #4738 — *3.13 freethreaded deadlock* — https://github.com/PyO3/pyo3/discussions/4738
- Python Free-Threading Guide — *Compatibility Status Tracking* — https://py-free-threading.github.io/tracking/
- Python Free-Threading Guide — *Porting / Updating Extension Modules* — https://py-free-threading.github.io/porting-extensions/
- CPython docs — *Python support for free threading* — https://docs.python.org/3/howto/free-threading-python.html
- danilchenko.dev — *Python 3.14 Free-Threading: Real Benchmarks, Real Breakage* — https://www.danilchenko.dev/posts/python-314-free-threading/
- Trent Nelson — *PyTorch and Python Free-Threading* — https://trent.me/articles/pytorch-and-python-free-threading/
- TechNet — *Polars Compatibility with Python Free-Threading* — https://www.technetexperts.com/polars-python-free-threading-compatibility/
- pydantic-core issue #1555 — *Plan to support free-threaded Python* — https://github.com/pydantic/pydantic-core/issues/1555
- LWN — *Getting extensions to work with free-threaded Python* — https://lwn.net/Articles/1025893/
