# southstar-browser

[Southstar Browser](https://github.com/nordstjernen-web/southstar-browser) is a
minimal, secure web browser written from scratch in Rust with GTK 4.

![Southstar Browser](https://raw.githubusercontent.com/nordstjernen-web/southstar-browser/main/docs/screenshot.png)

```sh
npm install -g southstar-browser
southstar https://example.com
```

Installing builds Southstar from the release source on your machine, with
meson, ninja and cargo, against your system's GTK 4 and other libraries. The
source tarball is checked against the SHA-256 recorded in this package. The
build takes several minutes and shows its progress in the terminal; it is
installed inside the npm package, so `npm uninstall -g southstar-browser`
removes it.

npm 11 warns that global install scripts are not yet allowed, and future
versions skip them. Either allow the build at install time:

```sh
npm install -g --allow-scripts=southstar-browser southstar-browser
```

or let the first `southstar` launch build it. Package managers that never run
install scripts (pnpm, `--ignore-scripts`) build on first launch the same way.

## Before you install

Before downloading anything, the installer checks the build tools, Rust and
every required library, and names whatever is missing or too old, with the
command that installs it on your system. Optional libraries (Brotli and zstd
compression, audio, PDF, Ogg audio, AVIF, spell-checking) are reported as a
warning naming the features the build will lack.

Rust 1.91 or newer is required. With [rustup](https://rustup.rs) installed,
the toolchain pinned in `rust-toolchain.toml` is fetched automatically. Debian
13 and Alpine 3.22 package an older Rust, so use rustup there; on Ubuntu 24.04
the `rustc-1.91` and `cargo-1.91` packages work too.

The build needs Ubuntu 24.04, Debian 13, Fedora 40, openSUSE Tumbleweed,
Arch Linux, Alpine 3.21 or a newer distribution: older releases ship a GTK,
HarfBuzz, Cairo or FFmpeg that is too old.

Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config meson ninja-build python3 \
    libgtk-4-dev libepoxy-dev libssl-dev zlib1g-dev libbrotli-dev libzstd-dev \
    libuchardet-dev libpsl-dev libsqlite3-dev libseccomp-dev libwebp-dev libsdl2-dev \
    libavformat-dev libavcodec-dev libavutil-dev libswscale-dev libswresample-dev
```

macOS ([Homebrew](https://brew.sh), plus `xcode-select --install`):

```sh
brew install meson ninja pkg-config gtk4 libepoxy openssl@3 \
    brotli zstd uchardet libpsl sqlite webp sdl2 rust
```

Windows: install [MSYS2](https://www.msys2.org) in `C:\msys64` (or set
`MSYS2_ROOT`), then in its UCRT64 terminal:

```sh
pacman -S --needed tar mingw-w64-ucrt-x86_64-{clang,pkgconf,meson,ninja,python,rust,gtk4,libepoxy,openssl,uchardet,libpsl,sqlite3,libwebp,ffmpeg}
```

The browser runs against the MSYS2 libraries it was built with, so keep MSYS2
installed. It is built on your machine and unsigned: if Windows Smart App
Control is on, it blocks the browser.

If the build fails, the installer prints the end of the build output and the
path of the full log, `build-<version>.log` in `~/.cache/southstar-browser`
(`%LOCALAPPDATA%\southstar-browser` on Windows).

`SOUTHSTAR_SKIP_INSTALL=1` skips the build at install time; the first
`southstar` launch builds it instead.

## License

Dual-licensed under the Nordstjernen Source License v1.0 or the GNU General
Public License version 3 or later, at your option. Copyright 2026 Andreas
Røsdal.
