pub mod backend;
pub mod cpu;
pub mod cpu_backend;
pub mod frame;
pub mod wgpu_backend;

pub use backend::{BackendTiming, RenderBackend, RenderReport};
pub use cpu::render_cpu;
pub use cpu_backend::CpuBackend;
pub use frame::{FrameRgba8, LabParams};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BenchEffect {
    Copy,
    Color,
    BoxBlur,
    Diffusion,
    ChromaWarp,
}

impl BenchEffect {
    pub fn shader_entry(self) -> &'static str {
        match self {
            Self::Copy => "copy_main",
            Self::Color => "color_main",
            Self::BoxBlur => "box_blur_main",
            Self::Diffusion => "diffusion_main",
            Self::ChromaWarp => "chroma_warp_main",
        }
    }
}
