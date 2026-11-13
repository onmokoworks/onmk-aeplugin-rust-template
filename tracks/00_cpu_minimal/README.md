# 00 CPU Minimal (recommended starting point)

Purpose: the smallest complete CPU effect that gets the AE boundary right.
Copy this crate when starting a new effect.

Crate: `tracks/00_cpu_minimal/crates/ae_minimal_plugin` -> `AeMinimal.aex`
(match name `ONMK AeMinimal`, category `ONMK Template`).

What it demonstrates:

- SmartFX (`SmartPreRender` / `SmartRender`) and legacy `Render`.
- 8 / 16 / 32 bpc (`DeepColorAware` + `FloatColorAware`). The depth comes from
  the World Suite pixel format (`ae_plugin_kit::ae::pixel_depth`), not from
  `Layer::bit_depth()`.
- Every world is validated (signed stride, alignment, NULL, overflow) before
  any pixel access (`ae_plugin_kit::ae::read_layer` / `write_layer`).
- Every layer checked out in `SmartRender` is checked in on success, error and
  panic (`ae_plugin_kit::with_checkouts`).
- Straight alpha at the boundary: Invert works on straight RGB; Box Blur and
  Blend Layer are computed premultiplied and un-premultiplied once.
- Row-parallel, deterministic Box Blur with an AVX horizontal pass and a
  bit-identical scalar fallback.
- PiPL embedded as a binary resource on Windows (`ae_plugin_build`).
- Parameters: `Mode` popup (Invert / Box Blur / Blend Layer, default Box Blur),
  `Amount` (%), `Blur Radius`, `Blend Layer` (layer parameter). Comments mark
  the indices that must stay stable.

Layout:

```text
build.rs        PiPL (name, match name, flags) via ae_plugin_build
src/lib.rs      AE glue: params, Render, SmartPreRender, SmartRender
src/effect.rs   host-independent render logic + unit tests
```

Build, test, smoke:

```powershell
cargo test --release -p ae_minimal_plugin
.\scripts\install-ae-plugin.ps1 -Plugin minimal          # dist\AeMinimal.aex
python scripts/aexcompat_smoke.py --harness <aexcompat-harness.exe> --plugin dist\AeMinimal.aex --output local\smoke-minimal
```

Starting a new effect from it:

1. Copy the crate folder, rename the package, `Property::Name` and the
   `.aex` entry in `scripts/ae-plugin-deploy.ps1`.
2. Choose a new, permanent match name (see
   [parameter compatibility](../../docs/parameter-compatibility.md)).
3. Replace `effect.rs` and its tests; keep partial-alpha fixtures with
   independent expected values ([alpha contract](../../docs/alpha-contract.md)).

Not covered yet:

- Not run in After Effects. Unit tests and the optional AEXCompat smoke
  (8/16/32 bpc x Render/SmartRender, Invert and Box Blur) pass; Blend Layer,
  MFR and resize behaviour are untested outside unit tests.
- The blur is clipped to the input bounds (the result rect is not expanded).
- `SupportsThreadedRendering` (MFR) is not enabled.
