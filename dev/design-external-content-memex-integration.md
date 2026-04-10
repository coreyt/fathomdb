# Design: External Content Objects -- Memex Integration

This document explores how Option C from `design-external-content-objects.md`
works in practice inside Memex: how content gets ingested, how users find it,
how the agent decides to use it, and what the agent needs to know to actually
*do something* with the content once found.

## What Memex Is (for context)

Memex is a personal AI agent that maintains a persistent world model about its
human. It stores goals, entities, meetings, plans, knowledge items, and
conversation history in FathomDB. The agent uses FTS + vector + graph queries
to retrieve relevant context when answering questions or taking action. Every
piece of data has provenance, supports supersession, and can be surgically
removed.

External content objects extend this world model. They are not a separate
subsystem -- they are nodes in the same graph, discoverable through the same
queries, linked to the same goals and entities. The difference is that the
*bulk content* lives outside FathomDB while the *meaning, metadata, and
searchable text* live inside it.

## The Agent's Problem

When a user asks "what was that patent about ceramic heat shields?" the agent
needs to:

1. **Find it** -- text search or vector search matches against extracted chunks.
2. **Understand what it is** -- a saved Google Patent search result, not a local
   file or a meeting transcript.
3. **Know how to present it** -- provide the URL so the user can open it in a
   browser, not try to inline a 40-page patent document.
4. **Know how to use it** -- if the user says "summarize that patent," the agent
   needs to know it can fetch the content via the URI and process it.

The existing Option C design handles #1 (chunks + FTS/vector) and partially #2
(the `kind` field). But it's missing the agent-facing guidance for #3 and #4:
*what kind of thing is this, and what can I do with it?*

## What the Agent Needs to Know

An agent operating on an external content node needs a small number of signals
beyond what bare metadata provides. These aren't complex -- they're hints that
prevent the agent from doing something stupid like trying to "read aloud" a
dataset or "display" an MP3.

### The `access` field

A structured hint in node properties that tells the agent how a human (or the
agent itself) can interact with this content:

```json
{
  "access": {
    "method": "browse",
    "uri": "https://patents.google.com/patent/US20210123456A1",
    "label": "Open in Google Patents"
  }
}
```

The `method` field is a short vocabulary that tells the agent what action to
take. The names are verbs -- they describe what the agent *does*, not where
the content lives or how it's transported:

| method | What the agent does | Example |
|---|---|---|
| `read` | Directly consume the content to answer questions, summarize, or extract information. The agent can process this. | A 3 MB PDF on disk. A local CSV. A podcast episode the user wants transcribed. |
| `play` | Present to the human for sensory experience. The agent can describe metadata but should not try to inline the content. | A Coltrane album. A movie on Plex. A YouTube video. |
| `browse` | Give the user a clickable link to open in their browser. The agent can fetch the page for summarization if asked. | A Google Patent result. A Wikipedia article. A saved web page. |
| `open` | Hand off to a named application. The agent cannot directly process the content but knows which app to suggest. | A Photoshop file. A CAD drawing. A Keynote presentation. |
| `query` | Make programmatic requests to retrieve specific data on demand. The agent can call the API to answer questions. | A Census Bureau REST API. A Plex server API. An internal database endpoint. |
| `lookup` | Resolve an identifier to find more information. The content isn't directly accessible -- the identifier points to a registry or catalog. | A DOI. An ISBN. An arXiv ID. A GenBank accession number. |

This isn't a type system -- it's a pragmatic signal about intent. The same
MP3 file might be `"method": "read"` if the agent should transcribe a podcast
episode, or `"method": "play"` if it's a song in the user's music library.
The adapter (or the user, or the ingesting agent) chooses based on what the
human wants the agent to do with this content.

### The `content_type` field

Already part of Option C as `mime_type` in properties. But the agent benefits
from a coarser **category** that maps to behavior, not just a MIME string:

```json
{
  "content_type": {
    "mime": "audio/mpeg",
    "category": "audio"
  }
}
```

