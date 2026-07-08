//! `OrtBgeEmbedder` — cross-vendor ONNX-Runtime BGE-small embedder.
//!
//! A caller-supplied `impl fathomdb_embedder_api::Embedder`, sibling of
//! `candle_bge` / `nomic`, that produces `BAAI/bge-small-en-v1.5` vectors
//! (dim 384) through the `ort` ONNX-Runtime binding. Injected purely via
//! `EmbedderChoice::Caller(Arc::new(OrtBgeEmbedder::…))` — the engine never
//! names it, so there is ZERO engine change (ADR-0.8.16-onnx-embedder-backend
//! §2). The `Default` variant stays candle-only, preserving the footprint
//! invariant.
//!
//! Why ONNX at all: candle reaches only CPU / CUDA / Metal — no AMD ROCm,
//! Intel OpenVINO, or Windows DirectML. ONNX Runtime reaches all of those, so
//! this backend is the cross-vendor reach-hardware path (ADR §1). It is behind
//! the NON-default `onnx-embedder` Cargo feature so the thin default build
//! gains zero deps (EMB-3 wheel-size gate).
//!
//! Numeric equivalence to the candle reference is MEASURED (not enforced) at
//! Slice 15 (ADR §3 / design §5); the interim guard is same-backend
//! build-and-read, enforced here structurally by giving ONNX a DISTINCT
//! embedder identity name (`…-onnx`) so the engine's identity check never
//! silently reads candle-written vectors with the ONNX backend.

use std::path::Path;
use std::sync::Mutex;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use ort::execution_providers::{
    CPUExecutionProvider, CUDAExecutionProvider, DirectMLExecutionProvider, ExecutionProvider,
    ExecutionProviderDispatch, OpenVINOExecutionProvider, ROCmExecutionProvider,
};
use ort::session::Session;
use ort::value::Tensor;
use tokenizers::{Tokenizer, TruncationParams};

use crate::device::{parse_device_request, DeviceRequest};

/// Engine-facing identity name. Deliberately DISTINCT from the candle default
/// (`fathomdb-bge-small-en-v1.5`) so the engine's identity check enforces the
/// R-ONNX-3 same-backend build-and-read discipline: candle-written vectors and
/// ONNX-read queries never silently mix until 0.8.18 #5 enforces a candle↔ONNX
/// tolerance (ADR §3).
pub const ORT_BGE_EMBEDDER_NAME: &str = "fathomdb-bge-small-en-v1.5-onnx";

/// Output dimension for `bge-small-en-v1.5` (matches the candle reference).
pub const ORT_BGE_EMBEDDER_DIM: u32 = 384;

/// Pinned HF revision of `BAAI/bge-small-en-v1.5` — same commit the candle
/// loader pins (`loader::HF_REVISION`), recorded so an ONNX build is traceable
/// to the same upstream weights the equivalence measurement compares against.
const HF_REVISION: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";

/// Tokenizer truncation ceiling — BGE-small's 512-slot learned position
/// embeddings, identical to the candle path (`candle_bge::MAX_SEQUENCE_TOKENS`).
const MAX_SEQUENCE_TOKENS: usize = 512;

/// Sentence-vector pooling. Mirrors `candle_bge::Pooling`. Default is
/// [`OrtPooling::Cls`] — the model-native, CLS-corrected mode the candle
/// reference is compared against at Slice 15 (design §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrtPooling {
    /// Mean over the attention mask (candle's historical default).
    Mean,
    /// `[CLS]` token (position 0) — the mode BGE-small was trained for.
    Cls,
}

/// An ORT execution provider selection, resolved from the `FATHOMDB_EMBED_DEVICE`
/// grammar. Kept as a plain enum (no `ort` types) so the request→provider
/// mapping is pure + unit-testable without a model or a native runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrtProvider {
    Cpu,
    Cuda(i32),
    Rocm(i32),
    DirectMl(i32),
    OpenVino,
}

