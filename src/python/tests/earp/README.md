# EARP tests

This directory contains test-first coverage for strict configuration,
gold/refusal handling, Rust-parity metrics, durable artifact ordering,
SDK-backed diagnostic execution, corpus characterization, projection witnesses,
comparison statistics, public result-limit adoption, and the priced-arm guard.

Runner integration tests use a real FathomDB database with small,
human-authored fixtures. Generated relevance oracles are not permitted.
Network, real-model, and priced paths are opt-in and visibly skipped by default.
