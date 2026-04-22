# Design: Vector Regeneration Hardening

## Purpose

Define the follow-up hardening work for the implemented vector regeneration path
tracked by GitHub issue `#9`.

This design does **not** revisit the high-level architecture choice for vector
recovery. That question is tracked separately in GitHub issue `#10` and in
[arch-decision-vector-embedding-recovery.md](./arch-decision-vector-embedding-recovery.md).

This design is about making the current v0.1 Choice C implementation more
mature, safer, easier to operate, and easier to evolve.

## Current State

The current implementation already provides:

- a persisted regeneration contract in `vector_embedding_contracts`
- a three-phase snapshot / generate / apply flow
- atomic application of:
  - persisted contract metadata
  - vec-row replacement
  - successful apply audit event
- bounded external generator execution with timeout and size limits
- bridge and CLI policy plumbing
- Rust, Go, and end-to-end tests for:
  - happy path
  - malformed generator output
  - timeouts
  - stdout/stderr overflow
  - snapshot drift

Relevant implementation:

- `crates/fathomdb-engine/src/admin.rs`
- `crates/fathomdb-engine/src/bin/fathomdb-admin-bridge.rs`
- `crates/fathomdb-schema/src/bootstrap.rs`
- `go/fathom-integrity/internal/bridge/client.go`
- `go/fathom-integrity/internal/cli/cli.go`
- `go/fathom-integrity/internal/commands/vector_regeneration.go`

Relevant support docs:

- [repair-support-contract.md](./repair-support-contract.md)
- [arch-decision-vector-embedding-recovery.md](./arch-decision-vector-embedding-recovery.md)
- [../docs/vector-regeneration.md](../docs/vector-regeneration.md)

## Problem Statement

The current vector regeneration path is functional, but it remains the most
complex recovery-sensitive surface in the repo because it combines:

- persisted application-supplied metadata
- engine-owned schema
- external process execution
- operator policy
- recovery-time correctness

The remaining hardening work is about reducing operational ambiguity and
tightening trust, validation, migration, and audit behavior.

## Goals

1. Reject invalid or dangerous regeneration contracts before they can become
   durable operational state.
2. Make the external generator trust boundary explicit and enforceable.
3. Give operators actionable, specific feedback for slow, failed, or retryable
   regeneration attempts.
4. Define how persisted regeneration contracts evolve across schema and product
   changes.
5. Expand adversarial coverage so the hardening rules are enforced by tests.
6. Improve provenance and auditability of regeneration activity.

## Non-Goals

This design does not:

- change Choice C as the current vector recovery contract
- make embeddings canonical
- make recovery preserve vec rows directly
- add a distributed job system
- move embedding generation into the core engine

## Scope

The work is scoped to the vector regeneration implementation and adjacent
operator tooling:

- Rust engine admin path
- Rust bridge
- Go bridge client and CLI
- documentation and operator contract
- Rust, Go, and end-to-end tests

## Design

### 1. Tighten Validation of Persisted Regeneration Contracts

#### Problem

The current contract parsing validates basic shape, but not enough semantic or
operational constraints to treat the record as mature durable state.

#### Design

Add a dedicated contract validation pass in `admin.rs` before any persistence or
generator execution.

Validation rules:

- `profile`
  - non-empty
  - length-bounded
  - must match an existing profile or be explicitly creatable under the current
    vec capability rules
- `table_name`
  - must be exactly `vec_nodes_active`
- `model_identity`
  - non-empty
  - length-bounded
- `model_version`
  - non-empty
  - length-bounded
- `dimension`
  - must be greater than zero
  - must match the active profile dimension when the profile already exists
- `normalization_policy`
  - non-empty
  - length-bounded
- `chunking_policy`
  - non-empty
  - length-bounded
- `preprocessing_policy`
  - non-empty
  - length-bounded
- `generator_command`
  - must contain at least one element
  - executable path must be absolute
  - executable path length and argument lengths must be bounded
  - total serialized command length must be bounded