/// PURE map from the backend-agnostic [`DeviceRequest`] (parsed by the shared
/// `parse_device_request`, grammar parity with candle) to an [`OrtProvider`].
///
/// Returns the provider plus an optional LOUD-fallback message when the request
/// could not be honored and CPU was substituted (mirrors candle's loud CPU
/// fallback). The cross-vendor providers candle cannot reach — ROCm / DirectML
/// / OpenVINO — are requested through the base grammar's `Unknown` arm
/// (`FATHOMDB_EMBED_DEVICE=rocm|rocm:N|directml|openvino`), so the shared
/// parser stays unchanged and ONNX only extends the interpretation.
pub(crate) fn map_device_request(req: &DeviceRequest) -> (OrtProvider, Option<String>) {
    match req {
        DeviceRequest::Cpu => (OrtProvider::Cpu, None),
        DeviceRequest::Cuda(idx) => (OrtProvider::Cuda(*idx as i32), None),
        DeviceRequest::Metal => (
            OrtProvider::Cpu,
            Some(
                "FATHOMDB_EMBED_DEVICE=metal is a candle backend; the ONNX path has no Metal \
                 execution provider (use rocm|directml|openvino for cross-vendor GPUs, or \
                 candle's embed-metal build); using CPU"
                    .to_string(),
            ),
        ),
        DeviceRequest::Unknown(name) => map_extended_provider(name),
    }
}

/// Interpret an `Unknown` device token as a cross-vendor ORT provider.
/// Accepts `rocm`|`rocm:N`, `directml`|`dml`|`directml:N`, `openvino`|`ovep`.
/// Anything else is a LOUD CPU fallback.
fn map_extended_provider(raw: &str) -> (OrtProvider, Option<String>) {
    let (head, idx) = match raw.split_once(':') {
        Some((h, i)) => (h, i.parse::<i32>().unwrap_or(0)),
        None => (raw, 0),
    };
    match head {
        "rocm" => (OrtProvider::Rocm(idx), None),
        "directml" | "dml" => (OrtProvider::DirectMl(idx), None),
        "openvino" | "ovep" => (OrtProvider::OpenVino, None),
        other => (
            OrtProvider::Cpu,
            Some(format!(
                "FATHOMDB_EMBED_DEVICE={other} is not a recognized ONNX execution provider \
                 (expected cpu|cuda|cuda:N|rocm|rocm:N|directml|openvino); using CPU"
            )),
        ),
    }
}

/// Emit a LOUD construction-time fallback warning to stderr. Centralized so the
/// `clippy::print_stderr` allow is scoped to construction (never `embed()`), and
/// so the loud fallback is OUR OWN — `ort` is built `default-features = false`,
/// which compiles out its warn/error macros, so we cannot rely on it to surface
/// a silent CPU fallback (R-ONNX-2).
#[allow(clippy::print_stderr)] // construction-time only (not in `embed()`)
fn emit_onnx_warning(msg: &str) {
    eprintln!("fathomdb-embedder(onnx): {msg}");
}

/// RUNTIME resolution of the ORT provider from `FATHOMDB_EMBED_DEVICE`
/// (R-ONNX-2 — not a compile-time constant). Emits the LOUD stderr fallback
/// message at construction time, never inside `embed()`.
fn resolve_provider_from_env() -> OrtProvider {
    let raw = std::env::var("FATHOMDB_EMBED_DEVICE").unwrap_or_default();
    let (provider, warn) = map_device_request(&parse_device_request(&raw));
    if let Some(msg) = warn {
        emit_onnx_warning(&msg);
    }
    provider
}

