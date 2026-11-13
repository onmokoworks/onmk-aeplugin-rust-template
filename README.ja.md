# onmk-aeplugin-rust-template

[日本語](./README.ja.md) | [English](./README.md#english)

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
