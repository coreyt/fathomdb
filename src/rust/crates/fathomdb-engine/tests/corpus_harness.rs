//! PR-5 corpus-harness TDD surface.
//!
//! Exercises [`corpus_harness::CorpusFixture`] on the always-available
//! synthetic-embedder path so the whole suite runs WITHOUT the
//! `default-embedder` feature and on corpus-present checkouts. The real
//! embedder is covered transitively (same cache + ingest code path; only
//! the inner `Embedder` differs) and gated by `corpus_harness_skip_paths`.

#[path = "support/corpus_harness.rs"]
mod corpus_harness;

use std::fs;

use corpus_harness::CorpusFixture;

/// Exercises every assert helper on a tiny fixture.
#[test]
fn corpus_harness_invariants() {
    // Isolated cache dir so the cold-miss assertion is independent of any
    // warm cache a prior run left in the default location.
    let cache = tempfile::tempdir().expect("tmp cache");
    let fx = CorpusFixture::small().with_cache_dir(cache.path());
    let Some((_dir, engine)) = fx.open_or_skip() else { return };

    let report = fx.ingest_into(&engine);
    assert!(report.nodes > 0, "ingest wrote 0 nodes");

    // First (cold) ingest is a cache miss that embeds live — and the miss
    // is LOUD: reason observable (and a CORPUS_CACHE_MISS line emitted).
    assert!(!report.embed_cache_hit, "expected a cold-cache miss on first ingest");
    assert!(report.embedded_live > 0, "cold ingest embedded nothing live");
    assert_eq!(
        report.cache_miss_reason.as_deref(),
        Some("cold"),
        "cold ingest must surface a loud cold-miss reason"
    );

    fx.assert_vec0_row_count_matches_ingest(&engine);
    fx.assert_fts_index_populated(&engine);

    let qs = fx.query_set(25, 0xC0FFEE);
    assert!(!qs.is_empty(), "query_set produced no queries");
    // Held-out queries must never be the verbatim source body.
    for q in &qs {
        assert_ne!(q.text.trim(), q.target_body.trim(), "query equals its source body");
    }
    fx.assert_search_returns_non_empty_for_each(&engine, &qs);
}

/// Two identical fixture constructions produce byte-identical cache files.
#[test]
fn corpus_harness_determinism() {
    let dir_a = tempfile::tempdir().expect("tmp a");
    let dir_b = tempfile::tempdir().expect("tmp b");

    let fx_a = CorpusFixture::small().with_cache_dir(dir_a.path());
    let fx_b = CorpusFixture::small().with_cache_dir(dir_b.path());
    if fx_a.skip_reason().is_some() {
        eprintln!("SKIP: {:?}", fx_a.skip_reason());
        return;
    }

    let (_da, ea) = fx_a.open_engine();
    let ra = fx_a.ingest_into(&ea);
    let (_db, eb) = fx_b.open_engine();
    let rb = fx_b.ingest_into(&eb);

    // Same identity + same subset => same cache filename.
    assert_eq!(
        ra.cache_path.file_name(),
        rb.cache_path.file_name(),
        "equivalent fixtures produced different cache keys"
    );

    let bytes_a = fs::read(&ra.cache_path).expect("read cache a");
    let bytes_b = fs::read(&rb.cache_path).expect("read cache b");
    assert_eq!(
        bytes_a,
        bytes_b,
        "cache blobs are not byte-identical across equivalent constructions ({} vs {} bytes)",
        bytes_a.len(),
        bytes_b.len()
    );
    assert!(!bytes_a.is_empty(), "cache blob is empty");

    // And the warm read-back is a genuine HIT (proves the cache is used).
    let fx_warm = CorpusFixture::small().with_cache_dir(dir_a.path());
    let (_dw, ew) = fx_warm.open_engine();
    let rw = fx_warm.ingest_into(&ew);
    assert!(rw.embed_cache_hit, "warm run did not hit the cache");
    assert_eq!(rw.embedded_live, 0, "warm run still embedded live");
    assert!(rw.cache_miss_reason.is_none(), "warm hit must not report a miss reason");
}

