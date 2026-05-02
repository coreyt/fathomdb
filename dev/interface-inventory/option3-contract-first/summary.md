# Summary

The 0.6.0 contract model is split by contract class rather than by crate row. `design/engine.md` owns runtime call semantics and cursor meaning, `design/lifecycle.md` owns typed observability contracts, `design/bindings.md` owns cross-language parity and adapter protocol, `design/errors.md` owns open-path corruption payloads, and `design/recovery.md` plus `interfaces/cli.md` own operator CLI roots and machine-readable recovery/reporting surfaces.

The highest-risk overlaps are migration progress, corruption-on-open routing, and SDK-versus-CLI boundary claims. Migration step payloads are routed through lifecycle subscribers but are supposed to be owned by `design/migrations.md`, which is still only a stub; corruption detail spans engine, errors, recovery, and bindings; and the Python/TypeScript/Rust interface docs are still `TBD`, so `design/bindings.md` is currently acting as the de facto owner for several public contracts it says should live elsewhere.

The contract-first pass surfaces distinctions that an architecture-first pass would likely flatten: lifecycle is three separate public surfaces rather than "logging," `doctor check-integrity` finding codes are not the same contract as `Engine.open` corruption enums, and the SDK surface is defined as much by non-presence (`recover` is forbidden) as by the five verbs that do exist.
