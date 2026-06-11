#[derive(Clone, Debug)]
pub struct FrameRgba8 {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl FrameRgba8 {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        debug_assert_eq!(pixels.len(), width as usize * height as usize * 4);
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn synthetic(width: u32, height: u32) -> Self {
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let i = ((y * width + x) * 4) as usize;
                let fx = x as f32 / width.max(1) as f32;
                let fy = y as f32 / height.max(1) as f32;
                pixels[i] = (fx * 255.0) as u8;
                pixels[i + 1] = (fy * 255.0) as u8;
                pixels[i + 2] = (((fx * 19.0).sin() * (fy * 23.0).cos() * 0.5 + 0.5) * 255.0) as u8;
                pixels[i + 3] = 255;
            }
        }
        Self::new(width, height, pixels)
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LabParams {
    pub size: [u32; 4],
    pub control0: [f32; 4],
    pub control1: [f32; 4],
}

impl LabParams {
    pub fn new(width: u32, height: u32, passes: u32, strength: f32, radius: u32) -> Self {
        Self {
            size: [width, height, passes.max(1), radius.max(1)],
            control0: [strength, 0.0, 0.0, 0.0],
            control1: [0.0; 4],
        }
    }
}
