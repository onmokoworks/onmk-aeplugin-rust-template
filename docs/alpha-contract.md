# Alpha Contract for AE Effects

[日本語](./alpha-contract.ja.md)

AE effect input and output are **straight RGB + alpha**, not premultiplied.
8 bpc white is 255, AE 16 bpc white is **32768** (not 65535), 32 bpc white is
1.0. Float RGB is not clamped without a reason: HDR (> 1) and negative values
must survive. Channel order (AE `ARGB` in memory, `RGBA` in most internal
buffers) and alpha association are two separate properties; do not mix them up.

Sources: the [ProEXR manual](https://www.fnord.com/ProEXR/ProEXR_Manual.pdf),
page 23 (AE's internal representation and extracted buffers), and
`PF_MF_Alpha_STRAIGHT` in `PF_BEGIN_SAMPLING` / `PF_END_SAMPLING` of the
Adobe SDK header `AE_EffectCB.h`. Do not transfer the conventions of general
compositing libraries to AE without checking.

## Implementation order

1. Before writing code, state the alpha form of the input, of the internal
   processing and of the output. If a filter premultiplies internally, also
   decide where it un-premultiplies and what happens at alpha = 0.
2. Derive the expected result from the effect's role:
   - Generated colour: RGB does not depend on coverage / opacity.
   - Mask-only effects: keep RGB, change alpha only.
   - Colour correction: the same straight RGB grades to the same result at
     every alpha.
   - Over-compositing several images: composite premultiplied, then
     un-premultiply by the final alpha before returning to AE.
3. Test with more than opaque images: use alpha = 0, 1/255, 0.125, 0.5 and 1.
   Include white, yellow and coloured semi-transparent pixels, colour hidden
   under alpha = 0, low bits at 16 bpc, and HDR / negative floats.
4. CPU/GPU agreement is not a pass: both can share the same mistake. Compare
   against an expected result computed independently from the specification.
   Test reading and writing separately, not only an encode/decode roundtrip.
5. Check blurs and distortions with "opaque white next to transparent black or
   another hidden colour". They must be alpha-weighted internally. Do not
   "fix" fringes by deleting every RGB multiplication.
6. Where a host harness is available (for example
   [AEXCompat](https://github.com/onmokoworks/AEXCompat), optional), compare
   raw output data for Render / SmartRender x 8 / 16 / 32 bpc. Name the routes
   you did not run and do not count them as passed. A PNG preview alone is not
   a verdict.
7. Composite the result as straight over white, black, gray and a checkerboard.
   Generated white must not darken its soft edges over white. Also prove that
   the test rejects an intentionally premultiplied output.
8. Record the artifact SHA-256, source revision, inputs and parameters, depths
   and routes, the expected values and the command. A pass of an older binary is
   not evidence for the current source.

## Tools in this template

| Where | What |
|---|---|
| `ae_plugin_kit::pixels` | AE ARGB row codecs (8/16/32 bpc) that change order and scale only, never association |
| `ae_plugin_kit::alpha` | `premultiply` / `unpremultiply` and `check_straight`, which needs partial alpha and rejects premultiplied output |
| `tracks/00_cpu_minimal/.../effect.rs` | Reference effect: straight per-pixel Invert, alpha-weighted Box Blur, premultiplied over for Blend Layer; tests include premultiplied and naive-blur mutants that must fail |
| `tracks/01_smartfx_wgpu/.../cpu.rs`, `shader.wgsl` | Box Blur / Diffusion are alpha-weighted on CPU and GPU. Chroma Warp still samples straight RGB and is a known gap |
| `scripts/alpha_contract.py` | Checker for packed raw RGBA dumps (for example from a harness) |

```powershell
python -m unittest discover -s scripts -p test_alpha_contract.py
python scripts/alpha_contract.py --raw output.rgba16le --format rgba16le --contract constant-color --color 1 0.8666666666666667 0.2
python scripts/alpha_contract.py --raw output.rgba32f-le --format rgba32f-le --contract identity --reference input.rgba32f-le
python scripts/alpha_contract.py --raw output.rgba16le --format rgba16le --contract alpha-only --reference input.rgba16le
```

The script reads **packed RGBA** raw data. Reorder AE's strided ARGB buffers
first. Width, height, stride and the expected alpha shape are the job of each
plug-in's own tests. `constant-color` is for generators; do not apply it to an
arbitrary glow or grade. `alpha-only` checks only that RGB is unchanged. Empty
and opaque-only images never pass.

`scripts/alpha_contract.py` is the canonical copy. Downstream repositories may
keep a byte-identical copy and detect drift with
`--compare-tool <other copy>`; update both deliberately instead of fetching the
latest version automatically. A green CI run of the checker only proves the
checker's own tests; it does not replace transparent-pixel checks of each
plug-in.
