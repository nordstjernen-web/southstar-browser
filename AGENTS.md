# Southstar — Codex operating guide

Southstar ("Southstar Browser") is a web
browser written from scratch in **C**, now being ported to **Rust**
module by module (`docs/rust-port.md`), using **GTK 4** for the UI and
an in-tree Rust HTTP client for networking (libcurl is being phased out —
see "HTTP client" below). Targets Linux,
macOS, and Windows.

See `README.md` for the product vision. Southstar is a fresh
implementation — there is no upstream browser engine, no fork,
nothing imported.

## Design constraints

- Minimalistic, compact, secure. Source should be readable and
  maintainable by a single human.
- HTML5 + modern CSS + modern JavaScript, supported pragmatically as
  far as is feasible without bloat.
- **No** AI-style web APIs. WebGL **is** supported: a working,
  minimalist WebGL 1 / 2 over OpenGL ES (`src/webgl.c`).
  It is enabled by default, can be disabled globally in Settings, and reports active use in the browser status bar.
- **WebGPU** (`navigator.gpu`) is an **experimental** feature that
  layers `src/webgpu.c` over the external wgpu-native library. The
  `webgpu` meson feature is `auto`: built only when wgpu-native is
  present, silently skipped otherwise, and even when built it stays
  off at runtime until the browser is started with `--enable-webgpu`
  (which sets `NS_WEBGPU_ALLOW=1`).
- The one vendored, in-tree video codec is MPEG-1, decoded by the
  vendored pl_mpeg decoder (`subprojects/plmpeg/`, wrapped by
  `src/video_decode.c`); such `<video>` plays inline (`src/video.c`).
  Audio plays via the unsandboxed `southstar-audio` helper
  (`src/audio/main.c`), which decodes MPEG-1/MP2 (pl_mpeg) and MP3
  (vendored minimp3, `src/audio/minimp3.h`) and outputs through SDL2.
  MSE video frames decode in the `southstar-video` helper
  (`src/videoproc/main.c`, built when libav is present); the shell
  composites its BGRA frames from a shm ring (see `docs/media.md`).
  When FFmpeg's libav\* system packages are present, the build also
  gains inline WebM (VP9/VP8 video + Opus/Vorbis audio) — never
  vendored. Other `<audio>`/`<video>` codecs render a poster and
  play overlay; the renderer resolves the media URL under the
  pointer via `ns_browser_media_at`.
- UI strings are English-source and translated to the operating-system
  language at startup through the in-tree catalogue lookup (`src/i18n.c`,
  `data/i18n/*.lang`); English is the fallback for any string a catalogue
  does not cover. No gettext dependency.
- Does not phone home, does not telemeter the user.

## Comments policy

**The code is self-explaining. Don't write code comments.**

- Each source file gets exactly one short header comment at the top
  naming the file and (at most) one sentence on what it does. That's
  it.
- No inline `/* … */` or `//` comments inside functions, in struct
  declarations, around tricky branches, or anywhere else. Rename a
  variable or extract a function instead.
- No "section banner" comments (`/* ---------- helpers ---------- */`).
  Group code by file or function instead.
- No `TODO`/`FIXME`/`XXX` markers — file a real task instead.
- Rust files follow the same rule: one `//!` header block (the file and one
  sentence, then the copyright and SPDX lines), nothing else — including no
  `// SAFETY:` comments; keep `unsafe` in small `ffi` modules instead.
- Bundled external projects are exempt from this comments policy.
  In particular, do not reformat or strip comments from `src/lexbor/`,
  `src/quickjs/`, or `subprojects/` just to satisfy Southstar style.

## Autonomous mode — read this every session

This repo is driven by Codex in long uninterrupted sessions.
**Default to acting, not asking.**

