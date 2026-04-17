//! JSON-based FFI surface for admin-plane operations that need richer
//! serde shapes than a flat string list.
//!
//! Pack P7.6a introduces this module so the Python and TypeScript SDKs
//! can register recursive FTS property schemas via the engine's
//! [`Engine::register_fts_property_schema_with_entries`] entry point.
//! The types are plain serde structures — no pyo3 / napi dependencies —
//! so translation can be unit- and integration-tested directly via
//! `cargo test` without linking against libpython or libnode, mirroring
//! the pattern established by [`crate::search_ffi`].

use serde::{Deserialize, Serialize};

use crate::{
    Engine, EngineError, FtsPropertyPathMode, FtsPropertyPathSpec, FtsPropertySchemaRecord,
};
use fathomdb_engine::{FtsProfile, ProjectionImpact, VecProfile};

/// Extraction mode for a single registered FTS property path, serialized
/// as `"scalar"` or `"recursive"` on the wire.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PyPropertyPathMode {
    /// Treat the path as a scalar — matches legacy pre-Phase-4 behaviour.
    Scalar,
    /// Recursively walk every scalar leaf rooted at the path.
    Recursive,
}

impl From<PyPropertyPathMode> for FtsPropertyPathMode {
    fn from(value: PyPropertyPathMode) -> Self {
        match value {
            PyPropertyPathMode::Scalar => Self::Scalar,
            PyPropertyPathMode::Recursive => Self::Recursive,
        }
    }
}

/// A single registered property-FTS path with its extraction mode.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PyPropertyPathSpec {
    /// JSON path to the property (must start with `$.`).
    pub path: String,
    /// Whether to treat this path as a scalar or recursively walk it.
    pub mode: PyPropertyPathMode,
    /// Optional BM25 weight multiplier for this path.
    #[serde(default)]
    pub weight: Option<f32>,
}

impl From<PyPropertyPathSpec> for FtsPropertyPathSpec {
    fn from(value: PyPropertyPathSpec) -> Self {
        let base = match value.mode {
            PyPropertyPathMode::Recursive => FtsPropertyPathSpec::recursive(value.path),
            PyPropertyPathMode::Scalar => FtsPropertyPathSpec::scalar(value.path),
        };
        match value.weight {
            Some(w) => base.with_weight(w),
            None => base,
        }
    }
}

/// JSON envelope for [`register_fts_property_schema_with_entries_json`].
///
/// Wire shape:
/// ```json
/// {
///   "kind": "KnowledgeItem",
///   "entries": [
///     {"path": "$.title", "mode": "scalar"},
///     {"path": "$.payload", "mode": "recursive"}
///   ],
///   "separator": " ",
///   "exclude_paths": []
/// }
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PyRegisterFtsPropertySchemaRequest {
    /// Node kind to register.
    pub kind: String,
    /// Ordered list of path specs.
    pub entries: Vec<PyPropertyPathSpec>,
    /// Concatenation separator. Use a single space when unspecified by
    /// the caller.
    #[serde(default = "default_separator")]
    pub separator: String,
    /// JSON paths to exclude from recursive walks.
    #[serde(default)]
    pub exclude_paths: Vec<String>,
}

fn default_separator() -> String {
    " ".to_owned()
}

/// Error produced by the admin FFI JSON translation path.
#[derive(Debug)]
pub enum AdminFfiError {
    /// The request JSON could not be deserialized.
    Parse(serde_json::Error),
    /// Engine rejected the request.
    Engine(EngineError),
    /// Response serialization failed.
    Serialize(serde_json::Error),
}

impl std::fmt::Display for AdminFfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "admin request JSON parse error: {e}"),
            Self::Engine(e) => write!(f, "admin operation error: {e}"),
            Self::Serialize(e) => write!(f, "admin response serialize error: {e}"),
        }
    }
}

impl std::error::Error for AdminFfiError {}

