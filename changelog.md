# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0]

### Modified

- Move from cbor serialization to custom format for zero-copy image buffer serialization.
- Various optimization.
- Reduced minimum Typst version from 0.15.0 to 0.13.0.
- Refactor and cleanup.

### Fixed

Fixed final test (JXL with huge header).

## [0.3.0]

### Modified

- Typst package name modified to `jxl-loader`.
- Crate name modified to `jxl-loader`

## [0.2.1]

### Added

- ICC profile is now optional.

### Modified

- Refactor grayscale images handling.
- Optimization of image buffer handling.

## [0.2.0]

### Added

- Added the possibility to output grayscale (`luma8` and `lumaa8`) encoded data for grayscale images.

### Fixed

- Refactored and optimized detection of input image data and color type.
