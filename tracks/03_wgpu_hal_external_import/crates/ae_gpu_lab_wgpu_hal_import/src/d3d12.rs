use crate::{
    conservative_backend_preflight, BackendExternalImporter, BackendImportPreflight,
    ExternalBackend, ExternalFrameRef, ExternalImporter, ImportedFrameToken,
};
use anyhow::{bail, Result};

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

impl ExternalImporter for D3D12ExternalImporter {
    fn active_backend(&self) -> ExternalBackend {
        ExternalBackend::D3D12
    }

    fn import_frame_unchecked(&self, _frame: ExternalFrameRef) -> Result<ImportedFrameToken> {
        bail!("D3D12 wgpu-hal external import is not implemented yet")
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
    use crate::{ExternalAccess, ExternalFormat, ExternalFrameDesc, ExternalHandle};

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
                format: ExternalFormat::Rgba32Float,
                access: ExternalAccess::ReadOnly,
            },
            handle: ExternalHandle::D3D12Resource(0x1234usize as *mut core::ffi::c_void),
        };

        assert!(importer.backend_preflight(frame).validate().is_err());
    }
}
