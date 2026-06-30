//! Shared device-request parsing for the Candle backends (embedder + reranker).
//!
//! Both the default embedder (`candle_bge`, `FATHOMDB_EMBED_DEVICE`) and the
//! default reranker (`candle_reranker`, `FATHOMDB_RERANK_DEVICE`) accept the
//! SAME device grammar (`cpu` | `cuda` | `cuda:N` | `metal`). Keeping ONE pure
//! parser here — independent of which Candle features are compiled in — avoids
//! the two paths drifting apart and lets the grammar be unit-tested without a
//! GPU or a feature build. The (feature- and hardware-dependent) mapping from a
//! request to an actual `candle_core::Device` stays in each backend's own
//! `resolve_device`, because the embedder and reranker are gated on independent
//! features (`embed-cuda` vs `rerank-cuda`).
//!
//! Compiled whenever EITHER Candle path is on (`default-embedder` or
//! `default-reranker`); the thin no-feature build pulls in none of this.

/// A parsed device request, independent of which backends are compiled in.
/// Keeping the env-grammar parse PURE (no `Device` construction, no `#[cfg]`
/// gating, no I/O) makes it unit-testable without a GPU or a feature build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceRequest {
    /// Default — explicit `cpu`, empty/unset, or whitespace-only.
    Cpu,
    /// `cuda` (index 0) or `cuda:N`. A non-numeric index (`cuda:x`) clamps to 0,
    /// matching the original `.unwrap_or(0)` behavior.
    Cuda(usize),
    /// `metal`.
    Metal,
    /// Anything else — honored as a loud CPU fallback, never silently.
    Unknown(String),
}

/// Parse a device env var (`FATHOMDB_EMBED_DEVICE` / `FATHOMDB_RERANK_DEVICE`)
/// into a [`DeviceRequest`]. Pure + total: case-insensitive, trims surrounding
/// whitespace, and never panics.
pub(crate) fn parse_device_request(raw: &str) -> DeviceRequest {
    let requested = raw.trim().to_ascii_lowercase();
    if requested.is_empty() || requested == "cpu" {
        return DeviceRequest::Cpu;
    }
    if requested == "cuda" {
        return DeviceRequest::Cuda(0);
    }
    if let Some(idx) = requested.strip_prefix("cuda:") {
        return DeviceRequest::Cuda(idx.parse::<usize>().unwrap_or(0));
    }
    if requested == "metal" {
        return DeviceRequest::Metal;
    }
    DeviceRequest::Unknown(requested)
}

#[cfg(test)]
mod device_request_tests {
    //! R-GPU-1 — pin the device grammar shared by `FATHOMDB_EMBED_DEVICE` and
    //! `FATHOMDB_RERANK_DEVICE`. These exercise the PURE parse
    //! (`parse_device_request`), not either backend's `resolve_device`, so they
    //! run on the default (CPU) build with no GPU and no
    //! `embed-cuda`/`embed-metal`/`rerank-cuda`/`rerank-metal` feature. The
    //! request→`Device` mapping is feature- and hardware-dependent and is
    //! covered by the GPU validation harness, not unit tests.
    use super::{parse_device_request, DeviceRequest};

    #[test]
    fn unset_or_empty_is_cpu() {
        // unset env decodes through `unwrap_or_default()` to "" at the call site.
        assert_eq!(parse_device_request(""), DeviceRequest::Cpu);
        assert_eq!(parse_device_request("   "), DeviceRequest::Cpu);
    }

    #[test]
    fn explicit_cpu_is_cpu_case_and_space_insensitive() {
        assert_eq!(parse_device_request("cpu"), DeviceRequest::Cpu);
        assert_eq!(parse_device_request("CPU"), DeviceRequest::Cpu);
        assert_eq!(parse_device_request("  Cpu  "), DeviceRequest::Cpu);
    }

    #[test]
    fn bare_cuda_is_device_zero() {
        assert_eq!(parse_device_request("cuda"), DeviceRequest::Cuda(0));
        assert_eq!(parse_device_request("CUDA"), DeviceRequest::Cuda(0));
    }

    #[test]
    fn cuda_n_selects_the_index() {
        assert_eq!(parse_device_request("cuda:0"), DeviceRequest::Cuda(0));
        assert_eq!(parse_device_request("cuda:1"), DeviceRequest::Cuda(1));
        assert_eq!(parse_device_request(" cuda:2 "), DeviceRequest::Cuda(2));
    }

    #[test]
    fn cuda_with_garbage_index_clamps_to_zero() {
        // Preserves the original `.unwrap_or(0)` behavior — a malformed index
        // is a GPU-0 request, never a panic.
        assert_eq!(parse_device_request("cuda:x"), DeviceRequest::Cuda(0));
        assert_eq!(parse_device_request("cuda:"), DeviceRequest::Cuda(0));
    }

    #[test]
    fn metal_is_metal() {
        assert_eq!(parse_device_request("metal"), DeviceRequest::Metal);
        assert_eq!(parse_device_request("Metal"), DeviceRequest::Metal);
    }

    #[test]
    fn unrecognized_is_a_named_unknown() {
        // Honored as a loud CPU fallback in each `resolve_device`, never silent.
        assert_eq!(parse_device_request("rocm"), DeviceRequest::Unknown("rocm".to_string()));
        assert_eq!(parse_device_request("gpu"), DeviceRequest::Unknown("gpu".to_string()));
    }
}
