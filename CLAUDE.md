# Southstar — Claude operating guide

Southstar ("Southstar Browser") is a web browser written from scratch in
**Rust**, using **GTK 4** for the UI and an in-tree Rust HTTP client for
networking (see "HTTP client" below). Targets Linux, macOS, and Windows. It
began in C and was ported to Rust module by module (`docs/rust-port.md`);
the port is complete and the tree holds no C code of its own — the one C
file left is `src/libsouthstar.h`, the embedding API's header.

See `README.md` for the product vision. Southstar is a fresh
implementation — there is no upstream browser engine, no fork,
nothing imported.

Update Changelog.md

## Design constraints

- Minimalistic, compact, secure. Source should be readable and
  maintainable by a single human.
- HTML5 + modern CSS + modern JavaScript, supported pragmatically as
  far as is feasible without bloat.
- **No** AI-style web APIs. WebGL **is** supported: a working, minimalist
  WebGL 1 / 2 over OpenGL ES (`rust/webgl`). It is
  enabled by default, can be disabled globally in Settings, and reports
  active use in the browser status bar. The `WebGLRenderingContext` /
  `WebGL2RenderingContext` interface objects also carry the GL enum
  constants so feature code resolves them without a live context.
- **WebGPU** (`navigator.gpu`) is an **experimental** feature that layers
  `rust/webgpu` over the external
  [wgpu-native](https://github.com/gfx-rs/wgpu-native) library (the
  `webgpu.h`/`wgpu.h` headers are vendored in-tree under
  `third_party/wgpu-native/`). The `webgpu` meson feature is `auto`: it is
  built whenever wgpu-native is present (its pkg-config file, or
  `-Dwgpu_native_root` pointing at an extracted release) and silently skipped
  otherwise — so a stock build on a machine without wgpu-native still carries
  **no** WebGPU symbol or dependency, exactly as before. `-Dwebgpu=enabled`
  hard-requires it; `-Dwebgpu=disabled` never builds it. Even in a build that
  contains it, WebGPU is **off at runtime** until the browser is started with
  `--enable-webgpu` (which sets `NS_WEBGPU_ALLOW=1`, inherited by the
  sandboxed renderer); without that flag `navigator.gpu.requestAdapter()`
  resolves to `null`. wgpu-native is a large dependency and deliberately
  stays an opt-in build input, never vendored.
