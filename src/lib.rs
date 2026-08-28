use ciborium::ser::into_writer;
use jxl::api::*;
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

    pub width: u32,
    pub height: u32,

    /// Either "rgb8" or "rgba8".
    pub encoding: &'static str,

    /// ICC profile describing the color space of `pixels`.
    #[serde(with = "serde_bytes")]
    pub icc: Vec<u8>, // option?
}

/// Decode a static JXL image to tightly packed RGB[A]8 pixels.
///
/// The returned pixel buffer is row-major, top-to-bottom, with
/// `width * height * (3 for rgb or 4 for rgba)` bytes.
///
/// `icc` is the ICC profile corresponding to the color space of the
/// decoded RGB[A]8 pixels and can be passed along to consumers.
pub fn decode_jxl_to_vec_u8(data: &[u8]) -> Result<DecodedJxl, String> {
    let options = JxlDecoderOptions::default();
    let mut decoder = JxlDecoder::<states::Initialized>::new(options);
    let mut input = data;

    // Read the JXL header and basic image information.
    let mut decoder = loop {
        match decoder.process(&mut input, None) {
            Ok(ProcessingResult::Complete { result }) => {
                break result;
            }

            Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                if input.is_empty() {
                    return Err("Corrupted header".into());
                }
                decoder = fallback;
            }

            Err(_) => {
                // return Err(format!("Corrupted header: {}", e));
                return Err("Corrupted header".into());
            }
        }
    };

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

    let basic_info = decoder.basic_info().clone();

    let is_grayscale = decoder.current_pixel_format().color_type.is_grayscale();
    let mut has_alpha = false;

    for channel in &basic_info.extra_channels {
        match channel.ec_type {
            ExtraChannel::Black => return Err("CMYK is not currently supported.".into()),
            ExtraChannel::Alpha => has_alpha = true,
            _ => {}
        }
    }

    let (color_type, encoding) = match (is_grayscale, has_alpha) {
        (true, true) => (JxlColorType::GrayscaleAlpha, "lumaa8"),
        (true, false) => (JxlColorType::Grayscale, "luma8"),
        (false, true) => (JxlColorType::Rgba, "rgba8"),
        (false, false) => (JxlColorType::Rgb, "rgb8"),
    };

    let target_pixel_format = JxlPixelFormat {
        color_type,
        color_data_format: Some(JxlDataFormat::U8 { bit_depth: 8 }),
        extra_channel_format: vec![None; basic_info.extra_channels.len()],
    };

    let (width, height) = basic_info.size;

    if width == 0 || height == 0 {
        return Err("Corrupted image (width, height)".into());
    }

    // Configure the decoder's actual output format before obtaining the
    // color profile. The ICC returned below describes the pixels produced
    // by this decoder configuration.
    decoder.set_pixel_format(target_pixel_format);

    let icc = decoder
        .output_color_profile()
        .try_as_icc()
        // or i don't return anything (empty u8 vec as sentinel value)
        // .map(|icc| icc.into_owned())
        // .unwrap_or_default();
        .ok_or("Could not convert JXL color profile to ICC")?
        // .ok_or_else(|| "Could not convert JXL color profile to ICC".to_string())?
        .into_owned();

    let stride = width
        .checked_mul(color_type.samples_per_pixel())
        // .ok_or_else(|| "Image width is too large".to_string())?;
        .ok_or("Image width is too large")?;

    let buffer_len = stride
        .checked_mul(height)
        // .ok_or_else(|| "Image dimensions are too large".to_string())?;
        .ok_or("Image dimensions are too large")?;

    // Advance the decoder to the frame/image data.
    let mut decoder = loop {
        match decoder.process(&mut input, None) {
            Ok(ProcessingResult::Complete { result }) => {
                break result;
            }

            Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                if input.is_empty() {
                    return Err("Corrupted frame data".into());
                }
                decoder = fallback;
            }

            Err(_) => {
                // return Err(format!("Corrupted frame data: {}", e));
                return Err("Corrupted frame data".into());
            }
        }
    };

    let mut image_buffer = Image::<u8>::new((stride, height))
        // .map_err(|e| format!("Buffer allocation failed: {}", e))?;
        .map_err(|_| "Buffer allocation failed".to_owned())?;

    {
        let rect = Rect {
            origin: (0, 0),
            size: (stride, height),
        };

        let mut buffers = [JxlOutputBuffer::from_image_rect_mut(
            image_buffer.get_rect_mut(rect).into_raw(),
        )];

        loop {
            match decoder.process(&mut input, &mut buffers, None) {
                Ok(ProcessingResult::Complete { .. }) => {
                    break;
                }

                Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                    if input.is_empty() {
                        return Err("Corrupted pixel data".into());
                    }
                    decoder = fallback;
                }

                Err(_) => {
                    // return Err(format!("Corrupted image: {}", e));
                    return Err("Corrupted pixel data".into());
                }
            }
        }
    }

    let mut pixels = Vec::with_capacity(buffer_len);

    for y in 0..height {
        pixels.extend_from_slice(image_buffer.row(y));
    }

    Ok(DecodedJxl {
        pixels,
        width: width as u32,
        height: height as u32,
        encoding,
        icc,
    })
}

#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn jxl(image_data: &[u8]) -> Result<Vec<u8>, String> {
    let results = decode_jxl_to_vec_u8(image_data)?;
    let mut out = Vec::with_capacity(results.pixels.len() + results.icc.len() + 64);

    into_writer(&results, &mut out)
        // .map_err(|e| format!("CBOR serialization error: {}", e))?;
        .map_err(|_| "CBOR serialization error".to_owned())?;
    Ok(out)
}
