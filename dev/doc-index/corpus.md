# DOC-INDEX detail — `(corpus/eval, out-of-band)`

> Long-form per-doc notes for this area of the doc tree. The thin map at
> `dev/DOC-INDEX.md` links here; **every path listed below also has its own row in
> `dev/DOC-INDEX.md`** (path + a ≤120-char purpose clause) — this file carries the
> full slice-history / decision-record prose that DOC-INDEX.md compresses away.
> Update this file (not DOC-INDEX.md) when you want to add narrative detail; update
> BOTH files when you add/rename/materially change a doc (DOC-INDEX.md row +
> this file's row, same closing commit — mirrors DOC-INDEX.md's own rule).

## Corpus / eval expansion (out-of-band, owner-managed — integrated at Slice-5 push 2026-06-02)

> These come from the parallel **corpus-work** line (origin/main `83f5156`), integrated into
> `main` when the 0.8.0 campaign was pushed. They are **owner-managed**, not driven by a campaign
> slice; the owner curates/expands these rows. Listed here so DOC-INDEX maps the full shipped doc
> surface (Slice-40 gate m).

| Doc | Purpose | Owning slice/AC | Last-touched |
|-----|---------|-----------------|--------------|
| `dev/corpus-creation/README.md` + `architecture.md` | Corpus-creation overview + architecture | corpus-work (out-of-band) | 2026-06-02 |
| `dev/notes/0.8.x-corpus-source-expansion-research.md` | Corpus source-expansion research notes | corpus-work (0.8.x) | 2026-06-02 |
| `dev/notes/0.8.x-pmc-oa-reconsideration.md` | PMC-OA source reconsideration note | corpus-work (0.8.x) | 2026-06-02 |
| `dev/plans/prompts/0.8.x-corpus-qa-expansion-handoff.md` | Corpus QA-expansion handoff prompt | corpus-work (0.8.x) | 2026-06-02 |
| `dev/plans/prompts/0.8.x-corpus-source-expansion-search.md` | Corpus source-expansion search prompt | corpus-work (0.8.x) | 2026-06-02 |
| `tests/corpus/corpus-card.md` + `README.md` | Eval corpus card + acquisition README (scripts under `tests/corpus/scripts/`) | corpus-work (eval) | 2026-06-02 |
| `tests/corpus/scripts/acquire_musique.py` | **MuSiQue-Ans corpus acquire script** — deterministic acquisition of `bdsaglam/musique` (re-hosts StonyBrookNLP/musique v1.0, CC-BY-4.0); downloads `default/validation` split (4 834 questions: answerable + unanswerable contrast set); materializes per-question paragraph corpus → `data/corpus-data/raw/musique_dev.jsonl`; pins `musique_hash` in `manifest.json`; shared prerequisite for Slices 5 ∥ 10 | 0.8.2 Slice 4 | 2026-06-17 |
| `tests/corpus/scripts/test_acquire_musique.py` | **TDD tests for MuSiQue materializer** — 6 tests: hop_count ∈ {2,3,4}; hop_count == supporting-para count (answerable); ≥1 supporting + ≥2 distractor paragraphs; unanswerable set non-empty + flagged; paragraph schema; byte-stability via manifest sha256 pin | 0.8.2 Slice 4 | 2026-06-17 |
