//! Adapters for the pinned `after-effects` crate.

use crate::cleanup::CheckinHost;
use crate::pixels::{decode_row, encode_row, Rgba};
use crate::sdk::has_flag;
use crate::world::{checked_world_access, Depth, WorldAccessError, WorldLayout};
use after_effects as ae;

/// Map a World Suite pixel format to a CPU channel depth.
///
/// GPU worlds (`GpuBgra128`) and every non-ARGB format are rejected: a CPU
/// accessor must never dereference them as host pixels.
pub fn depth_from_format(format: ae::PixelFormat) -> Result<Depth, ae::Error> {
    match format {
        ae::PixelFormat::Argb32 => Ok(Depth::U8),
        ae::PixelFormat::Argb64 => Ok(Depth::U16),
        ae::PixelFormat::Argb128 => Ok(Depth::F32),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

/// Channel depth of a CPU world, from the World Suite pixel format.
///
/// Do not use `Layer::bit_depth()` for this: in the pinned wrapper it returns
/// 16 whenever `PF_WorldFlag_DEEP` is set, which is also the case for 32 bpc
/// (ARGB128) worlds, so a float world would be read as 16-bit integers.
pub fn pixel_depth(layer: &ae::Layer) -> Result<Depth, ae::Error> {
    depth_from_format(layer.pixel_format()?)
}

pub fn map_access_error(error: WorldAccessError) -> ae::Error {
    match error {
        WorldAccessError::Overflow => ae::Error::OutOfMemory,
        _ => ae::Error::BadCallbackParameter,
    }
}

/// Validate a layer's geometry (depth from the World Suite, signed stride,
/// alignment, NULL, overflow) before any pixel access.
pub fn world_layout(layer: &ae::Layer) -> Result<WorldLayout, ae::Error> {
    let depth = pixel_depth(layer)?;
    // SAFETY: reading the pointer value only; it is not dereferenced here.
    let address = unsafe { layer.data_ptr() } as usize;
    checked_world_access(
        layer.width(),
        layer.height(),
        layer.row_bytes(),
        depth,
        address,
    )
    .map_err(map_access_error)
}

/// `true` if `flag` (any `PF_WorldFlag_*` constant) is set on the world.
/// Works with the `i32` Windows and `u32` macOS constant types.
pub fn has_world_flag(layer: &ae::Layer, flag: impl crate::sdk::SdkInt) -> bool {
    let world: *const ae::sys::PF_EffectWorld = ae::AsPtr::as_ptr(layer);
    // SAFETY: `Layer` always wraps a valid, non-NULL world struct.
    has_flag(unsafe { (*world).world_flags }, flag)
}

/// Decode a validated layer into straight, normalized RGBA (row-major,
/// `width * height` pixels). Honours negative strides.
pub fn read_layer(layer: &ae::Layer) -> Result<(WorldLayout, Vec<Rgba>), ae::Error> {
    let layout = world_layout(layer)?;
    let len = layout
        .width
        .checked_mul(layout.height)
        .ok_or(ae::Error::OutOfMemory)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(len)
        .map_err(|_| ae::Error::OutOfMemory)?;
    pixels.resize(len, [0.0; 4]);
    if layout.is_empty() {
        return Ok((layout, pixels));
    }
    let data = unsafe { layer.data_ptr() };
    for (y, row) in pixels.chunks_exact_mut(layout.width).enumerate() {
        // SAFETY: layout validated against this pointer; AE owns the allocation
        // for the duration of the render call and we only read.
        let src = unsafe { layout.row(data, y) };
        decode_row(layout.depth, src, row);
    }
    Ok((layout, pixels))
}

/// Encode straight RGBA into a validated layer. `pixels` must be exactly
/// `layer.width() * layer.height()` long.
pub fn write_layer(layer: &mut ae::Layer, pixels: &[Rgba]) -> Result<WorldLayout, ae::Error> {
    let layout = world_layout(layer)?;
    if layout.width.checked_mul(layout.height) != Some(pixels.len()) {
        return Err(ae::Error::BadCallbackParameter);
    }
    if layout.is_empty() {
        return Ok(layout);
    }
    let data = unsafe { layer.data_ptr_mut() };
    for (y, row) in pixels.chunks_exact(layout.width).enumerate() {
        // SAFETY: as in `read_layer`; `&mut Layer` guarantees exclusive access.
        let dst = unsafe { layout.row_mut(data, y) };
        encode_row(layout.depth, row, dst);
    }
    Ok(layout)
}

impl CheckinHost for ae::pf::SmartRenderCallbacks {
    type Layer = ae::Layer;
    type Error = ae::Error;

    fn checkout(&self, id: u32) -> Result<Option<ae::Layer>, ae::Error> {
        self.checkout_layer_pixels(id)
    }

    fn checkin(&self, id: u32) -> Result<(), ae::Error> {
        self.checkin_layer_pixels(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_world_formats_determine_channel_width() {
        assert_eq!(depth_from_format(ae::PixelFormat::Argb32), Ok(Depth::U8));
        assert_eq!(depth_from_format(ae::PixelFormat::Argb64), Ok(Depth::U16));
        assert_eq!(depth_from_format(ae::PixelFormat::Argb128), Ok(Depth::F32));
        assert!(depth_from_format(ae::PixelFormat::GpuBgra128).is_err());
        assert!(depth_from_format(ae::PixelFormat::Bgra32).is_err());
    }

    #[test]
    fn deep_flag_constant_compares_on_every_platform() {
        // i32 on Windows, u32 on macOS: this line must compile on both.
        assert!(crate::sdk::has_flag(
            ae::sys::PF_WorldFlag_DEEP | ae::sys::PF_WorldFlag_WRITEABLE,
            ae::sys::PF_WorldFlag_DEEP
        ));
    }
}