/// Cache invalidates on embedder-identity change AND on subset-content
/// change; and is genuinely reused when neither changes.
#[test]
fn corpus_harness_cache_invalidation() {
    let dir = tempfile::tempdir().expect("tmp");

    let fx_a = CorpusFixture::small().with_synthetic_revision("rev-A").with_cache_dir(dir.path());
    if fx_a.skip_reason().is_some() {
        eprintln!("SKIP: {:?}", fx_a.skip_reason());
        return;
    }
    let fx_b = CorpusFixture::small().with_synthetic_revision("rev-B").with_cache_dir(dir.path());

    let (_da, ea) = fx_a.open_engine();
    let ra = fx_a.ingest_into(&ea);
    let (_db, eb) = fx_b.open_engine();
    let rb = fx_b.ingest_into(&eb);

    // (1) Identity change -> different cache file, rebuilt not reused.
    assert_ne!(
        ra.cache_path, rb.cache_path,
        "identity change did not change the cache key — STALE-CACHE RISK"
    );
    assert!(!rb.embed_cache_hit, "identity change reused a stale cache");
    assert!(rb.embedded_live > 0, "identity change did not re-embed");
    assert!(rb.cache_miss_reason.is_some(), "identity-change miss must be loud (reason surfaced)");

    // Sanity: re-running rev-A from the same dir IS a hit (cache works).
    let fx_a2 = CorpusFixture::small().with_synthetic_revision("rev-A").with_cache_dir(dir.path());
    let (_da2, ea2) = fx_a2.open_engine();
    let ra2 = fx_a2.ingest_into(&ea2);
    assert!(ra2.embed_cache_hit, "rev-A warm re-run was not a cache hit");
    assert!(ra2.cache_miss_reason.is_none(), "rev-A warm re-run wrongly reported a miss");

    // (1b) A PARTIAL cache (sidecar deleted, blob present) is a loud miss,
    // never silently trusted or treated as cold.
    let meta = ra.cache_path.with_extension("meta.json");
    fs::remove_file(&meta).expect("remove sidecar");
    let fx_a3 = CorpusFixture::small().with_synthetic_revision("rev-A").with_cache_dir(dir.path());
    let (_da3, ea3) = fx_a3.open_engine();
    let ra3 = fx_a3.ingest_into(&ea3);
    assert!(!ra3.embed_cache_hit, "partial cache was wrongly treated as a hit");
    assert!(
        ra3.cache_miss_reason.as_deref().is_some_and(|r| r.contains("partial")),
        "partial-cache miss must be loud with a 'partial' reason, got {:?}",
        ra3.cache_miss_reason
    );

    // (1c) A PRESENT-but-STALE cache is a loud miss via read_cache's
    // manifest/identity validation — NOT a silent fallback. The key-based
    // cases above can't reach this branch (changing identity/manifest also
    // changes the filename), so we tamper a sidecar IN PLACE (filename
    // unchanged) to force the present-but-stale path.
    let dir_stale = tempfile::tempdir().expect("tmp stale");
    let fx_s1 =
        CorpusFixture::small().with_synthetic_revision("rev-S").with_cache_dir(dir_stale.path());
    let (_ds1, es1) = fx_s1.open_engine();
    let rs1 = fx_s1.ingest_into(&es1);
    assert!(!rs1.embed_cache_hit, "first rev-S ingest should be a cold write");

    let meta_path = rs1.cache_path.with_extension("meta.json");
    let mut meta: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_path).expect("read meta")).expect("parse meta");
    // Corrupt the manifest sha while leaving the filename (= cache key) intact.
    meta["doc_manifest_sha"] = serde_json::Value::String("0".repeat(64));
    fs::write(&meta_path, serde_json::to_vec_pretty(&meta).expect("ser meta")).expect("write meta");

    let fx_s2 =
        CorpusFixture::small().with_synthetic_revision("rev-S").with_cache_dir(dir_stale.path());
    let (_ds2, es2) = fx_s2.open_engine();
    let rs2 = fx_s2.ingest_into(&es2);
    assert!(!rs2.embed_cache_hit, "present-but-stale cache was wrongly trusted");
    assert!(
        rs2.cache_miss_reason.as_deref().is_some_and(|r| r.contains("mismatch")),
        "stale-cache (manifest) miss must be loud with a mismatch reason, got {:?}",
        rs2.cache_miss_reason
    );

    // (1d) The sibling identity-mismatch branch: tamper the sidecar's
    // identity (manifest left correct) so the identity check — not the
    // manifest check — is what fires. Locks that branch independently.
    let dir_id = tempfile::tempdir().expect("tmp id");
    let fx_i1 =
        CorpusFixture::small().with_synthetic_revision("rev-I").with_cache_dir(dir_id.path());
    let (_di1, ei1) = fx_i1.open_engine();
    let ri1 = fx_i1.ingest_into(&ei1);
    let meta_i = ri1.cache_path.with_extension("meta.json");
    let mut mi: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_i).expect("read meta")).expect("parse meta");
    mi["identity"]["revision"] = serde_json::Value::String("rev-TAMPERED".to_string());
    fs::write(&meta_i, serde_json::to_vec_pretty(&mi).expect("ser meta")).expect("write meta");

    let fx_i2 =
        CorpusFixture::small().with_synthetic_revision("rev-I").with_cache_dir(dir_id.path());
    let (_di2, ei2) = fx_i2.open_engine();
    let ri2 = fx_i2.ingest_into(&ei2);
    assert!(!ri2.embed_cache_hit, "identity-tampered cache was wrongly trusted");
    assert!(
        ri2.cache_miss_reason.as_deref().is_some_and(|r| r.contains("identity")),
        "stale-cache (identity) miss must be loud with an identity reason, got {:?}",
        ri2.cache_miss_reason
    );

    // (2) Subset-content change -> different cache file.
    let dir2 = tempfile::tempdir().expect("tmp2");
    let all = fx_a.docs().to_vec();
    assert!(all.len() >= 2, "need >=2 docs to drop one");
    let dropped = all[1..].to_vec();

    let fx_full = CorpusFixture::from_docs("sub", all).with_cache_dir(dir2.path());
    let fx_drop = CorpusFixture::from_docs("sub", dropped).with_cache_dir(dir2.path());
    let (_df, ef) = fx_full.open_engine();
    let rf = fx_full.ingest_into(&ef);
    let (_dd, ed) = fx_drop.open_engine();
    let rd = fx_drop.ingest_into(&ed);
    assert_ne!(
        rf.cache_path, rd.cache_path,
        "subset-content change did not change the cache key — STALE-CACHE RISK"
    );
    assert!(!rd.embed_cache_hit, "subset-content change reused a stale cache");
}

