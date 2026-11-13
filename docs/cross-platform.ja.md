# Windows / macOS の移植性

[English](./cross-platform.md)

## プラットフォームで SDK の型が異なる

`after-effects-sys` は Windows 版と macOS 版の SDK ヘッダから別々に生成した bindgen 出力を
持っています。同じ C の宣言が異なる Rust の型になることがあります。

| 項目 | Windows | macOS |
|---|---|---|
| `PF_WorldFlag_DEEP` など `PF_WorldFlag_*` 定数 | `c_int`（`i32`） | `c_uint`（`u32`） |
| `PF_EffectWorld::world_flags`（`A_long`） | `i32` | `i32` |

`world.world_flags & PF_WorldFlag_DEEP` は Windows ではコンパイルでき、macOS では失敗します。
手書きの `as i32` / `as u32` は片方のプラットフォームでしか通らないか、黙って切り詰めがちです。
両辺を広げて比較します。

```rust
use ae_plugin_kit::sdk::{has_flag, same_bits};

if has_flag(world.world_flags, ae::sys::PF_WorldFlag_DEEP) { /* ... */ }
assert!(same_bits(a_long_value, some_u32_constant));
```

`ae_plugin_kit::sdk::SdkInt` は 8/16/32 bit の符号付き・符号なし整数を正確に `i64` へ広げます。
レイヤーには `ae_plugin_kit::ae::has_world_flag(layer, flag)` を使えます。

## ピクセル深度は World Suite で判定する

固定しているラッパーの `Layer::bit_depth()` は `PF_WorldFlag_DEEP` があると常に16を返します。
AE は 32 bpc（ARGB128）のワールドにもこのフラグを立てるため、float のワールドが16 bit 整数として
読まれてしまいます。`ae_plugin_kit::ae::pixel_depth(layer)` を使ってください。World Suite に
`PF_PixelFormat_ARGB32 / ARGB64 / ARGB128` を問い合わせ、それ以外（`GPU_BGRA128` を含む）は拒否します。

## プラットフォーム固有のコード

- Windows 専用 API（`nvcuda.dll` 経由の CUDA、D3D12、DirectX のフラグ）は `#[cfg(windows)]`
  で囲み、そのクレートは `[target.'cfg(windows)'.dependencies]` にします。probe プラグインの
  CUDA 経路はこの形で、macOS では `SmartRenderGpu` を拒否して GPU ワールドの情報だけを記録します。
- Metal のコードは `#[cfg(target_os = "macos")]` で囲みます（track 03 参照）。
- **`build.rs`** の中の `#[cfg(target_os)]` はターゲットではなくビルドするホストを表します。
  `CARGO_CFG_TARGET_OS` を読みます。`ae_plugin_build::build` はターゲットに合わせて
  `CodeWin64X86` または `CodeMacIntel64` + `CodeMacARM64` を追加します。
- バイト順: AE の ARGB ワールドはネイティブエンディアン。対応する CPU はどちらもリトルエンディアンで、
  `ae_plugin_kit::pixels` は `from_ne_bytes` を使います。

## macOS 向けビルド

- Mac 上でビルドします。`pipl::plugin_build` はビルド出力の隣に `<crate>.rsrc`、
  `<crate>_PkgInfo`、`<crate>_Info.plist` を書き出します。dylib と一緒に
  `<Name>.plugin/Contents/{MacOS,Resources}` へまとめて署名します。バンドル化スクリプトは
  まだテンプレートに含まれていません。
- ユニバーサルバイナリ: `aarch64-apple-darwin` と `x86_64-apple-darwin` をビルドして `lipo` で結合します。
- CI（`.github/workflows/ci.yml`）は macOS arm64 / x86_64 ランナーでワークスペースをビルド・テスト
  します。バンドル化、署名、プラグインの読み込みは行いません。
- Windows 上でも `cargo check --target aarch64-apple-darwin` で型検査できます（cfg の誤り
  の発見に便利）。依存の `oslog` のために C コンパイラが必要で、生成される PiPL リソースは
  仮のものです。リンクやバンドル化はできません。

## Windows の PiPL リソース

固定している `pipl` クレートは PiPL をエスケープした文字列リテラルとして `.rc` に書きます。
`rc.exe` は文字列リテラルをアクティブなコードページで再エンコードするため、日本語 Windows（CP932）
では 0x80 以上のバイト（非 ASCII の名前や一部のバージョン・フラグのバイト）が壊れます。
`ae_plugin_build` は正確なバイト列を `$OUT_DIR/effect.pipl` に書き、リソース `16000 PiPL`
としてファイルごと埋め込みます。`rc.exe` はファイルの内容をそのままコピーします。
