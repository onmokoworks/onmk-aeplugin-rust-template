use crate::{
    conservative_backend_preflight, BackendExternalImporter, BackendImportPreflight,
    ExternalBackend, ExternalFrameRef, ExternalImporter, ImportedFrameToken,
};
use anyhow::{bail, Result};

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

impl ExternalImporter for MetalExternalImporter {
    fn active_backend(&self) -> ExternalBackend {
        ExternalBackend::Metal
    }

    fn import_frame_unchecked(&self, _frame: ExternalFrameRef) -> Result<ImportedFrameToken> {
        bail!("Metal wgpu-hal external import is not implemented yet")
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
    use crate::{ExternalAccess, ExternalFormat, ExternalFrameDesc, ExternalHandle};

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
                format: ExternalFormat::Rgba16Float,
                access: ExternalAccess::ReadWrite,
            },
            handle: ExternalHandle::MetalTexture(0x1234usize as *mut core::ffi::c_void),
        };

        assert!(importer.backend_preflight(frame).validate().is_err());
    }
}
