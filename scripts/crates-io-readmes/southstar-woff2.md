# southstar-woff2

WOFF and WOFF2 web font decoding for
[Southstar](https://github.com/nordstjernen-web/southstar-browser): it turns
a downloaded `.woff` or `.woff2` file back into the plain SFNT (TrueType or
OpenType) font the text engine loads.

- **WOFF 1.0:** tables decompressed with zlib through
  [miniz_oxide](https://crates.io/crates/miniz_oxide), in pure Rust.
- **WOFF 2.0:** the Brotli stream decompressed through libbrotlidec. The
  transformed `glyf`/`loca` and `hmtx` tables are reconstructed as the
  W3C WOFF2 specification describes.

Southstar decodes WOFF2 itself rather than leaving it to FreeType, because
FreeType reads WOFF2 only when it was built with Brotli, and some
distributions build it without.

Part of the browser that `cargo install southstar` builds. The browser's
`woff2` feature, on by default, enables it. The API follows the browser's
needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
