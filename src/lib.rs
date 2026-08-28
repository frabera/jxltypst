use ciborium::ser::into_writer;
use jxl::api::states::{Initialized, WithFrameInfo, WithImageInfo};
use jxl::api::{
    JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, JxlPixelFormat,
    ProcessingResult,
};
use jxl::headers::extra_channels::ExtraChannel;
use jxl::image::{Image, Rect};

#[cfg(target_arch = "wasm32")]
use wasm_minimal_protocol::{initiate_protocol, wasm_func};

#[cfg(target_arch = "wasm32")]
initiate_protocol!();

#[derive(serde::Serialize)]
pub struct DecodedJxl {
    /// Tightly packed row-major RGB/RGBA8 pixels.
    #[serde(with = "serde_bytes")]
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub encoding: Encoding,
    #[serde(with = "serde_bytes", skip_serializing_if = "Option::is_none")]
    pub icc: Option<Vec<u8>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Rgb8,
    Rgba8,
    Luma8,
    Lumaa8,
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

/// Decode a static JXL image to tightly packed RGB[A]8 pixels.
///
/// The returned pixel buffer is tightly packed, row-major, top-to-bottom.
/// Each pixel contains 1, 2, 3, or 4 bytes depending on `encoding`.
///
/// `icc` is the ICC profile corresponding to the color space of the decoded image, if available.
pub fn decode_jxl_to_vec_u8(data: &[u8]) -> Result<DecodedJxl, &'static str> {
    let mut input = data;

    // Read the JXL header and basic image information.
    let mut decoder_with_image_info = decode_header(&mut input)?;

    // ! Doesn't work, has_alpha and has_black are always false, why?
    // let input_color_type = decoder.current_pixel_format().color_type;
    // if input_color_type == JxlColorType::Cmyk {
    //     return Err("CMYK is not currently supported.".into());
    // }
    // let (color_type, encoding) = match (
    //     input_color_type.is_grayscale(),
    //     input_color_type.has_alpha(),
    // ) {
    //     (true, true) => (JxlColorType::GrayscaleAlpha, "lumaa8"),
    //     (true, false) => (JxlColorType::Grayscale, "luma8"),
    //     (false, true) => (JxlColorType::Rgba, "rgba8"),
    //     (false, false) => (JxlColorType::Rgb, "rgb8"),
    // };

    let basic_info = decoder_with_image_info.basic_info();

    // ? is it robust?
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
        .map(std::borrow::Cow::into_owned);

    let stride = width
        .checked_mul(color_type.samples_per_pixel())
        .ok_or("Image width is too large")?;

    let buffer_len = stride
        .checked_mul(height)
        .ok_or("Image dimensions are too large")?;

    // Advance the decoder to the frame/image data.
    let decoder_with_frame_info = decode_frame_header(decoder_with_image_info, &mut input)?;

    let mut image_buffer =
        Image::<u8>::new((stride, height)).map_err(|_| "Buffer allocation failed")?;

    {
        let rect = Rect {
            origin: (0, 0),
            size: (stride, height),
        };

        let mut buffers = [JxlOutputBuffer::from_image_rect_mut(
            image_buffer.get_rect_mut(rect).into_raw(),
        )];

        decode_pixels(decoder_with_frame_info, &mut input, &mut buffers)?;
    }

    let mut pixels = Vec::with_capacity(buffer_len);

    for y in 0..height {
        pixels.extend_from_slice(image_buffer.row(y));
    }

    Ok(DecodedJxl {
        pixels,
        width,
        height,
        encoding,
        icc,
    })
}

#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn jxl(image_data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let results = decode_jxl_to_vec_u8(image_data)?;
    let icc_len = results.icc.as_ref().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(results.pixels.len() + icc_len + 64);

    into_writer(&results, &mut out).map_err(|_| "CBOR serialization error")?;
    Ok(out)
}
