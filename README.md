# onmk-aeplugin-rust-template

[日本語](./README.ja.md) | [English](#english)

Adobe After Effects向けRustプラグインのテンプレート実験です。

このリポジトリは、単体の完成エフェクトを配布することよりも、今後のAEプラグイン開発で使い回せる実装パイプラインを作ることを目的にしています。

> 仕様、構成、API境界、パラメータ名は実験の進行に合わせて変わる可能性があります。

## はじめに: 推奨の出発点

新しいCPUエフェクトは [`tracks/00_cpu_minimal`](tracks/00_cpu_minimal/README.md) の **AeMinimal** から始めてください。
SmartFX + legacy Render、8/16/32 bpc、ストレートalpha、ワールドの事前検証、SmartRenderでの確実なcheck-in、
バイナリPiPLリソースを備えた最小構成です。

```powershell
cargo test --release                                     # 既定メンバーのユニットテスト
.\scripts\install-ae-plugin.ps1 -Plugin minimal          # dist\AeMinimal.aex を出力（インストールしない）
```

MediaCoreへのインストールは、管理者として起動したPowerShellで行います（ビルドは先に通常権限で）:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin minimal -Install -SkipBuild
.\scripts\restore-ae-plugin.ps1                          # 直前のインストールを元に戻す
```

インストーラはAfter Effectsの起動中は何もせず、上書きするファイルをハッシュ検証付きでバックアップし、
MediaCoreとAEの `Support Files\Plug-ins` に同じマッチネームの `.aex` があれば中止します。
インストール先は `MediaCore\onmk-aeplugin-rust-template\` です。以前の版のスクリプトで
`MediaCore` 直下に入れた `AeGpuLab.aex` などがあると重複として検出されるので、先に削除してください。

## 目的

- RustからAE SmartFX / SmartRenderを扱う
- AEとの境界（ピクセル深度、stride、アルファ、check-in、パラメータ互換性）を安全にする
- CPU fallbackを持つ
- `wgpu` compute + upload/readback経路を検証する
- AE official GPU pathのprobeを分離する
- CUDA / Metal / DirectX / `wgpu-hal` external import研究の入口を作る

## 構成

```text
crates/ae_plugin_kit
  共通部品: ワールド検証、ARGB行コーデック、アルファ検査、SmartRender後始末、
  上限付き行並列、実行時判定SIMD、SDK型の移植性ヘルパー（feature "ae" でafter-effects連携）

crates/ae_plugin_build
  build.rs用ヘルパー: ターゲットOSに合わせたエントリポイント、WindowsでのバイナリPiPLリソース

tracks/00_cpu_minimal/crates/ae_minimal_plugin
  CPUだけの最小SmartFXエフェクト。AeMinimal.aexを生成（推奨の出発点）

tracks/01_smartfx_wgpu/crates/ae_gpu_lab_core
  共通frame型、CPU effect、wgpu upload/readback backend

tracks/01_smartfx_wgpu/crates/ae_gpu_lab_cli
  AE外で動かすローカルbenchmark runner

tracks/01_smartfx_wgpu/crates/ae_gpu_lab_plugin
  SmartFX + wgpu実験。AeGpuLab.aexを生成

tracks/02_native_official_gpu_probe/crates/ae_gpu_lab_cuda
  AEに依存しないnative CUDA実験backend

tracks/02_native_official_gpu_probe/crates/ae_gpu_lab_probe_plugin
  AE official GPU path probeとnative CUDA実験（CUDAはWindowsのみ）。AeGpuProbe.aexを生成

tracks/03_wgpu_hal_external_import/crates/ae_gpu_lab_wgpu_hal_import
  D3D12 / Metal buffer import、preflight、共通BGRA128 compute

scripts/
  install-ae-plugin.ps1 / restore-ae-plugin.ps1   安全なビルド・インストール・復元
  alpha_contract.py                               生RGBAのアルファ契約検査
  aexcompat_smoke.py                              AEXCompatによる任意のヘッドレス検査
```

## ビルド

CLI benchmark:

```powershell
cargo run -p ae_gpu_lab_cli --release -- --effect box-blur --width 3840 --height 2160 --passes 3 --iterations 5
```

AEプラグインを`dist/`へ出力（複数可）:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin minimal,lab,probe
```

ワークスペース全体:

```powershell
cargo build --release --workspace
cargo test --release --workspace
```

`AESDK_ROOT` は不要です（`after-effects-sys` の同梱バインディングを使います）。
`after-effects` / `pipl` は `0e0b197850f28c1bb24e8c82fce2e3d4c120d1e9` に固定しています。

## ドキュメント

- [アルファ契約](docs/alpha-contract.ja.md): AE境界はストレートRGB。部分alphaのフィクスチャと独立した期待値
- [パラメータと識別子の互換性](docs/parameter-compatibility.ja.md): 並べ替え・削除・型変更をしない、末尾に追加、ポップアップ、マッチネーム
- [Windows / macOSの移植性](docs/cross-platform.ja.md): SDKの型の違い、World Suiteによる深度判定、cfg、PiPLリソース
- [CPU性能のための土台](docs/performance.ja.md): 上限付き行並列と実行時判定SIMD
- [検証](docs/verification.ja.md): 証拠の段階、ローカル確認、CI、AEXCompatによる任意のスモークテスト

## 実験トラック

- [00 CPU Minimal（推奨の出発点）](tracks/00_cpu_minimal/README.md)
- [01 SmartFX + wgpu](tracks/01_smartfx_wgpu/README.md)
- [02 Native / Official GPU Probe](tracks/02_native_official_gpu_probe/README.md)
- [03 wgpu-hal External Import](tracks/03_wgpu_hal_external_import/README.md)

## 現在の状態

- どのプラグインもAfter Effects実機ではまだ検証していません。ユニットテスト、CI、任意のAEXCompatスモークは
  AEでの検証の代わりになりません（[検証](docs/verification.ja.md)）
- CI（`.github/workflows/ci.yml`）はWindows x64とmacOS arm64 / x86_64でビルドとテストを行います。
  macOSのバンドル化・署名は未対応です
- `target/`, `dist/`, `local/`, `.dll`, `.aex`, `.pdb`, `.lib`, `.exp`, `.log` はGit管理外
- probe pluginの診断ログはOSのtemp directoryへ出力
- CUDA pathは研究用probeであり、本番向け互換layerではない
- `wgpu-hal` external importはbackend依存が強いため、通常の実装経路から分離

## 参考

- [Prior Art](docs/prior-art.md)
- [Benchmark Effect Notes](docs/effects.md)
- [wgpu-hal External Import 調査](docs/wgpu-hal-interop.ja.md)

---

## English

Rust template experiments for Adobe After Effects plug-ins.

This repository is not mainly about shipping one polished effect. The useful artifact is a reusable implementation pipeline for future AE plug-in work.

> Specifications, structure, API boundaries, and parameter names may change as the experiments evolve.

## Start Here: Recommended Starting Point

Start new CPU effects from **AeMinimal** in [`tracks/00_cpu_minimal`](tracks/00_cpu_minimal/README.md).
It is the smallest complete setup: SmartFX + legacy Render, 8/16/32 bpc, straight alpha, validated
worlds, guaranteed SmartRender check-in and a binary PiPL resource.

```powershell
cargo test --release                                     # unit tests of the default members
.\scripts\install-ae-plugin.ps1 -Plugin minimal          # writes dist\AeMinimal.aex, no install
```

Install into MediaCore from an elevated PowerShell (build first without elevation):

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin minimal -Install -SkipBuild
.\scripts\restore-ae-plugin.ps1                          # undo the latest install
```

The installer does nothing while After Effects runs, backs up every file it overwrites with hash
verification, and stops if another `.aex` with the same match name exists in MediaCore or in an AE
`Support Files\Plug-ins` folder. It installs into `MediaCore\onmk-aeplugin-rust-template\`. Copies
that the previous script placed directly in `MediaCore` (for example `AeGpuLab.aex`) are reported as
duplicates; remove them first.

## Goals

- Use AE SmartFX / SmartRender from Rust
- Make the AE boundary safe (pixel depth, stride, alpha, check-in, parameter compatibility)
- Keep a CPU fallback
- Validate `wgpu` compute with upload/readback
- Isolate AE official GPU path probing
- Prepare research entry points for CUDA, Metal, DirectX, and `wgpu-hal` external import

## Crates

```text
crates/ae_plugin_kit
  Shared building blocks: world validation, ARGB row codecs, alpha checks, SmartRender cleanup,
  bounded row parallelism, runtime-detected SIMD, SDK type portability (feature "ae" for after-effects).

crates/ae_plugin_build
  build.rs helper: entry points for the target OS, binary PiPL resource on Windows.

tracks/00_cpu_minimal/crates/ae_minimal_plugin
  Minimal CPU-only SmartFX effect. Builds AeMinimal.aex (recommended starting point).

tracks/01_smartfx_wgpu/crates/ae_gpu_lab_core
  Shared frame types, CPU effects, and wgpu upload/readback backend.

tracks/01_smartfx_wgpu/crates/ae_gpu_lab_cli
  Local benchmark runner outside AE.

tracks/01_smartfx_wgpu/crates/ae_gpu_lab_plugin
  SmartFX + wgpu experiment. Builds AeGpuLab.aex.

tracks/02_native_official_gpu_probe/crates/ae_gpu_lab_cuda
  Native CUDA experiment backend without an AE dependency.

tracks/02_native_official_gpu_probe/crates/ae_gpu_lab_probe_plugin
  AE official GPU path probe and native CUDA experiment (CUDA on Windows only). Builds AeGpuProbe.aex.

tracks/03_wgpu_hal_external_import/crates/ae_gpu_lab_wgpu_hal_import
  D3D12 / Metal buffer import, preflight checks, and shared BGRA128 compute.

scripts/
  install-ae-plugin.ps1 / restore-ae-plugin.ps1   safe build, install and restore
  alpha_contract.py                               alpha contract checker for raw RGBA
  aexcompat_smoke.py                              optional headless check with AEXCompat
```

## Build

Run the CLI benchmark:

```powershell
cargo run -p ae_gpu_lab_cli --release -- --effect box-blur --width 3840 --height 2160 --passes 3 --iterations 5
```

Build AE plug-ins into `dist/` (one or more):

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin minimal,lab,probe
```

Whole workspace:

```powershell
cargo build --release --workspace
cargo test --release --workspace
```

`AESDK_ROOT` is not needed (the bundled `after-effects-sys` bindings are used).
`after-effects` / `pipl` are pinned to `0e0b197850f28c1bb24e8c82fce2e3d4c120d1e9`.

## Documentation

- [Alpha contract](docs/alpha-contract.md): the AE boundary is straight RGB; partial-alpha fixtures and independent expected results
- [Parameter and identity compatibility](docs/parameter-compatibility.md): never reorder, remove or retype; append; popups; match names
- [Windows / macOS portability](docs/cross-platform.md): SDK type differences, World Suite depth, cfg, PiPL resource
- [CPU performance scaffolding](docs/performance.md): bounded row parallelism and runtime-detected SIMD
- [Verification](docs/verification.md): evidence levels, local checks, CI, optional AEXCompat smoke test

## Tracks

- [00 CPU Minimal (recommended starting point)](tracks/00_cpu_minimal/README.md)
- [01 SmartFX + wgpu](tracks/01_smartfx_wgpu/README.md)
- [02 Native / Official GPU Probe](tracks/02_native_official_gpu_probe/README.md)
- [03 wgpu-hal External Import](tracks/03_wgpu_hal_external_import/README.md)

## Current Notes

- None of the plug-ins has been validated in After Effects yet. Unit tests, CI and the optional
  AEXCompat smoke test do not replace AE validation ([verification](docs/verification.md)).
- CI (`.github/workflows/ci.yml`) builds and tests on Windows x64 and macOS arm64 / x86_64. macOS
  bundling and signing are not covered.
- Generated artifacts such as `target/`, `dist/`, `local/`, `.dll`, `.aex`, `.pdb`, `.lib`, `.exp`, and `.log` are ignored.
- The probe plug-in writes diagnostics to the OS temp directory.
- The CUDA path is a research probe, not a production compatibility layer.
- `wgpu-hal` external import is intentionally isolated because real zero-copy interop is backend-specific.
