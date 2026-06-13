//! QD ELPS `result`-envelope deserializer + fix-33 edge↔node linkage regression.
//!
//! Verifies that the 8 canonical wire-byte `result` envelopes from
//! `~/projects/memex/dev/elps/QD-ENVELOPE-SAMPLE.md` deserialize through
//! FathomDB's actual ingest parse path and survive a real `ingest_with_extractor`
//! run against a stub harness (no network).
//!
//! These envelopes are the first *contract-faithful* inputs run end-to-end:
//! entities carry their real `type` (Person/Organization/…) and edges reference
//! endpoints BY NAME with NO `from_type`/`to_type` (the protocol has none). That
//! shape is exactly what surfaced the fix-33 [P1] orphaned-edge bug — so
//! `eight_envelopes_ingest_via_stub` additionally asserts that every active edge
//! endpoint resolves to an active node (RED before fix-33, where the edge reader
//! defaulted the endpoint kind to "entity" while nodes used their real type).
//!
//! NOTE: FathomDB's engine parses the result envelope as an untyped
//! `serde_json::Value` (see `ingest_with_extractor_inner` in lib.rs), NOT typed
//! serde structs. `warnings`, `source_span`, and `synthesized` are never read by
//! the engine. These tests therefore (a) exercise the real serde_json parse +
//! field-extraction the engine uses, and (b) assert the contract-relevant fields
//! parse from the canonical bytes.

use fathomdb_engine::{Engine, ExtractDocument};
use rusqlite::Connection;
use serde_json::Value;
use std::io::Write;
use tempfile::TempDir;

use fathomdb_schema::SQLITE_SUFFIX;

// (doc_id, body, exact canonical `result` JSON line, expected entities, expected edges)
struct Case {
    id: &'static str,
    body: &'static str,
    result: &'static str,
    exp_entities: u64,
    exp_edges: u64,
}

