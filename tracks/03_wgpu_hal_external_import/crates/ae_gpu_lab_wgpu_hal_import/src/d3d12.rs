use crate::{
    conservative_backend_preflight, BackendExternalImporter, BackendImportPreflight,
    ExternalBackend, ExternalFrameRef, ExternalImporter, ImportedFrameToken,
};
use anyhow::{bail, Result};

#[cfg(windows)]
use crate::{ExternalHandle, ExternalResourceKind};
#[cfg(windows)]
use windows::Win32::Graphics::Direct3D12::{
    ID3D12Device, ID3D12Resource, D3D12_RESOURCE_DIMENSION_BUFFER,
};
#[cfg(windows)]
use windows_core::Interface;

#[derive(Clone, Copy, Debug)]
pub struct D3D12ImportPlan {
    pub synchronization_defined: bool,
    pub ownership_defined: bool,
    pub allow_write_access: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct D3D12ExternalImporter {
    pub plan: D3D12ImportPlan,
}

impl D3D12ExternalImporter {
    /// Wraps an AE-owned D3D12 buffer as a `wgpu::Buffer` without copying.
    ///
    /// # Safety
    ///
    /// The caller must prove that AE permits the declared access for the whole
    /// returned buffer lifetime, that prior host work is visible, and that all
    /// submitted wgpu work completes before AE reuses the world. The D3D12
    /// resource state contract must also be honored by the caller.
    #[cfg(windows)]
    pub unsafe fn import_buffer_into_wgpu(
        &self,
        device: &wgpu::Device,
        frame: ExternalFrameRef,
        usage: wgpu::BufferUsages,
    ) -> Result<wgpu::Buffer> {
        self.backend_preflight(frame).validate()?;
        if frame.desc.resource_kind != ExternalResourceKind::Buffer {
            bail!("D3D12 external resource is not declared as a buffer");
        }
        let ExternalHandle::D3D12Resource(raw_pointer) = frame.handle else {
            bail!("D3D12 buffer import requires an ID3D12Resource pointer");
        };
        if raw_pointer.is_null() {
            bail!("D3D12 resource pointer is null");
        }

        let borrowed = unsafe { ID3D12Resource::from_raw_borrowed(&raw_pointer) }
            .ok_or_else(|| anyhow::anyhow!("ID3D12Resource pointer could not be borrowed"))?;
        let resource = borrowed.clone();
        let resource_desc = unsafe { resource.GetDesc() };
        if resource_desc.Dimension != D3D12_RESOURCE_DIMENSION_BUFFER {
            bail!("AE D3D12 resource is not a buffer");
        }
        if resource_desc.Width < frame.desc.byte_len as u64 {
            bail!(
                "AE D3D12 buffer has {} bytes but frame declares {}",
                resource_desc.Width,
                frame.desc.byte_len
            );
        }

        let mut resource_device: Option<ID3D12Device> = None;
        unsafe { resource.GetDevice(&mut resource_device) }?;
        let resource_device = resource_device
            .ok_or_else(|| anyhow::anyhow!("ID3D12Resource::GetDevice returned null"))?;
        let hal_device = unsafe { device.as_hal::<wgpu_hal::api::Dx12>() }
            .ok_or_else(|| anyhow::anyhow!("wgpu device is not using the D3D12 backend"))?;
        if resource_device.as_raw() != hal_device.raw_device().as_raw() {
            bail!("AE resource and wgpu device were created by different ID3D12Device objects");
        }

        let hal_buffer = unsafe {
            wgpu_hal::dx12::Device::buffer_from_raw(resource, frame.desc.byte_len as u64)
        };
        let descriptor = wgpu::BufferDescriptor {
            label: Some("AE external D3D12 GPU world"),
            size: frame.desc.byte_len as u64,
            usage,
            mapped_at_creation: false,
        };
        Ok(
            unsafe {
                device.create_buffer_from_hal::<wgpu_hal::api::Dx12>(hal_buffer, &descriptor)
            },
        )
    }
}

impl ExternalImporter for D3D12ExternalImporter {
    fn active_backend(&self) -> ExternalBackend {
        ExternalBackend::D3D12
    }

    fn import_frame_unchecked(&self, _frame: ExternalFrameRef) -> Result<ImportedFrameToken> {
        bail!("generic token API does not own D3D12 resources; use import_buffer_into_wgpu")
    }
}

impl BackendExternalImporter for D3D12ExternalImporter {
    fn backend_preflight(&self, frame: ExternalFrameRef) -> BackendImportPreflight {
        let mut preflight = conservative_backend_preflight(frame, self.active_backend());
        preflight.access_supported =
            frame.desc.access == crate::ExternalAccess::ReadOnly || self.plan.allow_write_access;
        preflight.synchronization_defined = self.plan.synchronization_defined;
        preflight.ownership_defined = self.plan.ownership_defined;
        preflight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExternalAccess, ExternalFormat, ExternalFrameDesc, ExternalHandle, ExternalResourceKind,
    };

    #[test]
    fn d3d12_plan_requires_sync_and_ownership_before_import() {
        let importer = D3D12ExternalImporter {
            plan: D3D12ImportPlan {
                synchronization_defined: false,
                ownership_defined: false,
                allow_write_access: false,
            },
        };
        let frame = ExternalFrameRef {
            desc: ExternalFrameDesc {
                backend: ExternalBackend::D3D12,
                width: 1920,
                height: 1080,
                row_bytes: 1920 * 16,
                byte_len: 1920 * 1080 * 16,
                format: ExternalFormat::Rgba32Float,
                access: ExternalAccess::ReadOnly,
                resource_kind: ExternalResourceKind::Buffer,
            },
            handle: ExternalHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        assert!(importer.backend_preflight(frame).validate().is_err());
    }
}
