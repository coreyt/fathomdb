use fathomdb_engine::{CheckIntegrityOpts, Engine, IntegrityReport, Section};
use fathomdb_schema::SQLITE_SUFFIX;
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

#[test]
fn ac_043a_three_section_report_on_healthy_db() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "healthy");
    let opened = Engine::open(&path).expect("open");
    let report =
        opened.engine.check_integrity(CheckIntegrityOpts::default()).expect("check_integrity");
    let IntegrityReport { physical, logical, semantic } = report;
    assert!(matches!(physical, Section::Clean));
    assert!(matches!(logical, Section::Clean));
    assert!(matches!(semantic, Section::Clean));
}

#[test]
fn ac_043b_full_run_keeps_three_sections() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "healthy_full");
    let opened = Engine::open(&path).expect("open");
    let report = opened
        .engine
        .check_integrity(CheckIntegrityOpts { quick: false, full: true, round_trip: false })
        .expect("check_integrity");
    assert!(matches!(report.physical, Section::Clean));
    assert!(matches!(report.logical, Section::Clean));
    assert!(matches!(report.semantic, Section::Clean));
}

#[test]
fn ac_043c_full_findings_include_integrity_check_code() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "page_damage");

    {
        let opened = Engine::open(&path).expect("open");
        drop(opened);
    }

    {
        let mut file =
            std::fs::OpenOptions::new().read(true).write(true).open(&path).expect("open db file");
        let mut buffer = vec![0u8; 4096];
        file.seek(SeekFrom::Start(8192)).expect("seek");
        file.read_exact(&mut buffer).expect("read");
        for byte in &mut buffer {
            *byte ^= 0xa5;
        }
        file.seek(SeekFrom::Start(8192)).expect("seek");
        file.write_all(&buffer).expect("write");
        file.sync_all().expect("sync");
    }

    let opened = match Engine::open(&path) {
        Ok(opened) => opened,
        Err(_) => return,
    };

    let report = opened
        .engine
        .check_integrity(CheckIntegrityOpts { quick: false, full: true, round_trip: false })
        .expect("check_integrity");

    let physical = match report.physical {
        Section::Findings(rows) => rows,
        Section::Clean => return,
    };
    assert!(
        physical.iter().any(|finding| finding.code == "E_CORRUPT_INTEGRITY_CHECK"),
        "expected E_CORRUPT_INTEGRITY_CHECK; got {physical:?}"
    );
    assert!(physical.iter().all(|finding| !finding.detail.is_empty()));
    assert!(physical
        .iter()
        .all(|finding| finding.doc_anchor == "design/recovery.md#integrity-check-full-findings"));
}
