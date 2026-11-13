//! Host-independent render logic. Everything here works on straight,
//! normalized RGBA so it can be unit tested without After Effects.
//!
//! Alpha contract (see `docs/alpha-contract.md`):
//! - input and output are straight RGB + alpha;
//! - Invert is a per-pixel colour operation on straight RGB (alpha unchanged);
//! - Box Blur and Blend Layer are computed in premultiplied space and
//!   un-premultiplied once at the end;
//! - Amount mixes original and result in premultiplied space.

use ae_plugin_kit::alpha::{premultiply, unpremultiply};
use ae_plugin_kit::parallel::{
    for_each_row_chunk, workers_for, DEFAULT_MAX_WORKERS, DEFAULT_MIN_ELEMENTS,
};
use ae_plugin_kit::pixels::Rgba;
use ae_plugin_kit::simd::convolve_row;

pub const MAX_RADIUS: u32 = 256;

/// Popup values are 1-based in AE. Never reorder these: saved projects store
/// the number, not the label. Append new modes at the end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Invert = 1,
    BoxBlur = 2,
    BlendLayer = 3,
}

impl Mode {
    pub const LABELS: [&'static str; 3] = ["Invert", "Box Blur", "Blend Layer"];
    pub const DEFAULT: Mode = Mode::BoxBlur;

    /// Unknown values (e.g. a project saved by a newer version with more
    /// options) fall back to the default instead of failing the render.
    pub fn from_popup(value: i32) -> Self {
        match value {
            1 => Self::Invert,
            2 => Self::BoxBlur,
            3 => Self::BlendLayer,
            _ => Self::DEFAULT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub mode: Mode,
    /// 0..=1 mix between the original and the effect result.
    pub amount: f32,
    pub radius: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// Straight RGBA, row-major.
    pub pixels: Vec<Rgba>,
}

impl Image {
    pub fn new(width: usize, height: usize, pixels: Vec<Rgba>) -> Self {
        assert_eq!(pixels.len(), width * height, "pixel count mismatch");
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn transparent(width: usize, height: usize) -> Self {
        Self::new(width, height, vec![[0.0; 4]; width * height])
    }

    fn get(&self, x: usize, y: usize) -> Option<Rgba> {
        (x < self.width && y < self.height).then(|| self.pixels[y * self.width + x])
    }

    /// Copy into a `width x height` canvas aligned at the top-left corner;
    /// uncovered pixels are transparent.
    pub fn fit(&self, width: usize, height: usize) -> Image {
        if width == self.width && height == self.height {
            return self.clone();
        }
        let mut out = Image::transparent(width, height);
        for y in 0..height.min(self.height) {
            for x in 0..width.min(self.width) {
                out.pixels[y * width + x] = self.pixels[y * self.width + x];
            }
        }
        out
    }
}

pub fn render(settings: Settings, input: &Image, blend: Option<&Image>) -> Image {
    let workers = workers_for(
        input
            .pixels
            .len()
            .saturating_mul(settings.radius as usize + 1),
        DEFAULT_MIN_ELEMENTS,
        DEFAULT_MAX_WORKERS,
    );
    render_with_workers(settings, input, blend, workers)
}

pub fn render_with_workers(
    settings: Settings,
    input: &Image,
    blend: Option<&Image>,
    workers: usize,
) -> Image {
    let amount = if settings.amount.is_finite() {
        settings.amount.clamp(0.0, 1.0)
    } else {
        1.0
    };
    if amount == 0.0 {
        return input.clone();
    }
    if settings.mode == Mode::BlendLayer {
        return match blend {
            Some(top) => blend_over(input, top, amount),
            None => input.clone(),
        };
    }
    // Premultiplied effect result.
    let result: Vec<Rgba> = match settings.mode {
        Mode::Invert => input
            .pixels
            .iter()
            .map(|p| premultiply([1.0 - p[0], 1.0 - p[1], 1.0 - p[2], p[3]]))
            .collect(),
        Mode::BoxBlur => {
            let premultiplied: Vec<Rgba> = input.pixels.iter().map(|p| premultiply(*p)).collect();
            box_blur_premultiplied(
                &premultiplied,
                input.width,
                input.height,
                settings.radius.min(MAX_RADIUS) as usize,
                workers,
            )
        }
        Mode::BlendLayer => unreachable!("handled above"),
    };
    let pixels = input
        .pixels
        .iter()
        .zip(result)
        .map(|(original, effect)| {
            let o = premultiply(*original);
            unpremultiply(std::array::from_fn(|c| o[c] + (effect[c] - o[c]) * amount))
        })
        .collect();
    Image::new(input.width, input.height, pixels)
}

/// `top` composited over `input` with the Porter-Duff over operator.
/// `opacity` scales the premultiplied top layer. `top` is sampled at the same
/// pixel coordinates; outside its bounds it is transparent.
fn blend_over(input: &Image, top: &Image, opacity: f32) -> Image {
    let pixels = input
        .pixels
        .iter()
        .enumerate()
        .map(|(i, dst)| {
            let (x, y) = (i % input.width, i / input.width);
            let src = top
                .get(x, y)
                .map_or([0.0; 4], premultiply)
                .map(|c| c * opacity);
            let dst = premultiply(*dst);
            let k = 1.0 - src[3];
            unpremultiply(std::array::from_fn(|c| src[c] + dst[c] * k))
        })
        .collect();
    Image::new(input.width, input.height, pixels)
}

/// Separable box blur of premultiplied pixels; outside the layer is
/// transparent. The horizontal pass uses the SIMD kernel from
/// `ae_plugin_kit`; both passes are row-parallel and deterministic.
fn box_blur_premultiplied(
    src: &[Rgba],
    width: usize,
    height: usize,
    radius: usize,
    workers: usize,
) -> Vec<Rgba> {
    if radius == 0 || width == 0 || height == 0 {
        return src.to_vec();
    }
    let taps = vec![1.0 / (2 * radius + 1) as f32; 2 * radius + 1];
    let mut horizontal = vec![[0.0f32; 4]; src.len()];
    for_each_row_chunk(&mut horizontal, width, workers, |first_row, chunk| {
        let mut padded = vec![0.0f32; width + 2 * radius];
        let mut out = vec![0.0f32; width];
        for (r, row) in chunk.chunks_exact_mut(width).enumerate() {
            let y = first_row + r;
            let source = &src[y * width..(y + 1) * width];
            for c in 0..4 {
                for (x, p) in source.iter().enumerate() {
                    padded[radius + x] = p[c];
                }
                convolve_row(&padded, &taps, &mut out);
                for (x, p) in row.iter_mut().enumerate() {
                    p[c] = out[x];
                }
            }
        }
    });
    let mut vertical = vec![[0.0f32; 4]; src.len()];
    let horizontal = &horizontal;
    for_each_row_chunk(&mut vertical, width, workers, |first_row, chunk| {
        for (r, row) in chunk.chunks_exact_mut(width).enumerate() {
            let y = first_row + r;
            for (x, p) in row.iter_mut().enumerate() {
                // Same tap order as the horizontal kernel: first tap multiplies.
                let mut acc = [0.0f32; 4];
                for (i, k) in taps.iter().enumerate() {
                    let sy = y as isize + i as isize - radius as isize;
                    let sample = if sy >= 0 && (sy as usize) < height {
                        horizontal[sy as usize * width + x]
                    } else {
                        [0.0; 4]
                    };
                    for c in 0..4 {
                        acc[c] = if i == 0 {
                            sample[c] * k
                        } else {
                            acc[c] + sample[c] * k
                        };
                    }
                }
                *p = acc;
            }
        }
    });
    vertical
}

#[cfg(test)]
mod tests {
    use super::*;
    use ae_plugin_kit::alpha::{check_straight, StraightCheckError};

    const ALPHAS: [f32; 5] = [0.0, 1.0 / 255.0, 0.125, 0.5, 1.0];

    fn settings(mode: Mode) -> Settings {
        Settings {
            mode,
            amount: 1.0,
            radius: 1,
        }
    }

    fn row(pixels: Vec<Rgba>) -> Image {
        Image::new(pixels.len(), 1, pixels)
    }

    #[test]
    fn invert_is_independent_of_coverage() {
        let input = row(ALPHAS.iter().map(|&a| [1.0, 0.8, 0.2, a]).collect());
        let out = render(settings(Mode::Invert), &input, None);
        // Independent expectation from the spec: straight RGB inverted, alpha kept.
        let expected: Vec<Rgba> = ALPHAS.iter().map(|&a| [0.0, 0.2, 0.8, a]).collect();
        check_straight(&expected, &out.pixels, 1e-6).unwrap();
    }

    #[test]
    fn premultiplied_output_is_rejected_by_the_check() {
        let expected: Vec<Rgba> = ALPHAS.iter().map(|&a| [0.0, 0.2, 0.8, a]).collect();
        let wrong: Vec<Rgba> = expected.iter().map(|p| premultiply(*p)).collect();
        assert!(matches!(
            check_straight(&expected, &wrong, 1.0 / 255.0),
            Err(StraightCheckError::RgbMismatch { .. })
        ));
    }

    /// Opaque white next to transparent pixels whose hidden RGB is red.
    fn edge_fixture() -> Image {
        let pixels = (0..9)
            .map(|x| {
                if x < 4 {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [1.0, 0.0, 0.0, 0.0]
                }
            })
            .collect();
        row(pixels)
    }

    fn edge_expected() -> Vec<Rgba> {
        // Radius 1 box on a 1-pixel-high image: horizontal coverage / 3, then
        // the two vertical neighbours are outside (transparent): another / 3.
        (0..9)
            .map(|x: i32| {
                let covered = (x - 1..=x + 1).filter(|sx| (0..4).contains(sx)).count();
                let alpha = covered as f32 / 3.0 / 3.0;
                // Spec: blurring opaque white must stay white wherever visible.
                [1.0, 1.0, 1.0, alpha]
            })
            .collect()
    }

    #[test]
    fn blur_has_no_dark_or_coloured_fringe() {
        let out = render(settings(Mode::BoxBlur), &edge_fixture(), None);
        check_straight(&edge_expected(), &out.pixels, 1e-5).unwrap();
    }

    #[test]
    fn naive_straight_blur_fails_the_same_check() {
        // A blur that ignores alpha drags the hidden red into the edge.
        let input = edge_fixture();
        let naive: Vec<Rgba> = (0..9)
            .map(|x: i32| {
                let mut acc = [0.0; 4];
                for sx in x - 1..=x + 1 {
                    if (0..9).contains(&sx) {
                        let p = input.pixels[sx as usize];
                        for c in 0..4 {
                            acc[c] += p[c] / 9.0;
                        }
                    }
                }
                acc
            })
            .collect();
        assert!(check_straight(&edge_expected(), &naive, 1e-5).is_err());
    }

    #[test]
    fn blend_layer_is_straight_over_composite() {
        let input = row(vec![[1.0, 0.0, 0.0, 1.0], [0.3, 0.3, 0.3, 0.0]]);
        let top = row(vec![[0.0, 0.0, 1.0, 0.5], [0.0, 0.0, 1.0, 0.5]]);
        let out = render(settings(Mode::BlendLayer), &input, Some(&top));
        let expected = vec![[0.5, 0.0, 0.5, 1.0], [0.0, 0.0, 1.0, 0.5]];
        check_straight(&expected, &out.pixels, 1e-6).unwrap();
        // Half opacity: over opaque red mixes 25 % blue; over transparent keeps
        // straight blue with alpha 0.25.
        let half = Settings {
            amount: 0.5,
            ..settings(Mode::BlendLayer)
        };
        let out = render(half, &input, Some(&top));
        check_straight(
            &[[0.75, 0.0, 0.25, 1.0], [0.0, 0.0, 1.0, 0.25]],
            &out.pixels,
            1e-6,
        )
        .unwrap();
    }

    #[test]
    fn blend_without_layer_and_zero_amount_are_identity() {
        let input = row(ALPHAS.iter().map(|&a| [0.9, 0.1, 0.4, a]).collect());
        assert_eq!(render(settings(Mode::BlendLayer), &input, None), input);
        for mode in [Mode::Invert, Mode::BoxBlur, Mode::BlendLayer] {
            let s = Settings {
                amount: 0.0,
                ..settings(mode)
            };
            assert_eq!(render(s, &input, Some(&input)), input);
        }
    }

    #[test]
    fn float_hdr_and_negative_values_survive() {
        let input = row(vec![[4.0, -1.0, 0.5, 0.5]]);
        let out = render(settings(Mode::Invert), &input, None);
        check_straight(&[[-3.0, 2.0, 0.5, 0.5]], &out.pixels, 1e-6).unwrap();
    }

    #[test]
    fn blur_is_bit_identical_for_any_worker_count() {
        let (w, h) = (37, 23);
        let pixels = (0..w * h)
            .map(|i| {
                let f = i as f32;
                [
                    (f * 0.37).sin(),
                    (f * 0.11).cos(),
                    0.5,
                    ((f * 0.07).sin() + 1.0) / 2.0,
                ]
            })
            .collect();
        let input = Image::new(w, h, pixels);
        let s = Settings {
            radius: 4,
            ..settings(Mode::BoxBlur)
        };
        let bits = |img: &Image| {
            img.pixels
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        };
        let serial = bits(&render_with_workers(s, &input, None, 1));
        for workers in [2, 3, 8] {
            let parallel = bits(&render_with_workers(s, &input, None, workers));
            assert_eq!(serial, parallel, "workers={workers}");
        }
    }

    #[test]
    fn popup_values_are_one_based_and_unknown_falls_back() {
        assert_eq!(Mode::from_popup(1), Mode::Invert);
        assert_eq!(Mode::from_popup(3), Mode::BlendLayer);
        assert_eq!(Mode::from_popup(0), Mode::DEFAULT);
        assert_eq!(Mode::from_popup(99), Mode::DEFAULT);
        assert_eq!(Mode::LABELS[Mode::DEFAULT as usize - 1], "Box Blur");
    }

    #[test]
    fn fit_aligns_top_left_and_pads_transparent() {
        let img = Image::new(2, 1, vec![[1.0; 4], [0.5; 4]]);
        let fitted = img.fit(3, 2);
        assert_eq!(fitted.pixels[0], [1.0; 4]);
        assert_eq!(fitted.pixels[1], [0.5; 4]);
        assert_eq!(fitted.pixels[2], [0.0; 4]);
        assert_eq!(fitted.pixels[3], [0.0; 4]);
        assert_eq!(img.fit(1, 1).pixels, vec![[1.0; 4]]);
    }
}
