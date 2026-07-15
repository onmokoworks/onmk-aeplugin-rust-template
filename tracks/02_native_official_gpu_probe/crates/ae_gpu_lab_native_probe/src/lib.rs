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
    ReadyForBackend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePixelFormat {
    Rgba8Unorm,
    Bgra8Unorm,
    Rgba16Float,
    Rgba32Float,
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
    MetalTexture(*mut core::ffi::c_void),
    CudaDevicePtr(u64),
    Unsupported,
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
    pub format: NativePixelFormat,
    pub access: NativeFrameAccess,
    pub handle: NativeHandle,
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

        NativeProbeStatus::ReadyForBackend
    }
}

fn handle_matches_framework(handle: NativeHandle, framework: AeGpuFramework) -> bool {
    matches!(
        (handle, framework),
        (NativeHandle::D3D11Texture2D(_), AeGpuFramework::DirectX11)
            | (NativeHandle::D3D12Resource(_), AeGpuFramework::DirectX12)
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
            format: NativePixelFormat::Rgba16Float,
            access: NativeFrameAccess::ReadWrite,
            handle: NativeHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        assert_eq!(frame.classify(), NativeProbeStatus::MissingFrameHandle);
    }
}