/// PURE decision for the SESSION-BUILD stage of the loud fallback (R-ONNX-2):
/// given a requested [`OrtProvider`] and an availability probe, return the
/// EFFECTIVE provider plus an optional LOUD warning. A non-CPU provider the
/// probe reports unavailable is downgraded to CPU and the warning names the
/// requested provider; the caller emits it. This is distinct from the earlier
/// grammar-mapping warning ([`map_device_request`]): a request like `rocm` maps
/// cleanly to `Rocm`, but if this ONNX Runtime build lacks the ROCm EP, `ort`'s
/// own dispatch would fall back to CPU SILENTLY (its log macros are compiled out
/// under `default-features = false`), making a cross-vendor run look successful
/// while secretly on CPU. Kept pure (probe injected) so it is unit-testable
/// with no model and no ORT native lib.
fn resolve_effective_provider_with(
    requested: OrtProvider,
    is_available: impl Fn(OrtProvider) -> bool,
) -> (OrtProvider, Option<String>) {
    if matches!(requested, OrtProvider::Cpu) {
        return (OrtProvider::Cpu, None);
    }
    if is_available(requested) {
        (requested, None)
    } else {
        (
            OrtProvider::Cpu,
            Some(format!(
                "requested ONNX execution provider {requested:?} is unavailable in this ONNX \
                 Runtime build/runtime (ort is built default-features=false, so its own fallback \
                 log is compiled out); falling back to CPU"
            )),
        )
    }
}

/// Thin LIVE wrapper over [`resolve_effective_provider_with`] using `ort`'s real
/// `ExecutionProvider::is_available()` probe.
fn resolve_effective_provider(requested: OrtProvider) -> (OrtProvider, Option<String>) {
    resolve_effective_provider_with(requested, provider_is_available)
}

/// Probe whether this ONNX Runtime build was compiled with support for the
/// requested non-CPU provider (`ort`'s `ExecutionProvider::is_available()`).
/// Returns `false` when the probe errors — e.g. the ORT dylib is absent under
/// `load-dynamic` — which is precisely an unavailable provider, the silent-CPU
/// case we must surface.
fn provider_is_available(provider: OrtProvider) -> bool {
    match provider {
        OrtProvider::Cpu => true,
        OrtProvider::Cuda(_) => CUDAExecutionProvider::default().is_available().unwrap_or(false),
        OrtProvider::Rocm(_) => ROCmExecutionProvider::default().is_available().unwrap_or(false),
        OrtProvider::DirectMl(_) => {
            DirectMLExecutionProvider::default().is_available().unwrap_or(false)
        }
        OrtProvider::OpenVino => {
            OpenVINOExecutionProvider::default().is_available().unwrap_or(false)
        }
    }
}

/// Build the concrete `ort` execution-provider dispatch for a resolved
/// [`OrtProvider`]. ORT's default dispatch is non-fatal — if the provider's
/// shared lib is absent at runtime it logs and falls back to CPU rather than
/// erroring (the loud-but-non-fatal behavior candle also uses).
fn provider_dispatch(provider: OrtProvider) -> ExecutionProviderDispatch {
    match provider {
        OrtProvider::Cpu => CPUExecutionProvider::default().build(),
        OrtProvider::Cuda(idx) => CUDAExecutionProvider::default().with_device_id(idx).build(),
        OrtProvider::Rocm(idx) => ROCmExecutionProvider::default().with_device_id(idx).build(),
        OrtProvider::DirectMl(idx) => {
            DirectMLExecutionProvider::default().with_device_id(idx).build()
        }
        OrtProvider::OpenVino => OpenVINOExecutionProvider::default().build(),
    }
}

/// Cross-vendor ONNX-Runtime BGE-small embedder.
///
/// `Session::run` needs `&mut self` but the `Embedder` trait is `&self` +
/// `Send + Sync`, so the session lives behind a `Mutex`. Embedding is a short
/// forward pass, so lock contention is not a concern for the offline/eval use
/// this backend targets.
pub struct OrtBgeEmbedder {
    identity: EmbedderIdentity,
    tokenizer: Tokenizer,
    session: Mutex<Session>,
    pooling: OrtPooling,
}

fn err(context: &str, e: impl std::fmt::Display) -> EmbedderError {
    EmbedderError::Failed { message: format!("ort_bge {context}: {e}") }
}

impl OrtBgeEmbedder {
    /// Construct from an on-disk `.onnx` model + `tokenizer.json`, selecting the
    /// ORT execution provider at RUNTIME from `FATHOMDB_EMBED_DEVICE` (R-ONNX-2).
    /// Paths are caller-supplied (no hardcoded absolute path) so the model is an
    /// offline-build/eval asset the caller provisions.
    pub fn from_files(model_path: &Path, tokenizer_path: &Path) -> Result<Self, EmbedderError> {
        Self::from_files_with_provider(model_path, tokenizer_path, resolve_provider_from_env())
    }

