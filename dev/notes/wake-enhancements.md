# Wake-oriented enhancements: reframing adaptive text search

Exploratory notes on how the adaptive-text-search tranche interacts with a
larger goal: a fathomdb that stores code (code, tests, scripts, comments,
docstrings) and documentation (needs, requirements, architecture,
traceability, test plans, design) as peers, lets agents search one
modality and walk relationships into the other, pulls external references
(git log especially), and supports temporal "projection from journal"
query responses. Intended consumer: the wake project and the agentic
coding, architecture, test, and technical-writing agents layered on top
of it.

## Part 1 — Tokenizer tension if stored data is code

The default in `design-adaptive-text-search-surface.md` —
`unicode61 + remove_diacritics 2 + porter` — is word-oriented English
prose. For code blobs, several failure modes get much worse than the
doc's "recall is English-biased" caveat suggests.

### Recall collapses on identifiers

- `unicode61` splits on non-alphanumeric, so `user_id` and `foo.bar.baz`
  tokenize fine, but **camelCase stays glued**: `getUserById` is one
  token. Searching `user` or `id` won't find it. The relaxed branch
  can't rescue this — there is no token boundary to relax across.
- Operators and sigils (`->`, `::`, `=>`, `!=`, `&&`, `@decorator`) are
  stripped entirely. "Find call sites of `::foo`" is unrepresentable.
- Case folding collapses `Foo` (type) and `foo` (binding), which is
  routinely *wrong* in code review / navigation queries.

### Porter actively misranks code

Stemming prose suffixes over identifiers produces false positives:
`classes`→`class` conflates a keyword with a plural-noun identifier;
`running`→`run` matches a `run()` function; `Tester`/`tests`/`testing`
all collapse. For prose this is desired; for code it puts noise at the
top of BM25.

### Snippet and attribution consequences

- Attribution (position map §) stays **correct** — it's offset-based
  and tokenizer-faithful. So this is a **recall/ranking** problem, not
  a correctness one.
- Snippets, however, hard-wrap on `unicode61` boundaries and will look
  ugly around operators and punctuation-dense lines.

### The escape hatch is already named but deferred

`design-adaptive-text-search-surface.md` line 437 reserves "per-property
-schema tokenizer override — e.g. trigram for identifier-shaped fields"
as an explicit non-goal for this tranche. Two things worth flagging
against that framing:

1. The doc treats the override as a *per-property* knob (URLs,
   usernames, file paths). Code blobs are not a property-shaped field —
   they live in **chunk FTS**. The current escape hatch wouldn't cover
   them; the correct seam is **per-kind** (or per-ingest-pipeline), not
   per-property.
2. Lines 428–429: "changing the tokenizer later is a full FTS rebuild,
   not a migration." Anyone indexing code under v1 is locked into the
   prose tokenizer until a rebuild path ships. That's worth at least a
   one-line caveat in the Tokenization § ("code-shaped content is out of
   scope for the default tokenizer; indexing it today will require a
   rebuild when code-aware tokenization lands") so users don't discover
   it after ingesting a corpus.

The cheapest v1 move — if scope is constrained — is to leave the default
tokenizer alone and document code as explicitly out-of-scope, rather
than try to make one tokenizer serve both. Trigram-for-code as a second
index is a real feature with real cost (3–5× bloat per the doc) and
deserves its own design pass — it isn't a drop-in for the prose path.

## Part 2 — Reframe for the wake use case

The tokenizer analysis above assumes the tranche ships as scoped: a
retrieval surface on top of SQLite FTS5. The wake use case reframes
that. If fathomdb is meant to store code and documentation as peers and
let agents walk between them, **this isn't a text-search feature —
it's a typed, temporal knowledge graph** where FTS is one edge type
among several (graph edges, git refs, journal events, later vectors).

v1 of search shouldn't try to *become* the knowledge graph — but it
should avoid closing doors on it.

## Part 3 — Three consequences for the current design

**1. One tokenizer can't serve both modalities.** This is the tension
from Part 1, now sharpened by the wake framing. `porter` helps prose
and misranks code identifiers; identifier-splitting helps code and
destroys prose snippets. The per-property override at line 437 is the
wrong seam — code lives in chunk FTS, not properties. The correct seam
is **per-kind** (per ingest pipeline): docs get `unicode61+porter`,
code gets identifier-aware tokenization, the FTS schema holds both.
Cheap to decide before v1; a full rebuild to retrofit.

**2. `SearchHit` must resolve to a typed graph node.** If the workflow
is "land on a hit, then walk edges," the hit is a graph *entry point*,
not a final answer. It needs a typed kind (`Code`, `Doc`, `Test`,
`Requirement`, `Commit`) and a stable handle for "what's connected to
this?" The current shape was designed for ranking, not traversal —
worth a look before the wire format freezes across Rust/Python/TS.

**3. Journal-as-truth is an architectural stance, not a feature.**
"Temporal projection from journal" means the canonical state is an
append-only log of facts (file ingested, edge asserted, commit
observed), and FTS/graph/vector indexes are *derived views* —
rebuildable, replayable, queryable as-of. That's CQRS on top of the
engine, and a different write path from today's direct-to-engine
model. Git history falls out naturally (it's already a journal). The
big question: does wake's event store **become** the journal, with
fathomdb projecting from it — or does fathomdb grow its own journal?
The former avoids two sources of truth; the latter keeps fathomdb
self-contained.

## Part 4 — Recommendation

Don't widen v1 to build the knowledge system. Do narrow two v1
decisions so they don't box the larger goal out:

- make tokenizer a **per-kind** choice in the schema, even if only one
  kind is wired up at first
- confirm `SearchHit` carries enough typed identity to serve as a graph
  entry point later

**Tradeoff:** small scope bump to a mid-flight tranche, in exchange for
not rebuilding indexes or breaking wire formats when the graph/journal
layer lands. If the knowledge system is hypothetical, skip both; if
it's the destination, retrofit cost dominates upfront cost.

## Part 5 — Questions to answer before anything bigger

1. Is wake's event store the journal, or does fathomdb grow its own?
2. Is the graph a first-class store, or a projection over relational
   tables?
3. Are git refs ingested as reprojectable events, or resolved live
   against the working repo?

These three decisions determine whether the knowledge system is one
layer above fathomdb or a new shape *of* fathomdb — and they're worth
pinning down before the text-search wire format sets.
