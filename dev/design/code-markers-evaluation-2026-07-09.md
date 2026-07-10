# Code Markers — Evaluation & Lifecycle (2026-07-09)

**Status:** experiment report (out-of-band). **Ships nothing.** Determines, with
evidence from this repo's own history, whether in-source *markers* — structured
comments that cross-reference a governance artifact (`// ADR-0.8.14 §D4`,
`// AC-074`, `// TC-8`, `// dev/design/foo.md`) — are a net win at code-review
time, where they degrade, and what managed lifecycle they would require.

Scripts + data: `dev/experiments/code-markers-eval/` (0 LLM at detection; `git`
read-only). Every number below is reproducible from `out/*.json`.

> **The one-line question (HITL):** when a reviewer (human or agent) sees a
> marker, can they cross-reference it to the governing artifact with **non-token
> tools** (grep, a linker, CI) instead of reading everything into an LLM — and
> can we keep markers from rotting into false confidence?

---

## 1. The object — taxonomy

A marker is a `(reference, code-region)` pair. Three axes:

- **Referent** — what it points at: `ADR` (design decision), `AC`/`REQ`
  (acceptance criterion / requirement), `TC` (todo/consideration ledger entry),
  `DESIGN` (design doc / section), `F` (feature id), `ledger-seq`, `session`.
- **Cardinality** — **point** (a single id: `AC-074`) vs **region** (a path or
  `§section` naming a span of prose the code is meant to satisfy).
- **Direction** — **code→doc** (comment names an artifact), **doc→code** (a
  design doc names a file/symbol), **bidirectional** (both, kept in sync).

**Operational states** (what a non-token validator can decide):

| State           | Definition (decidable by grep/git, no LLM)                                   |
|-----------------|------------------------------------------------------------------------------|
| **aligned**     | referent resolves to a live artifact **and** carries a non-terminal status   |
| **dangling**    | referent does **not** resolve (file/id/record absent)                        |
| **terminal**    | referent resolves but its lifecycle status is closed/resolved/superseded     |
| **drifted**     | adjacent code changed *after* the marker's last touch (not re-validated)     |
| **misrepresenting** | *aligned-looking but false* — resolves + non-terminal, yet the code no longer does what the referent says. **Not** decidable without semantic reading. |

The last row is the danger zone (H5): the cases a validator **cannot** catch are
exactly the ones that manufacture false confidence. Drift is its measurable proxy.

---

## 2. Hypotheses, metrics, thresholds (set before measuring)

| # | Hypothesis | Metric | Win/loss threshold |
|---|------------|--------|--------------------|
| **H1** | Markers cut cross-reference cost/error at review | reviewer time/error to locate the governing artifact, marker vs none | win if marker path is 0-LLM grep vs an LLM-read search |
| **H2** | Markers drift when nearby code changes without the marker updating | `git blame`: adjacent code newer than marker, ±3 lines | material if drift ≥ 10% over the tree's age horizon |
| **H3** | Markers dangle when the referent closes / supersedes / renames / subdivides | resolution rate against the artifact registry | material if dangling ≥ 5% in any class |
| **H4** | Lifecycle-managed markers (expiry + CI validator) beat forever-markers | dangling/terminal caught pre-merge vs left in tree | win if a validator resolves ≥ 95% deterministically |
| **H5** | A stale marker is worse than none (false confidence) | share of resolvable-but-drifted markers (the un-catchable set) | loss surface = drifted ∩ non-dangling |

---

## 3. Measured results

### Reference classes and N

`src/**` (Rust/TS/Py/JS), scanned at HEAD: **736 marker occurrences** across the
shipped source, resolving against real registries (62 ADR files, 123 AC ids, 67
REQ ids, 9 TC records, 23 F ids). Distinct tokens: AC 74, DESIGN-PATH 49, ADR-ID
33, ADR-PATH 10, REQ 6, F 1, TC 1. Plus non-code classes: **189** memory
wikilink occurrences, **44** commit TC refs, **15** commit ledger-seq refs, **217**
`Claude-Session` trailers.

### H3 — HEAD-state dangling: **0.4%** (3 / 736)

The in-code marker discipline in this repo is **near-perfectly aligned at HEAD.**
The three genuine dangles are *instructive*, each a distinct rot mode:

