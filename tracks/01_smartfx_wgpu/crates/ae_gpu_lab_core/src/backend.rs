use crate::{BenchEffect, FrameRgba8, LabParams};
use anyhow::Result;
use std::time::Duration;

pub trait RenderBackend {
    fn name(&self) -> &'static str;
    fn render(
        &self,
        effect: BenchEffect,
        input: &FrameRgba8,
        params: LabParams,
    ) -> Result<RenderReport>;
}

#[derive(Clone, Debug)]
pub struct RenderReport {
    pub frame: FrameRgba8,
    pub timing: BackendTiming,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BackendTiming {
    pub upload: Duration,
    pub encode: Duration,
    pub submit_wait: Duration,
    pub readback_map: Duration,
    pub readback_copy: Duration,
    pub total: Duration,
}

impl BackendTiming {
    pub fn cpu(total: Duration) -> Self {
        Self {
            total,
            ..Default::default()
        }
    }
}