- The **one vendored, in-tree** video codec is MPEG-1, decoded by the
  in-tree `rust/plmpeg` (`southstar-plmpeg`), a pure-Rust port of the
  MIT-licensed [pl_mpeg](https://github.com/phoboslab/pl_mpeg) demuxer and decoders, used by
  `rust/video/src/decode.rs`). A `<video>` whose source is an `.mpg`/`.mpeg`/`.m1v`
  stream plays **inline** — frames are decoded in the sandboxed renderer
  and advanced off the animation tick (`rust/video`), honouring
  `autoplay`/`loop`/`muted`/`poster` and click-to-play/pause. Audio plays
  via the unsandboxed `southstar-audio` helper (`rust/audio-helper`),
  which decodes in-tree — `rust/plmpeg` for the MPEG-1/MP2 track, `rust/minimp3`
  (a pure-Rust port of the CC0 [minimp3](https://github.com/lieff/minimp3))
  for standalone `.mp3` files — and outputs through SDL2's audio device
  (WASAPI/CoreAudio/ALSA), mixing and resampling the streams itself. The
  renderer emits `open`/`play`/`pause`/`seek`/`stop`/`loop`/`volume`
  commands that ride the render-response `X-Audio` side-channel to the
  shell, which spawns and pumps the helper (`rust/shell/src/procview/media.rs`).
  MSE video frames decode in a third process, `southstar-video`
  (`rust/video-helper`, built when libav is present): the renderer
  materializes the growing stream to `~/.cache/southstar/msvideo/`
  and drives it with `video …` lines on the same side-channel; the
  helper writes BGRA frames into a shm ring that the shell composites
  over the page surface each tick (see `docs/media.md`). Without the
  helper (headless, Windows) the renderer decodes in-process as before. The
  in-tree decoders stay `rust/plmpeg` (MPEG-1 video + MP2) + `rust/minimp3` (MP3) — don't
  vendor further single-file codecs. **WebM is the one FFmpeg-backed
  extension**, over `libav\*` (`libavformat`/`libavcodec`/`libavutil`/
  `libswscale`/`libswresample`, system packages — never vendored): the
  `libav` build path (the crates' `libav` cargo feature) adds inline
  **VP9/VP8 video** (libav demux+decode → swscale → texture, in
  `rust/video/src/decode.rs`) and **Opus/Vorbis audio** (decoded in the helper,
  `rust/audio-helper`) for `.webm`/`.opus`/`.ogg` sources. It is **required on
  Linux and Windows** (`libav_required` in `meson.build` — YouTube and most
  modern sites serve VP9/WebM, so the external-player fallback there is
  unacceptable; the Windows CI/packaging builds a minimal LGPL FFmpeg via
  `scripts/build-ffmpeg-lgpl.sh` and the `--werror` build fails without it)
  and **auto-detected on macOS** (a stock build there without libav carries no
  libav symbol or dependency and behaves exactly as before). The version floor
  is FFmpeg 6.0's library sonames (libavcodec ≥ 60, libavutil ≥ 58, …) — the
  oldest release carrying the `AVChannelLayout` API this code uses. Other
  `<audio>` and other `<video>`
  codecs render a
  poster and play overlay; clicking resolves the media URL in the renderer
  (`ns_browser_media_at`) and reports it over the renderer protocol for
  embedders — the GTK shell does not launch an external player.
- UI strings are English-source and translated to the operating-system
  language at startup through the in-tree catalogue lookup (`rust/i18n`,
  `data/i18n/*.lang`); English is the fallback for any string a catalogue
  does not cover. No gettext dependency.
- Does not phone home, does not telemeter the user.
- Make good descriptive commit messages always

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
  sentence, then the copyright and SPDX lines), nothing else — with one
  exception: every `unsafe` block carries a `// SAFETY:` comment saying why
  its single unsafe operation is sound, and every `unsafe fn` a `# Safety`
  doc section stating the caller's obligations (clippy's
  `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block` and
  `missing_safety_doc`). Prefer removing the `unsafe` to documenting it; keep
  what remains in small `ffi` modules.
- Bundled external projects are exempt from this comments policy.
  In particular, do not reformat or strip comments from `third_party/`
  or `subprojects/` just to satisfy Southstar style.

## Autonomous mode — read this every session

This repo is driven by Claude in long uninterrupted sessions.
**Default to acting, not asking.**

- **Permissions: run in `bypassPermissions` mode.** `.claude/settings.json`
  sets `defaultMode: bypassPermissions` plus a broad allow-list for the
  build/run/git/inspect workflow, so routine commands must never prompt.
  If the session is still prompting, it was started in a more restrictive
  mode — start with `--dangerously-skip-permissions` (or pick bypass in the
  trust dialog). Don't burn turns getting individual commands approved.
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
  driven from either a Linux box (GTK 4 / meson / Rust +
  an X session at `DISPLAY=:0`) or a Windows 11 box via MSYS2
  MINGW64 (same toolchain, same meson/ninja invocation; the binary
  is `./builddir/src/gtk/southstar.exe`). Every commit must pass
  `meson compile -C builddir` locally before pushing. Smoke-launch
  the browser (in the background, then kill it) on material changes
  — that's the per-change correctness gate, not CI. See
  `docs/Windows.md` for the MSYS2 setup; the rest of this guide
  uses Unix-style invocations that work in either shell.
- **CI is enabled for Linux.** That workflow runs on every push to
  `main` and every PR targeting `main`, plus manual `workflow_dispatch`;
  FreeBSD and NetBSD run nightly. The Windows, macOS and musl workflows are
  disabled: they run only when started by hand (`workflow_dispatch`).
  The local machine is still the primary correctness gate before
  pushing; CI provides cross-platform sanity coverage.

## Build / verify locally

The intended build system is **meson + ninja**. From a clean checkout:

```sh
meson setup builddir
meson compile -C builddir
./builddir/src/gtk/southstar
```

### Rust: the workspace

All of Southstar is the Cargo workspace (`Cargo.toml`, one crate per
directory under `rust/`). meson builds it: `rust/meson.build` runs
`scripts/cargo-build.py`, which builds the static libraries the executables
link (`rust/static` with everything in `rust/southstar-ffi`, plus the shell,
helper and launcher libraries) and hands ninja cargo's dependency file, so
Rust rebuilds only when Rust changes. Rust 1.99 or newer is required;
`rust-toolchain.toml` pins 1.99.0 for rustup users, so local builds and CI
compile with the minimum supported version.

- The crates mirror the C files they were ported from, and many still reach
  each other through the port's scaffolding: `ns_*`/`nsr_*` exports
  (`#[unsafe(no_mangle)]`) and `extern` declarations in the caller's
  `ffi.rs`. That internal C ABI is being removed. New code calls another
  crate through an ordinary Rust API (a Cargo dependency, typed arguments,
  `String`/`Vec`/`Box` rather than `*mut c_char` and `g_malloc`) wherever the
  dependency graph allows, and keeps `extern "C"` for the real boundaries:
  `src/libsouthstar.h`, GTK/Cairo/Pango callbacks, FFmpeg, OpenSSL, SDL and
  the other system libraries. A new crate is added to `rust/southstar-ffi`
  (dependency plus `pub use`).
- `unsafe` follows `docs/unsafe-policy.md`: only for system FFI, the
  embedding API, the JS/GC binding layer, OS primitives and SIMD; never in
  parsers or engine logic; one operation per block with a `// SAFETY:`
  comment; no `static mut`.
- `cargo fmt --check` and `python3 scripts/unsafe-budget.py --clippy` must be
  clean before pushing. The script runs `cargo clippy --workspace
  --all-targets`, fails on any warning except the three unsafe-documentation
  lints, and holds each crate to its line in `rust/unsafe-budget.tsv`:
  `unsafe` keywords, FFI imports and exports, `static mut`, and those three
  lints' hits. A number may only go down — when it drops, lower the budget in
  the same commit with `--update`; raising one needs an
  `Unsafe-Budget-Increase:` trailer explaining why. A crate with no unsafe
  code carries `#![forbid(unsafe_code)]`.
- A new crates.io dependency needs a reason in its commit message; keep the
  dependency tree small. Crates from Servo or Firefox (`html5ever`,
  `cssparser`, `selectors`, `url`, `encoding_rs`, …) count as upstream browser
  engine code and stay out.
- Unit tests live next to the code: `#[cfg(test)] mod tests;` in the crate
  root and the tests in `src/tests.rs`. `scripts/rust-tests.sh` runs every
  crate that has them (CI runs it too). A test binary links only the crate and
  its Cargo dependencies, so tests work for crates whose code needs no
  meson-supplied library — one more reason to keep parsers and engine logic
  free of FFI. Test parsers, URL handling, cookies and HSTS, CSP and the
  cascade first; check a behavior change in the browser itself as well.
- **The JavaScript engine is Boa**, forked in-tree at `third_party/boa_engine`
  (patched in through `[patch.crates-io]`). QuickJS-ng was removed in October
  2026. Bindings use the `rust/js-engine` API (`Scope`, `Value`, `HostClass`);
  engine hooks the bindings need go into the Boa fork.
- `southstar-jsshell` (`rust/jsshell`) runs scripts and test262 on the same
  engine layer.

### Text layout: ns-pango

Desktop builds shape text through
[ns-pango](https://github.com/nordstjernen-web/ns-pango), a fork of Pango
pinned as a meson subproject (`subprojects/ns-pango.wrap`). Pango keeps no
cache that outlives a `PangoLayout`, so the same bytes reached HarfBuzz once
to measure a run and again to paint it, and a table cell was shaped for
`min-content`, for `max-content` and again to lay out. The fork caches
finished glyph strings process-wide and caches
`pango_context_get_metrics` per font description.

Three rules when touching text code:

- **The engine spells the API renamed** -- `ns_pango_*`, `NsPango*`,
  `NS_PANGO_*`, `NS_TYPE_PANGO_*`: each `extern` declaration of a Pango
  function carries `#[cfg_attr(feature = "ns-pango", link_name =
  "ns_pango_…")]` over its stock name. The renaming is not cosmetic: GTK
  loads the system Pango into the same process, and GObject aborts when a
  second library registers a type name it already holds.
- **`-Dns-pango=disabled` links the system Pango.** meson leaves the crates'
  `ns-pango` cargo feature off, so the declarations link the stock names.
  Anything the fork adds and stock Pango lacks -- `ns_pango_cache_*` -- must
  sit behind `#[cfg(feature = "ns-pango")]`.
- **A run is cached only when its shaping cannot depend on the text around
  it**, because HarfBuzz receives the paragraph as context.
  `NS_PANGO_SHAPE_CACHE=verify` shapes both ways and warns on any
  difference; run it over the affected pages after touching the cache key.
  `NS_PANGO_SHAPE_CACHE=0` disables the cache and `--debug=net` reports
  hits, misses and skips.

### HTML parser: `rust/html5`

The single HTML→DOM backend is `rust/html5` (`southstar-html5`), an HTML5
tokenizer and tree builder written from the WHATWG HTML Standard with no
crates.io dependency (Servo's `html5ever` stays out, D1). The tokenizer
implements every tokenizer state, including the processing-instruction
states, over a whole newline-normalized input; named character references
come from `rust/html5/src/entities.rs`, generated by
`scripts/gen-html-entities.py` from WHATWG's `entities.json` (regenerate it
rather than editing it). The tree builder covers every insertion mode, the
adoption agency, foster parenting, templates, foreign content, quirks,
fragment parsing with a context element and the relaxed `<select>` parsing,
and builds through a `TreeSink` trait. `rust/html-parser` is the sink that
writes `ns_node`s directly, with names, text and attribute values in one
string arena per document attached as the root's `backing`; script source
positions come from the tokenizer. The parser keeps no global state (SVG image
threads parse concurrently).

### Image decoding: pure-Rust decoders

PNG/APNG, GIF, BMP and JPEG bytes are decoded in Rust by
`rust/image-decoders` (`src/raster/`): the `png` crate (including APNG
frames), the `gif` crate and `zune-jpeg`, plus a small in-tree BMP decoder
(core/info/V2–V5 headers, 1/2/4/8-bit palettes, RLE4/RLE8, bit fields,
24/32-bit, top-down). They draw into premultiplied BGRA canvases with the
same premultiplication, compositing, disposal and delay rules the former
Wuffs chain used, so PNG, GIF and BMP pixels are unchanged; JPEG differs
from Wuffs by a few levels of IDCT rounding. No orientation is applied
from EXIF. `ns_image_decode_bytes` (`rust/image`) is the whole chain:
ICO (`rust/image-decoders`), then `ns_image_decode_raster`, then WebP
(`rust/image-decoders`, libwebp), then AVIF when built, then SVG in-engine
(`rust/svg`). Nothing follows — gdk-pixbuf no longer decodes page
images, so an unsupported format simply fails to decode rather than
reaching a loader plugin installed on the user's machine.

### URL parsing: `rust/url`

The `ns_url_*` helpers (`rust/net/src/url.rs`) route URL resolution,
origin extraction, host extraction and the URL setters through
`rust/url` (`southstar-url`), a WHATWG URL parser written from the URL
Standard with no crates.io dependency: the basic URL parser, the host
parser with UTS #46 IDNA, Punycode and NFC, the serializer and the URL
API setters. Its Unicode tables (`rust/url/src/tables.rs`) are generated
by `scripts/gen-url-tables.py` from the Unicode Character Database;
regenerate them for a new Unicode version rather than editing them.
Servo's `url` and `idna` crates stay out (D1).

### HTTP client: `rust/http`

All networking goes through `rust/http` (`southstar-http`), an HTTP client
written from scratch in Rust with no crates.io dependency; libcurl and
libnghttp2 are not used. It speaks HTTP/1.1 and HTTP/2 with its own framing,
flow control and HPACK (the static table and Huffman code are generated from
RFC 7541); those wire formats — HTTP/1.1 heads and chunked bodies, HTTP/2
frames, HPACK and the sans-I/O `h2::Connection` — live in `rust/http-wire`,
which forbids unsafe code and does no I/O. `rust/http` adds FTP (`ftp.rs`), HTTP CONNECT and SOCKS4/4a/5/5h proxies with
no-proxy matching (`proxy.rs`), redirect-following fetches for callers outside
the page fetch path (`fetch.rs`) and the HTTP/1.1 upgrade WebSocket uses.
TLS is OpenSSL's libssl behind `rust/http/src/ffi/tls.rs` (ALPN, per-host
session resumption, the insecure-certificate override, the Windows root store
when no CA bundle is found); gzip/deflate go through zlib and br/zstd through
libbrotlidec/libzstd when present (`rust/http/src/ffi/codec.rs`). HTTP/2
connections are pooled per `scheme://host:port` and **multiplex concurrent
requests over one connection**: an I/O thread per connection drives the
sans-I/O `h2::Connection`, workers queue a request and wait for its stream
events, and connecting to an origin is serialized. Sockets are non-blocking
after connect and every wait is a `poll()` (never a socket timeout, which
Windows reports as an error).

Everything above one hop — redirects, HSTS (recorded by `rust/net/src/hsts.rs`),
referer, cache, cookie partitioning — lives in `rust/net`
(`rust/net/src/ffi/hop.rs` is the hop). Other crates reach the network
through `southstar_http::fetch` with `southstar_net::route` supplying TLS
settings and the proxy; the media helpers download through `rust/helper-ffi`.
Extend `rust/http`; don't reintroduce libcurl. Not supported: HTTP/3, DNS over
HTTPS, TLS Encrypted Client Hello.

### WebAssembly: wasmi

The `WebAssembly` JS API (`compile`, `instantiate`, `Memory`,
`Table`, `Global`, externref) is implemented in `rust/js-wasm` over
[wasmi](https://github.com/wasmi-labs/wasmi), a pure-Rust interpreter
from crates.io with no JIT. It runs wasm-bindgen bundles. `-Dwasm=disabled`
drops the crate and the `WebAssembly` global.

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
GIFs) — is decoded by `rust/image-decoders/src/ffi/libwebp.rs`, tried right after
the PNG/GIF/BMP/JPEG decoders. No gdk-pixbuf loader, no
`loaders.cache` registration, and no sandbox interaction is
involved; the old `webp-pixbuf-loader` runtime dependency is gone.

### Web Cryptography: OpenSSL libcrypto

Required dependency (Debian/Ubuntu `libssl-dev`, Fedora/RHEL
`openssl-devel`, openSUSE `libopenssl-devel`). `crypto.subtle` (the
WebCrypto SubtleCrypto surface) is implemented in `rust/webcrypto`
directly over OpenSSL's EVP/`OSSL_PARAM` APIs — hashing, HMAC, AES
(GCM/CBC/CTR/KW), RSA (PKCS1/PSS/OAEP), ECDSA/ECDH (P-256/384/521),
Ed25519/X25519, PBKDF2 and HKDF. The `CryptoKey` class and `subtle.*` argument
marshalling live in `rust/js-crypto`. The same OpenSSL provides `rust/http`'s
TLS (libssl); `meson` depends on `libcrypto` explicitly.

System packages required on Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config meson ninja-build cargo rustc \
    libgtk-4-dev libepoxy-dev libssl-dev zlib1g-dev libbrotli-dev libzstd-dev \
    libuchardet-dev libpsl-dev libsqlite3-dev libseccomp-dev libwebp-dev libsdl2-dev \
    libavformat-dev libavcodec-dev libavutil-dev libswscale-dev libswresample-dev
```

Optional: `libenchant-2-dev` (plus a dictionary such as `hunspell-en-us`)
enables on-screen spell-checking of editable text. It is auto-detected —
the build works without it and simply does no spell-checking. `libavif-dev` is optional too and adds
AVIF decoding; it drags in a full AV1 decoder for a format that is rare
on the web, so `-Davif=disabled` drops it.

The FFmpeg libav\* dev packages in the command above (MSYS2
`mingw-w64-x86_64-ffmpeg`, or the LGPL build from
`scripts/build-ffmpeg-lgpl.sh`) enable inline WebM playback (VP9/VP8 video
+ Opus/Vorbis audio). **Required on Linux and Windows** (`meson setup` fails
without them, or with a pre-6.0 FFmpeg); auto-detected on macOS, where without
them the build carries no libav dependency and WebM falls back to the
external-player path. FFmpeg 6.0 or newer is required.

On Fedora/RHEL:

```sh
sudo dnf install gcc pkgconf meson ninja-build cargo rust gtk4-devel libepoxy-devel \
    openssl-devel zlib-devel brotli-devel libzstd-devel uchardet-devel libpsl-devel sqlite-devel \
    libseccomp-devel libwebp-devel SDL2-devel ffmpeg-devel
```

(`ffmpeg-devel` comes from RPM Fusion.)

On openSUSE:

```sh
sudo zypper install gcc pkgconf meson ninja cargo rust gtk4-devel libepoxy-devel \
    libopenssl-devel zlib-devel libbrotli-devel libzstd-devel libuchardet-devel libpsl-devel sqlite3-devel \
    libseccomp-devel libwebp-devel libSDL2-devel ffmpeg-devel
```

(`ffmpeg-devel` comes from Packman.)

Rust must be 1.99 or newer. Rolling distributions ship it; elsewhere,
Ubuntu 24.04 (whose newest packaged compiler is `rustc-1.91`) and Debian 13
included, install it with rustup, which reads `rust-toolchain.toml` and
selects 1.99.0.

`libseccomp` is required on Linux — `meson setup` fails without it.
On macOS and Windows it is not used and the syscall filter is a no-op.

### Fast iteration (recommended for agent loops)

The default meson build is a release build with fat LTO and one codegen
unit, so a clean build takes many minutes and every Rust change relinks the
whole static library. For the edit-check loop use `cargo check -p <crate>`
or `cargo clippy -p <crate>`, which take seconds; build the browser with
meson to run it. A `meson setup build-debug -Dbuildtype=debug` tree builds
the dev profile — no LTO, integer overflow checks on — which compiles much
faster and runs slower. `-Doverflow_checks=true` turns the overflow checks on
in a release build too (CI does).

Optionally use the `lld` linker for faster final links
(`CC_LD=lld meson setup builddir`). Not required.

`./scripts/dev.sh build` runs `meson setup` (only if needed) and
`meson compile -C builddir` in one shot — use it instead of typing
the two commands separately.

### WPT scoreboard

`docs/wpt-scores.md` tracks web-platform-tests scores over time and
documents the improvement loop: pick the highest-ROI area from its
"ROI by area" table, find the failing subtests in
`docs/wpt-subtests.tsv`, fix the engine, then rerun just that area
with `scripts/wpt-score.sh --wpt-root=~/wpt AREA` — it updates the
doc and data files in place. Commit the regenerated files together
with the engine change. When asked to improve the WPT score, start
from the "Top 10 improvements" list there (or the ROI table if the
list is stale), and re-cluster the list when the scores move.

## Definition of done

A change is done when:

1. It compiles cleanly (no new warnings), and `cargo fmt --check` and
   `python3 scripts/unsafe-budget.py --clippy` are clean.
2. The browser launches and the affected UI path works manually.
3. The change is committed and pushed to `origin/main`.

Unit tests (`scripts/rust-tests.sh`) cover the parsers; otherwise verify
behavior by running the browser. Don't add a `tests/` directory or
`meson test` targets.

## Don't

- Don't introduce Mozilla/Gecko code, WebKit code, or any other
  upstream browser engine source. Southstar is an independent
  implementation, not a fork.
- **Don't add site-specific hacks.** No per-site rendering shims, no
  hardcoded hostnames, no grepping a site's private JSON (e.g.
  `ytInitialPlayerResponse`, `mediaDefinitions`, `ytimg`, `movie_player`).
  Always improve the **generic** engine so it runs the **real** site: read
  web standards (OpenGraph, JSON-LD, WHATWG DOM/JS APIs) and aim to execute
  the site's own JavaScript rather than reverse-engineering its data. When a
  page renders wrong, fix the engine capability it exercises — never
  special-case the host. The standards-based media metadata extractor in
  `rust/html-parser` is the pattern; the deleted YouTube scraper was the
  anti-pattern.
- Don't add AI-style web-API surface area, even as stubs. WebGL is a
  deliberate exception — extend `rust/webgl`, don't re-architect it.
- WebGPU is an experimental exception layered over external wgpu-native
  (`rust/webgpu`). The `webgpu` feature is `auto`: built
  only when wgpu-native is actually present, so a machine without it still
  gets a build with no WebGPU surface or dependency. Keep it behind the
  `--enable-webgpu` / `NS_WEBGPU_ALLOW` runtime gate, and don't make
  wgpu-native a hard/default dependency or vendor its library into the tree
  (headers only) — a stock `meson setup builddir` on a clean machine must
  still produce a WebGPU-free binary.
- Don't add telemetry, crash reporters, update pingers, or "studies"
  infrastructure. UI translation goes through `rust/i18n` and the
  `data/i18n/*.lang` catalogues — don't introduce gettext or `.po`
  tooling.
- Don't write planning docs unless asked.
