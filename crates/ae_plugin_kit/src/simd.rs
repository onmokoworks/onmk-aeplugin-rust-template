//! Example SIMD kernel with runtime CPU feature detection.
//!
//! Pattern:
//! 1. A plain scalar reference that defines the exact arithmetic order.
//! 2. A `#[target_feature]` variant that performs the *same* IEEE operations
//!    per lane in the same order (here: AVX `mul` then `add`, never FMA, which
//!    would round differently).
//! 3. A dispatcher that checks `is_x86_feature_detected!` at run time, so one
//!    binary runs on every x86_64 CPU AE supports, and an explicit
//!    `allow_simd` switch so tests can force the fallback without globals or
//!    environment variables.
//!
//! The result is bit-identical between paths, so CPU feature differences
//! between machines never change rendered pixels. Rust does not contract
//! `a + b * c` into FMA on its own, so the scalar path is stable too.
//!
//! On aarch64 (Apple Silicon) the scalar path is used; LLVM auto-vectorises it
//! with NEON, which is baseline on that target.

/// 1-D correlation: `dst[x] = sum_i src[x + i] * taps[i]`, accumulated in tap
/// order starting from `src[x] * taps[0]`.
///
/// `src` must contain at least `dst.len() + taps.len() - 1` samples (pad the
/// row by the kernel radius on both sides before calling).
pub fn convolve_row(src: &[f32], taps: &[f32], dst: &mut [f32]) {
    convolve_row_dispatch(src, taps, dst, true);
}

/// Same as [`convolve_row`] with an explicit switch to force the scalar path.
pub fn convolve_row_dispatch(src: &[f32], taps: &[f32], dst: &mut [f32], allow_simd: bool) {
    assert!(!taps.is_empty(), "kernel must have at least one tap");
    assert!(
        src.len() >= dst.len() + taps.len() - 1,
        "source row must be padded by taps.len() - 1 samples"
    );
    #[cfg(target_arch = "x86_64")]
    if allow_simd && std::is_x86_feature_detected!("avx") {
        // SAFETY: AVX availability was checked above; length checked by the assert.
        unsafe { convolve_avx(src, taps, dst) };
        return;
    }
    let _ = allow_simd;
    convolve_scalar(src, taps, dst, 0);
}

/// Reference implementation; also handles the SIMD tail.
pub fn convolve_scalar(src: &[f32], taps: &[f32], dst: &mut [f32], start: usize) {
    for (x, out) in dst.iter_mut().enumerate().skip(start) {
        let mut v = src[x] * taps[0];
        for (i, &k) in taps.iter().enumerate().skip(1) {
            v += src[x + i] * k;
        }
        *out = v;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn convolve_avx(src: &[f32], taps: &[f32], dst: &mut [f32]) {
    use std::arch::x86_64::*;
    let mut x = 0;
    while x + 8 <= dst.len() {
        let p = src.as_ptr().add(x);
        let mut v = _mm256_mul_ps(_mm256_loadu_ps(p), _mm256_set1_ps(taps[0]));
        for (i, &k) in taps.iter().enumerate().skip(1) {
            v = _mm256_add_ps(
                v,
                _mm256_mul_ps(_mm256_loadu_ps(p.add(i)), _mm256_set1_ps(k)),
            );
        }
        _mm256_storeu_ps(dst.as_mut_ptr().add(x), v);
        x += 8;
    }
    convolve_scalar(src, taps, dst, x);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Bit-exact except NaN payloads, which IEEE leaves implementation-defined
    // when operands are commuted; any NaN must stay NaN.
    fn bits(v: &[f32]) -> Vec<u32> {
        v.iter()
            .map(|f| if f.is_nan() { u32::MAX } else { f.to_bits() })
            .collect()
    }

    #[test]
    fn runtime_dispatch_matches_forced_scalar_bit_for_bit() {
        let mut rng = 0x9381_9472u32;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng
        };
        let specials = [
            0.0,
            -0.0,
            f32::from_bits(1),
            f32::MAX,
            -f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ];
        for width in [1, 2, 7, 8, 9, 15, 16, 17, 33, 257] {
            for taps_len in [1, 2, 3, 9, 31] {
                let taps: Vec<f32> = (0..taps_len)
                    .map(|i| (i + 1) as f32 / (taps_len * taps_len) as f32)
                    .collect();
                for special in [false, true] {
                    let src: Vec<f32> = (0..width + taps_len - 1)
                        .map(|i| {
                            if special {
                                specials[i % specials.len()]
                            } else {
                                (next() as f32 / u32::MAX as f32) * 4.0 - 1.0
                            }
                        })
                        .collect();
                    let mut simd = vec![0.0; width];
                    let mut scalar = vec![0.0; width];
                    convolve_row_dispatch(&src, &taps, &mut simd, true);
                    convolve_row_dispatch(&src, &taps, &mut scalar, false);
                    assert_eq!(
                        bits(&simd),
                        bits(&scalar),
                        "width={width} taps={taps_len} special={special}"
                    );
                }
            }
        }
    }

    #[test]
    fn box_kernel_matches_hand_computed_values() {
        let src = [0.0, 3.0, 6.0, 9.0, 12.0];
        let taps = [1.0 / 3.0; 3];
        let mut dst = [0.0; 3];
        convolve_row(&src, &taps, &mut dst);
        for (got, want) in dst.iter().zip([3.0f32, 6.0, 9.0]) {
            assert!((got - want).abs() < 1e-6, "{got} vs {want}");
        }
    }

    #[test]
    #[should_panic(expected = "padded")]
    fn unpadded_source_is_rejected() {
        convolve_row(&[1.0; 4], &[1.0; 3], &mut [0.0; 4]);
    }
}
