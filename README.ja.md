# Ae Rust GPU Lab

After Effects 向け Rust GPU plugin pipeline の実験場です。

このプロジェクトの主目的は、特定のエフェクトを完成させることではなく、次の部品を使い回せる形で育てることです。

- CPU SmartFX 相当の flat frame 処理
- `wgpu` compute + upload/readback の計測
- AE official GPU path / native backend を後から差し込む境界
- `wgpu-hal` / external texture import の隔離された実験場所
- CPU fallback と GPU backend の比較ベンチ

## 構成

```text
crates/ae_gpu_lab_core
  reusable frame types, CPU effects, wgpu upload/readback backend

crates/ae_gpu_lab_cli
  local benchmark runner, AE なしで GPU 実験を回す

crates/ae_gpu_lab_plugin
  future AE plugin crate placeholder
```

## 最初のベンチ候補

GPU の性格を見るには、1つの重いエフェクトより複数の「負荷の形」が違う kernel を持つのが良いです。

- `copy`: upload/readback の最低コストを見る
- `color`: 1 pixel 完結の ALU 軽め処理
- `box-blur`: 近傍参照が多い memory bandwidth / cache 系
- `diffusion`: 複数 iteration の dispatch / intermediate buffer 系
- `chroma-warp`: 座標変形 + 複数 sample 系

## 研究メモ

- [先行研究 / prior art](docs/prior-art.md)
- [ベンチマークeffect候補](docs/effects.md)

## 実装レーン

- [01 SmartFX + wgpu](tracks/01_smartfx_wgpu/README.md)
- [02 Native / Official GPU Probe](tracks/02_native_official_gpu_probe/README.md)
- [03 wgpu-hal External Import](tracks/03_wgpu_hal_external_import/README.md)

## 実行

```powershell
cd AeRustGpuLabRust
cargo run -p ae_gpu_lab_cli --release -- --effect box-blur --width 3840 --height 2160 --passes 3 --iterations 5
```

まずは `wgpu upload/readback` を確実に測れるようにし、その後 `crates/ae_gpu_lab_plugin` から AE SmartFX に接続していく想定です。

現在のCLIは backend trait 経由で CPU と `wgpu_upload_readback` を実行し、`upload / encode / submit / map_wait / copy` のCPU側区間を表示します。これはGPU timestampではなく、zero-copy interopと比較するための最初の外形計測です。

## 実機確認の順番

`wgpu-hal` / zero-copy interop に入る前に、まず `01 SmartFX + wgpu` をAE上で確認するのが安全です。

```text
1. CLIでcopy/checksum/timing確認
2. AE内でSmartFX + wgpu upload/readback確認
3. AE公式GPU probeでframework/device/frame情報だけログ
4. probe結果をExternalFrameRefへ変換してpreflight確認
5. D3D12またはMetalの片方でread-only import PoC
```

この順番にすると、zero-copy側で失敗した時に、AE plugin基盤、wgpu基盤、official GPU probe、external importのどこが原因か切り分けやすくなります。
