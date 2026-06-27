# 03 wgpu-hal External Import

Purpose: isolated entrance for zero-copy interop research.

Current implementation lives in:

- `crates/ae_gpu_lab_wgpu_hal_import`

This track is intentionally separate because the import code will be unstable and backend-specific.

Minimum questions before real import:

- Does AE's native resource backend match the active wgpu-hal backend?
- Is the resource a texture/buffer type that can be imported or wrapped?
- Is the pixel format supported?
- Who owns synchronization?
- Who owns layout/resource state transitions?
- Is the resource read-only, write-only, or read-write for this pass?
- Can the resource outlive the wgpu-hal object that wraps it?

Initial flow:

```text
AE official GPU probe
  -> ExternalFrameRef
  -> ImportPreflight
  -> backend-specific unsafe import PoC
```

Rule:

The reusable asset is the boundary and preflight logic. The actual unsafe import implementation is replaceable.

Current typed boundary:

```text
from_ae_probe(AeGpuFrameProbe) -> Option<ExternalFrameRef>
preflight(ExternalFrameRef, active_backend) -> ImportPreflight
ImportPreflight::validate()
```

The current implementation only validates:

- declared backend matches the active backend
- format is not unknown
- extent is nonzero
- handle variant matches declared backend

Next work:

- Run the SmartFX + wgpu path inside AE before attempting unsafe import.
- Wire AE official GPU probe data into `AeGpuFrameProbe`.
- Decide whether the first unsafe PoC should target D3D12 or Metal.
- Replace the D3D12 or Metal placeholder importer with a backend-specific unsafe PoC.

Backend-specific entry points:

- `src/d3d12.rs`: D3D12 import plan and placeholder importer
- `src/metal.rs`: Metal import plan and placeholder importer

These importers intentionally return "not implemented" after preflight. They are present to make synchronization, ownership, and write-access decisions explicit before any real resource wrapping happens.

## Suggested Real-machine Test Order

1. Run the CLI benchmark outside AE.
   Confirm `copy` checksum and upload/readback timing.

2. Build/load the SmartFX + wgpu plugin in AE.
   Confirm device creation, repeated renders, plugin reload, and fallback behavior.

3. Add an AE official GPU probe that only logs framework/device/frame facts.
   Do not import the frame yet.

4. Convert logged frame facts into `AeGpuFrameProbe` and `ExternalFrameRef`.
   Confirm preflight results for D3D12 on Windows and Metal on macOS.

5. Attempt the first unsafe import PoC on one backend only.
   Prefer read-only input import before trying write/read-write output.

## First Hot Probe Result On Windows/NVIDIA

Observed from `AeGpuProbe` hot mode:

```text
SmartPreRender what_gpu=Cuda device=0 bit_depth=8
SmartRenderGpu what_gpu=Cuda device=0 bit_depth=8
GPUDeviceInfo device_framework=3 compatibleB=1
contextPV=<non-null>
command_queuePV=<non-null>
input/output gpu_world_data=<non-null CUDA-side pointer>
input/output pixel_data_float_gpu=<same pointer>
```

Interpretation:

- AE is providing the official GPU SmartRender path.
- On this Windows/NVIDIA machine, AE selects CUDA, not DirectX.
- The hot probe currently does not write the GPU output world, so black output is expected.
- This is a native CUDA interop entry point, not a direct `wgpu-hal` entry point.

Implication for track 03:

```text
CUDA GPU world
  -> native CUDA backend candidate
  -> CUDA external memory / D3D interop research candidate
  -> not directly importable through ordinary wgpu-hal D3D12/Metal paths
```

Next branch:

- For product-like speed on this machine: add a `cuda_native_probe` / CUDA copy kernel first.
- For `wgpu-hal` import research: try to force/observe a DirectX GPU framework, or run the same probe on macOS/Metal.

## Metal / Apple Silicon Hypothesis

Metal on Apple Silicon is the most promising first target for `wgpu-hal` external
import research.

Reasoning:

- AE's official GPU path on Apple Silicon is expected to use Metal.
- `wgpu` on macOS normally uses the Metal backend.
- Apple Silicon has less GPU/API/driver variation than Windows.
- There is no CUDA-style alternate GPU universe on Apple Silicon.
- If AE hands the plug-in a real Metal texture or buffer, the active `wgpu-hal`
  backend may be the same API family as the host resource.

This does **not** mean that a working Metal import becomes generally portable.
It only means the backend mismatch problem is likely smaller than on Windows/NVIDIA.

Expected probe questions:

- Does AE report `what_gpu = Metal` in `GpuDeviceSetup`, `SmartPreRender`, and
  `SmartRenderGpu`?
- What do `platformPV`, `devicePV`, `contextPV`, and `command_queuePV` contain on
  macOS?
- Is `gpu_world_data` an `MTLTexture`, `MTLBuffer`, raw device pointer, or
  another AE-managed wrapper?
- Does `PixelDataSuite` expose a Metal-specific texture/buffer pointer for the
  same world?
- Does the input and output world share the same Metal device as the `wgpu`
  adapter/device?
- Is the world format `RGBA16Float`, `RGBA32Float`, BGRA, or an AE-specific
  layout?
- Who owns synchronization between AE's command buffer/queue and any wrapped
  `wgpu-hal` resource?
- Can the AE world outlive the imported/wrapped `wgpu-hal` object for the
  duration of the render pass?

Practical sequence:

```text
macOS / Apple Silicon AE probe
  -> log Metal device/queue/world facts
  -> convert to ExternalFrameRef
  -> run Metal preflight only
  -> try native Metal copy first
  -> attempt read-only wgpu-hal Metal import only after sync/ownership is clear
```

Important distinction:

```text
Native Metal backend
  AE Metal world -> Metal compute/render code
  likely useful as a practical fast path

wgpu-hal Metal import
  AE Metal world -> unsafe wrapper/import -> wgpu-hal/wgpu-side processing
  research path; backend-specific; not automatically portable
```

The reusable template asset should be the probe data model and preflight
boundary. The actual Metal import code should be treated as replaceable unsafe
backend code.

## CUDA Copy Probe

`AeGpuProbe` now includes an optional first native write test:

```text
Enable GPU Probe = on
Request GPU SmartRender = on
CUDA Copy Output = on
```

When AE selects CUDA, the plugin dynamically loads `nvcuda.dll`, uses AE's `contextPV` and
`command_queuePV`, and launches a tiny embedded PTX kernel against the AE GPU worlds.

This answers a narrower question than `wgpu-hal` import:

```text
Can Rust code launch native CUDA work against AE's official GPU world?
```

Current CUDA modes:

- `CUDA Copy Output`: pitch-aware BGRA128 input-to-output copy.
- `CUDA Invert Output`: pitch-aware RGB invert, preserving alpha.

Implementation note:

- The reusable CUDA Driver API/PTX code lives in `crates/ae_gpu_lab_cuda`.
- `AeGpuProbe` only gathers AE-specific context, stream, world pointers, extents,
  and row strides before calling the CUDA crate.

Current limitations:

- It synchronizes the AE CUDA stream after launch for easier first-pass debugging.
- It only runs on AE's CUDA GPU framework. DirectX/Metal import remains separate research.
- The diagnostics live in the plugin About message and the `AeGpuProbe.log` file.