    /// Construct from `FATHOMDB_ONNX_MODEL_PATH` + `FATHOMDB_ONNX_TOKENIZER_PATH`
    /// (device from `FATHOMDB_EMBED_DEVICE`). The env-driven entry point an eval
    /// harness / caller uses to engage the ONNX backend without recompiling.
    pub fn from_env() -> Result<Self, EmbedderError> {
        let model = std::env::var("FATHOMDB_ONNX_MODEL_PATH")
            .map_err(|_| err("from_env", "FATHOMDB_ONNX_MODEL_PATH is unset"))?;
        let tok = std::env::var("FATHOMDB_ONNX_TOKENIZER_PATH")
            .map_err(|_| err("from_env", "FATHOMDB_ONNX_TOKENIZER_PATH is unset"))?;
        Self::from_files(Path::new(&model), Path::new(&tok))
    }

    fn from_files_with_provider(
        model_path: &Path,
        tokenizer_path: &Path,
        provider: OrtProvider,
    ) -> Result<Self, EmbedderError> {
        let mut tokenizer =
            Tokenizer::from_file(tokenizer_path).map_err(|e| err("tokenizer load", e))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_SEQUENCE_TOKENS,
                ..Default::default()
            }))
            .map_err(|e| err("tokenizer truncation", e))?;

        // SESSION-BUILD stage of the loud fallback (R-ONNX-2): if the requested
        // non-CPU provider is unavailable in this ORT build, downgrade to CPU
        // and warn LOUDLY ourselves rather than letting `ort`'s compiled-out
        // dispatch fall back silently. CPU functionality is preserved.
        let (effective, avail_warn) = resolve_effective_provider(provider);
        if let Some(msg) = avail_warn {
            emit_onnx_warning(&msg);
        }

        let session = Session::builder()
            .map_err(|e| err("session builder", e))?
            .with_execution_providers([provider_dispatch(effective)])
            .map_err(|e| err("execution provider", e))?
            .commit_from_file(model_path)
            .map_err(|e| err("model load", e))?;

        let identity =
            EmbedderIdentity::new(ORT_BGE_EMBEDDER_NAME, HF_REVISION, ORT_BGE_EMBEDDER_DIM);

        Ok(Self { identity, tokenizer, session: Mutex::new(session), pooling: OrtPooling::Cls })
    }

    /// Select the pooling strategy (default [`OrtPooling::Cls`]). Does NOT change
    /// the identity — use only on a fresh workspace / in the equivalence harness.
    #[must_use]
    pub fn with_pooling(mut self, pooling: OrtPooling) -> Self {
        self.pooling = pooling;
        self
    }
}

