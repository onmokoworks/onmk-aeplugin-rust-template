//! Native / official GPU path probe boundary.
//!
//! This crate is intentionally small. Its job is to document and type the data
//! we need before attempting a real AE official GPU implementation.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AeGpuFramework {
    DirectX11,
    DirectX12,
    Metal,
    Cuda,
    OpenGL,
    Unknown(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeProbeStatus {
    UnsupportedFramework,
    MissingDeviceInfo,
    MissingFrameHandle,
    InvalidFrameLayout,
    ReadyForBackend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePixelFormat {
    Rgba8Unorm,
    Bgra8Unorm,
    Rgba16Float,
    Rgba32Float,
    Bgra32Float,
    Unknown(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeFrameAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeHandle {
    D3D11Texture2D(*mut core::ffi::c_void),
    D3D12Resource(*mut core::ffi::c_void),
    MetalBuffer(*mut core::ffi::c_void),
    MetalTexture(*mut core::ffi::c_void),
    CudaDevicePtr(u64),
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AeGpuDeviceHandles {
    pub platform: *mut core::ffi::c_void,
    pub device: *mut core::ffi::c_void,
    pub context: *mut core::ffi::c_void,
    pub command_queue: *mut core::ffi::c_void,
}

impl Default for AeGpuDeviceHandles {
    fn default() -> Self {
        Self {
            platform: core::ptr::null_mut(),
            device: core::ptr::null_mut(),
            context: core::ptr::null_mut(),
            command_queue: core::ptr::null_mut(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AeGpuDeviceContext {
    pub probe: AeGpuDeviceProbe,
    pub handles: AeGpuDeviceHandles,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AeGpuDeviceProbe {
    pub framework: AeGpuFramework,
    pub device_index: i32,
    pub can_render_f32: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AeGpuFrameProbe {
    pub device: AeGpuDeviceProbe,
    pub width: u32,
    pub height: u32,
    pub row_bytes: usize,
    pub byte_len: usize,
    pub format: NativePixelFormat,
    pub access: NativeFrameAccess,
    pub handle: NativeHandle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeRenderJob {
    pub device: AeGpuDeviceContext,
    pub input: AeGpuFrameProbe,
    pub output: AeGpuFrameProbe,
}

impl AeGpuDeviceProbe {
    pub fn classify(self) -> NativeProbeStatus {
        match self.framework {
            AeGpuFramework::Unknown(_) | AeGpuFramework::OpenGL => {
                NativeProbeStatus::UnsupportedFramework
            }
            _ if !self.can_render_f32 => NativeProbeStatus::MissingDeviceInfo,
            _ => NativeProbeStatus::ReadyForBackend,
        }
    }
}

impl AeGpuFrameProbe {
    pub fn classify(self) -> NativeProbeStatus {
        match self.device.classify() {
            NativeProbeStatus::ReadyForBackend => {}
            status => return status,
        }

        if self.width == 0 || self.height == 0 || matches!(self.handle, NativeHandle::Unsupported) {
            return NativeProbeStatus::MissingFrameHandle;
        }

        if !handle_matches_framework(self.handle, self.device.framework) {
            return NativeProbeStatus::MissingFrameHandle;
        }

        let Some(minimum_row_bytes) = self
            .format
            .bytes_per_pixel()
            .and_then(|bpp| (self.width as usize).checked_mul(bpp))
        else {
            return NativeProbeStatus::InvalidFrameLayout;
        };
        let Some(required_len) = self.row_bytes.checked_mul(self.height as usize) else {
            return NativeProbeStatus::InvalidFrameLayout;
        };
        if self.row_bytes < minimum_row_bytes || self.byte_len < required_len {
            return NativeProbeStatus::InvalidFrameLayout;
        }

        NativeProbeStatus::ReadyForBackend
    }
}

impl AeGpuDeviceContext {
    pub fn classify(self) -> NativeProbeStatus {
        match self.probe.classify() {
            NativeProbeStatus::ReadyForBackend => {}
            status => return status,
        }
        let valid = match self.probe.framework {
            AeGpuFramework::Cuda => {
                !self.handles.context.is_null() && !self.handles.command_queue.is_null()
            }
            AeGpuFramework::DirectX11 | AeGpuFramework::DirectX12 | AeGpuFramework::Metal => {
                !self.handles.device.is_null() && !self.handles.command_queue.is_null()
            }
            AeGpuFramework::OpenGL | AeGpuFramework::Unknown(_) => false,
        };
        if valid {
            NativeProbeStatus::ReadyForBackend
        } else {
            NativeProbeStatus::MissingDeviceInfo
        }
    }
}

impl NativeRenderJob {
    pub fn classify(self) -> NativeProbeStatus {
        match self.device.classify() {
            NativeProbeStatus::ReadyForBackend => {}
            status => return status,
        }
        for frame in [self.input, self.output] {
            match frame.classify() {
                NativeProbeStatus::ReadyForBackend => {}
                status => return status,
            }
            if frame.device != self.device.probe {
                return NativeProbeStatus::MissingDeviceInfo;
            }
        }
        if self.input.width != self.output.width
            || self.input.height != self.output.height
            || self.input.format != self.output.format
            || self.input.access == NativeFrameAccess::WriteOnly
            || self.output.access == NativeFrameAccess::ReadOnly
        {
            return NativeProbeStatus::InvalidFrameLayout;
        }
        NativeProbeStatus::ReadyForBackend
    }
}

impl NativePixelFormat {
    pub const fn bytes_per_pixel(self) -> Option<usize> {
        match self {
            Self::Rgba8Unorm | Self::Bgra8Unorm => Some(4),
            Self::Rgba16Float => Some(8),
            Self::Rgba32Float | Self::Bgra32Float => Some(16),
            Self::Unknown(_) => None,
        }
    }
}

fn handle_matches_framework(handle: NativeHandle, framework: AeGpuFramework) -> bool {
    matches!(
        (handle, framework),
        (NativeHandle::D3D11Texture2D(_), AeGpuFramework::DirectX11)
            | (NativeHandle::D3D12Resource(_), AeGpuFramework::DirectX12)
            | (NativeHandle::MetalBuffer(_), AeGpuFramework::Metal)
            | (NativeHandle::MetalTexture(_), AeGpuFramework::Metal)
            | (NativeHandle::CudaDevicePtr(_), AeGpuFramework::Cuda)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_matching_native_frame_as_ready() {
        let frame = AeGpuFrameProbe {
            device: AeGpuDeviceProbe {
                framework: AeGpuFramework::DirectX12,
                device_index: 0,
                can_render_f32: true,
            },
            width: 1920,
            height: 1080,
            row_bytes: 1920 * 16,
            byte_len: 1920 * 1080 * 16,
            format: NativePixelFormat::Rgba32Float,
            access: NativeFrameAccess::ReadWrite,
            handle: NativeHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        assert_eq!(frame.classify(), NativeProbeStatus::ReadyForBackend);
    }

    #[test]
    fn rejects_mismatched_handle_type() {
        let frame = AeGpuFrameProbe {
            device: AeGpuDeviceProbe {
                framework: AeGpuFramework::Metal,
                device_index: 0,
                can_render_f32: true,
            },
            width: 1920,
            height: 1080,
            row_bytes: 1920 * 8,
            byte_len: 1920 * 1080 * 8,
            format: NativePixelFormat::Rgba16Float,
            access: NativeFrameAccess::ReadWrite,
            handle: NativeHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        assert_eq!(frame.classify(), NativeProbeStatus::MissingFrameHandle);
    }

    #[test]
    fn validates_framework_specific_device_handles() {
        let context = AeGpuDeviceContext {
            probe: AeGpuDeviceProbe {
                framework: AeGpuFramework::Cuda,
                device_index: 0,
                can_render_f32: true,
            },
            handles: AeGpuDeviceHandles {
                context: 1usize as *mut _,
                command_queue: 2usize as *mut _,
                ..Default::default()
            },
        };
        assert_eq!(context.classify(), NativeProbeStatus::ReadyForBackend);
    }

    #[test]
    fn rejects_too_short_gpu_frame_allocation() {
        let frame = AeGpuFrameProbe {
            device: AeGpuDeviceProbe {
                framework: AeGpuFramework::Metal,
                device_index: 0,
                can_render_f32: true,
            },
            width: 1920,
            height: 1080,
            row_bytes: 1920 * 16,
            byte_len: 1920 * 16,
            format: NativePixelFormat::Rgba32Float,
            access: NativeFrameAccess::ReadOnly,
            handle: NativeHandle::MetalBuffer(0x1234usize as *mut core::ffi::c_void),
        };
        assert_eq!(frame.classify(), NativeProbeStatus::InvalidFrameLayout);
    }
}
