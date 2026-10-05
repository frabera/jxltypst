use jxl::api::{
    Event, ExtraChannel, JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions,
    JxlOutputBuffer, JxlPixelFormat,
};

#[cfg(target_arch = "wasm32")]
use wasm_minimal_protocol::{initiate_protocol, wasm_func};

#[cfg(target_arch = "wasm32")]
initiate_protocol!();

pub enum Encoding {
    Rgb8 = 0,
    Rgba8 = 1,
    Luma8 = 2,
    Lumaa8 = 3,
}

#[inline(always)]
fn serialize_header(
    out: &mut [u8],
    width: usize,
    height: usize,
    encoding: Encoding,
    icc: &[u8],
) -> usize {
    let mut offset = 0;

    out[offset..offset + 4].copy_from_slice(&(width as u32).to_le_bytes());
    offset += 4;

    out[offset..offset + 4].copy_from_slice(&(height as u32).to_le_bytes());
    offset += 4;

    out[offset] = encoding as u8;
    offset += 1;

    let icc_len = icc.len();
    out[offset..offset + 4].copy_from_slice(&(icc_len as u32).to_le_bytes());
    offset += 4;

    out[offset..offset + icc_len].copy_from_slice(icc);
    offset += icc_len;

    offset
}

/// Decode a static JXL image to tightly packed RGB[A]8 or LUMA[A]8 pixels.
///
/// The returned pixel buffer is tightly packed, row-major, top-to-bottom.
/// Each pixel contains 1, 2, 3, or 4 bytes depending on `encoding`.
#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn jxl(mut data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut decoder = JxlDecoder::new(JxlDecoderOptions::default());

    while decoder
        .process(&mut data, None, None)
        .map_err(|_| "Corrupted header")?
        != Event::BasicInfo
    {
        if data.is_empty() {
            return Err("Source file truncated");
        }
    }

    let basic_info = decoder.basic_info().unwrap().clone();

    let is_grayscale = decoder
        .current_pixel_format()
        .unwrap()
        .color_type
        .is_grayscale();

    let mut has_alpha = false;
    for channel in &basic_info.extra_channels {
        match channel.ec_type {
            ExtraChannel::Alpha => has_alpha = true,
            ExtraChannel::Black => return Err("CMYK is not currently supported."),
            _ => {}
        }
    }

    let (color_type, encoding) = match (is_grayscale, has_alpha) {
        (true, true) => (JxlColorType::GrayscaleAlpha, Encoding::Lumaa8),
        (true, false) => (JxlColorType::Grayscale, Encoding::Luma8),
        (false, true) => (JxlColorType::Rgba, Encoding::Rgba8),
        (false, false) => (JxlColorType::Rgb, Encoding::Rgb8),
    };

    let target_pixel_format = JxlPixelFormat {
        color_type,
        color_data_format: Some(JxlDataFormat::U8 { bit_depth: 8 }),
        extra_channel_format: vec![None; basic_info.extra_channels.len()],
    };

    let (width, height) = basic_info.size;
    if width == 0 || height == 0 {
        return Err("Corrupted image (width, height)");
    }

    // Configure the decoder's actual output format before obtaining the
    // color profile. The ICC returned below describes the pixels produced
    // by this decoder configuration. (can't panic if set before frame dec.)
    decoder.set_pixel_format(target_pixel_format).unwrap();

    let stride = width
        .checked_mul(color_type.samples_per_pixel())
        .ok_or("Image width is too large")?;

    let buffer_len = stride
        .checked_mul(height)
        .ok_or("Image dimensions are too large")?;

    // The ICC profile corresponding to the color space of the decoded image, _if available_.
    let icc = decoder.output_color_profile().unwrap().try_as_icc();

    let icc: &[u8] = match &icc {
        Some(icc) => icc.as_slice(),
        None => &[],
    };

    let icc_len = icc.len();

    const HEADER_LEN: usize = 4 + 4 + 1 + 4;

    let total_len = HEADER_LEN
        .checked_add(icc_len)
        .and_then(|x| x.checked_add(buffer_len))
        .ok_or("Output is too large")?;

    // FORMAT:
    // width: u32 -> 4
    // height: u32 -> 4
    // encoding: u8 -> 1
    // icc_len: u32 -> 4
    // icc: Vec<u8> -> icc_len
    // pixels: Vec<u8> -> buffer_len (width * height * samples_per_pixel)
    let mut out = Vec::with_capacity(total_len);

    // SAFETY:
    // We immediately initialize every byte of `out` with `serialize_header`
    // and  `JxlOutputBuffer` for the pixel region before `out` is returned.
    #[allow(clippy::uninit_vec)]
    unsafe {
        out.set_len(total_len);
    }
    let offset = serialize_header(&mut out, width, height, encoding, icc);
    // The remainder of `out` is the pixel buffer.
    let pixels = &mut out[offset..];

    let mut buffers = [JxlOutputBuffer::new(pixels, height, stride)];

    loop {
        match decoder
            .process(&mut data, Some(&mut buffers), None)
            .map_err(|_| "Corrupted pixel data")?
        {
            Event::BasicInfo => unreachable!(),
            Event::FrameHeader => {} // Only first frame is decoded for animated JXL, duration not needed
            Event::FrameComplete { .. } => break, // Only first frame is decoded for animated JXL
            Event::Complete => break,
            Event::NeedMoreInput { .. } => {
                if data.is_empty() {
                    return Err("Source file truncated");
                }
            }
        }
    }

    Ok(out)
}
