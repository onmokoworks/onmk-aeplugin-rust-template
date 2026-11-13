//! Checked world geometry.
//!
//! Every CPU accessor must validate the world before forming a slice or a
//! pixel pointer: AE hands plug-ins a raw `PF_EffectWorld` whose `rowbytes` may
//! be negative, padded, or (in a broken host / harness) inconsistent with the
//! pixel depth. [`checked_world_access`] rejects layouts that would make row
//! arithmetic overflow, read unaligned channels, or dereference NULL.

use std::fmt;

/// Channel depth of an AE CPU world. Determine it from the World Suite pixel
/// format, not from `PF_WorldFlag_DEEP` alone (see `docs/cross-platform.md`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Depth {
    /// `PF_PixelFormat_ARGB32`: `u8` channels, white = 255.
    U8,
    /// `PF_PixelFormat_ARGB64`: `u16` channels, white = 32768 (not 65535).
    U16,
    /// `PF_PixelFormat_ARGB128`: `f32` channels, white = 1.0, HDR allowed.
    F32,
}

impl Depth {
    pub const fn bits(self) -> u32 {
        match self {
            Self::U8 => 8,
            Self::U16 => 16,
            Self::F32 => 32,
        }
    }

    /// Size of one channel in bytes; also the required alignment.
    pub const fn channel_bytes(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::F32 => 4,
        }
    }

    pub const fn pixel_bytes(self) -> usize {
        self.channel_bytes() * 4
    }

    pub const fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            8 => Some(Self::U8),
            16 => Some(Self::U16),
            32 => Some(Self::F32),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldAccessError {
    /// Width/height/rowbytes outside the `A_long` range AE can describe.
    DimensionOutOfRange,
    /// `|rowbytes|` is smaller than `width * pixel_bytes`.
    RowBytesTooSmall { minimum: usize, actual: usize },
    /// Data pointer or stride is not aligned to the channel size.
    Misaligned,
    /// Non-empty world with a NULL data pointer.
    NullData,
    /// Row or total extent overflows `usize` / `isize`.
    Overflow,
}

impl fmt::Display for WorldAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionOutOfRange => write!(f, "world dimensions are outside the A_long range"),
            Self::RowBytesTooSmall { minimum, actual } => {
                write!(
                    f,
                    "|rowbytes| {actual} is smaller than the row size {minimum}"
                )
            }
            Self::Misaligned => write!(f, "world data or stride is not channel aligned"),
            Self::NullData => write!(f, "non-empty world has NULL data"),
            Self::Overflow => write!(f, "world extent overflows the address space"),
        }
    }
}

impl std::error::Error for WorldAccessError {}

/// Geometry that passed [`checked_world_access`]. Row offsets computed from it
/// cannot overflow `isize`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldLayout {
    pub width: usize,
    pub height: usize,
    /// Signed stride exactly as AE reported it. Row `y` starts at
    /// `data + y * row_bytes`, also when `row_bytes` is negative.
    pub row_bytes: isize,
    pub depth: Depth,
}

impl WorldLayout {
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Bytes of pixel data in one row (excluding padding).
    pub const fn row_len(&self) -> usize {
        self.width * self.depth.pixel_bytes()
    }

    /// Byte offset of row `y` relative to the world's data pointer.
    pub fn row_offset(&self, y: usize) -> isize {
        assert!(
            y < self.height,
            "row {y} outside world height {}",
            self.height
        );
        // Validated: |row_bytes| * height <= isize::MAX.
        y as isize * self.row_bytes
    }

    /// Borrow row `y` of a validated world.
    ///
    /// # Safety
    /// `data` must be the data pointer that was passed to
    /// [`checked_world_access`] for this layout, the allocation must really
    /// span every row, and no mutable alias may exist for the lifetime `'a`.
    pub unsafe fn row<'a>(&self, data: *const u8, y: usize) -> &'a [u8] {
        std::slice::from_raw_parts(data.offset(self.row_offset(y)), self.row_len())
    }

    /// Mutably borrow row `y` of a validated world.
    ///
    /// # Safety
    /// As for [`WorldLayout::row`], and no other reference to the row may exist.
    pub unsafe fn row_mut<'a>(&self, data: *mut u8, y: usize) -> &'a mut [u8] {
        std::slice::from_raw_parts_mut(data.offset(self.row_offset(y)), self.row_len())
    }
}

