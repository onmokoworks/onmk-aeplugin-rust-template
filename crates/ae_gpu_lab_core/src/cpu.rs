use crate::{BenchEffect, FrameRgba8, LabParams};

pub fn render_cpu(effect: BenchEffect, input: &FrameRgba8, params: LabParams) -> FrameRgba8 {
    match effect {
        BenchEffect::Copy => input.clone(),
        BenchEffect::Color => color(input, params.control0[0]),
        BenchEffect::BoxBlur => box_blur(input, params.size[3]),
        BenchEffect::Diffusion => diffusion(input, params.size[2]),
        BenchEffect::ChromaWarp => chroma_warp(input, params.control0[0]),
    }
}

fn color(input: &FrameRgba8, strength: f32) -> FrameRgba8 {
    let mut out = input.clone();
    for px in out.pixels.chunks_exact_mut(4) {
        let r = px[0] as f32;
        let g = px[1] as f32;
        let b = px[2] as f32;
        px[0] = clamp_u8(r * (1.0 + strength * 0.20) + g * 0.05);
        px[1] = clamp_u8(g * (1.0 - strength * 0.10) + b * 0.04);
        px[2] = clamp_u8(b * (1.0 + strength * 0.15) + r * 0.03);
    }
    out
}

fn box_blur(input: &FrameRgba8, radius: u32) -> FrameRgba8 {
    let w = input.width as i32;
    let h = input.height as i32;
    let r = radius.min(32) as i32;
    let mut out = vec![0u8; input.pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for oy in -r..=r {
                let sy = (y + oy).clamp(0, h - 1);
                for ox in -r..=r {
                    let sx = (x + ox).clamp(0, w - 1);
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += input.pixels[i + c] as u32;
                    }
                    count += 1;
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (sum[c] / count) as u8;
            }
        }
    }
    FrameRgba8::new(input.width, input.height, out)
}

fn diffusion(input: &FrameRgba8, passes: u32) -> FrameRgba8 {
    let mut cur = input.clone();
    for _ in 0..passes.min(64) {
        cur = box_blur(&cur, 1);
    }
    cur
}

fn chroma_warp(input: &FrameRgba8, strength: f32) -> FrameRgba8 {
    let w = input.width;
    let h = input.height;
    let mut out = vec![0u8; input.pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let fx = x as f32 / w.max(1) as f32;
            let fy = y as f32 / h.max(1) as f32;
            let dx = ((fy * 37.0).sin() * strength * 12.0) as i32;
            let dy = ((fx * 31.0).cos() * strength * 8.0) as i32;
            let r = sample(input, x as i32 + dx, y as i32);
            let g = sample(input, x as i32, y as i32 + dy);
            let b = sample(input, x as i32 - dx, y as i32 - dy);
            let o = ((y * w + x) * 4) as usize;
            out[o] = r[0];
            out[o + 1] = g[1];
            out[o + 2] = b[2];
            out[o + 3] = input.pixels[o + 3];
        }
    }
    FrameRgba8::new(w, h, out)
}

fn sample(input: &FrameRgba8, x: i32, y: i32) -> [u8; 4] {
    let sx = x.clamp(0, input.width as i32 - 1) as u32;
    let sy = y.clamp(0, input.height as i32 - 1) as u32;
    let i = ((sy * input.width + sx) * 4) as usize;
    [
        input.pixels[i],
        input.pixels[i + 1],
        input.pixels[i + 2],
        input.pixels[i + 3],
    ]
}

fn clamp_u8(v: f32) -> u8 {
    v.clamp(0.0, 255.0) as u8
}