const CASES: &[Case] = &[
    Case {
        id: "d1",
        body: "Alice joined Acme Corp in 2021.",
        result: r#"{"edges":[{"body":"Alice joined Acme Corp in 2021.","confidence":0.92,"from_entity":"Alice","relation":"works_at","source_doc_id":"d1","source_span":[0,5],"t_invalid":null,"t_valid":"2021-06-01T00:00:00Z","to_entity":"Acme Corp"}],"entities":[{"aliases":[],"name":"Acme Corp","synthesized":false,"type":"Organization"},{"aliases":[],"name":"Alice","synthesized":false,"type":"Person"}],"protocol":"fathomdb.extract.v1","request_id":"qd-1","type":"result","warnings":[]}"#,
        exp_entities: 2,
        exp_edges: 1,
    },
    Case {
        id: "d2",
        body: "Bob now leads the Platform team.",
        result: r#"{"edges":[{"body":"Bob now leads the Platform team.","confidence":0.88,"from_entity":"Bob","relation":"leads","source_doc_id":"d2","source_span":[0,3],"t_invalid":null,"t_valid":"2024-02-01T00:00:00Z","to_entity":"Platform team"}],"entities":[{"aliases":[],"name":"Bob","synthesized":false,"type":"Person"},{"aliases":[],"name":"Platform team","synthesized":false,"type":"Team"}],"protocol":"fathomdb.extract.v1","request_id":"qd-2","type":"result","warnings":[{"detail":null,"dropped":null,"kept":null,"kind":"supersedes","prior_body":"Bob is a member of the Platform team.","raw_t_valid":null,"source_doc_id":"d2","substituted_t_valid":null,"supersedes_hint":"Bob leads Platform team"}]}"#,
        exp_entities: 2,
        exp_edges: 1,
    },
    Case {
        id: "d3",
        body: "Carol introduced Dave to Eve at the Berlin summit.",
        result: r#"{"edges":[{"body":"Carol introduced Dave to Eve at the Berlin summit.","confidence":0.81,"from_entity":"Carol","relation":"introduced","source_doc_id":"d3","source_span":[0,5],"t_invalid":null,"t_valid":"2023-09-15T00:00:00Z","to_entity":"Dave"},{"body":"Carol introduced Dave to Eve at the Berlin summit.","confidence":0.77,"from_entity":"Dave","relation":"met","source_doc_id":"d3","source_span":[17,21],"t_invalid":null,"t_valid":"2023-09-15T00:00:00Z","to_entity":"Eve"}],"entities":[{"aliases":[],"name":"Carol","synthesized":false,"type":"Person"},{"aliases":[],"name":"Dave","synthesized":false,"type":"Person"},{"aliases":[],"name":"Eve","synthesized":false,"type":"Person"}],"protocol":"fathomdb.extract.v1","request_id":"qd-3","type":"result","warnings":[]}"#,
        exp_entities: 3,
        exp_edges: 2,
    },
    Case {
        id: "d4",
        body: "The weather was pleasant and the coffee was warm.",
        result: r#"{"edges":[],"entities":[],"protocol":"fathomdb.extract.v1","request_id":"qd-4","type":"result","warnings":[{"detail":null,"dropped":null,"kept":null,"kind":"no_facts","prior_body":null,"raw_t_valid":null,"source_doc_id":"d4","substituted_t_valid":null,"supersedes_hint":null}]}"#,
        exp_entities: 0,
        exp_edges: 0,
    },
    Case {
        id: "d5",
        body: "Café 🚀 launch: Renée shipped Zürich pilot.",
        result: r#"{"edges":[{"body":"Café 🚀 launch: Renée shipped Zürich pilot.","confidence":0.84,"from_entity":"Renée","relation":"shipped","source_doc_id":"d5","source_span":[19,25],"t_invalid":null,"t_valid":"2022-11-03T00:00:00Z","to_entity":"Zürich pilot"}],"entities":[{"aliases":[],"name":"Renée","synthesized":false,"type":"Person"},{"aliases":[],"name":"Zürich pilot","synthesized":false,"type":"Project"}],"protocol":"fathomdb.extract.v1","request_id":"qd-5","type":"result","warnings":[]}"#,
        exp_entities: 2,
        exp_edges: 1,
    },
    Case {
        id: "d6",
        body: "Frank reports to Grace.",
        result: r#"{"edges":[{"body":"Frank reports to Grace.","confidence":0.79,"from_entity":"Frank","relation":"reports_to","source_doc_id":"d6","source_span":[0,5],"t_invalid":null,"t_valid":"2024-07-01T00:00:00Z","to_entity":"Grace"}],"entities":[{"aliases":[],"name":"Frank","synthesized":false,"type":"Person"},{"aliases":[],"name":"Grace","synthesized":true,"type":"unknown"}],"protocol":"fathomdb.extract.v1","request_id":"qd-6","type":"result","warnings":[]}"#,
        exp_entities: 2,
        exp_edges: 1,
    },
    Case {
        id: "d7",
        body: "Heidi acquired the Northwind contract.",
        result: r#"{"edges":[{"body":"Heidi acquired the Northwind contract.","confidence":0.83,"from_entity":"Heidi","relation":"acquired","source_doc_id":"d7","source_span":[0,5],"t_invalid":null,"t_valid":"2025-03-20T09:30:00Z","to_entity":"Northwind contract"}],"entities":[{"aliases":[],"name":"Northwind contract","synthesized":false,"type":"Contract"},{"aliases":[],"name":"Heidi","synthesized":false,"type":"Person"}],"protocol":"fathomdb.extract.v1","request_id":"qd-7","type":"result","warnings":[{"detail":null,"dropped":null,"kept":null,"kind":"temporal_fallback","prior_body":null,"raw_t_valid":"sometime last year","source_doc_id":"d7","substituted_t_valid":"2025-03-20T09:30:00Z","supersedes_hint":null}]}"#,
        exp_entities: 2,
        exp_edges: 1,
    },
    Case {
        id: "d8",
        body: "Ivan signed four deals across the quarter.",
        result: r#"{"edges":[{"body":"Ivan signed Deal A.","confidence":0.95,"from_entity":"Ivan","relation":"signed","source_doc_id":"d8","source_span":null,"t_invalid":null,"t_valid":"2025-01-10T00:00:00Z","to_entity":"Deal A"},{"body":"Ivan signed Deal B.","confidence":0.9,"from_entity":"Ivan","relation":"signed","source_doc_id":"d8","source_span":null,"t_invalid":null,"t_valid":"2025-02-10T00:00:00Z","to_entity":"Deal B"}],"entities":[{"aliases":[],"name":"Deal A","synthesized":false,"type":"Deal"},{"aliases":[],"name":"Deal B","synthesized":false,"type":"Deal"},{"aliases":[],"name":"Deal C","synthesized":false,"type":"Deal"},{"aliases":[],"name":"Deal D","synthesized":false,"type":"Deal"},{"aliases":[],"name":"Ivan","synthesized":false,"type":"Person"}],"protocol":"fathomdb.extract.v1","request_id":"qd-8","type":"result","warnings":[{"detail":null,"dropped":2,"kept":2,"kind":"capped","prior_body":null,"raw_t_valid":null,"source_doc_id":"d8","substituted_t_valid":null,"supersedes_hint":null}]}"#,
        exp_entities: 5,
        exp_edges: 2,
    },
];