- **Permissions: run with sandboxing/approvals relaxed for this repo.**
  `.claude/settings.json` sets `defaultMode: bypassPermissions` plus a broad
  allow-list for the build/run/git/inspect workflow (used by Claude Code; the
  equivalent for other harnesses is full-access / never-ask). Routine
  build/run/git/inspect commands must never prompt. Don't burn turns getting
  individual commands approved.
- **Don't ask "do you want me to proceed?", "should I continue?",
  "ready to commit?"** — just do it. The user interrupts if they
  disagree.
- **Don't summarize after every step.** One-line status is enough.
- **Don't pause for path/file/branch confirmation when context is
  unambiguous.** Grep, pick, proceed.
- **Commit and push aggressively.** Small commits, push to
  `origin/main` as soon as a logical unit lands.
- **Run for hours.** Diagnose, fix, retry. Only stop on genuine
  external blockers. When stopping: one line on what's blocked.
- **Never ask the user to run the build.** Run it yourself.
- **Local machine is the build *and* run oracle.** The repo can be
  driven from either a Linux box (GTK 4 / libcurl / meson / clang +
  an X session at `DISPLAY=:0`) or a Windows 11 box via MSYS2
  MINGW64 (same toolchain, same meson/ninja invocation; the binary
  is `./builddir/src/gtk/southstar.exe`). Every commit must pass
  `meson compile -C builddir` locally before pushing. Smoke-launch
  the browser (in the background, then kill it) on material changes
  — that's the per-change correctness gate, not CI. See
  `docs/Windows.md` for the MSYS2 setup; the rest of this guide
  uses Unix-style invocations that work in either shell.
- **CI is enabled.** The Linux / macOS / Windows / musl
  workflows run on every push to `main` and every PR targeting
  `main`, plus manual `workflow_dispatch`. Local Linux is still the primary
  correctness gate before pushing; CI provides cross-platform
  sanity coverage.

## Build / verify locally

The intended build system is **meson + ninja**. From a clean checkout:

```sh
meson setup builddir
meson compile -C builddir
./builddir/src/gtk/southstar
```

### Rust: the port in progress

Southstar is being ported from C to Rust in place, module by module
(`docs/rust-port.md`). The Cargo workspace (`Cargo.toml`, `rust/`) is built
by meson: `rust/meson.build` runs `scripts/cargo-build.py`, which builds
`rust/southstar-ffi` — the one static library every C target links — and hands
ninja cargo's dependency file, so Rust rebuilds only when Rust changes. Rust
1.85 or newer is required; `rust-toolchain.toml` pins 1.85.0 for rustup users,
so local builds and CI compile with the minimum supported version.

- A ported module is a crate under `rust/` that exports the same `ns_*`
  functions its C header declares (`#[unsafe(no_mangle)] extern "C"`). Pointer
  handling lives in the crate's `ffi.rs`; the logic is safe Rust. The C header
  stays as the contract, the `.c` file is deleted in the same commit, and the
  crate is added to `rust/southstar-ffi` (dependency plus `pub use`).
- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`
  must be clean before pushing, like the C warnings.
- A new crates.io dependency needs a reason in its commit message; keep the
  dependency tree small. Crates from Servo or Firefox (`html5ever`,
  `cssparser`, `selectors`, `url`, `encoding_rs`, …) count as upstream browser
  engine code and stay out.
- No `#[test]`s. Check a port against the C it replaces with a throwaway
  differential harness, then in the browser itself.