Additional record-level validation:

- serialized contract size must be bounded
- `snapshot_hash` must be engine-owned only; callers cannot supply it
- future format changes must be versioned explicitly

#### Implementation

- add `validate_vector_regeneration_config(...)` in `admin.rs`
- call it before snapshot collection
- return specific `EngineError::Bridge(...)` messages for each failure class

#### Acceptance

- invalid contracts fail before any DB mutation
- persisted contracts are normalized and bounded
- tests cover each validation rule

### 2. Harden the External Generator Trust and Security Model

#### Problem

The generator is operator-controlled, but the repo still needs a stronger and
clearer execution policy than “run a configured command with warnings.”

#### Design

Keep semantic regeneration config application-owned, and keep execution policy
operator-owned. Strengthen enforcement around executable trust.

Add enforcement policy fields to the operator policy layer:

- `require_absolute_executable`
- `reject_world_writable_executable`
- `allowed_executable_roots`
- `preserve_env_vars`

Initial policy behavior:

- absolute executable path required by default
- world-writable executable rejected by default
- allowlisted roots optional but supported
- environment inheritance reduced by default
  - preserve only explicitly allowed variables

Execution policy remains separate from the persisted semantic contract and is
not stored in `vector_embedding_contracts`.

#### Implementation

- extend `VectorGeneratorPolicy`
- validate executable metadata before spawn
- use a reduced child environment instead of inheriting the full process env
- document the trust boundary in:
  - [repair-support-contract.md](./repair-support-contract.md)
  - [../docs/vector-regeneration.md](../docs/vector-regeneration.md)

#### Acceptance

- relative paths can be denied explicitly
- world-writable executables are rejected when policy requires it
- allowlisted roots can be enforced
- tests cover path and permissions rejection

### 3. Improve Operator UX for Slow, Partial, or Unavailable Regeneration

#### Problem

The implementation already distinguishes several failure cases internally, but
operator output can still be more actionable.

#### Design

Normalize regeneration failures into a small operator-facing taxonomy:

- invalid contract
- payload too large
- generator timeout
- generator stdout overflow
- generator stderr overflow
- generator nonzero exit
- malformed generator JSON
- snapshot drift, retry
- unsupported vec capability

For each class, define:

- CLI/bridge message
- retryability
- recommended operator action

Examples:

- snapshot drift
  - message: “chunk snapshot changed during generation; retry”
  - retryable: yes
- malformed JSON
  - message: “generator returned invalid JSON”
  - retryable: no, until generator is fixed
- timeout
  - message: “generator exceeded timeout; increase limit or fix generator”
  - retryable: maybe

Integrate these messages with the existing response-cycle feedback path where
useful, but do not turn regeneration into a streaming protocol redesign.

#### Implementation

- improve message mapping in:
  - `admin.rs`
  - `fathomdb-admin-bridge.rs`
  - `go/fathom-integrity/internal/cli/cli.go`
- keep specific error text on stderr / bridge payload while preserving the
  existing response framing

#### Acceptance

- CLI errors are specific and actionable
- retryable failures are clearly labeled
- tests assert operator-visible output for major failure classes

### 4. Define Upgrade and Migration Policy for Persisted Contracts

#### Problem

`vector_embedding_contracts` is now part of the durable admin contract, but its
evolution rules are still only implicit.

#### Design

Define the table as the durable record of the **currently applied semantic
contract**, not a run-history table.

Add a small explicit format version:

- `contract_format_version INTEGER NOT NULL DEFAULT 1`

Rules:

- additive fields require schema migration and bootstrap compatibility
- semantic interpretation changes require format-version bump
- old versions must either:
  - migrate automatically, or
  - fail with a clear operator error

Document compatibility expectations for:

- bootstrap
- recover
- restore-vector-profile flows
- regenerate-vectors after upgrade

#### Implementation

- schema migration for `contract_format_version`
- bootstrap support for mixed-version DBs
- documentation updates in:
  - [repair-support-contract.md](./repair-support-contract.md)
  - [../docs/vector-regeneration.md](../docs/vector-regeneration.md)