- **`AC-028` ×2** (`fathomdb-engine/src/lib.rs`): the acceptance criterion was
  later **subdivided into `AC-028a/b/c`**; the code's bare `AC-028` no longer
  resolves exactly. → **granularity/refinement drift** (the artifact was refined,
  the marker wasn't). Compound case: this same line is also *drifted* (§H2, 29.6d).
- **`dev/design/projection-registry-and-async-embed.md` ×1**: a **forward
  reference** to a design doc that doesn't exist yet (OPP-12, ≥0.9.x). →
  **aspirational dangle** — deliberate, benign, but indistinguishable from rot to
  a validator.

Per class, HEAD dangling: ADR-PATH 0/31, ADR-ID 0/64, REQ 0/16, F 0/9, TC 0/1;
AC 2/416 (0.5%), DESIGN-PATH 1/199 (0.5%). **Below the 5% H3 threshold in every
in-code class.** H3 is **not** supported for in-code markers *at a point in time*
— because the artifact side here has stable ids and stable paths.

### H3 (history) — commit-message TC refs dangle at **9.1%** (4 / 44)

Where the artifact side lacks a stable **addressable id**, dangling appears. All
4 dangling commit refs are **`TC-10` / `TC-11`**, which exist only as **free-text
mentions inside ledger record bodies**, never minted as first-class `id` records
(the ledger has `id` fields only for `TC-1..TC-9`). → **referenced-before-
registered / identity-granularity** dangle. This is the single most important
precondition finding: *markers are only as resolvable as the artifact side is
addressable.*

### H2 — drift: **15.1%** (111 / 733 usable), a **lower bound**

Adjacent code (±3 lines) was last edited *after* the marker line in 15% of
markers — the code moved on without the marker being re-touched. Lower bound:
`git blame` attributes a line to its last edit, so any format/whitespace sweep
touching a marker line resets its clock and **masks** drift.

- **Source-comment markers drift 2.2× more than test-binding markers: 22.5%
  (66/293) vs 10.2% (45/440).** Test markers pair an `AC-id` with a *named test*
  that repeats the id — renaming the test touches both, so they co-evolve. Source
  comments *describe* adjacent logic; the logic changes, the prose stays.
- By class: **DESIGN-PATH 19.1%** (region markers into evolving design prose) >
  REQ 18.8% > AC 14.3% > ADR-ID 12.7% > **ADR-PATH 9.7%** (a bare path has little
  to drift against). F 0%, TC 0% (N too small).
- **Horizon caveat:** median marker age **46d**, p90 **68d**, max observed drift
  **56d**. The 0.8.x source is young; 15% in ~2 months is a *rate*, and drift
  compounds monotonically in a long-lived tree.

### H5 loss surface — resolvable-but-drifted markers

The markers a validator **cannot** flag (resolve fine, non-terminal, yet
adjacent code drifted) are **~15% of the tree** — H2's drifted∩non-dangling set.
These are the H5 candidates: they *look* authoritative to a reviewer and to CI,
while carrying the highest chance of silently misrepresenting. We did **not**
semantically confirm how many are actually false (that needs reading); the honest
claim is *this is the size of the un-auto-checkable risk surface, and it is an
order of magnitude larger than the dangling surface CI can catch (0.4%).*

### Natural experiments — other classes

- **Memory `[[wikilink]]` graph: 0.0% dangling** (0/189 occurrences, 61 distinct
  targets all resolve) — **despite** CLAUDE.md *explicitly tolerating* dangling
  links by design. The designed-in tolerance is **unused slack**: cheap insurance,
  not a rot source. Curation, not the tolerance, is doing the work.
- **Commit `ledger seq` refs: 0% dangling** (15/15 resolve ≤ max seq).
- **`Claude-Session:` trailers: 217, unresolvable offline** — a marker class whose
  target lives **outside the verifiable universe**. Can't dangle-check without the
  hosting service; effectively a forever-opaque pointer.
- **`@sha` pins: the one in-repo match is a false positive** — `@5c38ec7c` in
  `dev/design/embedder.md` is a **HuggingFace model revision hash**
  (`BAAI/bge-small-en-v1.5@5c38ec7c…`), *not* a git commit. A naive sha-pin
  validator flags it as a dead object (100% "dangling" of N=1) — a **validator-
  design lesson**: `@hex` is ambiguous between git-sha and content-hash; a
  validator must be told which namespace it's in. (The frequently-cited
  `@c3b3d631` in the auto-memory prose *is* a real, reachable commit.)

---

## 4. Cost / benefit and the break-even

**Benefit** of a marker = review cross-reference done by **grep/linker (0 token,
~0 error)** instead of an LLM reading the artifact corpus to *infer* the link.
Realized **only when** the marker resolves to the right region — i.e. when it's
aligned, not drifted/misrepresenting.

