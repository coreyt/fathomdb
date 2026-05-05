use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

#[test]
fn ac_039a_manifest_digest_matches_export_bytes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "export_match");
    let opened = Engine::open(&path).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "hello world".to_string(),
            source_id: None,
        }])
        .expect("write");

    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("export.sqlite");
    let manifest_path = out_dir.path().join("export.sha256.json");

    let artifact = opened.engine.safe_export(&out_path, &manifest_path).expect("safe_export");

    assert!(out_path.exists(), "export file missing");
    assert!(manifest_path.exists(), "manifest missing");

    let bytes = std::fs::read(&out_path).expect("read export");
    let expected = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(artifact.manifest_sha256, expected);

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["sha256"].as_str().unwrap(), expected);
    assert_eq!(manifest["byte_count"].as_u64().unwrap(), bytes.len() as u64);
    assert!(manifest["export_path"].as_str().is_some());
}

#[test]
fn ac_039b_one_byte_tamper_detected_by_recompute() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "export_tamper");
    let opened = Engine::open(&path).expect("open");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "hello tamper".to_string(),
            source_id: None,
        }])
        .expect("write");

    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("export.sqlite");
    let manifest_path = out_dir.path().join("export.sha256.json");
    let artifact = opened.engine.safe_export(&out_path, &manifest_path).expect("safe_export");

    let mut bytes = std::fs::read(&out_path).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&out_path, &bytes).expect("rewrite");

    let recomputed = format!("{:x}", Sha256::digest(&bytes));
    assert_ne!(recomputed, artifact.manifest_sha256, "tamper not detected");
}
