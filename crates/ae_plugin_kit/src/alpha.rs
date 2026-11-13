//! Alpha association helpers and a straight-alpha acceptance check.
//!
//! See `docs/alpha-contract.md`. The AE boundary is straight RGB. Spatial
//! filters (blur, resample, warp) must still weight colour by alpha
//! internally, which is easiest in premultiplied space:
//!
//! ```text
//! straight in --premultiply--> filter --unpremultiply--> straight out
//! ```
//!
//! Per-pixel colour operations work directly on straight RGB.

use crate::pixels::Rgba;

/// Alpha at or below this is treated as fully transparent when un-premultiplying.
pub const TRANSPARENT_EPSILON: f32 = 1.0 / 65536.0;

pub fn premultiply(p: Rgba) -> Rgba {
    let a = p[3];
    [p[0] * a, p[1] * a, p[2] * a, a]
}

/// Inverse of [`premultiply`]. Fully transparent pixels get RGB = 0 (their
/// colour is undefined; 0 keeps output deterministic).
pub fn unpremultiply(p: Rgba) -> Rgba {
    let a = p[3];
    if a <= TRANSPARENT_EPSILON {
        [0.0, 0.0, 0.0, a.max(0.0)]
    } else {
        [p[0] / a, p[1] / a, p[2] / a, a]
    }
}

pub fn premultiply_in_place(pixels: &mut [Rgba]) {
    for p in pixels {
        *p = premultiply(*p);
    }
}

pub fn unpremultiply_in_place(pixels: &mut [Rgba]) {
    for p in pixels {
        *p = unpremultiply(*p);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StraightCheckError {
    LengthMismatch,
    /// The fixture has no pixel with 0 < alpha < 1, so it cannot distinguish
    /// straight from premultiplied output. Opaque-only data is not evidence.
    NoPartialAlpha,
    AlphaMismatch {
        index: usize,
        expected: f32,
        actual: f32,
    },
    RgbMismatch {
        index: usize,
        expected: Rgba,
        actual: Rgba,
    },
}

/// Compare `actual` against an independently computed straight `expected`.
///
/// RGB is compared only where expected alpha is visible (> `TRANSPARENT_EPSILON`),
/// because RGB under zero alpha is undefined at the AE boundary. Alpha is
/// always compared. The fixture must contain partial alpha.
pub fn check_straight(
    expected: &[Rgba],
    actual: &[Rgba],
    tolerance: f32,
) -> Result<(), StraightCheckError> {
    if expected.len() != actual.len() {
        return Err(StraightCheckError::LengthMismatch);
    }
    if !expected.iter().any(|p| p[3] > 0.01 && p[3] < 0.99) {
        return Err(StraightCheckError::NoPartialAlpha);
    }
    // NaN never counts as close.
    let close = |x: f32, y: f32| (x - y).abs() <= tolerance;
    for (index, (e, a)) in expected.iter().zip(actual).enumerate() {
        if !close(e[3], a[3]) {
            return Err(StraightCheckError::AlphaMismatch {
                index,
                expected: e[3],
                actual: a[3],
            });
        }
        if e[3] > TRANSPARENT_EPSILON && (0..3).any(|c| !close(e[c], a[c])) {
            return Err(StraightCheckError::RgbMismatch {
                index,
                expected: *e,
                actual: *a,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<Rgba> {
        [0.0, 1.0 / 255.0, 0.125, 0.5, 1.0]
            .into_iter()
            .map(|a| [1.0, 0.8666667, 0.2, a])
            .collect()
    }

    #[test]
    fn straight_output_passes() {
        let expected = fixture();
        check_straight(&expected, &expected.clone(), 1e-6).unwrap();
    }

    #[test]
    fn intentionally_premultiplied_output_fails() {
        let expected = fixture();
        let premultiplied: Vec<_> = expected.iter().map(|p| premultiply(*p)).collect();
        assert!(matches!(
            check_straight(&expected, &premultiplied, 1.0 / 255.0),
            Err(StraightCheckError::RgbMismatch { .. })
        ));
    }

    #[test]
    fn opaque_only_fixture_is_not_evidence() {
        let opaque = vec![[1.0, 1.0, 1.0, 1.0]; 4];
        assert_eq!(
            check_straight(&opaque, &opaque, 1e-6),
            Err(StraightCheckError::NoPartialAlpha)
        );
    }

    #[test]
    fn roundtrip_and_transparent_rgb() {
        let p = [0.25, 0.5, 4.0, 0.5];
        assert_eq!(unpremultiply(premultiply(p)), p);
        assert_eq!(unpremultiply([0.3, 0.3, 0.3, 0.0]), [0.0, 0.0, 0.0, 0.0]);
    }
}