**Cost** = maintenance (update on code *or* target change) + **decay cost** (a
stale marker's negative value: it doesn't just fail to help, it *misdirects* —
H5). Decay cost is asymmetric: a dangling marker is loud and cheap to catch
(0.4% here, CI-catchable); a **drifted-but-resolvable** marker is silent and
expensive (15% here, *not* CI-catchable).

**Break-even.** A marker is net-positive when **all** hold:

1. **Stable addressable id on the artifact side** (the TC-10/11 finding: no id →
   dangle). ADR files, AC/REQ ids, and TC records qualify; prose mentions and
   external URLs do not.
2. **Queryable lifecycle status** so a validator can flag *terminal* referents
   (the TC ledger's `status` field is the only class here that exposes this).
3. **Code region that co-evolves with, or is stably named by, the marker.** Test
   markers (id ⇄ named test) meet this at 10% drift; free-form source comments do
   not (22.5%).
4. **A validator enforcing non-dangling / non-terminal at merge.** Without it,
   dangling accrues; with it, the 0.4% dangling surface is closed but the 15%
   *drift* surface remains (validators can't read).

It **inverts** (net-negative) when: the referent has no id (commit TC-10/11); the
region is long-lived free-form prose that changes often (DESIGN-PATH source
comments, 19%); or there is no validator *and* no curation, so dangling
accumulates unseen. **Region markers into evolving design prose are the worst
cell; test-binding point markers to a status-bearing id are the best.**

---

## 5. Win-map / Loss-map

**WIN — adopt markers where:**

- **Test ⇄ acceptance-id binding** (`AC-074` on a `test_ac_074_*`). Lowest drift
  (10%), 0% dangling, the id ⇄ test-name coupling self-heals on rename, and the
  benefit (a reviewer greps `AC-074` to find both the criterion and its test) is
  exactly the non-token cross-reference the HITL wants. **This repo already does
  this and it works.**
- **Point markers to a status-bearing record** (`TC-8` → ledger with `status`).
  A validator can resolve *and* check the lifecycle state. Requires the record to
  exist first (mint-before-reference).
- **ADR *path* refs in module headers** (`//! ADR authority: dev/adr/X.md`).
  Lowest drift among prose markers (9.7%), 0% dangling; a coarse "this module is
  governed by that decision" pointer that rarely needs updating.

**LOSS — avoid markers where:**

- **`§section` region markers into evolving design prose** embedded in
  free-form source comments (DESIGN-PATH, 19% drift). The code drifts from the
  prose silently; the marker keeps asserting a stale contract → **H5 false
  confidence**. Prefer a coarse doc-level pointer over a `§`-precise one.
- **Refs to un-minted / prose-only referents** (commit `TC-10/11`, 9% dangle).
  Referencing before the artifact has a stable id guarantees a future dangle.
- **Opaque external pointers** (`Claude-Session:` URLs). Unverifiable offline;
  can rot without any local signal.
- **Ambiguous-namespace pins** (`@hex` that might be git or a model hash). Either
  disambiguate the namespace or don't pin.

**H5 verdict:** *supported in principle, unquantified in fact.* The un-auto-
checkable drift surface (15%) is ~35× the dangling surface (0.4%), so the danger
is real and structural — but we did not read the drifted markers to count how
many are *actually* false. The safe design rule stands regardless: **a marker CI
can't validate against its target's current state is a liability, not an asset.**

---

## 6. Managed lifecycle (sketch — do NOT build)

The HITL's hard requirement: **no forever-markers.** Lifecycle:

1. **Birth.** A marker is minted only *after* its referent exists as an
   **addressable id** (rejects TC-10/11-style forward refs). Syntax carries the
   namespace so it's unambiguous: `gov:ADR-0.8.14§D4`, `gov:AC-074`,
   `gov:TC-8` — never a bare `@hex`.
2. **Validation (non-token linter / CI).** For each marker: (a) **resolve** the
   id against the registry (file exists / id in `acceptance.md` / record in
   ledger); (b) read the referent's **lifecycle status**; (c) fail on *dangling*,
   warn on *terminal* (points at a resolved/superseded artifact → should be
   retired or re-pointed). This is pure grep/JSON — the miners here are a working
   prototype of exactly this check (`mine_incode_markers.py` = the resolver).
3. **Drift signal (advisory, the un-catchable middle).** CI cannot know if a
   drifted marker is *wrong*, but it can **surface** the 15%: "code within ±N
   lines of `gov:AC-074` changed in this PR but the marker didn't — reviewer,
   re-confirm it still holds." Cheap `git blame` diff; turns silent drift into a
   review prompt. This is the highest-leverage addition and the only thing that
   touches the H5 surface.
4. **Expiry / retirement.** When the referent goes terminal, the validator emits
   a retirement task; markers to superseded artifacts are re-pointed or deleted.
   Optional TTL for aspirational forward-refs (`gov:DESIGN-...?forward` expires if
   the doc isn't written by release N).
5. **GC.** Orphaned markers (referent deleted, not superseded) are deleted, not
   left dangling.

**Enforceable vs advisory:** birth-gate + dangling + terminal are **enforceable**
(deterministic). Drift and misrepresentation are **advisory only** (drift is a
blame heuristic; misrepresentation needs reading). Never present an advisory
signal as a guarantee — that would re-create the false confidence the whole
exercise is trying to kill.

### Artifact-side preconditions (what markers *depend on*)

A marker program is only as good as the registries behind it. Required of the
artifact side:

- **Stable addressable ids** — ADRs (path/id ✅), AC/REQ (`acceptance.md` ✅), TC
  (ledger `id`, but **only when minted** — TC-10/11 gap ✗).
- **Queryable lifecycle status** — **only the TC ledger exposes this** (`status`).
  ADRs/AC/REQ have *no machine-readable superseded/closed flag*, so a validator
  can check *existence* but not *currency* for the two largest in-code classes.
  **This is the binding precondition gap:** without a status field on ADR/AC,
  markers to them can dangle-check but never terminal-check.
- **Rename/subdivision discipline** — the `AC-028 → AC-028a/b/c` case shows
  refinement silently breaks point markers; the registry needs an alias/redirect
  table or the validator needs prefix-resolution with a warning.

---

## 7. Verdict

**Adopt-where:** (1) test ⇄ acceptance-id bindings — already in use, measurably
the healthiest class (10% drift, 0% dangling, self-healing); (2) point markers to
**status-bearing** ledger records (`TC-n`), *after* minting; (3) coarse ADR
**path** pointers in module headers.

**Avoid-where:** `§`-precise region markers into evolving design prose in
free-form source comments (highest drift, prime H5 surface); refs to un-minted or
prose-only referents; opaque external URLs; ambiguous `@hex` pins.

**Need-X-first:** the two largest in-code classes (ADR, AC/REQ) **lack a
machine-readable lifecycle status**, so markers to them can only be
existence-checked, not currency-checked. **Precondition for a real marker
program: add a queryable `status`/`superseded_by` field to the ADR and
acceptance registries** (the TC ledger already has it). Also: a **birth-gate**
(no marker before its referent is a minted id) and a **drift-surfacing** advisory
check.

**Not-worth-it as forever-markers:** an un-validated, un-expiring marker is a
net liability — the measured un-catchable drift surface (15%) dwarfs the
CI-catchable dangling surface (0.4%), so "just add comments with ids" without the
lifecycle machinery mostly manufactures future false confidence.

**Overall:** markers are a **real but conditional win** — worth it *exactly* in
the cells where a stable, status-bearing id meets a co-evolving code region under
a CI validator, and a net loss elsewhere. This repo's *existing* discipline
(test-id bindings, ADR path headers, a curated wikilink graph at 0% dangling)
already sits in the win cells; the failure modes appear precisely where that
discipline lapses (bare/subdivided ids, prose-only TC refs, `§`-precise source
comments).

---

## 8. Residual open questions / biggest evidence gap

- **H1 is not directly measured.** Whether a *missing or stale* marker actually
  cost a reviewer time/error would require mining the ~1 GB transcript corpus for
  review episodes that turned on a cross-reference — deliberately not read here,
  and likely under-powered because this program already markers heavily (the
  counterfactual rarely occurs). **Smallest real trial that would settle it:** a
  controlled A/B on N≈20 review tasks — same diffs, markers present vs stripped —
  measuring time-to-locate-governing-artifact and mis-attribution rate, with the
  §6 validator wired in for the marker arm. That is the one experiment that would
  move H1/H4/H5 from *argued* to *measured*.
- **H5 falseness is a risk-surface size (15%), not a confirmed misinformation
  rate.** Reading a sample of the drifted markers to count actually-false ones
  would quantify decay cost directly.
- **Horizon.** Drift is measured on a ~2-month-old tree; re-running these miners
  at 0.9.x / 1.0 would show the compounding curve and validate the "young tree
  under-counts" caveat.
- **ADR/AC lifecycle status is the missing precondition** — until the two largest
  registries expose currency, any marker program over them is existence-only.
