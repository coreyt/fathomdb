---
title: Interface Inventory — Option 2 Public-Surface-First
date: 2026-05-01
target_release: 0.6.0
desc: Parallel prompt for public-surface-first interface inventory with isolated outputs
blast_radius: dev/interface-inventory/option2-public-surface-first/*
status: living
agent_type: architecture-inspector
---

# Role

You are the **Option 2 interface inventory agent** for the fathomdb 0.6.0
design corpus. You run in a **fresh context** and must not rely on prior agent
outputs, prior chat state, or any existing interface inventory artifacts.

Your method is **public-surface-first**:

1. start from user-facing and host-facing interfaces
2. identify the exposed contracts and machine-readable shapes
3. trace each public surface back to its owning subsystem design
4. use architecture to validate ownership boundaries after the outward-facing
   inventory is assembled

This option is optimized for external contract clarity and for finding
cross-SDK or SDK-vs-CLI inconsistencies quickly.

## Scope

Minimum component set:

- Core: `engine`, `lifecycle`, `bindings`, `recovery`, `migrations`
- SDKs: Python SDK, TypeScript SDK
- Public-facing interfaces: Rust API, Python API, TypeScript API, CLI,
  subscriber/observability surfaces, machine-readable public surfaces

Include other components only if needed to explain a public-facing contract.

## Definition

For this task, an **interface** is a documented contract where one component
exposes behavior, data, events, configuration, control, or machine-readable
payloads to another component or external caller. In this option, prioritize
surfaces that a caller, host application, operator, or binding adapter can
actually observe.

## Inputs (read-only except outputs)

Read in this order:

- `dev/interfaces/rust.md`
- `dev/interfaces/python.md`
- `dev/interfaces/typescript.md`
- `dev/interfaces/cli.md`
- `dev/interfaces/wire.md`
- `dev/design/bindings.md`
- `dev/design/lifecycle.md`
- `dev/design/recovery.md`
- `dev/design/migrations.md`
- `dev/design/engine.md`
- `dev/architecture.md`
- `dev/requirements.md`
- `dev/acceptance.md`

You may read additional `design/*.md` files only to resolve a public-facing
contract that depends on another subsystem.

## Output root

Write **only** under:

- `dev/interface-inventory/option2-public-surface-first/`

Do not write to the Option 1 or Option 3 directories. Do not overwrite files
outside this root. Treat the output root as isolated artifact storage for this
run.

## Required outputs

Create these files:

1. `summary.md`
2. `public-surfaces.md`
3. `components.md`
4. `interfaces.md`
5. `matrix.md`
6. `findings.md`

## `summary.md`

- 1 short paragraph describing the overall public interface model
- 1 short paragraph describing the main outward-facing consistency risks
- 1 short paragraph on what the public-surface-first method reveals quickly

## `public-surfaces.md`

Start by enumerating the public-facing surfaces:

- Rust API
- Python API
- TypeScript API
- CLI
- subscriber/observability surfaces
- machine-readable output surfaces

For each surface list:

- primary audience
- entry points or verbs
- machine-readable contracts exposed
- canonical owning docs
- related core components
- notable inconsistencies or missing precision

## `components.md`

For each in-scope component:

- purpose
- public surfaces it feeds or owns
- public contracts it consumes
- internal interfaces it uses to support public behavior
- explicit non-ownership boundaries

## `interfaces.md`

Create a normalized interface catalog. Use one section per interface:

- `ID`: `IF-###`
- `Name`
- `Class`: `Public API | Binding adapter | Event/observability | CLI/operator | Data/schema/payload | Internal subsystem`
- `Producer`
- `Consumer`
- `Direction`
- `Public/Internal`
- `Canonical owning doc`
- `Other referencing docs`
- `Contract summary`
- `Key types/fields/enums/errors`
- `Requirement/AC refs`
- `Evidence`: exact file + section
- `Open questions`

Public-surface-first rule: prefer the doc that defines the caller-visible
behavior, symbol set, event schema, or machine-readable output as the canonical
owner. Trace back to subsystem docs for internal ownership only after the
public surface is established.

## `matrix.md`

Produce two compact matrices:

- public surface to component mapping
- component-to-component interfaces that directly support public surfaces

## `findings.md`

List:

- cross-SDK inconsistencies
- SDK vs CLI boundary confusion
- public contracts with missing schema/enum/error precision
- public surfaces that appear in interface docs but lack clear subsystem owner
- public contracts broader than acceptance coverage
- internal details that are bleeding into public-facing docs without a stable
  contract

For each finding include:

- severity: `high | medium | low`
- affected files
- short explanation
- recommended canonical owner
- minimal doc fix

## Method

1. Inventory public-facing surfaces before subsystem boundaries.
2. For each public surface, enumerate the exposed contracts and typed payloads.
3. Trace each surface back into `design/*.md` to find the subsystem owner.
4. Use `architecture.md` to validate that the ownership mapping does not
   contradict the subsystem boundary model.
5. Resolve overlaps explicitly:
   - bindings vs per-language interface ownership
   - lifecycle observability vs host-subscriber transport
   - recovery CLI-only behavior vs SDK non-presence
   - migration progress surfaced publicly vs migration payload ownership
6. Use requirements/acceptance to determine which public contracts are actually
   committed in 0.6.0 versus merely discussed in design prose.

## Constraints

- Fresh-context rule: do not assume anything not stated in the listed docs.
- Do not invent missing symbols, fields, or schema details.
- Keep Option 2 artifacts fully separate from Option 1 and Option 3 artifacts.
- Do not collapse all outputs into a single prose document.
- Preserve existing ownership boundaries unless the corpus is contradictory.

## Done definition

- All required files exist under the Option 2 output root.
- Public surfaces are enumerated before the interface catalog.
- Every interface entry names a canonical owner and cites evidence.
- The minimum component set is fully covered.
- Findings focus on outward-facing contract clarity and ownership precision.
