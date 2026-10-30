Southstar Browser
=================

Southstar is a web browser written from scratch in Rust, with a GTK 4
interface. It supports modern HTML, CSS and JavaScript, measured against the
standards' own text: the JavaScript engine (a fork of
[Boa](https://github.com/boa-dev/boa), no JIT) passes all of test262, and
each tab's engine runs in its own sandboxed process. It has no upstream
browser engine and no telemetry.

This crate builds the browser:

```sh
cargo install southstar
southstar
```

`cargo install` puts four executables in `~/.cargo/bin`: `southstar`, the
`southstar-renderer` process each tab runs in, and the `southstar-audio` and
`southstar-video` media helpers. On first start the browser writes its
translations and license texts to `~/.cargo/share/southstar`.

## System libraries

The build finds these through pkg-config, so their development packages must
be installed. On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config \
    libgtk-4-dev libepoxy-dev libssl-dev zlib1g-dev libbrotli-dev libzstd-dev \
    libuchardet-dev libpsl-dev libsqlite3-dev libseccomp-dev libwebp-dev libsdl2-dev \
    libharfbuzz-dev libfontconfig-dev libfreetype-dev \
    libavformat-dev libavcodec-dev libavutil-dev libswscale-dev libswresample-dev
```

Rust 1.91 or newer is required. On Windows, build from an MSYS2 MINGW64 shell
with the packages listed in the repository's `docs/Windows.md`.

## Features

| Feature | Default | Adds |
| --- | --- | --- |
| `libav` | yes | WebM video (VP9/VP8) and Opus/Vorbis audio through FFmpeg 6.0 or newer |
| `sdl` | yes | audio playback through SDL2 |
| `wasm` | yes | the `WebAssembly` JavaScript API (wasmi) |
| `rust-shaper` | yes | web fonts shaped and drawn in pure Rust |
| `http-brotli`, `http-zstd` | yes | `br` and `zstd` content encodings |
| `woff2` | yes | WOFF2 web fonts |
| `avif` | no | AVIF images (libavif) |
| `thai` | no | Thai word breaking (libthai) |
| `poppler` | no | PDF viewing (poppler-glib) |
| `enchant` | no | spell-checking (enchant-2) |
| `vorbisfile`, `opusfile` | no | native Ogg Vorbis and Opus audio without FFmpeg |

For example, `cargo install southstar --features avif,enchant`.

## The other crates

The browser's code is published as a few crates this one depends on:
`southstar-engine` (JavaScript bindings, style, layout, painting, networking
and media), `southstar-css`, `southstar-dom`, `southstar-text`,
`southstar-http`, `southstar-url`, `southstar-html5`, `southstar-platform`,
`southstar-runtime`, `southstar-shell` and others, plus the `southstar-boa-*`
fork of the Boa JavaScript engine. They are published so this crate can build
the browser; their APIs follow the browser's needs and are not stable.

## Links

- Source, issues and other build options (meson, packages):
  <https://github.com/nordstjernen-web/southstar-browser>
- License: NSL-1.0 or GPL-3.0-or-later, at your option (`License.md` and
  `COPYING` in the repository).
