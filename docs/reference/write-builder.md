# Write Builder

The `WriteRequestBuilder` assembles a batch of mutations (nodes, edges, chunks,
runs, steps, actions) into a single atomic `WriteRequest`. Handle objects
returned by each `add_*` method let you cross-reference entities within the same
request. See the [Writing Data](../guides/writing-data.md) guide for examples.

## WriteRequestBuilder

::: fathomdb.WriteRequestBuilder
    options:
      members_order: source
      heading_level: 3

## Handle Types

Opaque references returned by the builder so that newly added entities can be
wired together before the request is finalized.

::: fathomdb.NodeHandle
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.EdgeHandle
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.RunHandle
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.StepHandle
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ActionHandle
    options:
      heading_level: 3
      show_root_heading: true

::: fathomdb.ChunkHandle
    options:
      heading_level: 3
      show_root_heading: true
