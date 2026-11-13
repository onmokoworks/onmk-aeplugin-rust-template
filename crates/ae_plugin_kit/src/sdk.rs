//! Portable handling of SDK integer fields.
//!
//! bindgen output differs between the Windows and macOS AE SDK headers. For
//! example, in the pinned `after-effects-sys` bindings:
//!
//! | item | Windows | macOS |
//! |---|---|---|
//! | `PF_WorldFlag_DEEP` constant | `c_int` (`i32`) | `c_uint` (`u32`) |
//! | `PF_EffectWorld::world_flags` | `A_long` (`i32`) | `A_long` (`i32`) |
//!
//! So `world.world_flags & PF_WorldFlag_DEEP` compiles on Windows and fails on
//! macOS (`i32 & u32`). Writing `as i32` / `as u32` casts by hand tends to
//! compile on one platform only, or silently truncate. Instead, widen both
//! sides to `i64` with [`SdkInt::bits`] and compare there; every 8/16/32-bit
//! signed or unsigned SDK value is represented exactly.

/// Integer types that appear in SDK structs and constants.
pub trait SdkInt: Copy {
    fn bits(self) -> i64;
}

macro_rules! sdk_int {
    ($($t:ty),*) => {$(
        impl SdkInt for $t {
            #[inline]
            fn bits(self) -> i64 {
                self as i64
            }
        }
    )*};
}

sdk_int!(i8, i16, i32, u8, u16, u32);

/// `true` when every bit of `flag` is set in `flags`, whatever the two
/// platform-specific integer types are.
#[inline]
pub fn has_flag(flags: impl SdkInt, flag: impl SdkInt) -> bool {
    let flag = flag.bits() & 0xffff_ffff;
    flags.bits() & 0xffff_ffff & flag == flag
}

/// Value equality across differently typed SDK integers, comparing the 32-bit
/// patterns (an `A_long` of -1 equals a `u32` of `0xFFFF_FFFF`).
#[inline]
pub fn same_bits(a: impl SdkInt, b: impl SdkInt) -> bool {
    a.bits() & 0xffff_ffff == b.bits() & 0xffff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_compare_across_signedness() {
        let windows_flags: i32 = 0x0200_0001; // DEEP | RESERVED1
        let mac_deep: u32 = 1;
        let windows_deep: i32 = 1;
        assert!(has_flag(windows_flags, mac_deep));
        assert!(has_flag(windows_flags, windows_deep));
        assert!(!has_flag(0i32, mac_deep));
        assert!(has_flag(-1i32, 0x8000_0000u32));
        assert!(!has_flag(0x7fff_ffffi32, 0x8000_0000u32));
    }

    #[test]
    fn same_bits_matches_32_bit_patterns() {
        assert!(same_bits(-1i32, u32::MAX));
        assert!(same_bits(5u16, 5i32));
        assert!(!same_bits(1i32, 2u32));
    }
}
