use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    AeArgb8,
    AeArgb16,
    AeArgb32Float,
}

impl PixelFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::AeArgb8 => 4,
            Self::AeArgb16 => 8,
            Self::AeArgb32Float => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameDesc {
    pub width: u32,
    pub height: u32,
    pub row_bytes: usize,
    pub format: PixelFormat,
}

impl FrameDesc {
    pub fn packed(width: u32, height: u32, format: PixelFormat) -> Result<Self, FrameLayoutError> {
        let row_bytes = (width as usize)
            .checked_mul(format.bytes_per_pixel())
            .ok_or(FrameLayoutError::SizeOverflow)?;
        Ok(Self {
            width,
            height,
            row_bytes,
            format,
        })
    }

    pub fn minimum_row_bytes(self) -> Result<usize, FrameLayoutError> {
        (self.width as usize)
            .checked_mul(self.format.bytes_per_pixel())
            .ok_or(FrameLayoutError::SizeOverflow)
    }

    pub fn required_len(self) -> Result<usize, FrameLayoutError> {
        if self.width == 0 || self.height == 0 {
            return Err(FrameLayoutError::EmptyExtent);
        }
        let minimum = self.minimum_row_bytes()?;
        if self.row_bytes < minimum {
            return Err(FrameLayoutError::RowBytesTooSmall {
                minimum,
                actual: self.row_bytes,
            });
        }
        self.row_bytes
            .checked_mul(self.height as usize)
            .ok_or(FrameLayoutError::SizeOverflow)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameLayoutError {
    EmptyExtent,
    RowBytesTooSmall { minimum: usize, actual: usize },
    BufferTooSmall { required: usize, actual: usize },
    SizeOverflow,
    DimensionMismatch,
}

impl fmt::Display for FrameLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExtent => write!(f, "frame extent must be nonzero"),
            Self::RowBytesTooSmall { minimum, actual } => {
                write!(f, "row bytes {actual} is smaller than required {minimum}")
            }
            Self::BufferTooSmall { required, actual } => {
                write!(f, "frame buffer has {actual} bytes but needs {required}")
            }
            Self::SizeOverflow => write!(f, "frame byte size overflowed usize"),
            Self::DimensionMismatch => write!(f, "source and destination extents differ"),
        }
    }
}

impl Error for FrameLayoutError {}

#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    desc: FrameDesc,
    data: &'a [u8],
}

impl<'a> FrameView<'a> {
    pub fn new(desc: FrameDesc, data: &'a [u8]) -> Result<Self, FrameLayoutError> {
        validate_buffer(desc, data.len())?;
        Ok(Self { desc, data })
    }

    pub const fn desc(self) -> FrameDesc {
        self.desc
    }

    pub fn to_rgba8(self) -> FrameRgba8 {
        let mut pixels = vec![0u8; self.desc.width as usize * self.desc.height as usize * 4];
        let bpp = self.desc.format.bytes_per_pixel();
        for y in 0..self.desc.height as usize {
            let row = &self.data[y * self.desc.row_bytes..];
            for x in 0..self.desc.width as usize {
                let rgba = decode_ae_argb(self.desc.format, &row[x * bpp..]);
                let destination = (y * self.desc.width as usize + x) * 4;
                pixels[destination..destination + 4].copy_from_slice(&rgba);
            }
        }
        FrameRgba8::new(self.desc.width, self.desc.height, pixels)
    }
}

#[derive(Debug)]
pub struct FrameViewMut<'a> {
    desc: FrameDesc,
    data: &'a mut [u8],
}

impl<'a> FrameViewMut<'a> {
    pub fn new(desc: FrameDesc, data: &'a mut [u8]) -> Result<Self, FrameLayoutError> {
        validate_buffer(desc, data.len())?;
        Ok(Self { desc, data })
    }

    pub fn write_rgba8(&mut self, frame: &FrameRgba8) -> Result<(), FrameLayoutError> {
        if self.desc.width != frame.width || self.desc.height != frame.height {
            return Err(FrameLayoutError::DimensionMismatch);
        }
        let bpp = self.desc.format.bytes_per_pixel();
        for y in 0..self.desc.height as usize {
            let row = &mut self.data[y * self.desc.row_bytes..];
            for x in 0..self.desc.width as usize {
                let source = (y * self.desc.width as usize + x) * 4;
                encode_ae_argb(
                    self.desc.format,
                    &frame.pixels[source..source + 4],
                    &mut row[x * bpp..],
                );
            }
        }
        Ok(())
    }
}

fn validate_buffer(desc: FrameDesc, actual: usize) -> Result<(), FrameLayoutError> {
    let required = desc.required_len()?;
    if actual < required {
        return Err(FrameLayoutError::BufferTooSmall { required, actual });
    }
    Ok(())
}

fn decode_ae_argb(format: PixelFormat, pixel: &[u8]) -> [u8; 4] {
    match format {
        PixelFormat::AeArgb8 => [pixel[1], pixel[2], pixel[3], pixel[0]],
        PixelFormat::AeArgb16 => {
            let channel = |offset| {
                let value = u16::from_ne_bytes([pixel[offset], pixel[offset + 1]]).min(32768);
                ((value as u32 * 255 + 16384) / 32768) as u8
            };
            [channel(2), channel(4), channel(6), channel(0)]
        }
        PixelFormat::AeArgb32Float => {
            let channel = |offset| {
                let value = f32::from_ne_bytes(pixel[offset..offset + 4].try_into().unwrap());
                if value.is_finite() {
                    (value.clamp(0.0, 1.0) * 255.0).round() as u8
                } else {
                    0
                }
            };
            [channel(4), channel(8), channel(12), channel(0)]
        }
    }
}

