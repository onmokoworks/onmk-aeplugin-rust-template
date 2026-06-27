# onmk-aeplugin-rust-template

Rust template experiments for Adobe After Effects plug-ins.

The goal is not to ship one polished effect. The useful artifact is a reusable pipeline for future AE plug-in work:

- SmartFX / SmartRender integration from Rust
- CPU fallback
- `wgpu` compute with upload/readback
- AE official GPU path probing
- native CUDA experiments
- isolated `wgpu-hal` / external import research

## Crates

```text
crates/ae_gpu_lab_core
  Shared frame types, CPU effects, and wgpu upload/readback backend.

crates/ae_gpu_lab_cli
  Local benchmark runner outside AE.

crates/ae_gpu_lab_plugin
  SmartFX + wgpu experiment. Builds AeGpuLab.aex.

crates/ae_gpu_lab_probe_plugin
  AE official GPU path probe and native CUDA experiment. Builds AeGpuProbe.aex.

crates/ae_gpu_lab_wgpu_hal_import
  Experimental external import boundary and preflight checks.
```

## Build

Run the CLI benchmark:

```powershell
cargo run -p ae_gpu_lab_cli --release -- --effect box-blur --width 3840 --height 2160 --passes 3 --iterations 5
```

Build an AE plug-in into `dist/`:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin lab
.\scripts\install-ae-plugin.ps1 -Plugin probe
```

Build and install into the shared Adobe MediaCore folder on Windows:

```powershell
.\scripts\install-ae-plugin.ps1 -Plugin lab -InstallMediaCore
.\scripts\install-ae-plugin.ps1 -Plugin probe -InstallMediaCore
```

The install step asks for elevation because Adobe's shared MediaCore folder is under `CommonProgramFiles`.

## Tracks

- [01 SmartFX + wgpu](tracks/01_smartfx_wgpu/README.md)
- [02 Native / Official GPU Probe](tracks/02_native_official_gpu_probe/README.md)
- [03 wgpu-hal External Import](tracks/03_wgpu_hal_external_import/README.md)

## Current Notes

- Generated artifacts such as `target/`, `dist/`, `.dll`, `.aex`, `.pdb`, `.lib`, `.exp`, and `.log` are ignored.
- The probe plug-in writes diagnostics to the OS temp directory.
- The CUDA path is a research probe, not a production compatibility layer.
- `wgpu-hal` external import is intentionally isolated because real zero-copy interop is backend-specific.

## References

- [Prior Art](docs/prior-art.md)
- [Benchmark Effect Notes](docs/effects.md)
