# CPU 性能のための土台

[English](./performance.md)

`crates/ae_plugin_kit` にはテスト済みの小さなパターンが2つあります。AeMinimal の Box Blur は両方を使っています。

## 上限付きの行並列（`ae_plugin_kit::parallel`）

```rust
use ae_plugin_kit::parallel::{for_each_row_chunk, workers_for, DEFAULT_MAX_WORKERS, DEFAULT_MIN_ELEMENTS};

let workers = workers_for(width * height * taps, DEFAULT_MIN_ELEMENTS, DEFAULT_MAX_WORKERS);
for_each_row_chunk(&mut out, width, workers, |first_row, rows| {
    for (r, row) in rows.chunks_exact_mut(width).enumerate() {
        let y = first_row + r;
        // 読み取り専用の入力から y 行目を書く
    }
});
```

- **グローバルなプールも、描画全体で共有する可変状態もない。** スレッドはスコープ付きで、
  呼び出しが戻る前に終わります。AE のマルチフレームレンダリング（MFR）で複数スレッドから
  同時に呼ばれても安全です。
- **上限付き。** 1回の呼び出しで最大 `max_workers` スレッド（既定8）。小さな処理は直列のままです。
  AE はすでにフレーム間で並列化しているので、全コアに合わせないでください。
- **決定的。** 行を連続したかたまりに分けます。各出力行が共有の読み取り専用入力だけに依存するなら、
  ワーカー数によらず結果はビット単位で同じです。テストは1〜12ワーカーを直列と比較します。
- 依存のあるパス（横方向の結果を読む縦方向パスなど）は別々の呼び出しにします。

AeMinimal は `SupportsThreadedRendering`（MFR）を設定していません。このヘルパーは対応済みですが、
MFR を有効にするにはスレッド安全なグローバル／シーケンスデータと
`SupportsGetFlattenedSequenceData` が必要で、AE 実機での確認も必要です。

## 実行時判定付き SIMD（`ae_plugin_kit::simd`）

例として 1次元の相関 `convolve_row` を用意しています。

1. `convolve_scalar` が演算の順序を正確に定義します。
2. `convolve_avx`（`#[target_feature(enable = "avx")]`）は各レーンで同じ IEEE 演算を同じ順序
   （乗算してから加算）で行います。FMA は丸めが1回になり結果が変わるため **使いません**。
3. ディスパッチャは実行時に `is_x86_feature_detected!("avx")` を確認するので、1つのバイナリが
   すべての x86_64 CPU で動きます。`allow_simd = false` を渡すと、グローバル変数や環境変数なしで
   テストからフォールバックを強制できます。

`runtime_dispatch_matches_forced_scalar_bit_for_bit` は乱数データと特殊値（符号付きゼロ、
非正規化数、±MAX、±無限大。NaN は NaN のまま）で両経路をビット単位で比較します。Rust は
`a + b * c` を勝手に FMA にしないため、スカラー経路はマシン間で安定しています。aarch64 では
スカラー経路を使い、NEON で自動ベクトル化されます。

新しいカーネルを書くとき: スカラーの参照実装を残し、SIMD 版で演算順序を正確に合わせ、端数は
スカラーで処理し、さらに最適化する前に強制スカラーとの一致テストを追加します。

## 計測

- `cargo run -p ae_gpu_lab_cli --release -- --cpu --effect box-blur ...` で AE の外で lab の CPU 経路を計測できます。
- AE 内での計測は release ビルドだけで行います。`after-effects` クレートの debug ビルドは
  パニックフックとロガーを登録します。
