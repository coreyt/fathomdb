//! AC-036 / AC-037 security-fixture entry point. Performs a full
//! open + write + search + close cycle against a fresh temp database
//! so the surrounding strace/netns harnesses can scope syscall
//! capture to a deterministic, minimal-footprint process.
//!
//! Usage:
//!     security_cycle [db_path]
//!
//! If `db_path` is omitted, a tempdir under $TMPDIR is used and
//! deleted on exit. Exit 0 on full success; exit 1 with diagnostic
//! on stderr otherwise.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use fathomdb_engine::{Engine, PreparedWrite};

fn run(path: PathBuf) -> Result<(), String> {
    let opened = Engine::open(&path).map_err(|e| format!("open: {e:?}"))?;
    let engine = opened.engine;

    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "security cycle fixture body".to_string(),
            source_id: fathomdb_engine::SourceId::new("example:fixture")
                .expect("example source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .map_err(|e| format!("write: {e:?}"))?;

    let _ = engine.search("security").map_err(|e| format!("search: {e:?}"))?;

    engine.close().map_err(|e| format!("close: {e:?}"))?;
    Ok(())
}

fn main() -> ExitCode {
    let arg = env::args().nth(1);
    let (path, _guard) = match arg {
        Some(p) => (PathBuf::from(p), None),
        None => {
            let dir = match tempfile::TempDir::new() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("security_cycle: tempdir: {e}");
                    return ExitCode::from(1);
                }
            };
            let p = dir.path().join("security_cycle.sqlite");
            (p, Some(dir))
        }
    };
    match run(path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("security_cycle: {msg}");
            ExitCode::from(1)
        }
    }
}
