//! Asserts the `fathomdb` facade re-exports the typed surface owned by
//! `dev/interfaces/rust.md`.

#[test]
fn re_exports_compile() {
    let _ = std::any::type_name::<fathomdb::Engine>();
    let _ = std::any::type_name::<fathomdb::OpenedEngine>();
    let _ = std::any::type_name::<fathomdb::OpenReport>();
    let _ = std::any::type_name::<fathomdb::WriteReceipt>();
    let _ = std::any::type_name::<fathomdb::SearchResult>();
    let _ = std::any::type_name::<fathomdb::PreparedWrite>();
    let _ = std::any::type_name::<fathomdb::EngineError>();
    let _ = std::any::type_name::<fathomdb::EngineOpenError>();

    let _ = std::any::type_name::<fathomdb::CorruptionDetail>();
    let _ = std::any::type_name::<fathomdb::CorruptionKind>();
    let _ = std::any::type_name::<fathomdb::CorruptionLocator>();
    let _ = std::any::type_name::<fathomdb::OpenStage>();
    let _ = std::any::type_name::<fathomdb::RecoveryHint>();

    let _ = std::any::type_name::<fathomdb::SoftFallback>();
    let _ = std::any::type_name::<fathomdb::SoftFallbackBranch>();
    let _ = std::any::type_name::<fathomdb::CounterSnapshot>();
    let _ = std::any::type_name::<fathomdb::Subscription>();

    let _ = std::any::type_name::<fathomdb::CheckIntegrityOpts>();
    let _ = std::any::type_name::<fathomdb::ExciseReport>();
    let _ = std::any::type_name::<fathomdb::IntegrityReport>();
    let _ = std::any::type_name::<fathomdb::RebuildKind>();
    let _ = std::any::type_name::<fathomdb::RebuildReport>();
    let _ = std::any::type_name::<fathomdb::SafeExportArtifact>();
    let _ = std::any::type_name::<fathomdb::TraceEvent>();
    let _ = std::any::type_name::<fathomdb::TraceReport>();
}