| category | Examples | Agent can... |
|---|---|---|
| `document` | PDF, DOCX, TXT, HTML | Read extracted text. Summarize. Quote. Answer questions from content. |
| `data` | CSV, JSON, Parquet, database dumps | Query or sample. Describe schema/shape. Answer quantitative questions. |
| `audio` | MP3, WAV, podcast episodes | Describe metadata (title, duration, artist). Transcribe if needed. |
| `video` | MP4, MKV, streams | Describe metadata. Transcribe audio track. Cannot process visuals inline. |
| `image` | PNG, JPG, SVG, DICOM | Describe if vision-capable. Present as viewable. |
| `web` | Saved web page, cached search results | Read extracted text. Provide original URL. Note it may be stale. |
| `code` | Source files, notebooks, repos | Read and analyze. Run if sandboxed. |
| `other` | Anything else | Fall back to metadata-only. Present access method to user. |

The category helps the agent select a strategy without parsing MIME types. When
the category is `audio`, the agent knows not to try to "read" the content but
can offer to transcribe it or describe its metadata.

## Concrete Scenarios

### Scenario 1: MP3 Music Library

A user has a local music collection. They don't want Memex to index every lyric
-- they want it to *know what music they have* so it can answer "do I have any
Coltrane albums?" or "what was that jazz album I was listening to last Tuesday?"

**Ingestion** (batch, via a `LocalFileAdapter` scanning a directory):

```python
manager.ingest(
    uri="file:///home/user/music/coltrane/a-love-supreme.mp3",
    kind="content.audio",
    properties={
        "title": "A Love Supreme",
        "artist": "John Coltrane",
        "album": "A Love Supreme",
        "year": 1965,
        "duration_seconds": 1980,
        "content_type": {"mime": "audio/mpeg", "category": "audio"},
        "access": {"method": "play", "label": "Play in media player"}
    },
    source_ref="adapter/music-library-scan-001"
)
```

**Chunks**: For music, chunks hold metadata-as-text rather than extracted audio.
The adapter writes one chunk per track:

> John Coltrane - A Love Supreme (1965). Jazz. Album: A Love Supreme.
> Duration: 33 minutes.

This makes the track findable via text search ("Coltrane") and vector search
("jazz saxophone album from the 60s") without processing audio.

**Agent interaction**:
- User: "Do I have any Coltrane albums?"
- Agent queries: `text_search("Coltrane")` or `vector_search("John Coltrane jazz")`
- Finds content nodes with `category: audio`, `method: play`
- Responds: "Yes, you have *A Love Supreme* (1965). Would you like me to play it?"
- Agent knows not to try to "read" the MP3 content.

### Scenario 2: Plex Server on the Network

A user has a Plex media server with hundreds of movies and shows. The content
is huge and remote -- Memex should never try to download it. But the user wants
to ask "what comedies do I have?" or "recommend something like The Grand
Budapest Hotel from my library."

**Ingestion** (via a `PlexAdapter` that queries the Plex API):

```python
manager.ingest(
    uri="plex://server-192.168.1.50/library/movies/the-grand-budapest-hotel",
    kind="content.video",
    properties={
        "title": "The Grand Budapest Hotel",
        "year": 2014,
        "director": "Wes Anderson",
        "genres": ["comedy", "drama"],
        "rating": 8.1,
        "duration_minutes": 100,
        "content_type": {"mime": "video/mp4", "category": "video"},
        "access": {
            "method": "open",
            "uri": "https://app.plex.tv/desktop/#!/server/.../details/...",
            "app": "Plex",
            "label": "Open in Plex"
        },
        "source": {"server": "192.168.1.50", "library": "Movies"}
    },
    source_ref="adapter/plex-sync-2026-04-10"
)
```

**Chunks**: One chunk per item with a natural-language summary the agent can
match against:

> The Grand Budapest Hotel (2014), directed by Wes Anderson. Comedy, Drama.
> A writer encounters the owner of an aging high-class hotel, who tells him
> of his early years serving as a lobby boy. Rated 8.1/10.

**Agent interaction**:
- User: "What comedies do I have on Plex?"
- Agent queries: `vector_search("comedy movies")` filtered to `kind=content.video`
- Finds matches, sees `access.method: open`, `access.app: Plex`
- Responds with a list and Plex links: "Here are your comedies: *The Grand
  Budapest Hotel* ([open in Plex](...))"

**Staleness**: The Plex adapter runs periodically. `is_current()` checks the
Plex API's `updatedAt` field against the stored `content_hash` (which for Plex
is a hash of the metadata snapshot). New movies get ingested; removed ones get
superseded.

### Scenario 3: Saved Google Patent Search

A user is researching ceramic heat shield technology. They ran a Google Patents
search and saved several results. These are web resources -- the content is on
Google's servers, the user wants Memex to remember they exist and why they
matter.