#### Acceptance

- schema/bootstrap remains idempotent
- older persisted contracts have a defined behavior
- tests cover migration/bootstrap compatibility

### 5. Expand Adversarial Tests

#### Problem

The current tests cover major runtime failures, but not the full hardening
surface.

#### Design

Add adversarial tests for:

- contract field overflows
- invalid empty/whitespace-only fields
- relative executable paths when forbidden
- world-writable executable rejection
- executable path outside allowlisted roots
- oversized contract serialization
- partial stdout plus timeout
- huge malformed stdout
- huge stderr on nonzero exit
- generator exits zero with semantically invalid embeddings
- migration/bootstrap on older contract rows

Test layers:

- Rust unit tests for engine behavior
- Go tests for bridge/CLI forwarding and operator messages
- e2e tests for recover -> regenerate failure handling

#### Acceptance

- hardening rules are enforced in tests, not only documented
- at least one adversarial test exists for each policy family

### 6. Add Richer Provenance and Audit for Regeneration

#### Problem

The current success audit event is useful, but not yet rich enough for
post-incident review.

#### Design

Add a small regeneration audit lifecycle:

- `vector_regeneration_requested`
- `vector_regeneration_failed`
- `vector_regeneration_apply`

Recorded metadata should be bounded and operator-useful:

- profile
- model identity
- model version
- chunk count
- snapshot hash
- failure class

Do **not** record:

- raw embeddings
- full payload text
- huge stderr/stdout blobs
- secrets

Failure audit must be best-effort and must not create a metadata/content
atomicity problem. If failure events are recorded, they should be:

- written either before generation begins as a request record, or
- written after failure on a separate connection as a bounded audit note

Success audit remains inside the apply transaction.

#### Implementation

- expand provenance writes in `admin.rs`
- document the audit model in:
  - [repair-support-contract.md](./repair-support-contract.md)
  - [../docs/vector-regeneration.md](../docs/vector-regeneration.md)

#### Acceptance

- operators can tell:
  - when regeneration was attempted
  - what contract/profile it targeted
  - whether it failed or applied
  - why it failed at a coarse level

## Implementation Sequence

### Phase 1: Contract Validation

- add semantic and size validation
- add tests for invalid contracts

### Phase 2: Security Policy Enforcement

- add executable and environment policy enforcement
- add tests for denied execution cases

### Phase 3: Operator UX

- normalize errors
- tighten CLI/bridge messaging
- add retryability-oriented tests

### Phase 4: Contract Evolution Policy

- add `contract_format_version`
- add migration/bootstrap coverage
- document upgrade rules

### Phase 5: Adversarial Coverage

- add remaining malformed/misbehavior tests

### Phase 6: Richer Audit

- add request/failure/apply audit lifecycle
- add bounded metadata assertions

## Test Plan

TDD is required.

For each phase:

- write failing tests first
- implement the minimum behavior to pass
- then refactor

Feature-completeness tests should prove:

- invalid contracts cannot become durable state
- generator execution policy is actually enforced
- operators get actionable failure messages
- persisted contracts survive supported upgrades cleanly
- provenance/audit records are sufficient for incident review

## Acceptance Criteria

Issue `#9` is ready to close when all of the following are true:

- regeneration contract validation is strict and bounded
- executable trust policy is enforced, not only warned
- operator failure UX is specific and actionable
- persisted contract evolution rules are documented and tested
- adversarial tests exist for the major failure modes
- regeneration provenance is rich enough for post-incident analysis

## Relationship To Other Docs

- Choice C itself remains defined by
  [arch-decision-vector-embedding-recovery.md](./arch-decision-vector-embedding-recovery.md)
- the current production support boundary remains defined by
  [repair-support-contract.md](./repair-support-contract.md)
- operator-facing usage remains documented in
  [../docs/vector-regeneration.md](../docs/vector-regeneration.md)
