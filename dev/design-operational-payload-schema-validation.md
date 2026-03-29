# Design: Operational Payload Schema Validation

## Purpose

Define an optional, explicit write-time validation contract for operational
payloads without changing the current meaning of
[`schema_json`](/home/coreyt/projects/fathomdb/dev/design-add-operational-store-feature.md).

The current system deliberately treats `schema_json` as documentation-only
metadata. This design preserves that boundary and adds a separate validation
surface that collections may opt into when production needs justify it.

## Decision Summary

- `schema_json` remains documentation-only metadata by default and should not
  silently become executable validation.
- Write-time validation should be opt-in per collection.
- Validation should use a dedicated collection contract field, not overload
  `schema_json`, `retention_json`, or `filter_fields_json`.
- The first validation slice should be intentionally narrow and bounded:
  top-level field presence, scalar types, simple arrays, enum membership, and
  numeric/string limits.
- The implemented v1 slice supports `disabled`, `report_only`, and `enforce`.
- `report_only` is implemented through the generic cross-language
  `WriteReceipt.warnings` surface; it accepts the write and reports the same
  field-level validation message that `enforce` would reject on.
- Historical rows must remain readable and recoverable even if a newer
  validation contract would reject them on fresh writes.

## Goals

- Reject malformed operational writes deterministically when a collection opts
  into enforcement.
- Let operators stage validation without breaking existing collections by
  surprise.
- Preserve recoverability and upgrade safety.
- Keep the validation surface small enough to reason about and test well.

## Non-Goals

- Full JSON Schema compatibility.
- Arbitrary nested schema engines.
- Cross-row or cross-table validation.
- Query-time schema inference or dynamic schema discovery.
- Retrofitting strict validation onto old history without explicit operator
  intent.

## Why A Separate Contract Field

`schema_json` is already established as documentation-oriented metadata. If the
engine were to reinterpret it as mandatory validation, existing collections
could start failing writes after upgrade without any explicit opt-in. That
would violate the current application/engine boundary and make rollout brittle.

The correct design is a dedicated validation contract, for example
`validation_json`, stored alongside the existing collection metadata:

```sql
ALTER TABLE operational_collections
    ADD COLUMN validation_json TEXT NOT NULL DEFAULT '';
```

An empty string means "no validation contract configured". A non-empty payload
contains a versioned validation definition.

## Contract Shape

The initial contract should be versioned and deliberately small:

```json
{
  "format_version": 1,
  "mode": "disabled",
  "additional_properties": true,
  "fields": [
    {
      "name": "status",
      "type": "string",
      "required": true,
      "enum": ["queued", "running", "done", "failed"],
      "max_length": 32
    },
    {
      "name": "attempt",
      "type": "integer",
      "required": false,
      "minimum": 0
    },
    {
      "name": "ts",
      "type": "timestamp",
      "required": true
    },
    {
      "name": "tags",
      "type": "array[string]",
      "required": false,
      "max_items": 16
    }
  ]
}
```

### Supported v1 Field Types

- `string`
- `integer`
- `float`
- `boolean`
- `timestamp`
- `object`
- `array[string]`
- `array[integer]`
- `array[float]`
- `array[boolean]`

The initial contract should stay top-level only. Nested object validation can
be added later through a new `format_version`, but it should not be part of the
first implementation slice.

### Supported v1 Constraints

- `required`
- `nullable`
- `enum`
- `minimum`
- `maximum`
- `max_length`
- `max_items`
- `additional_properties`

## Validation Modes

### `disabled`

- The collection behaves exactly as it does today.
- `validation_json` may exist, but the engine does not enforce it.

### `report_only`

- Implemented in the current slice.
- Invalid payloads are accepted, but the write receipt includes a generic
  validation warning in `warnings`.
- The warning payload is transport-stable across Rust, Python, and the bridge
  surfaces because the generic warning channel now exists on `WriteReceipt`.
- History validation remains useful for preexisting rows and staged operator
  rollout before switching from `report_only` to `enforce`.

### `enforce`

- Invalid payloads fail the write deterministically with `InvalidWrite`.
- Failure messages should identify the collection, field, and violated rule.

## Runtime Behavior

Validation applies only to payload-bearing operational writes:

- `Append`
- `Put`

It does not apply to:

- `Delete` operations that carry no payload
- collection registration itself beyond validating the contract shape
- query-time reads

Validation runs before any mutation rows, current rows, or derived filter/index
rows are written. That preserves atomicity: either the full write passes or the
database remains unchanged.

## Admin Surface

The design should add two explicit admin/update surfaces:

1. `update_operational_collection_validation(collection, validation_json)`
2. `validate_operational_collection_history(collection)`

The second surface matters because validation is optional and collections may
already contain old payloads. Operators need a safe way to measure historical
compatibility before switching a collection to `enforce`.

## Evolution Rules

- Validation contract updates are explicit admin actions.
- Old rows are never silently rewritten to satisfy a new contract.
- Upgrading from `disabled` to `enforce` should be preceded by an operator
  review of historical compatibility through
  `validate_operational_collection_history`.
- Contract evolution must be versioned so later validation formats do not
  reinterpret older contracts incorrectly.

## Recovery And Bootstrap Requirements

- `validation_json` lives in `operational_collections`, so safe export,
  recovery, and bootstrap must preserve it.
- Recovery must not make old databases unrecoverable simply because a newer
  validation contract is stricter.
- History validation is diagnostic, not part of bootstrap.

## Error Model

Validation failures should be operator-visible and specific:

- collection name
- write kind
- field name
- expected constraint
- actual value summary

Example:

```text
invalid operational payload for collection 'connector_health':
field 'status' must be one of ["queued","running","done","failed"], got "bogus"
```

## Verification

Implementation should add requirement-level tests for:

- registration/update rejects malformed `validation_json`
- `disabled` mode accepts payloads that would fail under `enforce`
- `report_only` mode accepts invalid payloads and emits a generic write warning
- `enforce` mode rejects invalid `Append`
- `enforce` mode rejects invalid `Put`
- `Delete` bypasses payload validation
- historical validation reports existing invalid rows without mutating them
- safe export / recover / bootstrap preserve the validation contract

## Risks And Tradeoffs

### Risks Accepted By This Design

- The engine still does not become a full schema system.
- Deeply nested payload contracts remain application responsibility for now.

### Risks Reduced By This Design

- Invalid operational payloads can be rejected at the storage boundary when
  needed.
- Rollout can be staged safely instead of forcing immediate strictness.
- Recoverability remains intact because old history is not destroyed or made
  unreadable.

## Bottom Line

Operational payload validation now ships as a separate, opt-in, versioned
contract with staged rollout semantics: `disabled`, `report_only`, and
`enforce`. The engine validates a narrow write-time payload contract without
reinterpreting `schema_json` into a broader schema engine.
