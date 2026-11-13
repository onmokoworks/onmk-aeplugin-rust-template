# 検証

[English](./verification.md)

証拠には段階があります。主張がどの段階に基づくかを明記し、低い段階を高い段階として示さないでください。

| 段階 | 示せること | 方法 |
|---|---|---|
| 1. ユニットテスト | 描画ロジック、コーデック、ワールド検証、後始末、アルファ判定が仕様どおりに動く | `cargo test --release` |
| 2. CI | 移植可能なクレートが Windows x64、macOS arm64、macOS x86_64 でビルドされテストに通る | `.github/workflows/ci.yml` |
| 3. ヘッドレスのホストハーネス（任意） | ビルドした `.aex` が読み込め、パラメータを列挙でき、Render / SmartRender の 8/16/32 bpc で期待どおりの生ピクセルを出す | 下記の AEXCompat |
| 4. After Effects | 実ホストの挙動: UI、キャッシュ、MFR、GPU、保存済みプロジェクト、他のホストバージョン | AE での手動確認 |

段階 1〜3 は段階 4 の **代わりになりません**。ソースの一覧化や計算だけのテストは AE での検証ではありません。

## ローカルでの確認

```powershell
cargo fmt --all --check
cargo build --release
cargo test --release
cargo clippy --release -p ae_plugin_kit --all-features -p ae_plugin_build -p ae_minimal_plugin --all-targets -- -D warnings
python -m unittest discover -s scripts -p "test_*.py"
```

Python のテストはアルファ検査ツール、スモークテストの判定関数、インストーラ（一時ディレクトリ内の
偽のルートに対して。Windows のみ）を対象にします。

GPU アダプタが必要なテストは `#[ignore]` です。明示的に実行します:
`cargo test --release -p ae_gpu_lab_core -- --ignored`

## AEXCompat によるヘッドレスのスモークテスト（任意）

[AEXCompat](https://github.com/onmokoworks/AEXCompat) は別プロジェクトで、`.aex` を
After Effects の外で読み込み、JSON のフィクスチャを classic Render または SmartRender で
指定したピクセル形式で描画するヘッドレスハーネスを持っています。**任意** です。このテンプレートの
ビルドやテストには不要で、CI でも実行しません。

`scripts/aexcompat_smoke.py` は AeMinimal 用の、外部パッケージ不要の小さな例です。

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin minimal          # dist\AeMinimal.aex をビルド（インストールしない）
python scripts/aexcompat_smoke.py `
    --harness <path>\aexcompat-harness.exe `
    --plugin dist\AeMinimal.aex `
    --output local\smoke-minimal                          # まだ存在しないフォルダ
```

深度（`argb8`、`argb16`、`argb32f`）と経路（`classic`、`smart`）ごとに:

- `invert`: 部分 alpha の縞模様に Mode = Invert。alpha は不変で、ストレート RGB が反転していること。
  ハーネス自身の入力チェックポイントと比較します。
- `blur-edge`: 不透明な白と、RGB が赤の透明画素を並べ、Mode = Box Blur。見えている画素は
  すべて白のままで、部分 alpha が生じること。

期待値はプラグインではなく仕様から求めます。`scripts/test_aexcompat_smoke.py` は、判定関数が
premultiplied の出力、暗い縁や赤い縁、不透明だけのフィクスチャを不合格にすることを確認します。
`summary.json` にはプラグイン・ハーネス・実行スクリプトの SHA-256、各ケースとエラー、制約
（AE ではない、8 bit PNG 入力、MFR / GPU / レイヤーパラメータ / リサイズは対象外）を記録します。

自分のエフェクトに合わせるには、ケースごとのパラメータ値を変え、エフェクトの本来の役割から
導いた判定関数を書きます。否定対照も残してください。別のプラグイン（例えば素通しの
`AeGpuProbe.aex`）に対してスモークを実行すると失敗しなければなりません。

ハーネスの結果は、そのハーネスと実行したフィクスチャについての証拠にすぎません。
実行していない経路は対象外として報告します。

## リリースが AE で動くと言う前に

- 成果物の SHA-256 とソースのコミットを記録する。
- 8 / 16 / 32 bpc のプロジェクト、Render と SmartRender のホスト、部分 alpha
  （[アルファ契約](alpha-contract.ja.md)）、前のバージョンで保存したプロジェクト
  （[パラメータの互換性](parameter-compatibility.ja.md)）を確認する。
- `scripts/install-ae-plugin.ps1 -Install` でインストールし（バックアップ・検証付き）、
  `scripts/restore-ae-plugin.ps1` で元に戻す。
