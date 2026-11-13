//! GPU alpha-contract check. Ignored by default because CI runners may have no
//! usable adapter. Run with `cargo test -p ae_gpu_lab_core -- --ignored`.
//!
//! CPU/GPU agreement alone is not evidence (both could share the same mistake),
//! so the GPU output is compared against an independent expected result.

use ae_gpu_lab_core::wgpu_backend::WgpuUploadReadback;
use ae_gpu_lab_core::{BenchEffect, FrameRgba8, LabParams};
use ae_plugin_kit::alpha::check_straight;

#[test]
#[ignore = "needs a GPU adapter"]
fn gpu_box_blur_keeps_straight_white_edges() {
    let gpu = WgpuUploadReadback::new().expect("wgpu adapter");
    let (w, h) = (8u32, 3u32);
    let mut pixels = Vec::new();
    for _y in 0..h {
        for x in 0..w {
            pixels.extend(if x < 4 {
                [255, 255, 255, 255]
            } else {
                [255, 0, 0, 0]
            });
        }
    }
    let frame = FrameRgba8::new(w, h, pixels);
    let report = gpu
        .render_frame(
            BenchEffect::BoxBlur,
            &frame,
            LabParams::new(w, h, 1, 1.0, 1),
        )
        .expect("render");
    let actual: Vec<[f32; 4]> = report
        .frame
        .pixels
        .chunks_exact(4)
        .map(|p| std::array::from_fn(|c| p[c] as f32 / 255.0))
        .collect();
    let expected: Vec<[f32; 4]> = (0..w * h)
        .map(|i| {
            let x = (i % w) as i32;
            let covered = (x - 1..=x + 1).filter(|sx| (*sx).clamp(0, 7) < 4).count() as f32;
            [1.0, 1.0, 1.0, covered / 3.0]
        })
        .collect();
    // The shader truncates when packing to 8 bits.
    check_straight(&expected, &actual, 1.5 / 255.0).unwrap();
}