**Ingestion** (via a `WebAdapter` or manual agent action):

```python
for patent in saved_patents:
    manager.ingest(
        uri=f"https://patents.google.com/patent/{patent['id']}",
        kind="content.web",
        properties={
            "title": patent["title"],
            "patent_id": patent["id"],
            "assignee": patent["assignee"],
            "filing_date": patent["filing_date"],
            "content_type": {"mime": "text/html", "category": "web"},
            "access": {
                "method": "browse",
                "uri": f"https://patents.google.com/patent/{patent['id']}",
                "label": f"View patent {patent['id']}"
            }
        },
        source_ref="user/patent-search-ceramic-heat-shields"
    )
```

**Chunks**: The adapter extracts the abstract and claims from the patent page:

> US20210123456A1: Ceramic Heat Shield Assembly for Hypersonic Vehicles.
> Assignee: Lockheed Martin Corp. Filed: 2021-03-15.
> Abstract: A heat shield assembly comprising a plurality of ceramic tiles...
> Claim 1: A thermal protection system comprising...

**Agent interaction**:
- User: "What was that patent about ceramic heat shields?"
- Agent: text search finds the chunks, returns the node
- Sees `category: web`, `method: browse` -- presents the title, abstract
  snippet, and a link to the full patent
- If user asks "summarize claim 3 in detail," agent can fetch the web page
  via the URI (it's `method: browse`, content is web-accessible) and process it

**Provenance**: All patents from this search share `source_ref:
"user/patent-search-ceramic-heat-shields"`. The user can later say "forget that
patent research" and Memex uses `excise_source()` to remove the entire batch.

### Scenario 4: Government Dataset

A user works with Census Bureau data. The dataset is 50 GB and lives behind a
web API. Memex shouldn't store it, but should know it exists, what it contains,
and how to query it.

**Ingestion** (via a `DataCatalogAdapter` or manual):

```python
manager.ingest(
    uri="https://api.census.gov/data/2020/acs/acs5",
    kind="content.data",
    properties={
        "title": "American Community Survey 5-Year Data (2020)",
        "publisher": "U.S. Census Bureau",
        "description": "Detailed demographic, social, economic, and housing data",
        "variables_count": 20000,
        "geographies": ["state", "county", "tract", "block group"],
        "content_type": {"mime": "application/json", "category": "data"},
        "access": {
            "method": "query",
            "uri": "https://api.census.gov/data/2020/acs/acs5",
            "docs": "https://www.census.gov/data/developers/data-sets/acs-5year.html",
            "label": "Query via Census API"
        },
        "update_frequency": "annual"
    },
    source_ref="user/census-data-registration"
)
```

**Chunks**: A descriptive summary of the dataset plus key variable groups:

> American Community Survey 5-Year Data (2020). U.S. Census Bureau.
> Covers demographics, income, education, housing, employment, health
> insurance, commuting patterns for all U.S. geographies down to block
> group level. ~20,000 variables. Queryable via REST API.
>
> Key subject tables: B01001 (Age and Sex), B19013 (Median Household
> Income), B25001 (Housing Units), B15003 (Educational Attainment)...

**Agent interaction**:
- User: "What data sources do I have about household income by county?"
- Agent: vector search matches "income" + "county" against the Census chunk
- Sees `category: data`, `method: query`, plus the docs URL
- Responds: "You have the American Community Survey 5-Year Data from the
  Census Bureau. It includes median household income (table B19013) at the
  county level. I can query it via their API -- want me to pull income data
  for a specific county?"
- If user says yes, agent uses the `access.uri` and `access.docs` to
  construct an API call

## The Properties Contract

Putting this together, content nodes have a lightweight convention in
`properties`. This is not a rigid schema -- it's guidance that adapters
produce and the agent consumes:

```json
{
  "title": "Human-readable title",

  "content_type": {
    "mime": "application/pdf",
    "category": "document"
  },

  "access": {
    "method": "browse",
    "uri": "https://...",
    "label": "Open in browser"
  }
}
```

That's it for the required convention. Three fields: `title`, `content_type`,
`access`. Everything else is adapter-specific and goes into the rest of
`properties` as additional context for the agent.

### Why `access` matters more than `content_type`

The `category` tells the agent *what kind of thing* it is. But `access.method`
tells the agent *what to do about it*. The same MP3 file might be:

- `category: audio, method: read` -- "I can read and transcribe this"
- `category: audio, method: play` -- "I should offer to play this"

The same PDF might be:
- `category: document, method: read` -- "I can read the local file"
- `category: document, method: browse` -- "I should present a link; I can
  fetch and read if the user asks"

This separation lets the adapter encode **intent** -- what the user wants this
content to be used for -- not just what it technically is.

### The agent doesn't need a "how to use" manual

The `method` + `category` combination is enough for an LLM agent to infer
appropriate behavior. Agents already understand that you don't "read" a video
or "play" a CSV. The structured hint just prevents edge cases:

- Without hints: agent tries to `cat` a 1TB dataset
- With `category: data, method: query`: agent knows to query, not download

This is the minimum viable contract. If a specific adapter needs richer
guidance (e.g., "this API requires an auth token stored in env var X"), it
goes into `access` as additional fields. The agent reads JSON; it can
interpret novel fields.

## How Search and Discovery Work

External content participates in the same query pipeline as all other Memex
data. No special query mode is needed.

### Text search

User asks about "ceramic heat shields." FTS matches against chunk text content.
Content nodes surface alongside meeting notes, knowledge items, and entity
attributes that mention the same phrase. The agent sees `kind=content.web` and
knows this is a web resource with a URL, distinct from a conversation excerpt.

### Vector search

User asks "what jazz do I have?" The vector embedding of this query is close
to the embeddings of chunks like "John Coltrane - A Love Supreme (1965). Jazz."
The content node surfaces. The agent sees `category: audio, method: play` and
offers to play rather than to read.

### Graph traversal

A user linked a patent node to a goal node ("Research heat shield materials").
When the agent queries context for that goal, it traverses edges and finds
the linked patents. The content nodes appear as related context with their
`access` hints.

```
(goal: "Research heat shield materials")
    --[content.supports]--> (content.web: "US20210123456A1")
    --[content.supports]--> (content.web: "US20210789012A1")
    --[content.references]--> (entity: "Lockheed Martin")
```

### Temporal context

The user says "what was I looking at last week?" The agent queries recent
content nodes by `created_at` timestamp. External content nodes ingested
last week surface alongside meetings and conversations from that period.

## Lifecycle in Memex

### Ingestion paths

Content enters Memex through several paths:

1. **Agent-initiated**: During a conversation, the agent encounters a URL or
   file reference. It creates a content node on behalf of the user.
   `source_ref` traces to the conversation turn.

2. **Adapter-driven background sync**: A configured adapter (Plex, music
   library, file watcher) runs periodically and ingests/refreshes content.
   `source_ref` traces to the sync run.

3. **User-directed**: The user says "remember this paper" and provides a URL
   or file. The agent ingests it immediately.

### Forgetting

Content nodes participate in Memex's existing "forget" workflow:

- **"Forget that patent research"**: `excise_source("user/patent-search-*")`
  removes all nodes from that source, including their chunks.
- **"Remove the Plex integration"**: Supersede all nodes with
  `source_ref` matching the Plex adapter, or remove the adapter and let
  retention policies handle cleanup.
- **"I don't have that album anymore"**: Upsert the node as retired. The
  chunks are removed, the metadata stays briefly for consistency, then
  retention purges it.

### Staleness and refresh

Adapters check `is_current()` on a schedule appropriate to the content type:

| Content type | Refresh strategy |
|---|---|
| Local files | File watcher or periodic hash check |
| Plex library | Nightly sync via API |
| Web pages | Weekly re-fetch, or on user request |
| DOI/patents | Rarely -- content is usually immutable |
| API datasets | Check `update_frequency` in properties |

Stale content gets re-extracted: node upserted, chunks replaced, vector
embeddings regenerated. The old version is superseded, preserving history.

## What This Means for FathomDB (Engine Level)

The engine changes from Option C remain minimal:

1. **`content_ref` on nodes** -- the URI/identifier. Enables `WHERE content_ref
   IS NOT NULL` filtering without scanning properties JSON.
2. **`content_hash` on chunks** -- the source content hash. Enables staleness
   queries joining nodes to their chunks.

Everything else -- the `access` and `content_type` conventions, the adapter
library, the refresh lifecycle, the Memex-specific ingestion paths -- lives
in application code. The engine doesn't know about Plex or patents or MP3s.
It knows about nodes, chunks, edges, and search. That's the right boundary.
