//! EU-5b RED — `fathomdb doctor warm-cache` CLI subcommand.
//!
//! Per `dev/plans/prompts/0.7.1-EMBEDDER-UNDEFER-HANDOFF.md` §EU-5b step 7.
//! The subcommand invokes the default-embedder loader without opening an
//! engine, so users/CI can warm the cache in a controlled step before
//! `Engine::open` runs.

use std::process::Command;

fn fathomdb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fathomdb"))
}

#[test]
fn warm_cache_help_exits_zero() {
    let output = fathomdb()
        .args(["doctor", "warm-cache", "--help"])
        .output()
        .expect("spawn doctor warm-cache --help");
    assert!(output.status.success(), "doctor warm-cache --help must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.to_lowercase().contains("usage"), "help output must show Usage line");
}

#[cfg(feature = "default-embedder")]
#[test]
fn cli_doctor_warm_cache_succeeds() {
    // Invokes the loader; should exit 0 and surface a structured summary.
    let output = fathomdb()
        .args(["doctor", "warm-cache", "--json"])
        .output()
        .expect("spawn doctor warm-cache --json");
    assert!(
        output.status.success(),
        "warm-cache should exit 0 (status={:?}, stderr={})",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The CLI output must include the typed loader summary keys.
    assert!(stdout.contains("config_json"), "stdout must mention config_json path: {stdout}");
    assert!(stdout.contains("model_safetensors"), "stdout must mention model_safetensors path");
    assert!(stdout.contains("tokenizer_json"), "stdout must mention tokenizer_json path");
    assert!(
        stdout.contains("bytes_downloaded") || stdout.contains("bytes"),
        "stdout must report bytes downloaded (or 0 on cache-hit)"
    );
}
