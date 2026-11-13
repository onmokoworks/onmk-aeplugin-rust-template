# AE エフェクトのアルファ契約

[English](./alpha-contract.md)

AE エフェクトの入出力は **ストレート RGB + alpha** とする（premultiplied ではない）。
8 bit の白は255、AE 16 bit の白は **32768**（65535ではない）、32 bit の白は1.0。
float の RGB は理由なく丸めない。HDR（1超）や負値も保持する。チャンネル順
（メモリ上の AE `ARGB` / 内部バッファの `RGBA`）とアルファ形式は別の仕様であり、混同しない。

根拠: [ProEXR マニュアル](https://www.fnord.com/ProEXR/ProEXR_Manual.pdf) 23ページの
AE 内部表現と抽出バッファの説明、および Adobe SDK `AE_EffectCB.h` の
`PF_BEGIN_SAMPLING` / `PF_END_SAMPLING` にある `PF_MF_Alpha_STRAIGHT`。
一般的な合成ライブラリの規約を確認せずに AE へ当てはめない。

## 実装の順序

1. 実装前に、入力・内部処理・出力それぞれのアルファ形式を明記する。内部で premultiplied に
   変換してフィルタする場合は、戻す境界と alpha=0 の扱いも決める。
2. エフェクトの役割から期待値を決める。
   - 生成色: RGB は coverage / opacity に依存しない。
   - マスクのみ: RGB を維持して alpha だけを変える。
   - 色調補正: 同じストレート RGB なら alpha によらず同じ補正結果。
   - 複数画像の over 合成: 乗算済みで合成し、最終 alpha で戻してから AE に返す。
3. 不透明画像だけでなく alpha=0, 1/255, 0.125, 0.5, 1 を含む入力を使う。白・黄・色付きの
   半透明画素、alpha=0 の下に隠れた色、16 bit の低ビット、float の HDR / 負値を含める。
4. CPU と GPU の一致だけで合格にしない。同じ誤りを共有できるため、仕様から独立に計算した
   期待値と照合する。変換の往復テストだけでなく、読み取りと書き込みを別々に検査する。
5. ぼかし・変形は「不透明な白＋透明な黒／別色」で確認する。内部で alpha-weighted な処理が
   必要。縁の問題を RGB の乗算を全部消すことで「修正」しない。
6. ホストハーネスがあれば（例: [AEXCompat](https://github.com/onmokoworks/AEXCompat)、任意）、
   Render / SmartRender × 8 / 16 / 32 bit の生データを比較する。実行していない経路は明示し、
   合格件数に含めない。PNG の見た目だけでは判定しない。
7. 結果をストレートとして白・黒・灰色・市松に合成する。生成した白は白背景上で部分透明の縁が
   暗くならないこと。テスト自身が意図的な premultiplied 出力を確実に落とすことも確認する。
8. 成果物の SHA-256、ソースのリビジョン、入力とパラメータ、深度と経路、期待値、実行コマンドを
   残す。古いバイナリの合格を現在のソースの証拠にしない。

## このテンプレートの道具

| 場所 | 内容 |
|---|---|
| `ae_plugin_kit::pixels` | AE ARGB 行コーデック（8/16/32 bpc）。順序とスケールだけを変え、アルファ形式は変えない |
| `ae_plugin_kit::alpha` | `premultiply` / `unpremultiply` と `check_straight`。部分 alpha を必須とし、premultiplied 出力を不合格にする |
| `tracks/00_cpu_minimal/.../effect.rs` | 参照実装: ストレートの画素単位 Invert、alpha-weighted な Box Blur、Blend Layer は乗算済み over。premultiplied 版と素朴なぼかしが不合格になるテスト付き |
| `tracks/01_smartfx_wgpu/.../cpu.rs`, `shader.wgsl` | Box Blur / Diffusion は CPU・GPU とも alpha-weighted。Chroma Warp はストレート RGB をそのまま参照しており未対応 |
| `scripts/alpha_contract.py` | パックされた生 RGBA ダンプ（ハーネス出力など）の検査ツール |

```powershell
python -m unittest discover -s scripts -p test_alpha_contract.py
python scripts/alpha_contract.py --raw output.rgba16le --format rgba16le --contract constant-color --color 1 0.8666666666666667 0.2
python scripts/alpha_contract.py --raw output.rgba32f-le --format rgba32f-le --contract identity --reference input.rgba32f-le
python scripts/alpha_contract.py --raw output.rgba16le --format rgba16le --contract alpha-only --reference input.rgba16le
```

このスクリプトの入力は **パックされた RGBA** の生データ。AE の stride 付き ARGB バッファは
先に並べ替える。幅・高さ・stride の正しさや期待する alpha 形状は、各プラグインのテストで
確認する。`constant-color` は生成エフェクト用で、任意のグロー・色調補正には使わない。
`alpha-only` は RGB 不変条件だけを検査する。空画像や不透明のみの画像では成功しない。

`scripts/alpha_contract.py` が正本。下流リポジトリはバイト単位で同一のコピーを持ち、
`--compare-tool <もう一方のコピー>` で差分を検出できる。最新版を自動取得せず、
両方を意図的に更新する。検査ツールの CI 成功はツール自体のテスト成功にすぎず、
各プラグインの透明画素検査の代わりにはならない。