/// Register (or update) an FTS property projection schema whose entries
/// may include recursive-mode paths. The `request_json` payload must
/// match [`PyRegisterFtsPropertySchemaRequest`].
///
/// Returns the serialized [`FtsPropertySchemaRecord`] on success.
///
/// # Errors
/// Returns [`AdminFfiError`] on JSON parse, engine execution, or
/// response serialization failure.
pub fn register_fts_property_schema_with_entries_json(
    engine: &Engine,
    request_json: &str,
) -> Result<String, AdminFfiError> {
    let request: PyRegisterFtsPropertySchemaRequest =
        serde_json::from_str(request_json).map_err(AdminFfiError::Parse)?;
    let entries: Vec<FtsPropertyPathSpec> = request.entries.into_iter().map(Into::into).collect();
    let record: FtsPropertySchemaRecord = engine
        .register_fts_property_schema_with_entries(
            &request.kind,
            &entries,
            Some(request.separator.as_str()),
            &request.exclude_paths,
        )
        .map_err(AdminFfiError::Engine)?;
    serde_json::to_string(&record).map_err(AdminFfiError::Serialize)
}

/// Request envelope for [`set_fts_profile_json`].
#[derive(Debug, Deserialize)]
struct SetFtsProfileRequest {
    kind: String,
    tokenizer: String,
}

/// Set the FTS tokenizer profile for a node kind.
///
/// `request_json` must be `{"kind":"K","tokenizer":"T"}`.
///
/// Returns the serialized [`FtsProfile`] on success.
///
/// # Errors
/// Returns [`AdminFfiError`] on JSON parse, engine execution, or
/// response serialization failure.
pub fn set_fts_profile_json(engine: &Engine, request_json: &str) -> Result<String, AdminFfiError> {
    let request: SetFtsProfileRequest =
        serde_json::from_str(request_json).map_err(AdminFfiError::Parse)?;
    let profile: FtsProfile = engine
        .admin()
        .service()
        .set_fts_profile(&request.kind, &request.tokenizer)
        .map_err(AdminFfiError::Engine)?;
    serde_json::to_string(&profile).map_err(AdminFfiError::Serialize)
}

/// Retrieve the FTS tokenizer profile for a node kind.
///
/// Returns `"null"` if no profile has been set for `kind`.
///
/// # Errors
/// Returns [`AdminFfiError`] on engine execution or response serialization failure.
pub fn get_fts_profile_json(engine: &Engine, kind: &str) -> Result<String, AdminFfiError> {
    let profile: Option<FtsProfile> = engine
        .admin()
        .service()
        .get_fts_profile(kind)
        .map_err(AdminFfiError::Engine)?;
    serde_json::to_string(&profile).map_err(AdminFfiError::Serialize)
}

/// Request envelope for [`set_vec_profile_json`].
///
/// Used purely for typed validation of incoming JSON; fields are read by
/// `serde_json` deserialization but not accessed in code after that.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SetVecProfileRequest {
    model_identity: String,
    #[serde(default)]
    model_version: Option<String>,
    dimensions: u32,
    #[serde(default)]
    normalization_policy: Option<String>,
}

/// Set (or update) the global vector embedding profile.
///
/// `request_json` must be valid JSON with at least `model_identity` and
/// `dimensions` fields, e.g.
/// `{"model_identity":"...","model_version":"...","dimensions":384,"normalization_policy":"l2"}`.
///
/// Returns the serialized [`VecProfile`] on success.
///
/// # Errors
/// Returns [`AdminFfiError`] on JSON parse, engine execution, or
/// response serialization failure.
pub fn set_vec_profile_json(engine: &Engine, request_json: &str) -> Result<String, AdminFfiError> {
    // Typed validation: ensures model_identity and dimensions are present.
    // Gives a clear parse error if required fields are missing, rather than
    // storing a profile with NULL fields and failing cryptically on get_vec_profile.
    let _validated: SetVecProfileRequest =
        serde_json::from_str(request_json).map_err(AdminFfiError::Parse)?;
    let profile: VecProfile = engine
        .admin()
        .service()
        .set_vec_profile(request_json)
        .map_err(AdminFfiError::Engine)?;
    serde_json::to_string(&profile).map_err(AdminFfiError::Serialize)
}

