# 01 SmartFX + wgpu

Purpose: reusable Windows/macOS Rust GPU template where AE sees a normal SmartFX effect.

Current implementation lives in:

- `tracks/01_smartfx_wgpu/crates/ae_gpu_lab_core`
- `tracks/01_smartfx_wgpu/crates/ae_gpu_lab_cli`
- `tracks/01_smartfx_wgpu/crates/ae_gpu_lab_plugin`

Flow:

```text
AE CPU frame
  -> FrameRgba8 / future FrameDesc
  -> wgpu upload
  -> WGSL compute
  -> staging readback
  -> AE CPU output
```

Current status:

- CPU backend and wgpu backend share `RenderBackend`.
- wgpu backend reuses buffers and pipelines for same-size frames.
- CLI reports upload / encode / submit / map wait / copy timing.
- `FrameDesc`, `PixelFormat`, `FrameView`, and `FrameViewMut` validate rowbytes
  and buffer extents before conversion.
- AE ARGB 8-bit, 16-bit, and 32f CPU worlds convert through the same tested
  RGBA8 boundary. Row padding is preserved.
- The plug-in refuses `SmartRenderGpu`; this track intentionally uses CPU
  SmartFX worlds and performs its own wgpu upload/readback.
- The world depth comes from the World Suite pixel format
  (`ae_plugin_kit::ae::world_layout`); `Layer::bit_depth()` reported 16 for
  32 bpc float worlds. Worlds are validated before `Layer::buffer()` is used;
  negative strides are rejected.
- `DeepColorAware` / `FloatColorAware` are advertised, so AE actually delivers
  16/32 bpc worlds. The internal frame is RGBA8, so 16/32 bpc input is
  quantized to 8 bits by design (benchmark track).
- `SmartRender` checks the input back in on error and panic paths
  (`ae_plugin_kit::with_checkouts`).
- Alpha: frames are straight RGBA. Box Blur and Diffusion are alpha-weighted on
  CPU and in WGSL ([alpha contract](../../docs/alpha-contract.md)); Chroma Warp
  still samples straight RGB from neighbours (known gap). An ignored test
  (`cargo test -p ae_gpu_lab_core -- --ignored`) checks the GPU Box Blur
  against an independent expected result.

AE validation still required:

- Verify 8-bit, 16-bit, and 32f projects after AE becomes available.
- Confirm repeated renders, device-loss fallback, and plug-in reload behavior.