The QuickJS engine is integrated into the main tree at `src/quickjs/`
(forked from [quickjs-ng](https://github.com/quickjs-ng/quickjs); we
modify it freely — JIT, profiling, and other browser-side hooks live
there). It is not a meson subproject — `src/quickjs/meson.build` is
loaded via `subdir()` from the top-level and exposes `libquickjs` as
a declared dependency directly in the parent scope.

### Original QuickJS: `-Dquickjs=quickjs`

The `quickjs` meson option picks the engine under the QuickJS binding:
`quickjs-ng` (default) is the in-tree fork above; `quickjs` is Fabrice
Bellard's original [QuickJS](https://github.com/bellard/quickjs), fetched at
configure time by `subprojects/quickjs.wrap` (pinned to a release commit,
built by the overlay in `subprojects/packagefiles/quickjs/`, never vendored).

- **Engine code includes `"ns_quickjs.h"`, never `<quickjs.h>`.** On the
  original engine (`NS_QUICKJS_ORIGINAL`) that header and `src/ns_quickjs.c`
  supply the quickjs-ng API the binding is written against.
- **A new quickjs-ng-only call goes into the adapter**, not behind an
  `#ifdef` at the call site: build `-Dquickjs=quickjs` after binding changes,
  and add whatever fails to compile to `src/ns_quickjs.c`. Only the fork's own
  hooks with no equivalent (the receiver-aware `get_own_property_receiver`)
  are guarded in place.
- CI does not build it: build both configurations locally after binding
  changes.

### HTML engine: Lexbor

The single HTML→DOM backend is
[lexbor](https://github.com/lexbor/lexbor). It is integrated into
the main tree at `src/lexbor/` (forked from upstream; we modify it
freely for tight browser integration). It is not a meson subproject
— `src/lexbor/meson.build` is loaded via `subdir()` from the
top-level and exposes `liblexbor` as a declared dependency directly
in the parent scope. No system lexbor or CMake fallback is consulted
— the in-tree copy is always built.

### Image decoding: Wuffs

PNG/APNG, GIF, BMP, and JPEG bytes are decoded through
[Wuffs](https://github.com/google/wuffs), a memory-safe
transpiled-to-C image-decoder library. The single-file release is
vendored at `subprojects/wuffs/wuffs-v0.4.c` and built as a static
subproject. `ns_image_decode_bytes` (`rust/image`) runs the decoders
in order: ICO (`rust/image-decoders`) → Wuffs (PNG/APNG, GIF, BMP, JPEG)
→ WebP via libwebp (`rust/image-decoders`) → AVIF via libavif when built with it
(`src/image_avif.c`, optional) → SVG in-engine (`src/svg.c`). Nothing
follows, so an unsupported format fails to decode.

### URL parsing: lexbor URL module

The `ns_url_*` helpers in `src/net.c` route URL resolution, origin
extraction, and host extraction through `lxb_url_parse` /
`lxb_url_serialize` from lexbor's WHATWG URL module. No separate URL
library or build option — it's part of the same `liblexbor_static.a`
that the HTML parser uses.

### HTTP client: the Rust client in `rust/http`, replacing libcurl

Page and subresource fetches go through the transport seam
`ns_hop_transport()` (`src/net_backend.h`). The `http_backend` meson option
picks the implementation: `rust` (default) is the in-tree Rust HTTP client,
`rust/http` (`southstar-http`); `curl` drives a libcurl easy handle on the
shared multi-handle thread. Everything above one hop — redirects, HSTS,
referer, cache, cookie partitioning — lives in `rust/net` and is shared by
both, so they fetch through identical browser policy.

`rust/http` is written from scratch, with no crates.io dependency: HTTP/1.1,
and HTTP/2 with its own framing, flow control and HPACK (the static table and
Huffman code are generated from RFC 7541). TLS is OpenSSL's libssl behind
`rust/http/src/ffi/tls.rs` (ALPN `h2`/`http/1.1`, per-host session
resumption, the insecure-certificate override); gzip/deflate go through zlib
and br/zstd through libbrotlidec/libzstd when present
(`rust/http/src/ffi/codec.rs`). HTTP/2 connections are pooled per
`scheme://host:port` and **multiplex concurrent requests over one
connection**: an I/O thread per connection drives the sans-I/O
`h2::Connection`, workers queue a request and wait for its stream events,
and connecting to an origin is serialized; `ns_net_backend_shutdown()` tears
it all down. Proxies (HTTP forward and CONNECT, SOCKS4/4a/5/5h, credentials,
no-proxy matching) are in `rust/http/src/proxy.rs`.

**libcurl is being removed.** It is still linked for what has not moved
yet: the `curl` value of the `http_backend` option and the curl plumbing in
`rust/net` behind it. The media helpers download through `rust/helper-ffi`.
FTP is `rust/http/src/ftp.rs`.
WebSocket (`rust/websocket`) upgrades through `southstar_http::upgrade`. Other crates reach the network through `southstar_http::fetch`
(redirects followed) with `southstar_net::route` supplying TLS settings and
the proxy. Each of these moves onto
`rust/http` next; when the last one does, the `curl` backend and the
libcurl dependency are deleted. Extend `rust/http` — don't add new libcurl
uses. HTTP/3 (QUIC) is not supported by the Rust client.

### Charset detection: uchardet

Required dependency (Debian/Ubuntu `libuchardet-dev`,
Fedora/RHEL `uchardet-devel`). `ns_html_decode_body` hands the
response body to [uchardet](https://www.freedesktop.org/wiki/Software/uchardet/)
to identify the charset, then `g_convert`s to UTF-8. No
hand-rolled BOM / HTTP-charset / meta-charset sniffing — uchardet
handles all of that internally. The Latin-1 fallback only fires
if uchardet can't classify the bytes at all.

### WebP: libwebp

Required dependency (Debian/Ubuntu `libwebp-dev`, Fedora/RHEL
`libwebp-devel`, openSUSE `libwebp-devel`, Alpine `libwebp-dev`,
MSYS2 `mingw-w64-x86_64-libwebp`, Homebrew `webp`). WebP —
lossy VP8 (the dominant variant served by the BBC, Wikipedia
thumbnails, and most modern CDNs), lossless VP8L, and **animated
WebP** (via libwebpdemux's `WebPAnimDecoder`, playing like animated
GIFs) — is decoded in-tree by `src/image_webp.c`, tried right after
the Wuffs decoders. No gdk-pixbuf loader, no
`loaders.cache` registration, and no sandbox interaction is
involved; the old `webp-pixbuf-loader` runtime dependency is gone.

### Web Cryptography: OpenSSL libcrypto

Required dependency (Debian/Ubuntu `libssl-dev`, Fedora/RHEL
`openssl-devel`, openSUSE `libopenssl-devel`). `crypto.subtle` (the
WebCrypto SubtleCrypto surface) is implemented in `src/webcrypto.c`
directly over OpenSSL's EVP/`OSSL_PARAM` APIs — hashing, HMAC, AES
(GCM/CBC/CTR/KW), RSA (PKCS1/PSS/OAEP), ECDSA/ECDH (P-256/384/521),
Ed25519/X25519, PBKDF2 and HKDF. The QuickJS `CryptoKey` class and `subtle.*`
argument marshalling live in `src/js.c`. OpenSSL is already linked
transitively through libcurl's TLS backend on Linux and
Windows/MSYS2; `meson` depends on `libcrypto` explicitly so the
headers resolve.

### WebAssembly: WAMR

The `WebAssembly` JS API (`compile`, `instantiate`, `Memory`,
`Table`, `Global`, externref) is implemented in `src/wasm.c` over a
vendored subset of the [WebAssembly Micro Runtime
(WAMR)](https://github.com/bytecodealliance/wasm-micro-runtime)
interpreter at `src/wamr/`. It runs wasm-bindgen bundles.

System packages required on Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config meson ninja-build cargo rustc \
    libgtk-4-dev libepoxy-dev libcurl4-openssl-dev libssl-dev libuchardet-dev \
    libpsl-dev libsqlite3-dev libseccomp-dev libwebp-dev libsdl2-dev \
    libavformat-dev libavcodec-dev libavutil-dev libswscale-dev libswresample-dev
```

On Fedora/RHEL:

```sh
sudo dnf install gcc pkgconf meson ninja-build cargo rust gtk4-devel libepoxy-devel libcurl-devel \
    openssl-devel uchardet-devel libpsl-devel sqlite-devel \
    libseccomp-devel libwebp-devel SDL2-devel ffmpeg-devel
```

(`ffmpeg-devel` comes from RPM Fusion.)

On openSUSE:

```sh
sudo zypper install gcc pkgconf meson ninja cargo rust gtk4-devel libepoxy-devel libcurl-devel \
    libopenssl-devel libuchardet-devel libpsl-devel sqlite3-devel \
    libseccomp-devel libwebp-devel libSDL2-devel ffmpeg-devel
```

(`ffmpeg-devel` comes from Packman.)

Rust must be 1.85 or newer. Debian 13 and current Fedora and openSUSE ship
it; on Ubuntu 24.04, whose default is 1.75, install `rustc-1.85 cargo-1.85`
and put `/usr/lib/rust-1.85/bin` first on `PATH`, or use rustup.

`libseccomp` is required on Linux — `meson setup` fails without it.
On macOS and Windows it is not used and the syscall filter is a no-op.

The FFmpeg libav\* packages are required on Linux and Windows too (MSYS2
`mingw-w64-x86_64-ffmpeg`, or the LGPL build from
`scripts/build-ffmpeg-lgpl.sh`): they carry inline WebM playback, and
`meson setup` fails without them or with a pre-6.0 FFmpeg. On macOS they
are auto-detected, and a build without them carries no libav dependency and
falls back to the external player for WebM.

Optional: `libenchant-2-dev` (plus a dictionary such as `hunspell-en-us`)
enables on-screen spell-checking; `libavif-dev` adds AVIF decoding and can be
dropped with `-Davif=disabled`. Both are auto-detected.

### Fast iteration (recommended for AI/Codex loops)

`ccache` is the single biggest build-time win and meson picks it up
automatically. With `ccache` installed, a clean `meson setup builddir
&& meson compile -C builddir` drops from ~35s to ~1s once the cache
is warm — in-tree libraries (lexbor, quickjs) hit the cache
and re-link in negligible time. Install once:

```sh
sudo apt install ccache       # Debian/Ubuntu
sudo dnf install ccache       # Fedora/RHEL
```

Optionally use the `lld` linker for faster final links
(`CC_LD=lld meson setup builddir`). Not required.

`./scripts/dev.sh build` runs `meson setup` (only if needed) and
`meson compile -C builddir` in one shot — use it instead of typing
the two commands separately.

## Definition of done

A change is done when:

1. It compiles cleanly (no new warnings) with the configured GCC and
   Clang flags, and `cargo fmt --check` and `cargo clippy` are clean.
2. The browser launches and the affected UI path works manually.
3. The change is committed and pushed to `origin/main`.

Note: this project has **no automated test suite** and no plans to
add one. Verify behavior by running the browser. Don't add unit /
integration / property / fuzz tests, don't add a `tests/` directory,
don't add `meson test` targets.

## Don't

- Don't introduce Mozilla/Gecko code, WebKit code, or any other
  upstream browser engine source. Southstar is an independent
  implementation, not a fork.
- Don't add AI-style web-API surface area, even as stubs. (WebGL
  already exists as a deliberate exception — extend
  `src/webgl.c`, don't re-architect it. WebGPU is an experimental
  exception layered over external wgpu-native — `src/webgpu.c`, kept
  behind the `--enable-webgpu` / `NS_WEBGPU_ALLOW` runtime gate; don't
  make wgpu-native a hard/default dependency or vendor its library.)
- Don't add telemetry, crash reporters, update pingers, or "studies"
  infrastructure. UI translation goes through `src/i18n.c` and the
  `data/i18n/*.lang` catalogues — don't introduce gettext or `.po`
  tooling.
- Don't write planning docs unless asked.