/// Retrieve the vector embedding profile for a specific node kind.
///
/// Returns `"null"` if no profile has been persisted for this kind yet.
///
/// # Errors
/// Returns [`AdminFfiError`] on engine execution or response serialization failure.
pub fn get_vec_profile_json(engine: &Engine, kind: &str) -> Result<String, AdminFfiError> {
    let profile: Option<VecProfile> = engine
        .admin()
        .service()
        .get_vec_profile(kind)
        .map_err(AdminFfiError::Engine)?;
    serde_json::to_string(&profile).map_err(AdminFfiError::Serialize)
}

/// Estimate the cost of rebuilding a projection for a given node kind and facet.
///
/// `facet` must be `"fts"` or `"vec"`.
///
/// Returns the serialized [`ProjectionImpact`] on success.
///
/// # Errors
/// Returns [`AdminFfiError`] on engine execution or response serialization failure.
pub fn preview_projection_impact_json(
    engine: &Engine,
    kind: &str,
    facet: &str,
) -> Result<String, AdminFfiError> {
    let impact: ProjectionImpact = engine
        .admin()
        .service()
        .preview_projection_impact(kind, facet)
        .map_err(AdminFfiError::Engine)?;
    serde_json::to_string(&impact).map_err(AdminFfiError::Serialize)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{PyPropertyPathMode, PyPropertyPathSpec, PyRegisterFtsPropertySchemaRequest};
    use crate::FtsPropertyPathSpec;

    #[test]
    fn property_path_mode_snake_case_wire_form() {
        let json = serde_json::to_string(&PyPropertyPathMode::Scalar).expect("serialize");
        assert_eq!(json, "\"scalar\"");
        let json = serde_json::to_string(&PyPropertyPathMode::Recursive).expect("serialize");
        assert_eq!(json, "\"recursive\"");
    }

    #[test]
    fn property_path_spec_roundtrip() {
        let spec = PyPropertyPathSpec {
            path: "$.payload".to_owned(),
            mode: PyPropertyPathMode::Recursive,
            weight: None,
        };
        let json = serde_json::to_string(&spec).expect("serialize");
        let parsed: PyPropertyPathSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(spec, parsed);
    }

    #[test]
    fn register_request_defaults_separator_and_exclude_paths() {
        let request: PyRegisterFtsPropertySchemaRequest =
            serde_json::from_str(r#"{"kind":"K","entries":[{"path":"$.title","mode":"scalar"}]}"#)
                .expect("parse");
        assert_eq!(request.kind, "K");
        assert_eq!(request.separator, " ");
        assert!(request.exclude_paths.is_empty());
        assert_eq!(request.entries.len(), 1);
    }

    #[test]
    fn weight_round_trips_through_py_property_path_spec() {
        let json = r#"{"path": "$.title", "mode": "scalar", "weight": 10.0}"#;
        let spec: PyPropertyPathSpec = serde_json::from_str(json).expect("deserialize");
        assert_eq!(spec.weight, Some(10.0_f32));
        let fts_spec: FtsPropertyPathSpec = spec.into();
        let _ = fts_spec; // conversion must not panic
    }

    #[test]
    fn weight_absent_defaults_to_none() {
        let json = r#"{"path": "$.body", "mode": "scalar"}"#;
        let spec: PyPropertyPathSpec = serde_json::from_str(json).expect("deserialize");
        assert_eq!(spec.weight, None);
    }

    #[test]
    fn register_request_roundtrip_recursive_entry() {
        let request = PyRegisterFtsPropertySchemaRequest {
            kind: "KnowledgeItem".to_owned(),
            entries: vec![
                PyPropertyPathSpec {
                    path: "$.title".to_owned(),
                    mode: PyPropertyPathMode::Scalar,
                    weight: None,
                },
                PyPropertyPathSpec {
                    path: "$.payload".to_owned(),
                    mode: PyPropertyPathMode::Recursive,
                    weight: None,
                },
            ],
            separator: " ".to_owned(),
            exclude_paths: vec!["$.payload.ignored".to_owned()],
        };
        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: PyRegisterFtsPropertySchemaRequest =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(request, parsed);
    }
}
