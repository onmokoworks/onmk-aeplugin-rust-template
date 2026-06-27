# Benchmark Effect Notes

## Recommended order

1. `copy`
   Measures fixed transfer and mapping overhead. If this is already too slow, native GPU path work becomes more important.

2. `color`
   One-pixel-in, one-pixel-out math. Good for checking shader dispatch overhead without hiding it behind many samples.

3. `box-blur`
   Memory-heavy neighborhood reads. This resembles many practical AE filters and exposes row stride / cache behavior.

4. `diffusion`
   Repeated ping-pong passes. Good for command submission, temporary buffer reuse, and sync design.

5. `chroma-warp`
   Multi-sample coordinate warp. This is closer to stylize/distort effects and useful when comparing buffer vs texture implementations.

## Future native GPU path experiments

Keep native handle import behind an experimental backend boundary:

```text
FrameRef
  CpuFrame
  WgpuOwnedFrame
  AeNativeD3D
  AeNativeMetal
  AeNativeCuda

Processor
  CpuProcessor
  WgpuUploadReadbackProcessor
  CudaNativeProcessor
  AeNativeProcessor
  ExperimentalExternalImportProcessor
```

The reusable asset is the boundary. The native import code itself should be treated as replaceable.

## Current Measurement Shape

The CLI reports CPU-side timing buckets:

- `upload`: queue writes into GPU buffers
- `encode`: command encoder and compute pass recording
- `submit`: queue submission call
- `map_wait`: waiting for staging buffer mapping after dispatch/readback copy
- `copy`: mapped staging buffer copy into a Rust `Vec<u8>`

These are not GPU timestamp queries yet. They are intentionally simple because the first question is whether upload/readback overhead dominates before investigating native or zero-copy paths.
