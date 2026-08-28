#let _plugin = plugin("jxl_loader_opt.wasm")

#let encodings = ("rgb8", "rgba8", "luma8", "lumaa8")


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
  ..args,
) = {
  let input = _check_args(imagedata)
  let data = _plugin.jxl(input)

  // Rust format:
  //
  // 0..4    width       u32 LE
  // 4..8    height      u32 LE
  // 8       encoding    u8
  // 9..13   icc_len     u32 LE
  // 13..    icc + pixels

  // if data.len() < 13 {
  //   panic("Invalid JXL decoder output: header is too short")
  // }

  let width = _u32-le(data, 0)
  let height = _u32-le(data, 4)

  let encoding = encodings.at(data.at(8))

  let icc-len = _u32-le(data, 9)

  // if 13 + icc-len > data.len() {
  //   panic("Invalid JXL decoder output: ICC profile exceeds buffer")
  // }

  let icc = data.slice(13, count: icc-len)
  let pixels = data.slice(13 + icc-len)

  image(
    pixels,
    format: (
      width: width,
      height: height,
      encoding: encoding,
    ),
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
