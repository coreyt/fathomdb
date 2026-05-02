# Matrices

## Component-to-Component Interfaces

| Producer            | Consumer                       | Interfaces                                                 |
| ------------------- | ------------------------------ | ---------------------------------------------------------- |
| `engine`            | `bindings`                     | `IF-003`, `IF-004`, `IF-005`, `IF-006`, `IF-013`, `IF-021` |
| `lifecycle`         | `bindings`                     | `IF-007`, `IF-008`, `IF-009`, `IF-010`, `IF-011`           |
| `migrations`        | `lifecycle`                    | `IF-012`                                                   |
| `migrations`        | `bindings`                     | `IF-012`                                                   |
| `errors`            | `bindings`                     | `IF-013`, `IF-014`                                         |
| `bindings`          | Python SDK                     | `IF-001`, `IF-002`, `IF-014`, `IF-015`, `IF-021`           |
| `bindings`          | TypeScript SDK                 | `IF-001`, `IF-002`, `IF-014`, `IF-015`, `IF-021`           |
| `recovery`          | CLI                            | `IF-016`, `IF-017`, `IF-018`, `IF-019`, `IF-020`           |
| CLI interface layer | operator tooling               | `IF-017`, `IF-018`, `IF-022`                               |
| subscriber surface  | host logging/callback adapters | `IF-007`, `IF-008`, `IF-010`, `IF-011`, `IF-012`, `IF-015` |

## Contract-Class-to-Component Mapping

| Contract class      | Engine                                           | Lifecycle                                        | Bindings                     | Recovery                                         | Migrations      | Python SDK         | TypeScript SDK     | CLI                          |
| ------------------- | ------------------------------------------------ | ------------------------------------------------ | ---------------------------- | ------------------------------------------------ | --------------- | ------------------ | ------------------ | ---------------------------- |
| Public API          | `IF-003`, `IF-004`, `IF-005`, `IF-006`, `IF-021` |                                                  | `IF-001`                     |                                                  |                 | `IF-001`           | `IF-001`           |                              |
| Binding adapter     |                                                  |                                                  | `IF-002`, `IF-014`, `IF-015` |                                                  |                 | `IF-014`, `IF-015` | `IF-014`, `IF-015` | `IF-022`                     |
| Event/observability |                                                  | `IF-007`, `IF-008`, `IF-009`, `IF-010`, `IF-011` | `IF-015`                     |                                                  | routed `IF-012` | consumes           | consumes           | human render / `--json`      |
| CLI/operator        |                                                  |                                                  | `IF-002`                     | `IF-016`, `IF-017`, `IF-018`, `IF-019`, `IF-020` |                 | forbidden          | forbidden          | `IF-017`, `IF-018`, `IF-022` |
| Data/schema/payload | `IF-021`                                         | `IF-010`, `IF-011`                               | `IF-014`                     | `IF-016`, `IF-019`                               | `IF-012`        | consumes           | consumes           | consumes                     |
| Internal subsystem  | `IF-003`, `IF-004`, `IF-006`                     | routes producer envelopes                        | routes across languages      | routes operator workflows                        | `IF-012`        |                    |                    | routes to operator           |
