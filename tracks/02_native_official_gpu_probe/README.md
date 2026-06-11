# 02 Native / Official GPU Probe

Purpose: prepare for AE's official GPU path and native backends without mixing it into the working SmartFX + wgpu path.

Current implementation lives in:

- `crates/ae_gpu_lab_native_probe`

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
  width / height
  format
  access
  native handle
```

`AeGpuFrameProbe::classify()` rejects unsupported frameworks, missing frame handles, and mismatched handle/framework pairs before any backend-specific unsafe code runs.

Do not put wgpu-hal import code here. This track only classifies AE/native GPU state.

Next work:

- Wire `PF_Cmd_GPU_DEVICE_SETUP` into `AeGpuDeviceProbe`.
- Wire `PF_Cmd_GPU_SMART_RENDER_GPU` input/output checkout into `AeGpuFrameProbe`.
- Record exact AE/Premiere SDK suite calls needed to obtain native device/context/queue/frame handles.
