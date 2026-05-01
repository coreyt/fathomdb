| Producer | Consumer | Interface IDs | Public/Internal |
|---|---|---|---|
| Rust SDK | Rust application caller | `IF-001` | `Public` |
| Python SDK | Python application caller | `IF-002` | `Public` |
| TypeScript SDK | TypeScript application caller | `IF-003` | `Public` |
| Bindings facade | Engine | `IF-004` | `Public` |
| Engine | Rust SDK, Python SDK, TypeScript SDK | `IF-005`, `IF-016` | `Public` |
| Lifecycle | Host subscriber / application code | `IF-006`, `IF-007`, `IF-008`, `IF-009`, `IF-010` | `Public` |
| Errors | Rust SDK, Python SDK, TypeScript SDK, CLI | `IF-011` | `Public` |
| CLI | Operator | `IF-012` | `Public` |
| Recovery | Operator / automation tooling | `IF-013`, `IF-014` | `Public` |
| Migrations | Engine.open callers / host subscriber | `IF-015` | `Public` |
| Projection | Recovery / operator | `IF-017` | `Public` |
| Op Store | Engine / operator tooling | `IF-004`, `IF-017` | `Public` |
