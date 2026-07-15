use crate::backend::{BackendTiming, RenderBackend, RenderReport};
use crate::{render_cpu, BenchEffect, FrameRgba8, LabParams};
use anyhow::Result;
use std::time::Instant;

#[derive(Default)]
pub struct CpuBackend;

impl RenderBackend for CpuBackend {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn render(
        &self,
        effect: BenchEffect,
        input: &FrameRgba8,
        params: LabParams,
    ) -> Result<RenderReport> {
        let start = Instant::now();
        let frame = render_cpu(effect, input, params);
        Ok(RenderReport {
            frame,
            timing: BackendTiming::cpu(start.elapsed()),
        })
    }
}
