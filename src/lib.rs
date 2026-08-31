use jxl::api::states::{Initialized, WithFrameInfo, WithImageInfo};
use jxl::api::{
    JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, JxlPixelFormat,
    ProcessingResult,
};
use jxl::headers::extra_channels::ExtraChannel;

#[cfg(target_arch = "wasm32")]
use wasm_minimal_protocol::{initiate_protocol, wasm_func};

#[cfg(target_arch = "wasm32")]
initiate_protocol!();

pub struct DecodedJxl {
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub encoding: Encoding,
    pub icc: Option<Vec<u8>>,
}

pub enum Encoding {
    Rgb8 = 0,
    Rgba8 = 1,
    Luma8 = 2,
    Lumaa8 = 3,
}

fn decode_header(input: &mut &[u8]) -> Result<JxlDecoder<WithImageInfo>, &'static str> {
    let mut decoder = JxlDecoder::<Initialized>::new(JxlDecoderOptions::default());
    loop {
        match decoder.process(input, None) {
            Ok(ProcessingResult::Complete { result }) => return Ok(result),

            Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                if input.is_empty() {
                    return Err("Corrupted header");
                }

                decoder = fallback;
            }

            Err(_) => return Err("Corrupted header"),
        }
    }
}

fn decode_frame_header(
    mut decoder: JxlDecoder<WithImageInfo>,
    input: &mut &[u8],
) -> Result<JxlDecoder<WithFrameInfo>, &'static str> {
    loop {
        match decoder.process(input, None) {
            Ok(ProcessingResult::Complete { result }) => return Ok(result),

            Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                if input.is_empty() {
                    return Err("Corrupted frame data");
                }

                decoder = fallback;
            }

            Err(_) => return Err("Corrupted frame data"),
        }
    }
}

fn decode_pixels(
    mut decoder: JxlDecoder<WithFrameInfo>,
    input: &mut &[u8],
    buffers: &mut [JxlOutputBuffer<'_>],
) -> Result<(), &'static str> {
    loop {
        match decoder.process(input, buffers, None) {
            Ok(ProcessingResult::Complete { .. }) => return Ok(()),

            Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                if input.is_empty() {
                    return Err("Corrupted pixel data");
                }

                decoder = fallback;
            }

            Err(_) => return Err("Corrupted pixel data"),
        }
    }
}

/// Decode a static JXL image to tightly packed RGB[A]8 or LUMA[A]8 pixels.
///
/// The returned pixel buffer is tightly packed, row-major, top-to-bottom.
/// Each pixel contains 1, 2, 3, or 4 bytes depending on `encoding`.
///
/// `icc` is the ICC profile corresponding to the color space of the decoded image, _if available_.
#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn jxl(mut data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut decoder_with_image_info = decode_header(&mut data)?;
    let basic_info = decoder_with_image_info.basic_info();

    let is_grayscale = decoder_with_image_info
        .current_pixel_format()
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
    // by this decoder configuration.
    decoder_with_image_info.set_pixel_format(target_pixel_format);

    let icc = decoder_with_image_info
        .output_color_profile()
        .try_as_icc()
        .map(std::borrow::Cow::into_owned); // what happens if it's NO?

    let stride = width
        .checked_mul(color_type.samples_per_pixel())
        .ok_or("Image width is too large")?;

    let buffer_len = stride
        .checked_mul(height)
        .ok_or("Image dimensions are too large")?;

    // Advance the decoder to the frame/image data.
    let decoder_with_frame_info = decode_frame_header(decoder_with_image_info, &mut data)?;

    let icc_len = icc.as_ref().map_or(0, Vec::len);

    // FORMAT:
    // width: u32 -> 4
    // height: u32 -> 4
    // encoding: u8 -> 1
    // icc_len: u32 -> 4 (maybe smaller?)
    // icc: Vec<u8> -> icc_len
    // pixels: Vec<u8> -> buffer_len (width * height * samples_per_pixel)

    let total_len = 4 + 4 + 1 + 4 + icc_len + buffer_len;

    // SAFETY:
    // We immediately initialize every byte of `out` either through the
    // header/ICC writes below or through JxlOutputBuffer for the pixel
    // region before `out` is returned.
    let mut out = Vec::with_capacity(total_len);
    #[allow(clippy::uninit_vec)]
    unsafe {
        out.set_len(total_len);
    }

    let mut offset = 0;

    // width
    out[offset..offset + 4].copy_from_slice(&(width as u32).to_le_bytes());
    offset += 4;

    // height
    out[offset..offset + 4].copy_from_slice(&(height as u32).to_le_bytes());
    offset += 4;

    // encoding
    out[offset] = encoding as u8;
    offset += 1;

    // ICC length
    out[offset..offset + 4].copy_from_slice(&(icc_len as u32).to_le_bytes());
    offset += 4;

    // ICC data
    if let Some(icc) = &icc {
        out[offset..offset + icc_len].copy_from_slice(icc);
        offset += icc_len;
    }

    // The remainder of `out` is the pixel buffer.
    let pixels = &mut out[offset..offset + buffer_len];

    let mut buffers = [JxlOutputBuffer::new(pixels, height, stride)];

    decode_pixels(decoder_with_frame_info, &mut data, &mut buffers)?;
    Ok(out)
}