fn encode_ae_argb(format: PixelFormat, rgba: &[u8], pixel: &mut [u8]) {
    match format {
        PixelFormat::AeArgb8 => {
            pixel[..4].copy_from_slice(&[rgba[3], rgba[0], rgba[1], rgba[2]]);
        }
        PixelFormat::AeArgb16 => {
            for (offset, value) in [(0, rgba[3]), (2, rgba[0]), (4, rgba[1]), (6, rgba[2])] {
                let value = ((value as u32 * 32768 + 127) / 255) as u16;
                pixel[offset..offset + 2].copy_from_slice(&value.to_ne_bytes());
            }
        }
        PixelFormat::AeArgb32Float => {
            for (offset, value) in [(0, rgba[3]), (4, rgba[0]), (8, rgba[1]), (12, rgba[2])] {
                pixel[offset..offset + 4].copy_from_slice(&(value as f32 / 255.0).to_ne_bytes());
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FrameRgba8 {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl FrameRgba8 {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        debug_assert_eq!(pixels.len(), width as usize * height as usize * 4);
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn synthetic(width: u32, height: u32) -> Self {
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let i = ((y * width + x) * 4) as usize;
                let fx = x as f32 / width.max(1) as f32;
                let fy = y as f32 / height.max(1) as f32;
                pixels[i] = (fx * 255.0) as u8;
                pixels[i + 1] = (fy * 255.0) as u8;
                pixels[i + 2] = (((fx * 19.0).sin() * (fy * 23.0).cos() * 0.5 + 0.5) * 255.0) as u8;
                pixels[i + 3] = 255;
            }
        }
        Self::new(width, height, pixels)
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LabParams {
    pub size: [u32; 4],
    pub control0: [f32; 4],
    pub control1: [f32; 4],
}

impl LabParams {
    pub fn new(width: u32, height: u32, passes: u32, strength: f32, radius: u32) -> Self {
        Self {
            size: [width, height, passes.max(1), radius.max(1)],
            control0: [strength, 0.0, 0.0, 0.0],
            control1: [0.0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(format: PixelFormat) {
        let frame = FrameRgba8::new(2, 1, vec![10, 20, 30, 40, 255, 128, 0, 255]);
        let packed = FrameDesc::packed(2, 1, format).unwrap();
        let desc = FrameDesc {
            row_bytes: packed.row_bytes + 16,
            ..packed
        };
        let mut storage = vec![0xCD; desc.required_len().unwrap()];
        FrameViewMut::new(desc, &mut storage)
            .unwrap()
            .write_rgba8(&frame)
            .unwrap();
        let decoded = FrameView::new(desc, &storage).unwrap().to_rgba8();
        assert_eq!(decoded.pixels, frame.pixels);
        assert!(storage[packed.row_bytes..].iter().all(|byte| *byte == 0xCD));
    }

    #[test]
    fn rowbytes_aware_argb8_roundtrip() {
        roundtrip(PixelFormat::AeArgb8);
    }

    #[test]
    fn rowbytes_aware_argb16_roundtrip() {
        roundtrip(PixelFormat::AeArgb16);
    }

    #[test]
    fn rowbytes_aware_argb32f_roundtrip() {
        roundtrip(PixelFormat::AeArgb32Float);
    }

    // Read and write are checked separately against hand-computed bytes: a
    // roundtrip alone would also pass if both sides premultiplied.
    #[test]
    fn partial_alpha_argb8_decodes_straight() {
        let desc = FrameDesc::packed(1, 1, PixelFormat::AeArgb8).unwrap();
        let decoded = FrameView::new(desc, &[128, 255, 51, 0]).unwrap().to_rgba8();
        assert_eq!(decoded.pixels, [255, 51, 0, 128]);
    }

    #[test]
    fn partial_alpha_argb16_encodes_straight() {
        let desc = FrameDesc::packed(1, 1, PixelFormat::AeArgb16).unwrap();
        let mut storage = [0u8; 8];
        FrameViewMut::new(desc, &mut storage)
            .unwrap()
            .write_rgba8(&FrameRgba8::new(1, 1, vec![255, 0, 0, 128]))
            .unwrap();
        let c = |i: usize| u16::from_ne_bytes([storage[i * 2], storage[i * 2 + 1]]);
        // A = 128 * 32768 / 255 = 16448.25, rounded, R = 32768 (white is 32768, not alpha-scaled).
        assert_eq!([c(0), c(1), c(2), c(3)], [16448, 32768, 0, 0]);
    }

    #[test]
    fn partial_alpha_argb32f_encodes_straight() {
        let desc = FrameDesc::packed(1, 1, PixelFormat::AeArgb32Float).unwrap();
        let mut storage = [0u8; 16];
        FrameViewMut::new(desc, &mut storage)
            .unwrap()
            .write_rgba8(&FrameRgba8::new(1, 1, vec![255, 255, 255, 51]))
            .unwrap();
        let c = |i: usize| f32::from_ne_bytes(storage[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!([c(0), c(1), c(2), c(3)], [0.2, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn rejects_short_rows_and_buffers() {
        let short_row = FrameDesc {
            width: 2,
            height: 1,
            row_bytes: 7,
            format: PixelFormat::AeArgb8,
        };
        assert!(matches!(
            FrameView::new(short_row, &[0; 8]),
            Err(FrameLayoutError::RowBytesTooSmall { .. })
        ));

        let desc = FrameDesc::packed(2, 2, PixelFormat::AeArgb8).unwrap();
        assert!(matches!(
            FrameView::new(desc, &[0; 15]),
            Err(FrameLayoutError::BufferTooSmall { .. })
        ));
    }
}
