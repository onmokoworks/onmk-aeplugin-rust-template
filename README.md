# onmk-aeplugin-rust-template

[日本語](./README.ja.md) | [English](#english)

Adobe After Effects向けRustプラグインのテンプレート実験です。

このリポジトリは、単体の完成エフェクトを配布することよりも、今後のAEプラグイン開発で使い回せる実装パイプラインを作ることを目的にしています。

> 仕様、構成、API境界、パラメータ名は実験の進行に合わせて変わる可能性があります。

## 目的

- RustからAE SmartFX / SmartRenderを扱う
- CPU fallbackを持つ
- `wgpu` compute + upload/readback経路を検証する
- AE official GPU pathのprobeを分離する
- CUDA / Metal / DirectX / `wgpu-hal` external import研究の入口を作る

## 構成

```text
crates/ae_gpu_lab_core
  共通frame型、CPU effect、wgpu upload/readback backend

crates/ae_gpu_lab_cli
  AE外で動かすローカルbenchmark runner

crates/ae_gpu_lab_cuda
  AEに依存しないnative CUDA実験backend

crates/ae_gpu_lab_plugin
  SmartFX + wgpu実験。AeGpuLab.aexを生成

crates/ae_gpu_lab_probe_plugin
  AE official GPU path probeとnative CUDA実験。AeGpuProbe.aexを生成

crates/ae_gpu_lab_wgpu_hal_import
  wgpu-hal external importのpreflight研究
```

## ビルド

CLI benchmark:

```powershell
cargo run -p ae_gpu_lab_cli --release -- --effect box-blur --width 3840 --height 2160 --passes 3 --iterations 5
```

AEプラグインを`dist/`へ出力:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin lab
.\scripts\install-ae-plugin.ps1 -Plugin probe
```

WindowsのAdobe MediaCoreへインストール:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin lab -InstallMediaCore
.\scripts\install-ae-plugin.ps1 -Plugin probe -InstallMediaCore
```

MediaCoreへのコピーでは管理者権限が要求されます。

## 実験トラック

- [01 SmartFX + wgpu](tracks/01_smartfx_wgpu/README.md)
- [02 Native / Official GPU Probe](tracks/02_native_official_gpu_probe/README.md)
- [03 wgpu-hal External Import](tracks/03_wgpu_hal_external_import/README.md)

## 現在の状態

- `target/`, `dist/`, `.dll`, `.aex`, `.pdb`, `.lib`, `.exp`, `.log` はGit管理外
- probe pluginの診断ログはOSのtemp directoryへ出力
- CUDA pathは研究用probeであり、本番向け互換layerではない
- `wgpu-hal` external importはbackend依存が強いため、通常の実装経路から分離

## 参考

- [Prior Art](docs/prior-art.md)
- [Benchmark Effect Notes](docs/effects.md)

---

## English

Rust template experiments for Adobe After Effects plug-ins.

This repository is not mainly about shipping one polished effect. The useful artifact is a reusable implementation pipeline for future AE plug-in work.

> Specifications, structure, API boundaries, and parameter names may change as the experiments evolve.

## Goals

- Use AE SmartFX / SmartRender from Rust
- Keep a CPU fallback
- Validate `wgpu` compute with upload/readback
- Isolate AE official GPU path probing
- Prepare research entry points for CUDA, Metal, DirectX, and `wgpu-hal` external import

## Crates

```text
crates/ae_gpu_lab_core
  Shared frame types, CPU effects, and wgpu upload/readback backend.

crates/ae_gpu_lab_cli
  Local benchmark runner outside AE.

crates/ae_gpu_lab_cuda
  Native CUDA experiment backend without an AE dependency.

crates/ae_gpu_lab_plugin
  SmartFX + wgpu experiment. Builds AeGpuLab.aex.

crates/ae_gpu_lab_probe_plugin
  AE official GPU path probe and native CUDA experiment. Builds AeGpuProbe.aex.

crates/ae_gpu_lab_wgpu_hal_import
  Experimental external import boundary and preflight checks.
```

## Build

Run the CLI benchmark:

```powershell
cargo run -p ae_gpu_lab_cli --release -- --effect box-blur --width 3840 --height 2160 --passes 3 --iterations 5
```

Build an AE plug-in into `dist/`:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin lab
.\scripts\install-ae-plugin.ps1 -Plugin probe
```

Build and install into the shared Adobe MediaCore folder on Windows:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin lab -InstallMediaCore
.\scripts\install-ae-plugin.ps1 -Plugin probe -InstallMediaCore
```

The install step asks for elevation because Adobe's shared MediaCore folder is under `CommonProgramFiles`.

## Tracks

- [01 SmartFX + wgpu](tracks/01_smartfx_wgpu/README.md)
- [02 Native / Official GPU Probe](tracks/02_native_official_gpu_probe/README.md)
- [03 wgpu-hal External Import](tracks/03_wgpu_hal_external_import/README.md)

## Current Notes

- Generated artifacts such as `target/`, `dist/`, `.dll`, `.aex`, `.pdb`, `.lib`, `.exp`, and `.log` are ignored.
- The probe plug-in writes diagnostics to the OS temp directory.
- The CUDA path is a research probe, not a production compatibility layer.
- `wgpu-hal` external import is intentionally isolated because real zero-copy interop is backend-specific.
