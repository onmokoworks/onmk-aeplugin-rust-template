# Prior Art: AE GPU Rust Plugin Template

Goal: design a reusable Windows/macOS Rust template for After Effects GPU plug-ins.

This is not mainly about shipping one effect. The useful artifact is a template that separates:

- AE host integration
- CPU fallback
- wgpu upload/readback backend
- native GPU backend experiments
- shader/effect authoring
- benchmark and validation tools

## Main Classification

### 1. Internal GPU Acceleration Inside A Normal SmartFX Effect

AE sees a normal SmartFX effect. The plug-in checks out CPU pixel buffers, uploads them to its own GPU API, runs compute/render work, reads back, and writes the CPU output buffer.

Examples and references:

- Tweak Shader After Effects  
  https://mobilebungalow.itch.io/tweak-shader-after-effects
- tweak_shader crate  
  https://lib.rs/crates/tweak_shader
- Vulkanator / GLator style experiments  
  https://github.com/Wunkolo/Vulkanator

This is the closest path to the current `onmk-aeplugin-rust-template` project.

Pros:

- Works with ordinary AE SmartFX lifecycle.
- wgpu is practical here.
- Cross-platform Windows/macOS is plausible.
- Good for a clean reusable Rust template.
- Easier to benchmark and debug.

Cons:

- Requires upload/readback.
- Does not mean the effect is using AE's official GPU render path.
- Transfer cost may dominate small/light effects.

Template implication:

- Make this path the first supported backend.
- Treat it as useful production infrastructure, not only a throwaway prototype.
- Measure upload, dispatch, readback, and CPU copy separately.

## 2. Adobe Official GPU Path / Mercury-style Integration

AE drives a GPU render path. The plug-in receives GPU frames and is expected to render using the framework/device selected by AE.

Important selectors and flags:

- `PF_Cmd_GPU_DEVICE_SETUP`
- `PF_Cmd_GPU_DEVICE_SETDOWN`
- `PF_Cmd_SMART_PRE_RENDER`
- `PF_Cmd_GPU_SMART_RENDER_GPU`
- `PF_OutFlag2_SUPPORTS_GPU_RENDER_F32`

References:

- After Effects SDK GPU selectors  
  https://ae-plugins.docsforadobe.dev/print_page/
- `PF_OutFlag2_SUPPORTS_GPU_RENDER_F32`  
  https://ae-plugins.docsforadobe.dev/effect-basics/PF_OutData/

Key point:

The effect is not considered GPU-rendering-capable unless it responds to GPU device setup and advertises the proper GPU render support flags for that framework/device.

Pros:

- Real AE GPU path.
- Potentially avoids CPU readback.
- Better long-term direction for commercial-quality GPU effects.

Cons:

- Requires native GPU API handling.
- The backend matrix is platform/framework-specific.
- Rust bindings may need custom FFI around AE/Premiere SDK suites.
- wgpu alone is not enough; AE owns the GPU framework/device context.

Template implication:

- Design the core around a backend trait.
- Keep official GPU path in separate crates/modules.
- Start with native D3D/Metal/CUDA backend experiments after SmartFX+wgpu is stable.

## 3. Zero-copy / GPU Resource Interop

This is the hardest layer: import or wrap host/native GPU buffers/textures without copying to CPU.

Closest reference:

- Gyroflow technology notes  
  https://docs.gyroflow.xyz/app/technical-details/used-technologies
- Gyroflow Adobe plug-in docs  
  https://docs.gyroflow.xyz/app/video-editor-plugins/davinci-resolve-openfx-1

Gyroflow supports GPU processing through OpenCL or wgpu with CPU fallback, and its plug-in zero-copy path handles several native resource types: OpenCL buffers, CUDA buffers, Metal buffers/textures, Vulkan textures, and DirectX11 textures.

Pros:

- Most relevant precedent for GPU interop.
- Shows that cross-platform video plug-in zero-copy is possible.

Cons:

- Described as challenging even in a mature project.
- Not AE-specific only; it spans video editor plug-in APIs and native graphics APIs.
- wgpu interop with foreign resources is not a stable/simple public API story.

Template implication:

- Do not make this the first implementation target.
- Create an `experimental_external_import` backend boundary.
- Keep resource ownership, synchronization, and format conversion isolated.
- Reuse the abstraction and tests even if the backend is replaced.

Current project boundary:

- `crates/ae_gpu_lab_native_probe` models the AE/native GPU observation step.
- `crates/ae_gpu_lab_wgpu_hal_import` converts native observations into an external frame reference and runs preflight checks.
- No unsafe import is attempted until backend, format, extent, handle type, ownership, and synchronization questions are answered.

## 4. Shader Authoring Frameworks For AE

These are important for UX and effect authoring, even if their GPU path differs.

References:

- ISF4AE  
  https://github.com/baku89/ISF4AE
- ISF spec  
  https://github.com/mrRay/ISF_Spec
- ISF documentation  
  https://isf.video/

ISF4AE maps shader metadata to AE UI parameters and layer inputs. It is useful prior art for:

- shader parameter metadata
- uniform binding
- multiple inputs
- custom AE behavior around shader effects
- limitations of real-time shader formats inside AE

Template implication:

- Consider a WGSL-first metadata format later.
- Keep `params -> uniform` mapping explicit and generated where possible.
- Support multiple input layers as a first-class design goal.

## 5. Rust AE Plug-in Foundation

Reference:

- `virtualritz/after-effects` Rust bindings  
  https://github.com/virtualritz/after-effects

This is the practical Rust route for AE/Premiere SDK integration. Existing projects in this workspace already use it.

Template implication:

- Use `after-effects` crate for normal SmartFX first.
- Keep unsafe FFI localized.
- Reuse the workspace's existing plugin build/install conventions where possible.

## Recommended Reading Order

1. Tweak Shader AE / `tweak_shader`
   - Closest to SmartFX + wgpu.
   - Look for render flow, layer checkout, row stride handling, state lifetime, and readback.

2. Adobe SDK GPU selectors
   - Defines what "official GPU path" really means.
   - Focus on GPU device setup, GPU smart render, and render support flags.

3. Gyroflow
   - Best reference for zero-copy/native resource interop.
   - Read it as a hard-mode future path, not the initial template.

4. ISF4AE / ISF
   - Best reference for shader metadata and AE parameter mapping.

5. Vulkanator / GLator-like samples
   - Useful for native GPU API lifecycle inside AE plug-ins.

## Template Direction

The useful target is not "a single GPU effect." It is:

```text
Rust + after-effects crate
+ AE SmartFX
+ wgpu/WGSL upload-readback backend
+ CPU fallback
+ clean shader/parameter binding
+ benchmark harness
+ isolated future official-GPU/native-interop backends
```

The first stable milestone should be a reusable SmartFX + wgpu skeleton. The native/official GPU paths should be designed into the architecture, but implemented later as isolated backends.
