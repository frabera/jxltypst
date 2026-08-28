#let _plugin = plugin("jxl_loader_opt.wasm")

// CREDIT: grayness https://typst.app/universe/package/grayness/
/// Internal function to accept bytes and paths on Typst 0.15 or later
/// -> bytes
#let _check_args(
  /// -> bytes | path
  imagedata,
) = {
  if type(imagedata) == path {
    if sys.version < version(0, 15, 0) {
      panic("Using path as argument requires Typst 0.15 or later, use bytes instead on earlier versions.")
    }
    read(imagedata, encoding: none)
  } else if type(imagedata) == bytes {
    imagedata
  } else { panic("imagedata must be raw bytes or given as path") }
}

/// Read a little-endian u32 from 4 bytes.
#let _u32-le(data, offset) = {
  data.at(offset) + data.at(offset + 1) * 256 + data.at(offset + 2) * 65536 + data.at(offset + 3) * 16777216
}

#import "@preview/jumble:0.0.1": bytes-to-hex
/// Insert a JXL image in the document
///
///  _Example:_
/// ```example
/// #import "@preview/jxl-loader:0.3.0": image-jxl
/// <<<#let arturo = read("Arturo_Nieto-Dorantes.webp", encoding: none)
/// #image-grayscale(arturo)
/// ```
/// -> content
#let image-jxl(
  imagedata,
  ///	extra arguments to pass to the Typst image function
  /// e.g. width, height, format, etc...
  ..args,
) = {
  let imagebytes = _check_args(imagedata)
  let serializedoutput = _plugin.jxl(_check_args(imagedata))

  // Rust format:
  //
  // 0..4    width       u32 LE
  // 4..8    height      u32 LE
  // 8       encoding    u8
  // 9..13   icc_len     u32 LE
  // 13..    icc + pixels

  if serializedoutput.len() < 13 {
    panic("Invalid JXL decoder output: header is too short")
  }

  let width = _u32-le(serializedoutput, 0)
  let height = _u32-le(serializedoutput, 4)

  let encodings = ("rgb8", "rgba8", "luma8", "lumaa8")
  let encoding = encodings.at(serializedoutput.at(8))

  let icc-len = _u32-le(serializedoutput, 9)
  let icc-start = 13

  let pixels-start = icc-start + icc-len

  if pixels-start > serializedoutput.len() {
    panic("Invalid JXL decoder output: ICC profile exceeds buffer")
  }

  let icc = serializedoutput.slice(icc-start, pixels-start)
  let pixels = serializedoutput.slice(pixels-start)

  let format = (
    width: width,
    height: height,
    encoding: encoding,
  )

  image(
    pixels,
    format: format,
    ..args,
    ..(
      if icc-len > 0 {
        (icc: icc)
      } else {
        ()
      }
    ),
  )
}
