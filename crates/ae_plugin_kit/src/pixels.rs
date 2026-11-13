//! AE ARGB row codecs.
//!
//! AE CPU worlds store channels in **A, R, G, B** order and the RGB values are
//! **straight** (not premultiplied) at the effect boundary. These codecs only
//! change channel order and scale; they never multiply or divide by alpha.
//! Association (premultiplication) is a separate, explicit step in
//! [`crate::alpha`].
//!
//! Scales: 8 bpc white = 255, 16 bpc white = 32768, 32 bpc white = 1.0.
//! Float values are passed through unclamped so HDR and negative values survive.

use crate::world::Depth;

/// Straight, normalized RGBA. Index 3 is alpha.
pub type Rgba = [f32; 4];

pub const MAX_U16: f32 = 32768.0;

/// Decode one AE ARGB row into straight RGBA.
///
/// `src` must hold at least `dst.len()` pixels at `depth`.
pub fn decode_row(depth: Depth, src: &[u8], dst: &mut [Rgba]) {
    let bpp = depth.pixel_bytes();
    assert!(src.len() >= dst.len() * bpp, "source row too short");
    for (pixel, out) in src.chunks_exact(bpp).zip(dst.iter_mut()) {
        let argb = match depth {
            Depth::U8 => [
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
                pixel[3] as f32 / 255.0,
            ],
            Depth::U16 => {
                let c = |i: usize| u16::from_ne_bytes([pixel[i * 2], pixel[i * 2 + 1]]) as f32;
                [
                    c(0) / MAX_U16,
                    c(1) / MAX_U16,
                    c(2) / MAX_U16,
                    c(3) / MAX_U16,
                ]
            }
            Depth::F32 => {
                let c = |i: usize| f32::from_ne_bytes(pixel[i * 4..i * 4 + 4].try_into().unwrap());
                [c(0), c(1), c(2), c(3)]
            }
        };
        *out = [argb[1], argb[2], argb[3], argb[0]];
    }
}

/// Encode straight RGBA into one AE ARGB row.
///
/// Integer depths round to nearest and clamp to the AE range. Float depth is
/// written unchanged except that NaN becomes 0 (AE has no meaning for NaN).
pub fn encode_row(depth: Depth, src: &[Rgba], dst: &mut [u8]) {
    let bpp = depth.pixel_bytes();
    assert!(dst.len() >= src.len() * bpp, "destination row too short");
    for (rgba, pixel) in src.iter().zip(dst.chunks_exact_mut(bpp)) {
        let argb = [rgba[3], rgba[0], rgba[1], rgba[2]];
        match depth {
            Depth::U8 => {
                for (byte, value) in pixel.iter_mut().zip(argb) {
                    *byte = quantize(value, 255.0) as u8;
                }
            }
            Depth::U16 => {
                for (i, value) in argb.into_iter().enumerate() {
                    let v = quantize(value, MAX_U16) as u16;
                    pixel[i * 2..i * 2 + 2].copy_from_slice(&v.to_ne_bytes());
                }
            }
            Depth::F32 => {
                for (i, value) in argb.into_iter().enumerate() {
                    let v = if value.is_nan() { 0.0 } else { value };
                    pixel[i * 4..i * 4 + 4].copy_from_slice(&v.to_ne_bytes());
                }
            }
        }
    }
}

fn quantize(value: f32, max: f32) -> u32 {
    if value.is_nan() {
        return 0;
    }
    (value * max + 0.5).clamp(0.0, max) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    // Write and read are checked separately against hand-computed bytes so a
    // symmetric mistake (e.g. premultiplying in both directions) cannot pass.

    #[test]
    fn decode_8bit_is_straight_argb() {
        let src = [128u8, 255, 51, 0]; // A=128, R=255, G=51, B=0
        let mut out = [[0.0; 4]];
        decode_row(Depth::U8, &src, &mut out);
        assert_eq!(out[0], [1.0, 0.2, 0.0, 128.0 / 255.0]);
    }

    #[test]
    fn encode_8bit_does_not_premultiply() {
        let mut dst = [0u8; 4];
        encode_row(Depth::U8, &[[1.0, 0.2, 0.0, 0.5]], &mut dst);
        assert_eq!(dst, [128, 255, 51, 0]);
    }

    #[test]
    fn sixteen_bit_white_is_32768() {
        let mut dst = [0u8; 8];
        encode_row(Depth::U16, &[[1.0, 0.5, 2.0, 0.25]], &mut dst);
        let c = |i: usize| u16::from_ne_bytes([dst[i * 2], dst[i * 2 + 1]]);
        assert_eq!([c(0), c(1), c(2), c(3)], [8192, 32768, 16384, 32768]);
        let mut out = [[0.0; 4]];
        decode_row(Depth::U16, &dst, &mut out);
        assert_eq!(out[0], [1.0, 0.5, 1.0, 0.25]);
    }

    #[test]
    fn float_keeps_hdr_and_negative() {
        let mut dst = [0u8; 16];
        encode_row(Depth::F32, &[[4.0, -1.0, 0.125, 0.5]], &mut dst);
        let mut out = [[0.0; 4]];
        decode_row(Depth::F32, &dst, &mut out);
        assert_eq!(out[0], [4.0, -1.0, 0.125, 0.5]);
        assert_eq!(f32::from_ne_bytes(dst[0..4].try_into().unwrap()), 0.5);
    }

    #[test]
    fn integer_encode_clamps_and_maps_nan_to_zero() {
        let mut dst = [0u8; 4];
        encode_row(Depth::U8, &[[f32::NAN, -0.5, 7.0, 1.0]], &mut dst);
        assert_eq!(dst, [255, 0, 0, 255]);
    }
}
