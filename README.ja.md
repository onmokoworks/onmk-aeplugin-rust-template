# onmk-aeplugin-rust-template

After Effects向けRustプラグインのGPU実装テンプレート実験です。

このリポジトリは、単体の完成エフェクトを作ることよりも、今後のAEプラグイン開発で使い回せる実装パイプラインを整理することを目的にしています。

## 目的

- AE SmartFX / SmartRenderのRust実装を整理する
- CPU fallbackを持つ
- `wgpu` compute + upload/readback経路を検証する
- AE official GPU pathのnative backend実験を分離する
- CUDA / Metal / DirectX / `wgpu-hal` external import研究の入口を作る

## 構成

```text
crates/ae_gpu_lab_core
  reusable frame types, CPU effects, wgpu upload/readback backend

crates/ae_gpu_lab_cli
  local benchmark runner

crates/ae_gpu_lab_plugin
  SmartFX + wgpu plugin experiment

crates/ae_gpu_lab_probe_plugin
  AE official GPU path probe and native CUDA experiment

crates/ae_gpu_lab_wgpu_hal_import
  wgpu-hal external import preflight research
```

## Tracks

- [01 SmartFX + wgpu](tracks/01_smartfx_wgpu/README.md)
- [02 Native / Official GPU Probe](tracks/02_native_official_gpu_probe/README.md)
- [03 wgpu-hal External Import](tracks/03_wgpu_hal_external_import/README.md)

## CLI Benchmark

```powershell
cd onmk-aeplugin-rust-template
cargo run -p ae_gpu_lab_cli --release -- --effect box-blur --width 3840 --height 2160 --passes 3 --iterations 5
```

## Notes

- Generated artifacts such as `target/`, `.dll`, `.aex`, `.pdb`, `.lib`, `.exp`, and `.log` are ignored.
- The AE probe plugin writes diagnostics to the OS temp directory, not to a hard-coded local workspace path.
- The native CUDA path is currently a research probe, not a production-ready compatibility layer.

## References

- [Prior Art](docs/prior-art.md)
- [Benchmark Effect Notes](docs/effects.md)
