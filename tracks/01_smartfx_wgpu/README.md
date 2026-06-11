# 01 SmartFX + wgpu

Purpose: reusable Windows/macOS Rust GPU template where AE sees a normal SmartFX effect.

Current implementation lives in:

- `crates/ae_gpu_lab_core`
- `crates/ae_gpu_lab_cli`
- `crates/ae_gpu_lab_plugin`

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

Next work:

- Add `FrameDesc`, `PixelFormat`, and rowbytes-aware frame views.
- Connect the plugin crate to real SmartFX checkout/checkin.
- Add 8-bit / 16-bit / 32f conversion tests.
