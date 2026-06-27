# onmk-aeplugin-rust-template

[日本語](./README.ja.md) | [English](./README.md#english)

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