impl Embedder for OrtBgeEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, input: &str) -> Result<Vector, EmbedderError> {
        let encoding = self.tokenizer.encode(input, true).map_err(|e| err("tokenize", e))?;
        let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| i64::from(x)).collect();
        let mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&x| i64::from(x)).collect();
        let len = ids.len();
        let token_type: Vec<i64> = vec![0; len];
        let shape = vec![1_i64, len as i64];

        let ids_t = Tensor::from_array((shape.clone(), ids)).map_err(|e| err("input_ids", e))?;
        let mask_t =
            Tensor::from_array((shape.clone(), mask)).map_err(|e| err("attention_mask", e))?;
        let tt_t = Tensor::from_array((shape, token_type)).map_err(|e| err("token_type_ids", e))?;

        let mut session = self.session.lock().map_err(|_| err("session lock", "poisoned"))?;
        let outputs = session
            .run(ort::inputs![
                "input_ids" => ids_t,
                "attention_mask" => mask_t,
                "token_type_ids" => tt_t,
            ])
            .map_err(|e| err("forward", e))?;

        // last_hidden_state — first output, shape (1, L, H).
        let (out_shape, data) =
            outputs[0].try_extract_tensor::<f32>().map_err(|e| err("extract", e))?;
        let dims: Vec<usize> = out_shape.iter().map(|&d| d as usize).collect();
        if dims.len() != 3 {
            return Err(err("output shape", format!("expected rank-3 (1,L,H), got {dims:?}")));
        }
        let (seq_len, hidden) = (dims[1], dims[2]);
        if hidden != ORT_BGE_EMBEDDER_DIM as usize {
            return Err(err(
                "output dim",
                format!("expected hidden {ORT_BGE_EMBEDDER_DIM}, got {hidden}"),
            ));
        }

        // Pool. `embed()` is single-input (no padding), so the attention mask is
        // all-ones and mean-pool reduces to a plain mean over `seq_len`.
        let mut pooled = vec![0.0_f32; hidden];
        match self.pooling {
            OrtPooling::Cls => {
                pooled.copy_from_slice(&data[0..hidden]);
            }
            OrtPooling::Mean => {
                for pos in 0..seq_len {
                    let base = pos * hidden;
                    for (j, slot) in pooled.iter_mut().enumerate() {
                        *slot += data[base + j];
                    }
                }
                let denom = seq_len.max(1) as f32;
                for slot in &mut pooled {
                    *slot /= denom;
                }
            }
        }

        // L2-normalize (matches candle's `l2_normalize`).
        let norm = pooled.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
        for slot in &mut pooled {
            *slot /= norm;
        }
        Ok(pooled)
    }
}

#[cfg(test)]
mod tests {
    //! R-ONNX-2 device-mapping unit tests: the `FATHOMDB_EMBED_DEVICE` grammar
    //! (parsed by the shared `parse_device_request`) → the correct ORT execution
    //! provider. Pure — no model, no ONNX Runtime native lib, no GPU required.
    use super::{map_device_request, OrtProvider};
    use crate::device::parse_device_request;

    fn resolve(raw: &str) -> (OrtProvider, Option<String>) {
        map_device_request(&parse_device_request(raw))
    }

    #[test]
    fn cpu_and_unset_map_to_cpu_no_warning() {
        for raw in ["", "cpu", "  CPU  "] {
            let (p, warn) = resolve(raw);
            assert_eq!(p, OrtProvider::Cpu, "{raw:?}");
            assert!(warn.is_none(), "{raw:?} should not warn");
        }
    }

    #[test]
    fn cuda_maps_to_cuda_provider_with_index() {
        assert_eq!(resolve("cuda").0, OrtProvider::Cuda(0));
        assert_eq!(resolve("cuda:1").0, OrtProvider::Cuda(1));
        assert_eq!(resolve("cuda:2").0, OrtProvider::Cuda(2));
        assert!(resolve("cuda:1").1.is_none());
    }

    #[test]
    fn rocm_maps_to_rocm_provider() {
        // ROCm is unreachable through candle — the cross-vendor payoff.
        assert_eq!(resolve("rocm").0, OrtProvider::Rocm(0));
        assert_eq!(resolve("rocm:1").0, OrtProvider::Rocm(1));
        assert!(resolve("rocm").1.is_none());
        assert!(resolve("ROCm").1.is_none()); // grammar lower-cases
    }

    #[test]
    fn directml_maps_to_directml_provider() {
        assert_eq!(resolve("directml").0, OrtProvider::DirectMl(0));
        assert_eq!(resolve("dml").0, OrtProvider::DirectMl(0));
        assert_eq!(resolve("directml:1").0, OrtProvider::DirectMl(1));
        assert!(resolve("directml").1.is_none());
    }

    #[test]
    fn openvino_maps_to_openvino_provider() {
        assert_eq!(resolve("openvino").0, OrtProvider::OpenVino);
        assert_eq!(resolve("ovep").0, OrtProvider::OpenVino);
        assert!(resolve("openvino").1.is_none());
    }

    #[test]
    fn metal_falls_back_to_cpu_loudly() {
        // ORT has no Metal EP; candle owns that lane. Loud, never silent.
        let (p, warn) = resolve("metal");
        assert_eq!(p, OrtProvider::Cpu);
        assert!(warn.is_some(), "metal must warn on CPU fallback");
    }

