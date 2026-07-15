//! Experimental wgpu-hal external import boundary.
//!
//! This crate is the entrance to zero-copy interop research. It does not import
//! any real resource yet. It defines the minimum information that a real import
//! attempt must prove before touching `wgpu-hal` unsafe APIs.

pub mod d3d12;
pub mod metal;

use ae_gpu_lab_native_probe::{
    AeGpuFrameProbe, AeGpuFramework, NativeFrameAccess, NativeHandle, NativePixelFormat,
};
use anyhow::{bail, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalBackend {
    D3D11,
    D3D12,
    Metal,
    Vulkan,
    Cuda,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalFormat {
    Rgba8Unorm,
    Bgra8Unorm,
    Rgba16Float,
    Rgba32Float,
    Unknown(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalFrameDesc {
    pub backend: ExternalBackend,
    pub width: u32,
    pub height: u32,
    pub format: ExternalFormat,
    pub access: ExternalAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalHandle {
    D3D11Texture2D(*mut core::ffi::c_void),
    D3D12Resource(*mut core::ffi::c_void),
    MetalTexture(*mut core::ffi::c_void),
    VulkanImage(u64),
    CudaDevicePtr(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalFrameRef {
    pub desc: ExternalFrameDesc,
    pub handle: ExternalHandle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportedFrameToken {
    pub backend: ExternalBackend,
    pub width: u32,
    pub height: u32,
    pub format: ExternalFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportPreflight {
    pub has_matching_backend: bool,
    pub has_supported_format: bool,
    pub has_nonzero_extent: bool,
    pub handle_matches_backend: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendImportPreflight {
    pub common: ImportPreflight,
    pub access_supported: bool,
    pub synchronization_defined: bool,
    pub ownership_defined: bool,
}

impl BackendImportPreflight {
    pub fn validate(self) -> Result<()> {
        self.common.validate()?;
        if !self.access_supported {
            bail!("external resource access mode is not supported by this backend path");
        }
        if !self.synchronization_defined {
            bail!("external resource synchronization contract is not defined");
        }
        if !self.ownership_defined {
            bail!("external resource ownership/lifetime contract is not defined");
        }
        Ok(())
    }
}

impl ImportPreflight {
    pub fn validate(self) -> Result<()> {
        if !self.has_matching_backend {
            bail!("external resource backend does not match the active wgpu-hal backend");
        }
        if !self.has_supported_format {
            bail!("external resource format is not supported by this import path");
        }
        if !self.has_nonzero_extent {
            bail!("external resource extent is empty");
        }
        if !self.handle_matches_backend {
            bail!("external handle type does not match its declared backend");
        }
        Ok(())
    }
}

pub trait ExternalImporter {
    fn active_backend(&self) -> ExternalBackend;

    fn import_frame(&self, frame: ExternalFrameRef) -> Result<ImportedFrameToken> {
        preflight(frame, self.active_backend()).validate()?;
        self.import_frame_unchecked(frame)
    }

    fn import_frame_unchecked(&self, frame: ExternalFrameRef) -> Result<ImportedFrameToken>;
}

pub trait BackendExternalImporter: ExternalImporter {
    fn backend_preflight(&self, frame: ExternalFrameRef) -> BackendImportPreflight;

    fn import_frame_checked(&self, frame: ExternalFrameRef) -> Result<ImportedFrameToken> {
        self.backend_preflight(frame).validate()?;
        self.import_frame_unchecked(frame)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MockExternalImporter {
    pub backend: ExternalBackend,
}

impl ExternalImporter for MockExternalImporter {
    fn active_backend(&self) -> ExternalBackend {
        self.backend
    }

    fn import_frame_unchecked(&self, frame: ExternalFrameRef) -> Result<ImportedFrameToken> {
        Ok(ImportedFrameToken {
            backend: frame.desc.backend,
            width: frame.desc.width,
            height: frame.desc.height,
            format: frame.desc.format,
        })
    }
}

pub fn preflight(frame: ExternalFrameRef, active_backend: ExternalBackend) -> ImportPreflight {
    ImportPreflight {
        has_matching_backend: frame.desc.backend == active_backend,
        has_supported_format: !matches!(frame.desc.format, ExternalFormat::Unknown(_)),
        has_nonzero_extent: frame.desc.width > 0 && frame.desc.height > 0,
        handle_matches_backend: handle_matches_backend(frame.handle, frame.desc.backend),
    }
}

pub fn conservative_backend_preflight(
    frame: ExternalFrameRef,
    active_backend: ExternalBackend,
) -> BackendImportPreflight {
    BackendImportPreflight {
        common: preflight(frame, active_backend),
        access_supported: frame.desc.access == ExternalAccess::ReadOnly,
        synchronization_defined: false,
        ownership_defined: false,
    }
}

pub fn from_ae_probe(probe: AeGpuFrameProbe) -> Option<ExternalFrameRef> {
    let backend = match probe.device.framework {
        AeGpuFramework::DirectX11 => ExternalBackend::D3D11,
        AeGpuFramework::DirectX12 => ExternalBackend::D3D12,
        AeGpuFramework::Metal => ExternalBackend::Metal,
        AeGpuFramework::Cuda => ExternalBackend::Cuda,
        AeGpuFramework::OpenGL | AeGpuFramework::Unknown(_) => return None,
    };

    let format = match probe.format {
        NativePixelFormat::Rgba8Unorm => ExternalFormat::Rgba8Unorm,
        NativePixelFormat::Bgra8Unorm => ExternalFormat::Bgra8Unorm,
        NativePixelFormat::Rgba16Float => ExternalFormat::Rgba16Float,
        NativePixelFormat::Rgba32Float => ExternalFormat::Rgba32Float,
        NativePixelFormat::Unknown(v) => ExternalFormat::Unknown(v),
    };

    let access = match probe.access {
        NativeFrameAccess::ReadOnly => ExternalAccess::ReadOnly,
        NativeFrameAccess::WriteOnly => ExternalAccess::WriteOnly,
        NativeFrameAccess::ReadWrite => ExternalAccess::ReadWrite,
    };

    let handle = match probe.handle {
        NativeHandle::D3D11Texture2D(ptr) => ExternalHandle::D3D11Texture2D(ptr),
        NativeHandle::D3D12Resource(ptr) => ExternalHandle::D3D12Resource(ptr),
        NativeHandle::MetalTexture(ptr) => ExternalHandle::MetalTexture(ptr),
        NativeHandle::CudaDevicePtr(ptr) => ExternalHandle::CudaDevicePtr(ptr),
        NativeHandle::Unsupported => return None,
    };

    Some(ExternalFrameRef {
        desc: ExternalFrameDesc {
            backend,
            width: probe.width,
            height: probe.height,
            format,
            access,
        },
        handle,
    })
}

fn handle_matches_backend(handle: ExternalHandle, backend: ExternalBackend) -> bool {
    matches!(
        (handle, backend),
        (ExternalHandle::D3D11Texture2D(_), ExternalBackend::D3D11)
            | (ExternalHandle::D3D12Resource(_), ExternalBackend::D3D12)
            | (ExternalHandle::MetalTexture(_), ExternalBackend::Metal)
            | (ExternalHandle::VulkanImage(_), ExternalBackend::Vulkan)
            | (ExternalHandle::CudaDevicePtr(_), ExternalBackend::Cuda)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ae_gpu_lab_native_probe::{AeGpuDeviceProbe, NativeProbeStatus};

    #[test]
    fn converts_native_probe_to_external_frame() {
        let probe = AeGpuFrameProbe {
            device: AeGpuDeviceProbe {
                framework: AeGpuFramework::Metal,
                device_index: 0,
                can_render_f32: true,
            },
            width: 3840,
            height: 2160,
            format: NativePixelFormat::Rgba16Float,
            access: NativeFrameAccess::ReadWrite,
            handle: NativeHandle::MetalTexture(0x1234usize as *mut core::ffi::c_void),
        };
        assert_eq!(probe.classify(), NativeProbeStatus::ReadyForBackend);

        let external = from_ae_probe(probe).expect("Metal probe should convert");
        assert_eq!(external.desc.backend, ExternalBackend::Metal);
        assert_eq!(external.desc.format, ExternalFormat::Rgba16Float);
        preflight(external, ExternalBackend::Metal)
            .validate()
            .expect("matching backend should pass preflight");
    }

    #[test]
    fn preflight_rejects_backend_mismatch() {
        let frame = ExternalFrameRef {
            desc: ExternalFrameDesc {
                backend: ExternalBackend::D3D12,
                width: 1920,
                height: 1080,
                format: ExternalFormat::Rgba32Float,
                access: ExternalAccess::ReadWrite,
            },
            handle: ExternalHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        assert!(preflight(frame, ExternalBackend::Metal).validate().is_err());
    }

    #[test]
    fn mock_importer_uses_preflight_before_import() {
        let importer = MockExternalImporter {
            backend: ExternalBackend::D3D12,
        };
        let frame = ExternalFrameRef {
            desc: ExternalFrameDesc {
                backend: ExternalBackend::D3D12,
                width: 1280,
                height: 720,
                format: ExternalFormat::Rgba32Float,
                access: ExternalAccess::ReadWrite,
            },
            handle: ExternalHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        let token = importer
            .import_frame(frame)
            .expect("valid frame should pass mock import");
        assert_eq!(token.backend, ExternalBackend::D3D12);
        assert_eq!(token.width, 1280);
        assert_eq!(token.height, 720);
    }

    #[test]
    fn conservative_backend_preflight_rejects_missing_sync_and_ownership() {
        let frame = ExternalFrameRef {
            desc: ExternalFrameDesc {
                backend: ExternalBackend::D3D12,
                width: 1280,
                height: 720,
                format: ExternalFormat::Rgba32Float,
                access: ExternalAccess::ReadOnly,
            },
            handle: ExternalHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        let preflight = conservative_backend_preflight(frame, ExternalBackend::D3D12);
        assert!(preflight.common.validate().is_ok());
        assert!(preflight.validate().is_err());
    }
}
