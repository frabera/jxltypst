use jxl::api::{
    AlphaMode, Event, ExtraChannel, JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions,
    JxlOutputBuffer, JxlPixelFormat,
};

use png::{Compression::NoCompression, Filter::NoFilter};
#[cfg(target_arch = "wasm32")]
use wasm_minimal_protocol::{initiate_protocol, wasm_func};

#[cfg(target_arch = "wasm32")]
initiate_protocol!();

/// Decode a static JXL image to tightly packed RGB[A]8 or LUMA[A]8 pixels.
///
/// The returned pixel buffer is tightly packed, row-major, top-to-bottom.
/// Each pixel contains 1, 2, 3, or 4 bytes depending on `encoding`.
#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn jxl(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut options = JxlDecoderOptions::default();
    options.alpha_mode = AlphaMode::Unpremultiplied;
    options.max_profile_level = jxl::api::ProfileLevel::Main5;
    let mut decoder = JxlDecoder::new(options);

    while decoder
        .process(&mut data, None, None)
        .map_err(|_| "Corrupted header")?
        != Event::BasicInfo
    {
        if data.is_empty() {
            return Err("Source file truncated".into());
        }
    }

    let basic_info = decoder.basic_info().unwrap();

    let current_color_type = decoder.current_pixel_format().unwrap().color_type;

    let mut color_type = if current_color_type.is_grayscale() {
        JxlColorType::Grayscale
    } else {
        JxlColorType::Rgb
    };

    let mut has_alpha = false;
    for channel in &basic_info.extra_channels {
        match channel.ec_type {
            ExtraChannel::Alpha => has_alpha = true,
            ExtraChannel::Black => return Err("CMYK is not currently supported.".into()),
            _ => {}
        }
    }

    if has_alpha || current_color_type.has_alpha() {
        color_type = color_type.add_alpha().unwrap();
    }

    let current_bit_depth = decoder.current_pixel_format().unwrap().color_data_format;

    let output_bit_depth = match current_bit_depth {
        Some(JxlDataFormat::U8 { bit_depth }) => JxlDataFormat::U8 { bit_depth },
        Some(JxlDataFormat::U16 {
            endianness: _,
            bit_depth,
        }) => JxlDataFormat::U16 {
            endianness: jxl::api::Endianness::BigEndian,
            bit_depth,
        },
        _ => JxlDataFormat::U16 {
            endianness: jxl::api::Endianness::BigEndian,
            bit_depth: 16,
        },
    };

    let target_pixel_format = JxlPixelFormat {
        color_type,
        color_data_format: Some(output_bit_depth),
        extra_channel_format: vec![None; basic_info.extra_channels.len()],
    };

    let (width, height) = basic_info.size;
    if width == 0 || height == 0 {
        return Err("Corrupted image (width, height)".into());
    }

    // Configure the decoder's actual output format before obtaining the
    // color profile. The ICC returned below describes the pixels produced
    // by this decoder configuration. (can't panic if set before frame decoding.)
    decoder.set_pixel_format(target_pixel_format).unwrap();

    let samples_per_pixel = color_type.samples_per_pixel() * output_bit_depth.bytes_per_sample();

    let stride = width
        .checked_mul(samples_per_pixel)
        .ok_or("Image width is too large")?;

    let pixel_len = stride
        .checked_mul(height)
        .ok_or("Image dimensions are too large")?;

    let mut pixels: Vec<u8> = Vec::with_capacity(pixel_len);

    // SAFETY:
    // We immediately initialize every byte of `out` with `serialize_header`
    // and `JxlOutputBuffer` for the pixel region before `out` is returned.
    #[allow(clippy::uninit_vec)]
    unsafe {
        pixels.set_len(pixel_len);
    }

    let mut buffers = [JxlOutputBuffer::new(pixels.as_mut_slice(), height, stride)];

    loop {
        match decoder
            .process(&mut data, Some(&mut buffers), None)
            .map_err(|e| format!("Error pixel data {e}"))?
        {
            Event::BasicInfo => unreachable!(),
            Event::FrameHeader => {} // Only first frame is decoded for animated JXL, duration is not needed
            Event::FrameComplete { .. } => break, // Only first frame is decoded for animated JXL
            Event::Complete => break,
            Event::NeedMoreInput { .. } => {
                if data.is_empty() {
                    return Err("Source file truncated".into());
                }
            }
        }
    }

    // Encode the decoded pixels as PNG into an in-memory buffer.
    let mut png_data = Vec::new();

    {
        let mut info = png::Info::with_size(width as u32, height as u32);

        info.color_type = match color_type {
            JxlColorType::Grayscale => png::ColorType::Grayscale,
            JxlColorType::GrayscaleAlpha => png::ColorType::GrayscaleAlpha,
            JxlColorType::Rgb => png::ColorType::Rgb,
            JxlColorType::Rgba => png::ColorType::Rgba,
            _ => unreachable!(),
        };

        info.bit_depth = match output_bit_depth {
            JxlDataFormat::U8 { .. } => png::BitDepth::Eight,
            _ => png::BitDepth::Sixteen,
        };

        let icc_profile = {
            let out_color_profile = decoder.output_color_profile().unwrap();
            out_color_profile.try_as_icc().map(|icc| icc.to_vec())
        };

        if let Some(icc) = icc_profile {
            info.icc_profile = Some(std::borrow::Cow::Owned(icc));
        }

        let mut encoder = png::Encoder::with_info(std::io::Cursor::new(&mut png_data), info)
            .map_err(|_| "Failed to create PNG encoder")?;

        encoder.set_compression(NoCompression);
        encoder.set_deflate_compression(png::DeflateCompression::NoCompression);
        encoder.set_filter(NoFilter);

        let mut writer = encoder
            .write_header()
            .map_err(|_| "Failed to write PNG header")?;

        writer
            .write_image_data(&pixels)
            .map_err(|_| "Failed to encode PNG pixels")?;
    }

    Ok(png_data)
}
