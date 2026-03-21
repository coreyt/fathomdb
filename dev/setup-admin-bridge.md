# Setup: Admin Bridge

## Purpose

This document tracks the concrete work needed to turn the existing
JSON-over-stdio bridge into a stable v1 operator interface between the Rust
engine and the Go admin CLI.

This is a companion to:

- [design-repair-provenance-primitives.md](./design-repair-provenance-primitives.md)

## Current Repository State

The bridge already exists and works for basic command dispatch:

- one Rust binary reads a JSON request from stdin
- a string command selects the admin operation
- a JSON response is written to stdout
- the Go CLI wraps that response with timeout handling

What is missing is a more explicit protocol contract.

## Deliverables

1. A versioned request/response protocol.
2. Typed command names or enums on both sides.
3. Stable error categories for CLI exit-code mapping.
4. Binary discovery and compatibility rules.
5. Tests that cover malformed input, unsupported protocol versions, and command
   failures.

## Decisions To Make

### 1. Protocol Versioning

The bridge should expose a protocol version early, even if it starts at `1`.
That avoids silent drift between:

- Rust admin binary behavior
- Go CLI expectations

### 2. Error Shape

Errors should stop being plain strings only. The response should carry enough
structure for the CLI to distinguish:

- bad request
- unsupported command
- unsupported capability
- integrity failure
- execution failure

### 3. Command Shape

Decide whether the bridge keeps one generic envelope with `command` plus fields,
or evolves into a tagged request union. The second option is usually cleaner
once the command surface grows.

### 4. Output Discipline

The bridge should reserve:

- stdout for the JSON response only
- stderr for diagnostics only if needed

That keeps the Go side simple and predictable.

## Implementation Sequence

1. Add protocol version fields to request and response.
2. Replace stringly typed command parsing with a typed internal command enum.
3. Add structured error codes in Rust.
4. Map those error codes to Go CLI exit behavior.
5. Add contract tests around malformed JSON, unsupported versions, and one
   happy-path command per command family.

## Done When

- Rust and Go share one explicit bridge contract
- command and error handling are typed, not just string-matched
- protocol mismatches fail loudly
- bridge tests cover both happy path and bad input
