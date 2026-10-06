use jxl::api::{
    Event, ExtraChannel, JxlColorEncoding, JxlColorProfile, JxlColorType, JxlDataFormat,
    JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, JxlPixelFormat,
};

#[cfg(target_arch = "wasm32")]
use wasm_minimal_protocol::{initiate_protocol, wasm_func};

#[cfg(target_arch = "wasm32")]
initiate_protocol!();

// const SRGB: JxlColorProfile = JxlColorProfile::Simple(JxlColorEncoding::RgbColorSpace {
//     white_point: JxlWhitePoint::D65,
//     primaries: JxlPrimaries::SRGB,
//     transfer_function: JxlTransferFunction::SRGB,
//     rendering_intent: RenderingIntent::Relative,
// });

/// Allocates the output vector to be returned to Typst. It populates the header and
/// return the output buffer containing the header metadata and icc with the offset to the
/// empty data in which the decoder will write the pixel data.
#[inline(always)]
fn allocate_output(
    width: usize,
    height: usize,
    samples_per_pixel: usize,
    icc: Option<&[u8]>,
    pixel_len: usize,
) -> Result<(Vec<u8>, usize), &'static str> {
    let icc_len = icc.map_or(0, |icc| icc.len());

    const HEADER_LEN: usize = 4 + 4 + 1 + 4;

    let total_len = HEADER_LEN
        .checked_add(icc_len)
        .and_then(|x| x.checked_add(pixel_len))
        .ok_or("Output is too large")?;

    // FORMAT:
    // width: u32 -> 4
    // height: u32 -> 4
    // samples_per_pixel: u8 -> 1
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

    let mut offset = 0;

    out[offset..offset + 4].copy_from_slice(&(width as u32).to_le_bytes());
    offset += 4;

    out[offset..offset + 4].copy_from_slice(&(height as u32).to_le_bytes());
    offset += 4;

    out[offset] = samples_per_pixel as u8;
    offset += 1;

    out[offset..offset + 4].copy_from_slice(&(icc_len as u32).to_le_bytes());
    offset += 4;

    if let Some(icc) = icc {
        out[offset..offset + icc_len].copy_from_slice(icc);
        offset += icc_len;
    }

    Ok((out, offset))
}

/// Decode a static JXL image to tightly packed RGB[A]8 or LUMA[A]8 pixels.
///
/// The returned pixel buffer is tightly packed, row-major, top-to-bottom.
/// Each pixel contains 1, 2, 3, or 4 bytes depending on `encoding`.
#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn jxl(mut data: &[u8]) -> Result<Vec<u8>, &str> {
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
            ExtraChannel::Black => return Err("CMYK is not currently supported."),
            _ => {}
        }
    }

    if has_alpha || current_color_type.has_alpha() {
        color_type = color_type.add_alpha().unwrap();
    }

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

    let samples_per_pixel = color_type.samples_per_pixel();

    let stride = width
        .checked_mul(samples_per_pixel)
        .ok_or("Image width is too large")?;

    let pixel_len = stride
        .checked_mul(height)
        .ok_or("Image dimensions are too large")?;

    // The ICC profile corresponding to the color space of the decoded image, _if available_.

    // let outputcol = decoder.output_color_profile().unwrap();
    // let mut icc = Some(&[][..]);

    // if !outputcol.same_color_encoding(&SRGB) {
    //     let icc_temp = decoder.output_color_profile().unwrap().try_as_icc();
    //     icc = icc_temp.as_ref().map(|icc| icc.as_slice());
    // }

    let output_color_profile = decoder.output_color_profile().unwrap();

    let srgb = JxlColorProfile::Simple(JxlColorEncoding::srgb(current_color_type.is_grayscale()));

    let icc_temp = if !output_color_profile.same_color_encoding(&srgb) {
        output_color_profile.try_as_icc()
    } else {
        None
    };

    let icc = icc_temp.as_ref().map(|icc| icc.as_slice());

    let (mut out, offset) = allocate_output(width, height, samples_per_pixel, icc, pixel_len)?;
    // The remainder of `out` is the pixel buffer.
    let pixels = &mut out[offset..];

    let mut buffers = [JxlOutputBuffer::new(pixels, height, stride)];

    loop {
        match decoder
            .process(&mut data, Some(&mut buffers), None)
            .map_err(|_| "Corrupted pixel data")?
        {
            Event::BasicInfo => unreachable!(),
            Event::FrameHeader => {} // Only first frame is decoded for animated JXL, duration is not needed
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