/// Validate a CPU world before any pixel or row access.
///
/// `address` is the world's data pointer as an integer. Empty worlds
/// (`width == 0 || height == 0`) are accepted without touching the pointer so
/// callers can early-out; every other world must have a non-NULL, channel
/// aligned pointer and a stride covering a full row.
pub fn checked_world_access(
    width: usize,
    height: usize,
    row_bytes: isize,
    depth: Depth,
    address: usize,
) -> Result<WorldLayout, WorldAccessError> {
    let component = depth.channel_bytes();
    // `A_long` is 32-bit on every AE platform; `i32::MIN` has no absolute value.
    if width > i32::MAX as usize
        || height > i32::MAX as usize
        || row_bytes.unsigned_abs() > i32::MAX as usize
    {
        return Err(WorldAccessError::DimensionOutOfRange);
    }
    let stride = row_bytes.unsigned_abs();
    let row = width
        .checked_mul(depth.pixel_bytes())
        .ok_or(WorldAccessError::Overflow)?;
    let extent = stride
        .checked_mul(height)
        .ok_or(WorldAccessError::Overflow)?;
    if extent > isize::MAX as usize {
        return Err(WorldAccessError::Overflow);
    }
    let layout = WorldLayout {
        width,
        height,
        row_bytes,
        depth,
    };
    if layout.is_empty() {
        return Ok(layout);
    }
    if stride < row {
        return Err(WorldAccessError::RowBytesTooSmall {
            minimum: row,
            actual: stride,
        });
    }
    if address == 0 {
        return Err(WorldAccessError::NullData);
    }
    if address % component != 0 || stride % component != 0 {
        return Err(WorldAccessError::Misaligned);
    }
    // The last row's start must also be representable from `address`.
    if row_bytes < 0 && address < extent - stride {
        return Err(WorldAccessError::Overflow);
    }
    if row_bytes > 0 && address.checked_add(extent).is_none() {
        return Err(WorldAccessError::Overflow);
    }
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEPTHS: [Depth; 3] = [Depth::U8, Depth::U16, Depth::F32];

    #[test]
    fn accepts_padded_positive_and_negative_strides() {
        for depth in DEPTHS {
            let row = 17 * depth.pixel_bytes();
            for sign in [1isize, -1] {
                let stride = (row + 8) as isize * sign;
                let layout = checked_world_access(17, 31, stride, depth, 1 << 20).unwrap();
                assert_eq!(layout.row_len(), row);
                assert_eq!(layout.row_offset(30), 30 * stride);
            }
        }
    }

    #[test]
    fn rejects_short_rows() {
        for depth in DEPTHS {
            let short = (17 * depth.pixel_bytes() - 1) as isize;
            for sign in [1isize, -1] {
                assert!(matches!(
                    checked_world_access(17, 31, short * sign, depth, 1 << 20),
                    Err(WorldAccessError::RowBytesTooSmall { .. })
                ));
            }
        }
    }

    #[test]
    fn rejects_null_and_misaligned() {
        for depth in DEPTHS {
            assert_eq!(
                checked_world_access(1, 1, 16, depth, 0),
                Err(WorldAccessError::NullData)
            );
            if depth.channel_bytes() > 1 {
                assert_eq!(
                    checked_world_access(1, 1, 16, depth, 17),
                    Err(WorldAccessError::Misaligned)
                );
                assert_eq!(
                    checked_world_access(1, 1, 17, depth, 16),
                    Err(WorldAccessError::Misaligned)
                );
            }
        }
    }

    #[test]
    fn rejects_overflow_and_out_of_range() {
        for depth in DEPTHS {
            assert_eq!(
                checked_world_access(1, 2, isize::MAX, depth, 16),
                Err(WorldAccessError::DimensionOutOfRange)
            );
            assert_eq!(
                checked_world_access(1, 1, i32::MIN as isize, depth, 16),
                Err(WorldAccessError::DimensionOutOfRange)
            );
            assert!(checked_world_access(usize::MAX, 1, 16, depth, 16).is_err());
            assert!(checked_world_access(1, usize::MAX, 16, depth, 16).is_err());
            // Negative stride whose last row would start below address 0.
            assert_eq!(
                checked_world_access(1, 4, -16, depth, 16),
                Err(WorldAccessError::Overflow)
            );
            // Positive extent wrapping the address space.
            assert_eq!(
                checked_world_access(1, 4, 16, depth, usize::MAX - 15),
                Err(WorldAccessError::Overflow)
            );
        }
    }

    #[test]
    fn empty_worlds_never_touch_the_pointer() {
        for depth in DEPTHS {
            assert!(checked_world_access(0, 0, 0, depth, 0).unwrap().is_empty());
            assert!(checked_world_access(0, 5, 0, depth, 0).unwrap().is_empty());
        }
    }

    #[test]
    fn rows_follow_signed_stride() {
        // Two 1-pixel rows, 8 bytes apart; row 0 is the *higher* address.
        let mut storage = [0u8; 16];
        storage[8..12].copy_from_slice(&[1, 2, 3, 4]);
        storage[0..4].copy_from_slice(&[5, 6, 7, 8]);
        let base = unsafe { storage.as_ptr().add(8) };
        let layout = checked_world_access(1, 2, -8, Depth::U8, base as usize).unwrap();
        unsafe {
            assert_eq!(layout.row(base, 0), &[1, 2, 3, 4]);
            assert_eq!(layout.row(base, 1), &[5, 6, 7, 8]);
        }
    }
}