const KNOWN_WARNING_KINDS: &[&str] =
    &["supersedes", "doc_dropped", "no_facts", "validation_failed", "temporal_fallback", "capped"];

// ---------------------------------------------------------------------------
// 1. The exact canonical bytes deserialize via the same serde_json call the
//    engine uses, and the contract-relevant fields parse.
// ---------------------------------------------------------------------------
#[test]
fn eight_envelopes_static_deserialize() {
    for c in CASES {
        // This is exactly what ingest_with_extractor_inner does: from_str::<Value>.
        let v: Value = serde_json::from_str(c.result)
            .unwrap_or_else(|e| panic!("case {} must deserialize: {e}", c.id));

        assert_eq!(v.get("type").and_then(Value::as_str), Some("result"), "case {}", c.id);
        assert_eq!(
            v.get("protocol").and_then(Value::as_str),
            Some("fathomdb.extract.v1"),
            "case {}",
            c.id
        );

        let entities = v.get("entities").and_then(Value::as_array).expect("entities array");
        let edges = v.get("edges").and_then(Value::as_array).expect("edges array");
        assert_eq!(entities.len() as u64, c.exp_entities, "case {} entity count", c.id);
        assert_eq!(edges.len() as u64, c.exp_edges, "case {} edge count", c.id);

        // Flat-Warning shape: every warning carries all seven fields with nulls.
        let warnings = v.get("warnings").and_then(Value::as_array).expect("warnings array");
        for w in warnings {
            let kind = w.get("kind").and_then(Value::as_str).expect("warning.kind");
            assert!(KNOWN_WARNING_KINDS.contains(&kind), "case {} unknown kind {kind}", c.id);
            // All seven flat fields present (value may be null).
            for f in [
                "detail",
                "dropped",
                "kept",
                "kind",
                "prior_body",
                "raw_t_valid",
                "substituted_t_valid",
                "supersedes_hint",
            ] {
                assert!(w.get(f).is_some(), "case {} warning missing flat field {f}", c.id);
            }
        }
    }

    // Case 2 — supersedes carries hint + prior_body.
    let v2: Value = serde_json::from_str(CASES[1].result).unwrap();
    let w2 = &v2["warnings"][0];
    assert_eq!(w2["kind"], "supersedes");
    assert_eq!(w2["supersedes_hint"], "Bob leads Platform team");
    assert_eq!(w2["prior_body"], "Bob is a member of the Platform team.");

    // Case 4 — no_facts, empty edges + entities.
    let v4: Value = serde_json::from_str(CASES[3].result).unwrap();
    assert_eq!(v4["warnings"][0]["kind"], "no_facts");
    assert_eq!(v4["warnings"][0]["source_doc_id"], "d4");

    // Case 6 — synthesized dangling endpoint Grace{type:unknown,synthesized:true}.
    let v6: Value = serde_json::from_str(CASES[5].result).unwrap();
    let ents6 = v6["entities"].as_array().unwrap();
    let grace = ents6.iter().find(|e| e["name"] == "Grace").unwrap();
    assert_eq!(grace["synthesized"], serde_json::json!(true));
    assert_eq!(grace["type"], "unknown");
    let frank = ents6.iter().find(|e| e["name"] == "Frank").unwrap();
    assert_eq!(frank["synthesized"], serde_json::json!(false));

    // Case 7 — temporal_fallback carries substituted + raw.
    let v7: Value = serde_json::from_str(CASES[6].result).unwrap();
    let w7 = &v7["warnings"][0];
    assert_eq!(w7["kind"], "temporal_fallback");
    assert_eq!(w7["substituted_t_valid"], "2025-03-20T09:30:00Z");
    assert_eq!(w7["raw_t_valid"], "sometime last year");

    // Case 8 — capped carries kept + dropped.
    let v8: Value = serde_json::from_str(CASES[7].result).unwrap();
    let w8 = &v8["warnings"][0];
    assert_eq!(w8["kind"], "capped");
    assert_eq!(w8["kept"], 2);
    assert_eq!(w8["dropped"], 2);
}

