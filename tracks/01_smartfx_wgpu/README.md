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

AE validation still required:

- Verify 8-bit, 16-bit, and 32f projects after AE becomes available.
- Confirm repeated renders, device-loss fallback, and plug-in reload behavior.
