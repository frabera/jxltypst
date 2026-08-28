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
  } else {
    panic(
      "imagedata argument must be given as path() or bytes: image-jxl(path(\"path/to/image.jxl\")) or image-jxl(read(\"path/to/image.jxl\", encoding: none))",
    )
  }
}

/// Insert a JXL image in the document
///
///  _Example:_
/// ```example
/// #import "@preview/jxl-loader:0.3.0": image-jxl
/// <<<#image-jxl(path("path/to/image.jxl"))
/// ```
/// -> content
#let image-jxl(
  imagedata,
  ///	extra arguments to pass to the Typst image function
  /// e.g. width, height, format, etc...
  ..args,
) = {
  let imagebytes = _check_args(imagedata)
  let decoded = cbor(_plugin.jxl(imagebytes))
  image(
    decoded.pixels,
    format: (
      width: decoded.width,
      height: decoded.height,
      encoding: decoded.encoding,
    ),
    ..args,
    ..(
      if decoded.at("icc", default: none) != none {
        (icc: decoded.icc)
      } else {
        ()
      }
    ),
  )
}