    #[test]
    fn unrecognized_device_falls_back_to_cpu_loudly() {
        for raw in ["vulkan", "tpu", "gpu"] {
            let (p, warn) = resolve(raw);
            assert_eq!(p, OrtProvider::Cpu, "{raw:?}");
            assert!(warn.is_some(), "{raw:?} must warn on CPU fallback");
        }
    }

    /// SESSION-BUILD loud fallback (codex §9 fix-1): a requested GPU provider
    /// that the ORT build does not support must downgrade to CPU with a LOUD
    /// warning that names the requested provider — never a silent CPU fallback.
    /// Probe is injected (`|_| false`), so this runs with no ORT lib / no GPU.
    #[test]
    fn requested_gpu_unavailable_falls_back_to_cpu_loudly() {
        for requested in [
            OrtProvider::Cuda(0),
            OrtProvider::Rocm(1),
            OrtProvider::DirectMl(0),
            OrtProvider::OpenVino,
        ] {
            let (eff, warn) = super::resolve_effective_provider_with(requested, |_| false);
            assert_eq!(eff, OrtProvider::Cpu, "{requested:?} must downgrade to CPU");
            let msg = warn.expect("unavailable GPU provider must emit a warning");
            assert!(
                msg.contains(&format!("{requested:?}")),
                "warning must name the requested provider {requested:?}, got {msg:?}"
            );
            assert!(
                msg.to_lowercase().contains("cpu"),
                "warning must state the CPU fallback, got {msg:?}"
            );
        }
    }

    /// When the probe reports the requested provider available, it is honored
    /// and there is NO warning (no spurious loud fallback).
    #[test]
    fn requested_available_provider_is_honored_without_warning() {
        let (eff, warn) = super::resolve_effective_provider_with(OrtProvider::Cuda(2), |_| true);
        assert_eq!(eff, OrtProvider::Cuda(2));
        assert!(warn.is_none(), "an available provider must not warn");
    }

    /// A CPU request is always honored, never warns, and never probes (CPU is
    /// unconditionally available) — the probe closure must not run.
    #[test]
    fn cpu_request_never_probes_and_never_warns() {
        let (eff, warn) = super::resolve_effective_provider_with(OrtProvider::Cpu, |_| {
            panic!("CPU request must not probe provider availability")
        });
        assert_eq!(eff, OrtProvider::Cpu);
        assert!(warn.is_none());
    }

    /// R-ONNX-1 real-vector test — BLOCKED on an offline ONNX asset (see
    /// `output.json` BLOCKED_ON): no `bge-small-en-v1.5` `.onnx` model and no
    /// ONNX Runtime native lib exist in the offline build/eval envelope
    /// (only candle `safetensors` + Python-wheel `onnxruntime` `.so`s). Provide
    /// `FATHOMDB_ONNX_MODEL_PATH` + `FATHOMDB_ONNX_TOKENIZER_PATH` and
    /// `ORT_DYLIB_PATH` (load-dynamic), then run with `--ignored`.
    #[test]
    #[ignore = "needs offline bge-small .onnx model + ONNX Runtime native lib (ORT_DYLIB_PATH); see output.json BLOCKED_ON"]
    fn ort_bge_embeds_384_dim_finite_deterministic_vector() {
        use fathomdb_embedder_api::Embedder;

        let embedder = super::OrtBgeEmbedder::from_env()
            .expect("set FATHOMDB_ONNX_MODEL_PATH / FATHOMDB_ONNX_TOKENIZER_PATH / ORT_DYLIB_PATH");
        let v1 = embedder.embed("the quick brown fox").expect("embed");
        let v2 = embedder.embed("the quick brown fox").expect("embed");
        assert_eq!(v1.len(), super::ORT_BGE_EMBEDDER_DIM as usize);
        assert!(v1.iter().all(|x| x.is_finite()), "all components finite");
        assert_eq!(v1, v2, "deterministic for identical input");
        let norm = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "L2-normalized (got {norm})");
    }
}
