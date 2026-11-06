# southstar-build

Build-script helpers for compiling [Southstar](https://github.com/nordstjernen-web/southstar-browser)
with cargo.

Southstar links a long list of system C libraries: GTK 4, cairo, HarfBuzz,
FFmpeg, SDL2, libwebp and more. Its build scripts use this crate to:

- **find a library through pkg-config**: `pkg_config(&["libavformat", "libavcodec"])`
  returns its compiler flags and libraries, and `link_args` turns them into
  linker arguments. Nothing from crates.io is involved, just the `pkg-config`
  program the system already has.
- **probe C struct layouts**: `layout(list, &library)` compiles and runs a tiny C
  program that prints the field offsets, sizes and constants named in a list
  such as `AVFrame.pts=offsetof(AVFrame, pts)`. The Rust declarations of
  FFmpeg, libvorbisfile and libavif then match the headers actually installed,
  rather than one version frozen in the source.
- **link the engine's libraries**: `engine_link(target_os, gtk)` gathers the
  linker arguments for the libraries the enabled cargo features need, and
  `hardening_link_args` the ELF hardening flags the executables get.

It depends only on [`cc`](https://crates.io/crates/cc).

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`; the texts are in the
[repository](https://github.com/nordstjernen-web/southstar-browser).
