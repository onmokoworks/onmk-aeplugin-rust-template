# 02 Native / Official GPU Probe

Purpose: prepare for AE's official GPU path and native backends without mixing it into the working SmartFX + wgpu path.

Current implementation lives in:

- `tracks/02_native_official_gpu_probe/crates/ae_gpu_lab_native_probe`
- `tracks/02_native_official_gpu_probe/crates/ae_gpu_lab_cuda`
- `tracks/02_native_official_gpu_probe/crates/ae_gpu_lab_probe_plugin`

Target AE concepts:

```text
PF_Cmd_GPU_DEVICE_SETUP
PF_Cmd_GPU_DEVICE_SETDOWN
PF_Cmd_SMART_PRE_RENDER
PF_Cmd_GPU_SMART_RENDER_GPU
PF_OutFlag2_SUPPORTS_GPU_RENDER_F32
```

Backend targets:

```text
Windows:
  D3D11 probe
  D3D12 probe
  CUDA probe

macOS:
  Metal probe
```

This track should answer:

- Which GPU framework did AE select?
- Can the effect advertise F32 GPU render support?
- What device/context/queue handles are available?
- What frame/resource handle shape does AE provide?
- Which native backend should own rendering?

Current typed boundary:

```text
AeGpuDeviceProbe
  framework
  device_index
  can_render_f32

AeGpuFrameProbe
  device
  width / height / row_bytes / byte_len
  format
  access
  native handle

AeGpuDeviceContext
  framework / device index
  platform / device / context / command queue handles

NativeRenderJob
  validated device context
  read-capable input frame
  write-capable output frame
```

`AeGpuFrameProbe::classify()` rejects unsupported frameworks, missing frame handles, and mismatched handle/framework pairs before any backend-specific unsafe code runs.

The current contract is buffer-first. AE GPU SmartRender uses
`PF_PixelFormat_GPU_BGRA128`, so a GPU world is modeled as a pitched float buffer
unless a real-machine probe proves that a backend exposes a texture.

Do not put wgpu-hal import code here. This track only classifies AE/native GPU state.

Next work:

- Add native Metal BGRA128 copy/invert using `MTLBuffer`.
- Add a DirectX implementation only after the exact `ID3D12Resource` shape and
  resource state are observed.
- Keep the existing CUDA Driver API path as the first working native backend.