/// SKIP contracts: an empty doc set and the `default-embedder`-off real
/// path both surface a skip reason rather than panicking.
#[test]
fn corpus_harness_skip_paths() {
    // Empty/absent subset -> skip + None from open_or_skip.
    let empty = CorpusFixture::from_docs("empty", Vec::new());
    assert!(empty.skip_reason().is_some(), "empty fixture must report a skip reason");
    assert!(empty.open_or_skip().is_none(), "empty fixture must skip open");

    // Real-embedder availability is feature-gated.
    let real = CorpusFixture::small().with_real_embedder();
    #[cfg(not(feature = "default-embedder"))]
    {
        assert!(
            real.skip_reason().is_some(),
            "with_real_embedder must SKIP when default-embedder is off"
        );
        assert!(real.open_or_skip().is_none(), "real path must skip without the feature");
    }
    #[cfg(feature = "default-embedder")]
    {
        // With the feature on (and corpus present), the real path is
        // available — no skip for embedder reasons.
        if !real.docs().is_empty() {
            assert!(
                real.skip_reason().is_none(),
                "real embedder unexpectedly unavailable with feature on: {:?}",
                real.skip_reason()
            );
        }
    }
}

/// End-to-end real-embedder smoke: drives the shipped BGE model through the
/// SAME cache + ingest path the synthetic tests cover, proving a cold build
/// then a warm on-disk hit, plus live search. Gated behind BOTH the
/// `default-embedder` feature AND `AGENT_LONG=1` (real candle weights;
/// multi-second), so it never runs in the inner loop or feature-off CI.
/// The cache/ingest logic is identical to the synthetic path — only the
/// inner `Embedder` differs — so this is a belt-and-braces check, not the
/// primary coverage.
#[cfg(feature = "default-embedder")]
#[test]
fn corpus_harness_real_embedder_smoke() {
    if std::env::var("AGENT_LONG").is_err() {
        eprintln!("SKIP: corpus_harness_real_embedder_smoke requires AGENT_LONG=1");
        return;
    }
    let cache = tempfile::tempdir().expect("tmp cache");
    let fx = CorpusFixture::small().with_real_embedder().with_cache_dir(cache.path());
    let Some((_dir, engine)) = fx.open_or_skip() else { return };

    // Cold build: real embeds, loud cold miss, cache blob written.
    let cold = fx.ingest_into(&engine);
    assert!(cold.nodes > 0, "real ingest wrote 0 nodes");
    assert!(!cold.embed_cache_hit, "first real ingest should be a cold miss");
    assert_eq!(cold.cache_miss_reason.as_deref(), Some("cold"), "cold real miss not loud");
    assert!(cold.embedded_live > 0, "cold real ingest embedded nothing live");
    assert!(cold.cache_path.exists(), "real cache blob not written to disk");
    fx.assert_vec0_row_count_matches_ingest(&engine);
    fx.assert_fts_index_populated(&engine);

    // Warm reopen on the SAME cache dir: a genuine hit, zero live embeds —
    // i.e. the 7,667-doc inner-loop killer is avoided after the first run.
    let fx2 = CorpusFixture::small().with_real_embedder().with_cache_dir(cache.path());
    let (_dir2, engine2) = fx2.open_engine();
    let warm = fx2.ingest_into(&engine2);
    assert!(warm.embed_cache_hit, "warm real reopen did not hit the cache");
    assert_eq!(warm.embedded_live, 0, "warm real reopen still embedded bodies live");
    assert!(warm.cache_miss_reason.is_none(), "warm real reopen reported a miss");

    // Real search still returns results for held-out queries.
    let qs = fx2.query_set(10, 0xBEEF);
    fx2.assert_search_returns_non_empty_for_each(&engine2, &qs);
}
