#let _plugin = plugin("jxl_loader_opt.wasm")

/// Internal function to accept bytes and paths on Typst 0.15 or later
///
/// - imagedata (path, bytes): JXL image `path` or data as `bytes`
/// -> bytes
// CREDIT: grayness https://typst.app/universe/package/grayness/
#let _check_args(
  imagedata,
) = {
  if type(imagedata) == path {
    if sys.version < version(0, 15, 0) {
      panic("Using path as argument requires Typst 0.15 or later, pass the image as bytes instead on earlier versions.")
    }
    read(imagedata, encoding: none)
  } else if type(imagedata) == bytes {
    imagedata
  } else { panic("imagedata must be raw bytes or given as path") }
}

/// Internal constant map from samples_per_pixel value to Typst encoding strings
#let _SAMPLES_PER_PIXEL_TO_ENCODING = (none, "luma8", "lumaa8", "rgb8", "rgba8")

/// Insert a JXL image in the document
///
/// The image can be provided as raw `bytes` or as a `path`.
/// Additional arguments are passed to Typst's `image` function.
///
/// _Example:_
/// ```example
/// #import "@preview/jxl-loader:0.5.0": image-jxl
/// #image-jxl(path("path/to/img.jxl"), width: 50%)
/// #image-jxl(read("path/to/img.jxl", encoding: none))
/// ```
///
/// - imagedata (bytes, path): The JPEG XL image `path` or data as `bytes`.
#let image-jxl(imagedata, ..args) = {
  let data = _plugin.jxl(_check_args(imagedata))

  image(
    data,
    ..args,
  )
}
