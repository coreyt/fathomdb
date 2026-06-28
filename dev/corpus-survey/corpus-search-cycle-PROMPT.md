# PROMPT — run another FathomDB corpus-search cycle

Copy this whole file as the task for a fresh agent. It is self-contained: it points
at the on-disk artifacts, tells you how to re-orient fast, what is already covered
(don't redo it), and how to EXTEND the map + APPEND the ledger. The goal is to
DEEPEN the survey each cycle, not rebuild it.

---

## Mission

Extend FathomDB's data-corpus survey: connect every user-need and feature/function
to the well-respected dataset(s) that can TEST it, grounded in what is already on
disk, and leave the machinery sharper than you found it. Research + file-editing.
Web access available (WebSearch/WebFetch). Work against **current `origin/main`**.

## Setup (do this first)

```bash
cd /home/coreyt/projects/fathomdb && git fetch origin
git worktree add /home/coreyt/projects/fathomdb-corpus-wt origin/main
cd /home/coreyt/projects/fathomdb-corpus-wt
```

The plain checkout has been stale before — only trust `origin/main`. Corpus
payloads under `data/corpus-data/` are **`.gitignored`** and live physically in the
primary checkout (`/home/coreyt/projects/fathomdb/data/corpus-data/`), so a fresh
worktree will NOT contain them.

## Re-orient FAST (read these, in order)

1. **`dev/corpus-survey/corpus-map.md`** — the current MAP. This is what exists; do
   not re-derive it.
2. **`dev/corpus-survey/corpus-search-ledger.md`** — every prior cycle: what was
   searched, found, the **confirmed gaps**, and the **open questions**. Your job is
   to chip at the gaps/questions, not repeat covered searches.
3. **Run the enumeration script** to see current on-disk state (and any drift since
   the last cycle):

   ```bash
   FATHOMDB_DATA=/home/coreyt/projects/fathomdb/data/corpus-data \
     bash dev/corpus-survey/enumerate-corpora.sh
   ```

   It reports committed scaffolding, on-disk payloads (sizes/line-counts/licenses),
   gitignore posture, and a MAP-EXPECTS-vs-ON-DISK reconciliation.
4. **Ground in the capability frame** (only the parts relevant to your target need):
   - `dev/plans/runs/0.8.x-capability-status-report.md` — the 7-capability frame.
   - `dev/design/0.8.x-parity-portfolio-strategy.md` — the measure-routed portfolio
     (M1–M9) and which mechanism each measure wants.
   - `dev/experiments-ledger.md` — distilled results of record (per-experiment).
   - `tests/corpus/corpus-card.md` + `tests/corpus/scripts/manifest.json` — the
     0.7.0 test-corpus license roll-up + reproducible acquire scripts.
   - Eval loaders that define the corpus contracts:
     `src/python/eval/{locomo_loader.py,gold_repin.py,d0b_powered_recall.py,
     verify_embed_db.py,m1_*.py}`.

## What is ALREADY covered (do NOT redo)

On disk: LOCOMO, LongMemEval (HF-cached), MuSiQue, AP-News BenchmarkQED, the ~10k
0.7.0 test corpus (Enron/EnronQA/QMSum/QAConv/QASPER/CNN-DailyMail/Landes/bahmutov/
synthetic/chains), FathomDB IR-gold (eu8) + eu7 fidelity, memex-ELPS golden,
GraphRAG + Mem0 comparator artifacts. Candidates already mapped: BEIR, MS MARCO +
TREC-DL, Natural Questions/NQ-Open, TriviaQA, SQuAD 1.1/2.0, HotpotQA,
2WikiMultihopQA, MultiHop-RAG, MSC, MQuAKE, GraphRAG podcast/news.

## Where to push THIS cycle (pick from the ledger's gaps/questions)

The Cycle-1 confirmed gaps + open questions are the live worklist. High-value
directions:

- **Close a confirmed gap** — e.g. resolve whether BEIR ArguAna/Touché is a real
  "exploratory / discovery-in-k" proxy (the #1 gap), or stand up a few commit-clean
  BEIR subsets so FathomDB can report a standard external nDCG@10, or run CE-rerank
  on MS MARCO/TREC-DL (its native benchmark).
- **Resolve an open question** — license posture of LongMemEval / MultiHop-RAG / MS
  MARCO; eu7/eu8 rebuildability; MSC source terms.
- **Add a new need/function** if the capability frame has grown since the last cycle
  (check for new `dev/design/0.8.*` or roadmap docs).
- **Verify, don't assume.** For every dataset you add or touch: crisp description,
  a representative example, license + redistributability, size, and an acquisition
  source (URL / HF id). Distinguish ALREADY-ACQUIRED vs CANDIDATE-NEW with the
  status tags in the map.

## How to EXTEND (the deliverable)

1. **`corpus-map.md`** — add/refine rows. Keep the 6 columns
   (`User Need | Feature/Function | Corpus Name | Corpus Description | Local Storage
   Point | Acquisition Instructions (+license/redistributability)`). On-disk corpora
   are first-class rows with their `Local Storage Point` path filled in; standalone
   functions get rows with `User Need = "—"`. Update the Quick-stats block.
2. **`corpus-search-ledger.md`** — APPEND a new `## Cycle N — YYYY-MM-DD` section
   (use the template at the bottom). Record scope, on-disk delta, queries+sources,
   what was found, gaps closed / still open, questions resolved / new. **Never edit
   prior cycles.**
3. **`enumerate-corpora.sh`** — if you acquire a new corpus or the map's expected set
   changes, update the `check ...` list in §4 so the reconciliation stays honest.
   Keep it READ-ONLY and idempotent (no downloads, no mutations).
4. If you actually acquire a payload, do it via a reproducible
   `tests/corpus/scripts/acquire_*.py` script (with an inline license header), write
   it under the gitignored `data/corpus-data/`, and record the manifest/license. Add
   it to the map as `[ON-DISK]`.

## Hard constraints

- **Licensing:** never instruct committing non-redistributable payloads. LOCOMO
  (CC-BY-NC, EVAL-ONLY) and AP-News (MS-Research, NON-REDISTRIBUTABLE, EVAL-ONLY)
  stay gitignored forever. Mark eval-only / no-redistribute clearly. "commit-eligible"
  = the license *would* allow it, but the project default is still data-out-of-git.
- **Clean markdown** — compliant tables/fences/emphasis, no emojis.
- **Don't gold-plate.** A correct, well-grounded extension beats breadth. It's fine
  to close one gap well rather than skim ten.

## Land it

```bash
git checkout -b corpus-survey-cycleN
git add dev/corpus-survey/
git commit -m "docs(corpus): corpus-survey cycle N — <one-line summary>"
git push -u origin corpus-survey-cycleN
gh pr create --base main --title "docs(corpus): corpus-survey cycle N" --body "..."
git worktree remove /home/coreyt/projects/fathomdb-corpus-wt   # when done
```

If a step is blocked by policy, STOP and report it rather than working around it.

## Return

The PR URL + the changed file paths + a tight summary: needs/functions touched,
corpora added (on-disk vs candidate), gaps closed, and the top remaining gaps.
