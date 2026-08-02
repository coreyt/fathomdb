#![cfg(all(target_arch = "aarch64", target_os = "linux", feature = "default-embedder"))]

use candle_core::{DType, Device, Error, Tensor};

#[test]
fn default_embedder_cpu_supports_f32_matmul_without_full_fp16() -> candle_core::Result<()> {
    let lhs = Tensor::from_vec(vec![1f32, 2., 3., 4.], (2, 2), &Device::Cpu)?;
    let rhs = Tensor::from_vec(vec![5f32, 6., 7., 8.], (2, 2), &Device::Cpu)?;

    assert_eq!(lhs.matmul(&rhs)?.to_vec2::<f32>()?, vec![vec![19., 22.], vec![43., 50.]]);
    Ok(())
}

#[test]
fn cpu_f16_matmul_returns_an_error_without_full_fp16() -> candle_core::Result<()> {
    let tensor = Tensor::zeros((1, 1), DType::F16, &Device::Cpu)?;
    let error =
        tensor.matmul(&tensor).expect_err("Linux AArch64 CPU F16 matmul must be unsupported");

    assert!(matches!(error, Error::UnsupportedDTypeForOp(DType::F16, "matmul")));
    Ok(())
}
