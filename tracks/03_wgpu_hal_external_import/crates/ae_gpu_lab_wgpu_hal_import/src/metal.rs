use crate::{
    conservative_backend_preflight, BackendExternalImporter, BackendImportPreflight,
    ExternalBackend, ExternalFrameRef, ExternalImporter, ImportedFrameToken,
};
use anyhow::{bail, Result};

#[cfg(target_os = "macos")]
use crate::{ExternalHandle, ExternalResourceKind};
#[cfg(target_os = "macos")]
use metal::foreign_types::{ForeignType, ForeignTypeRef};

#[derive(Clone, Copy, Debug)]
pub struct MetalImportPlan {
    pub synchronization_defined: bool,
    pub ownership_defined: bool,
    pub allow_write_access: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MetalExternalImporter {
    pub plan: MetalImportPlan,
}

#[derive(Clone, Copy, Debug)]
pub struct MetalExternalContext {
    pub device: *mut core::ffi::c_void,
    pub command_queue: *mut core::ffi::c_void,
}

impl MetalExternalImporter {
    /// Wraps an AE-owned Metal buffer as a `wgpu::Buffer` without copying.
    ///
    /// # Safety
    ///
    /// `context.device`, `context.command_queue`, and the frame handle must be
    /// valid retained-by-AE Objective-C objects for the duration of this call.
    /// The caller must order AE queue work before wgpu submission and wait for
    /// all wgpu work before returning the output world to AE.
    #[cfg(target_os = "macos")]
    pub unsafe fn import_buffer_into_wgpu(
        &self,
        device: &wgpu::Device,
        context: MetalExternalContext,
        frame: ExternalFrameRef,
        usage: wgpu::BufferUsages,
    ) -> Result<wgpu::Buffer> {
        self.backend_preflight(frame).validate()?;
        if context.device.is_null() || context.command_queue.is_null() {
            bail!("Metal device or command queue pointer is null");
        }
        if frame.desc.resource_kind != ExternalResourceKind::Buffer {
            bail!("Metal external resource is not declared as a buffer");
        }
        let ExternalHandle::MetalBuffer(raw_pointer) = frame.handle else {
            bail!("Metal buffer import requires an MTLBuffer pointer");
        };
        if raw_pointer.is_null() {
            bail!("Metal buffer pointer is null");
        }

        let ae_device = unsafe { metal::DeviceRef::from_ptr(context.device.cast()) };
        let hal_device = unsafe { device.as_hal::<wgpu_hal::api::Metal>() }
            .ok_or_else(|| anyhow::anyhow!("wgpu device is not using the Metal backend"))?;
        let wgpu_device = hal_device.raw_device().lock();
        if wgpu_device.as_ptr() != ae_device.as_ptr() {
            bail!("AE and wgpu are using different MTLDevice objects");
        }
        drop(wgpu_device);

        let borrowed_buffer = unsafe { metal::BufferRef::from_ptr(raw_pointer.cast()) };
        if borrowed_buffer.length() < frame.desc.byte_len as u64 {
            bail!(
                "AE Metal buffer has {} bytes but frame declares {}",
                borrowed_buffer.length(),
                frame.desc.byte_len
            );
        }
        let retained_buffer = borrowed_buffer.to_owned();
        let hal_buffer = unsafe {
            wgpu_hal::metal::Device::buffer_from_raw(retained_buffer, frame.desc.byte_len as u64)
        };
        let descriptor = wgpu::BufferDescriptor {
            label: Some("AE external Metal GPU world"),
            size: frame.desc.byte_len as u64,
            usage,
            mapped_at_creation: false,
        };
        Ok(unsafe {
            device.create_buffer_from_hal::<wgpu_hal::api::Metal>(hal_buffer, &descriptor)
        })
    }
}

impl ExternalImporter for MetalExternalImporter {
    fn active_backend(&self) -> ExternalBackend {
        ExternalBackend::Metal
    }

    fn import_frame_unchecked(&self, _frame: ExternalFrameRef) -> Result<ImportedFrameToken> {
        bail!("generic token API does not own Metal resources; use import_buffer_into_wgpu")
    }
}

impl BackendExternalImporter for MetalExternalImporter {
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
    fn metal_plan_accepts_readwrite_only_when_write_access_allowed() {
        let importer = MetalExternalImporter {
            plan: MetalImportPlan {
                synchronization_defined: true,
                ownership_defined: true,
                allow_write_access: false,
            },
        };
        let frame = ExternalFrameRef {
            desc: ExternalFrameDesc {
                backend: ExternalBackend::Metal,
                width: 1920,
                height: 1080,
                row_bytes: 1920 * 8,
                byte_len: 1920 * 1080 * 8,
                format: ExternalFormat::Rgba16Float,
                access: ExternalAccess::ReadWrite,
                resource_kind: ExternalResourceKind::Texture2D,
            },
            handle: ExternalHandle::MetalTexture(0x1234usize as *mut core::ffi::c_void),
        };

        assert!(importer.backend_preflight(frame).validate().is_err());
    }
}