// ---------------------------------------------------------------------------
// 2. Case 5 — UTF-8 byte source_span independently verified against the body.
// ---------------------------------------------------------------------------
#[test]
fn case5_byte_span_is_renee() {
    let v: Value = serde_json::from_str(CASES[4].result).unwrap();
    let edge = &v["edges"][0];
    let span = edge["source_span"].as_array().unwrap();
    let start = span[0].as_u64().unwrap() as usize;
    let end = span[1].as_u64().unwrap() as usize;
    assert_eq!((start, end), (19, 25), "claimed byte span");

    let body = edge["body"].as_str().unwrap();
    let slice = &body.as_bytes()[start..end];
    assert_eq!(slice, "Renée".as_bytes(), "byte slice must equal 'Renée'");
    assert_eq!(std::str::from_utf8(slice).unwrap(), "Renée");
    // é is 2 bytes (0xC3 0xA9): "Renée" = 6 bytes.
    assert_eq!(slice.len(), 6);
    assert_eq!("Renée".as_bytes(), &[b'R', b'e', b'n', 0xC3, 0xA9, b'e']);
}

// ---------------------------------------------------------------------------
// 3. Real end-to-end: each envelope flows through ingest_with_extractor via a
//    stub harness that returns the EXACT sample entities/edges/warnings.
// ---------------------------------------------------------------------------
fn write_stub(dir: &TempDir) -> std::path::PathBuf {
    let mut src = String::from("import json,sys\nRESULTS={\n");
    for c in CASES {
        src.push('"');
        src.push_str(c.id);
        src.push_str("\": '");
        src.push_str(c.result); // no result line contains a single quote
        src.push_str("',\n");
    }
    src.push_str("}\n");
    src.push_str(
        r#"
for line in sys.stdin:
    line=line.strip()
    if not line:
        continue
    msg=json.loads(line)
    t=msg.get("type")
    if t=="hello":
        print(json.dumps({"protocol":"fathomdb.extract.v1","type":"ready","schema_version":1,"model":"memex-extract-v1","max_docs_per_request":1}),flush=True)
    elif t=="extract":
        docs=msg.get("documents",[])
        did=docs[0]["source_doc_id"]
        res=json.loads(RESULTS[did])
        res["request_id"]=msg.get("request_id")
        print(json.dumps(res,ensure_ascii=False),flush=True)
"#,
    );
    let path = dir.path().join("qd_stub.py");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(src.as_bytes()).unwrap();
    path
}

#[test]
fn eight_envelopes_ingest_via_stub() {
    let dir = TempDir::new().unwrap();
    let stub = write_stub(&dir);
    let stub_str = stub.to_string_lossy().to_string();

    for c in CASES {
        let db = dir.path().join(format!("qd_{}{}", c.id, SQLITE_SUFFIX));
        let opened = Engine::open_without_embedder_for_test(&db).expect("open");

        let cmd = ["python3".to_string(), stub_str.clone()];
        let cmd_refs: Vec<&str> = cmd.iter().map(String::as_str).collect();
        let docs =
            vec![ExtractDocument { source_doc_id: c.id.to_string(), body: c.body.to_string() }];

        let receipt = opened
            .engine
            .ingest_with_extractor(&cmd_refs, &docs)
            .unwrap_or_else(|e| panic!("case {} must ingest the canonical bytes: {e:?}", c.id));

        assert_eq!(receipt.docs_processed, 1, "case {}", c.id);
        assert_eq!(receipt.nodes_written, c.exp_entities, "case {} nodes_written (entities)", c.id);
        assert_eq!(receipt.edges_written, c.exp_edges, "case {} edges_written (edges)", c.id);

        // fix-33 [P1]: every active edge endpoint must resolve to an active node.
        // The QD envelopes carry no edge endpoint types; the engine resolves
        // `from_entity`/`to_entity` via the result's `entities[]` (by name/alias)
        // → the entity's canonical (name, type). Before fix-33 the reader defaulted
        // the endpoint kind to "entity" while nodes used their real type, so every
        // endpoint here (e.g. "Alice"→person, synthesized "Grace"→unknown) was
        // orphaned and this count was non-zero.
        let conn = Connection::open(&db).unwrap();
        let orphaned: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM canonical_edges e
                 WHERE e.superseded_at IS NULL
                   AND ( NOT EXISTS (SELECT 1 FROM canonical_nodes n
                                     WHERE n.logical_id = e.from_id AND n.superseded_at IS NULL)
                      OR NOT EXISTS (SELECT 1 FROM canonical_nodes n
                                     WHERE n.logical_id = e.to_id AND n.superseded_at IS NULL) )",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            orphaned, 0,
            "case {}: every extracted edge endpoint must link to an active node (fix-33)",
            c.id
        );

        println!(
            "case {}: nodes={} edges={} orphaned=0 OK",
            c.id, receipt.nodes_written, receipt.edges_written
        );
    }
}
