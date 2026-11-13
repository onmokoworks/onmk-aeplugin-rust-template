//! CPU reference effects on straight RGBA8 frames.
//!
//! Alpha contract (docs/alpha-contract.md): frames are straight RGB + alpha.
//! Per-pixel colour operations (Color) work on straight RGB directly. Spatial
//! filters (Box Blur, Diffusion) weight colour by alpha, i.e. they behave like
//! premultiply -> filter -> un-premultiply, so transparent pixels never leak
//! their hidden RGB into visible ones. The WGSL shader follows the same rule.
//!
//! Known gap: Chroma Warp samples straight RGB from displaced positions and can
//! still pick up hidden RGB from transparent neighbours.

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
            // Alpha-weighted: sum rgb * a (premultiplied) and a separately.
            let mut sum = [0u32; 3];
            let mut alpha_sum = 0u32;
            let mut count = 0u32;
            for oy in -r..=r {
                let sy = (y + oy).clamp(0, h - 1);
                for ox in -r..=r {
                    let sx = (x + ox).clamp(0, w - 1);
                    let i = ((sy * w + sx) * 4) as usize;
                    let a = input.pixels[i + 3] as u32;
                    for c in 0..3 {
                        sum[c] += input.pixels[i + c] as u32 * a;
                    }
                    alpha_sum += a;
                    count += 1;
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                // Un-premultiply; fully transparent results get RGB 0.
                out[o + c] = sum[c].checked_div(alpha_sum).unwrap_or(0) as u8;
            }
            out[o + 3] = (alpha_sum / count) as u8;
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

#[cfg(test)]
mod tests {
    use super::*;
    use ae_plugin_kit::alpha::{check_straight, premultiply};

    fn to_f32(frame: &FrameRgba8) -> Vec<[f32; 4]> {
        frame
            .pixels
            .chunks_exact(4)
            .map(|p| std::array::from_fn(|c| p[c] as f32 / 255.0))
            .collect()
    }

    /// Opaque white on the left, transparent pixels with hidden red RGB on the right.
    fn edge() -> FrameRgba8 {
        let mut pixels = Vec::new();
        for _y in 0..3 {
            for x in 0..8 {
                pixels.extend(if x < 4 {
                    [255, 255, 255, 255]
                } else {
                    [255, 0, 0, 0]
                });
            }
        }
        FrameRgba8::new(8, 3, pixels)
    }

    fn expected_edge(radius: i32) -> Vec<[f32; 4]> {
        // Independent of the implementation: blurred opaque white stays white
        // wherever it is visible; alpha is the (clamped-edge) box coverage.
        let mut out = Vec::new();
        for _y in 0..3 {
            for x in 0..8i32 {
                let covered = (x - radius..=x + radius)
                    .filter(|sx| sx.clamp(&0, &7) < &4)
                    .count() as u32;
                let count = (2 * radius + 1) as u32;
                let alpha = (covered * 255 / count) as f32 / 255.0;
                out.push([1.0, 1.0, 1.0, alpha]);
            }
        }
        out
    }

    #[test]
    fn box_blur_keeps_straight_white_edges() {
        let params = LabParams::new(8, 3, 1, 1.0, 1);
        let out = render_cpu(BenchEffect::BoxBlur, &edge(), params);
        check_straight(&expected_edge(1), &to_f32(&out), 0.5 / 255.0).unwrap();
    }

    #[test]
    fn premultiplied_or_unweighted_blur_fails_the_check() {
        let expected = expected_edge(1);
        let premultiplied: Vec<_> = expected.iter().map(|p| premultiply(*p)).collect();
        assert!(check_straight(&expected, &premultiplied, 0.5 / 255.0).is_err());
        // The previous channel-independent blur leaked the hidden red.
        let input = edge();
        let unweighted: Vec<[f32; 4]> = (0..24)
            .map(|i| {
                let (x, y) = ((i % 8) as i32, i / 8);
                let mut sum = [0u32; 4];
                for sx in x - 1..=x + 1 {
                    let p = &input.pixels[(y * 8 + sx.clamp(0, 7) as usize) * 4..][..4];
                    for c in 0..4 {
                        sum[c] += p[c] as u32 * 3;
                    }
                }
                std::array::from_fn(|c| (sum[c] / 9) as f32 / 255.0)
            })
            .collect();
        assert!(check_straight(&expected, &unweighted, 0.5 / 255.0).is_err());
    }

    #[test]
    fn diffusion_is_alpha_weighted_too() {
        let params = LabParams::new(8, 3, 3, 1.0, 1);
        let out = to_f32(&render_cpu(BenchEffect::Diffusion, &edge(), params));
        for p in out.iter().filter(|p| p[3] > 0.0) {
            assert_eq!(
                &p[..3],
                &[1.0, 1.0, 1.0],
                "visible pixel darkened or tinted: {p:?}"
            );
        }
        assert!(out.iter().any(|p| p[3] > 0.0 && p[3] < 1.0));
    }

    #[test]
    fn color_is_independent_of_alpha() {
        let pixels = [0u8, 1, 32, 128, 255]
            .iter()
            .flat_map(|&a| [200, 100, 50, a])
            .collect();
        let frame = FrameRgba8::new(5, 1, pixels);
        let out = render_cpu(BenchEffect::Color, &frame, LabParams::new(5, 1, 1, 1.0, 1));
        let first = &out.pixels[..3];
        for (i, p) in out.pixels.chunks_exact(4).enumerate() {
            assert_eq!(&p[..3], first, "pixel {i}");
            assert_eq!(p[3], frame.pixels[i * 4 + 3]);
        }
    }
}
