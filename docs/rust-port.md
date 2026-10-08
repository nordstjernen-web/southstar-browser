# Porting Southstar Browser to Rust

Status: **in progress** — phase 0 (toolchain, build integration, the first
ported module) has landed; see §12. Written in October 2026 against `main` at
the commit that added this file. Sizes and facts below were taken from the code on that tree.
Where they disagree with older docs (`media.md` calls the audio helper
unsandboxed and the video ring three slots), the code is right.

## 1. Summary

Southstar starts as about **221,000 lines of project C** (plus ~570,000 lines
of vendored C libraries) that already make a working, sandboxed,
process-per-tab browser on Linux, Windows, macOS, FreeBSD and NetBSD. The plan
is to port that code to Rust **in place and incrementally**, so that every
commit still builds, launches and passes the same checks as today:

1. **Cross process boundaries first.** The audio and video helpers, the
   renderer protocol and the GTK shell are separated from the engine by narrow
   IPC protocols. Rust replacements can speak the same protocol and drop in
   without touching the engine.
2. **Then replace engine modules behind their existing C headers.** A Rust
   module exports the same `ns_*` functions the C header declares, so the C
   that still calls it does not change. Leaf modules go first; the large
   coupled core (DOM, style, layout, paint) goes next; the JavaScript bindings,
   at ~78,000 lines the largest single piece, go last, written once against an
   already-Rust DOM.
3. **Translate first, redesign later.** Each port keeps behaviour, data layout
   and algorithms. Redesigns (a display list, a Rust-native text stack, an
   arena-based DOM) are separate, later steps with their own decisions.
4. **Vendored C libraries are decided one by one.** Some are replaced during
   the port where a Rust crate is a clear win (Wuffs, WAMR); lexbor, Cairo,
   ns-pango and GTK stay C behind FFI until a separate decision.
5. **The JavaScript engine becomes a build-time choice.** The Rust bindings
   are written once against an engine-neutral layer, so the in-tree
   QuickJS-ng fork stays the default while pure-Rust engines (Boa first, Nova
   as an experiment) can be built in and compared on the same pages, tests
   and benchmarks (§6, "JavaScript engines").

The end state of this plan: all **project** code is Rust; C remains only in
third-party libraries that are deliberately kept (phase 9 and §9).

| Phase | What | Project C retired (lines) |
|---|---|---:|
| 0 | Toolchain, build integration, CI, a pilot module | 150 |
| 1 | Helper processes and the sandbox library | 3,974 |
| 2 | Leaf modules, image decoders, a JavaScript pilot | 8,670 |
| 3 | Renderer host and IPC protocol | 5,028 |
| 4 | GTK shell, watchdog and headless driver | 13,291 |
| 5 | Networking and storage | 13,500 |
| 6 | Engine core: DOM → style → layout → paint → pipeline | 82,672 |
| 7 | JavaScript bindings | 77,729 |
| 8 | Remaining web platform features (WebGL, WebGPU, Wasm, media) | 16,438 |
| 9 | Vendored libraries; decide the long-term build | — |
| | **Total** | **221,452** |

Every project source file is assigned to exactly one phase.

The phases are ordered by dependency and risk, not by calendar; sizes are the
only estimates given. Decisions that need an owner are collected in §11.

## 2. Goals and non-goals

**Goals**

- All project code in Rust, with `unsafe` confined to FFI layers.
- Same platforms as today: Linux (glibc and musl), Windows (MSYS2/MinGW),
  macOS, FreeBSD, NetBSD.
- Feature parity at every step, including the curl and nghttp2 HTTP backends,
  single-process mode, headless mode, the C embedding API (`libsouthstar.h`)
  and the optional features (WebGPU, AVIF, spell checking, HTTP/3).
- The same security posture: process-per-tab, seccomp + Landlock on Linux, the
  macOS sandbox profile, Windows mitigations, no JIT.
- No performance or memory regression beyond noise.

**Non-goals of the port itself**

- New features or behaviour changes mixed into port commits.
- Replacing the rendering stack (Cairo, ns-pango, GTK) or the JavaScript
  engine while porting. These are separate decisions (§9, §11).
- Bringing back Android, iOS or a JVM binding.

## 3. Project rules and what they mean in Rust

The rules in `CLAUDE.md` and `SOUTHSTAR.md` carry over unchanged. Some need an
explicit Rust reading:

- **No upstream browser engine code.** Several popular crates come out of
  Servo or Firefox: `html5ever`, `markup5ever`, `cssparser`, `selectors`,
  `stylo`, `webrender`, `url` (rust-url, a Servo project), `encoding_rs` and
  `chardetng`. This plan treats them as excluded. That rules out the usual
  shortcuts for HTML parsing, CSS parsing, URL parsing and encoding detection;
  Southstar keeps lexbor and uchardet behind FFI or writes its own (decision
  D1).
- **Few small auditable dependencies.** Crates pull in transitive trees.
  Every crate is pinned in `Cargo.lock`, vendored for offline builds (§5.6),
  licence-checked with `cargo-deny`, and listed in `THIRD-PARTY-LICENSES.md`.
  Prefer crates with no or few dependencies; an async runtime (`tokio`) is not
  added unless decision D7 calls for it.
- **No comments beyond one header line per file.** In Rust that header is a
  `//!` line. Clippy's `undocumented_unsafe_blocks` lint expects
  `// SAFETY:` comments, which the rule forbids; this plan keeps the rule and
  confines `unsafe` to small `ffi` modules instead (decision D5).
- **No automated test suite.** No `#[test]` functions, no `tests/` directory,
  no `cargo test` targets. Parity is checked with the scripts the project
  already has (§8). Whether ported code may carry unit tests is decision D6.
- **No JIT, no telemetry, no AI-style web APIs, no site-specific hacks** — as
  today.
- **WebGPU stays opt-in.** Its Rust side is a Cargo feature that is off by
  default and still runtime-gated by `--enable-webgpu`; a stock build carries
  no WebGPU code.
- **UI translation** keeps `data/i18n/*.lang` and the `ns_i18n()` lookup; the
  Rust shell reads the same catalogues.

## 4. Starting point

### 4.1 Code inventory

Project code by subsystem (`.c`, `.h`, `.m`; generated image headers and
vendored code excluded):

| Subsystem | Lines | Main files |
|---|---:|---|
| JavaScript bindings | 79,269 | `js.c` (66.6k), `js_canvas*.c`, `js_date.c`, `js_intl.c`, `js_perf.c`, `ns_quickjs.c` |
| Style and animation | 38,604 | `css.c` (33.1k), `css_media.c`, `css_prop_syntax.c`, `css_syntax.c`, `anim.c` |
| Layout | 22,058 | `layout.c` (17.8k), `svg.c`, `mathml.c`, `selection.c`, `print.c`, `pdf.c` |
| Networking and storage | 15,864 | `net.c` (7.2k), `net_http2.c`, `cache.c`, `idb.c`, `ws.c`, `eventsource.c`, `csp.c`, `config.c` |
| Embedding and renderer process | 13,776 | `libsouthstar.c`, `engine.c`, `headless.c`, `renderer_*.c`, `rproc_*.c`, `ipc_http.c` |
| Paint and text | 11,262 | `paint.c` (8.5k), `render.c`, `font.c`, `woff2.c`, `ns_pango*.h` |
| WebGL, WebGPU, Wasm | 10,734 | `webgl.c`, `webgpu.c`, `wasm.c`, `glctx.c` |
| GTK shell | 10,414 | `gtk/procview.c`, `gtk/procwindow.c`, `gtk/appmain.c`, `gtk/macos_dock.m` |
| DOM and parsing | 5,831 | `dom.c`, `html.c`, `html_lexbor.c`, `xml.c`, `forms.c` |
| Images and media | 5,654 | `image*.c`, `video.c`, `video_decode.c`, `camera.c`, `mic.c` |
| Platform, security, misc | 5,574 | `security.c`, `watchdog.c`, `webcrypto.c`, `ext.c`, `win_launcher.c` |
| Audio and video helpers | 2,412 | `audio/main.c`, `videoproc/main.c` |
| **Total** | **221,452** | |

Vendored C: lexbor (~289k lines), QuickJS-ng fork (~101k), Wuffs (~97k), WAMR
(~73k), pl_mpeg (~4.5k), minimp3 (~1.9k); ns-pango is a meson subproject.
`data/js/polyfills.js` (11k lines) and `streaming.js` are JavaScript and stay
as they are.

### 4.2 Structure that shapes the plan

**Processes.** The shell (`southstar`) runs one sandboxed `southstar-renderer`
per tab, plus lazily spawned `southstar-audio` and `southstar-video` helpers;
the watchdog is a supervisor mode of `southstar` itself, and Windows adds a
small `southstar-launcher`. The interfaces between them are narrow:

- *Shell ↔ renderer*: `fork`+`exec` with a socketpair on fd 3 (inherited pipes
  on Windows, or stdio for embedders), carrying HTTP/1.1 requests with JSON
  bodies to about 33 POST paths (`/open`, `/tick`, `/render`, `/click`,
  `/key`, `/scroll`, `/find`, `/eval`, `/media`, `/print`, …). Pixels travel
  through one shared-memory framebuffer (Cairo ARGB32, passed with
  `SCM_RIGHTS`); a tiles mode packs tiles into it and describes them in a
  text body. Single-process mode runs the same `ns_renderer_session` on a
  thread over an fd pair, so the shell's client code is identical.
- *Shell ↔ audio helper*: about ten line-based text verbs over pipes and one
  shared-memory clock.
- *Shell ↔ video helper*: about ten text verbs and a shared-memory ring of
  eight BGRA frames.
- Behind the renderer protocol, the engine's public API is `libsouthstar.h`:
  about 80 `ns_browser_*` functions with no GLib types in them.

**DOM.** The engine has its own DOM, a tree of `ns_node` structs
(`dom.h`) with raw parent/child/sibling pointers, an attribute list, per-document
id/class/tag indexes and a JS wrapper pointer. lexbor parses, then
`html_lexbor.c` copies its tree into `ns_node`s; names, text and attributes
are *borrowed* from lexbor's memory, which stays alive as the document's
backing store. Nodes are not refcounted: the tree owns them, and nodes that
JavaScript detaches are collected by an orphan sweep that runs the QuickJS GC
first.

**Style.** `css.c` is a hand-written parser and cascade (lexbor's CSS module
is unused). Computed style (`ns_style`, ~242 properties plus pseudo-element
styles and custom properties) is refcounted and shared between nodes; it is
not stored on nodes but in a node → style hash table owned by the page.
Selector matching is right-to-left with an ancestor Bloom filter, a `:has`
memo and a match budget.

**Layout and paint.** Layout builds a separate `ns_box` tree from a global
free-list pool and rebuilds it on every relayout. Paint walks the box tree
straight into Cairo — there is no display list — with text measured and drawn
through ns-pango. Tiles and layers are planned in `renderer_tiles.c`.

**JavaScript.** All bindings are hand-written against a QuickJS-ng fork that
carries browser hooks (`get_own_property_receiver`, host-function mode,
WebIDL brands, engine-private names, realm queries, `JS_ThrowDOMException`,
…). There are about 27 classes; every DOM node uses one `Element` class whose
prototype is picked per node kind. js.c has ~714 entries in its function-list
tables and several hundred more functions bound one by one. Each node caches and pins its wrapper; freeing a
node invalidates the wrapper and scrubs side tables. Workers run their own
`JSRuntime` on their own thread with a private `GMainContext`.

**Threads.** The renderer's main thread owns DOM, style, layout, paint and
page JS. Around it: a GTask fetch pool, a network I/O thread owning the curl
multi handle, HTTP/2 I/O threads, an image decode pool (which also parses and
lays out SVG images, so `css.c`, `layout.c` and `svg.c` use `__thread`
globals), one thread per WebSocket and EventSource, and the worker threads.
The main thread runs no persistent GLib main loop; GLib is pumped from
`ns_browser_tick`, during settle, and in nested loops for blocking fetches.

**Globals and GLib.** The engine has ~431 mutable file-scope statics (css.c
101, net.c 53, js.c 53, layout.c 35). GLib is used throughout the engine —
`gboolean`, `GString`, `GHashTable`, `GPtrArray`, `g_malloc`/`g_free` — but it
defines no GObject types. The public embedding header `libsouthstar.h` (~80
`ns_browser_*` functions) uses no GLib types.

**Coupling.** The include graph has cycles (`css`↔`dom`, `layout`↔`paint`,
`net`↔`config`, `net`↔`js`), and a few hubs that most of the engine includes:
`net.h` (included by 28 modules), `dom.h` (19), `config.h` (19), `image.h`
(18), `css.h` (15). Modules cannot be ported in strict leaf-to-root order;
the headers, not the files, are the stable seams.

## 5. Toolchain and build (phase 0)

### 5.1 Layout

```
Cargo.toml              workspace, shared lints and profiles
Cargo.lock
rust/
  southstar-ffi/        the one staticlib linked into C targets; re-exports
                        every ported module's extern "C" functions
  sys/quickjs-sys/      bindgen over src/quickjs (the in-tree fork)
  js-engine/            the engine-neutral layer the bindings are written
                        against, with one backend module per engine
  jsshell/              southstar-jsshell, a command-line host for test262
                        and shell benchmarks on any backend
  sys/lexbor-sys/       bindgen over src/lexbor
  sys/ns-pango-sys/     bindgen over ns-pango's renamed API
  sys/southstar-sys/    bindgen over the engine headers still in C
  ipc/                  renderer protocol, both ends
  sandbox/              seccomp, Landlock, macOS and Windows policies
  …                     one crate per ported subsystem
  bin/southstar-audio/  helper executables
  bin/southstar-video/
```

Only **one** Rust staticlib is linked into any C executable: two Rust
staticlibs in one binary each carry their own copy of `std` and collide.
`southstar-ffi` therefore depends on every ported crate and is the single
archive meson links.

### 5.2 Meson drives Cargo

Meson stays the build entry point during the port, so `meson setup builddir &&
meson compile -C builddir` keeps working on every platform:

- A `custom_target` (`rust/meson.build`) runs `scripts/cargo-build.py`,
  which calls `cargo build --locked` (`--release` for meson's release and
  minsize build types) with the target directory inside the build
  directory. It copies `libsouthstar_ffi.a` out only when its bytes changed
  and hands ninja cargo's dependency file plus the manifests, so a no-op
  build runs neither cargo nor the linker.
- The native libraries Rust's `std` needs (`-lgcc_s -lpthread …` on Linux,
  different everywhere) are asked of `rustc --print native-static-libs` at
  configure time, not hard-coded per platform.
- The engine library and executables link `libsouthstar_ffi.a`; meson
  installs the Rust executables exactly where the C ones were installed, so
  packaging scripts do not change paths.
- Meson will pass the configured features (`webgpu`, `avif`, `http_backend`,
  `js_engine`, libav presence) to Cargo as `--features` once ported code
  depends on them, so one option set controls both languages.
- Cargo profiles mirror the meson profiles: release = LTO, `codegen-units = 1`,
  `panic = "abort"`, stripped; development = debug info, frame pointers.

At the end of the port (phase 9) the direction can flip: Cargo becomes the entry
point and builds whatever C remains through `build.rs` and the `cc` crate.

### 5.3 Keeping the C and Rust sides in agreement

- The existing C headers stay the contract. Exports whose signatures use
  only scalars and pointers (the first ports) are written by hand against the
  header with `core::ffi` types (`c_long` keeps C's width, 32-bit on
  Windows) and checked with a differential harness against the C they
  replace. Once headers with structs are ported, `southstar-sys` runs
  `bindgen` over them and each Rust export is checked against the bindgen
  declaration (a typed function-pointer `const` per export), so a signature
  drift fails the build instead of corrupting memory.
- Structs that both languages touch during the transition (`ns_node`,
  `ns_style`, `ns_box`, …) are defined once, in Rust, as `#[repr(C)]`, and
  the C header is generated from them with `cbindgen` for that period. Size
  and offset assertions are emitted on both sides.
- Memory crosses the boundary in GLib's allocator: anything Rust hands to C
  that C will `g_free` is allocated with `g_malloc`/`g_strdup`, and anything C
  hands to Rust is released with `g_free`. Rust-owned values stay behind
  opaque pointers with explicit `*_free` functions.
- Panics abort (`panic = "abort"`; since Rust 1.81 an unwind out of an
  `extern "C"` function aborts anyway), matching how a C crash behaves today:
  the renderer dies and the shell restarts it.

### 5.4 Toolchain version

Rust 2024 edition needs **Rust 1.85**, which is exactly what Debian 13
(trixie) ships; the nightly packages build in `debian:trixie`. Two pulls the
other way:

- `gtk4` 0.11 (current gtk-rs) requires Rust 1.92; `gtk4` 0.10 needs 1.83 but
  only exposes APIs up to GTK 4.20 (Windows uses GTK ≥ 4.22.1).
- Ubuntu 24.04's default `rustc` is 1.75; versioned `rustc-1.85`, `-1.89`,
  `-1.91` packages exist. trixie-backports carries 1.94–1.95.

Decision D3 picks between "MSRV 1.85, gtk4 0.10" and "MSRV 1.92+, distro
builds use backports/versioned compilers". CI pins the chosen version with
`rust-toolchain.toml`.

### 5.5 CI

The workflows that build today all gain a Rust toolchain:

| Workflow | Platform | Rust toolchain |
|---|---|---|
| `linux.yml` | Ubuntu 26.04, GCC | rustup, pinned |
| `macos.yml` | macOS 26 arm64 | rustup, pinned |
| `windows.yml` | MSYS2 MINGW64 | MSYS2's `mingw-w64-x86_64-rust` (`x86_64-pc-windows-gnu`, the ABI the C side already uses) |
| `musl.yml` | Alpine 3.24 | Alpine's `rust`/`cargo` packages |
| `freebsd.yml`, `netbsd.yml` | VMs, nightly | `pkg`/pkgsrc `rust` |
| `release.yml` | `ubuntu:24.04`, `ubuntu:26.04`, `debian:trixie` containers | distro packages (`rustc-1.85` on Ubuntu 24.04, whose default `rustc` is 1.75) |

Every workflow adds `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check`, the Rust form of "no new warnings".

### 5.6 Offline builds and packaging

Two package builds run without network access: `debian/rules` in a
network-isolated sbuild/pbuilder chroot, and the openSUSE package, which OBS
builds straight from git (`scmsync`). Cargo runs with `--locked`: the lockfile
is never rewritten, and nothing is fetched when the registry index and sources
are already present. A fresh machine with network fetches the index entries
once, including those of optional dependencies such as Boa that the default
build never compiles (`--frozen` refused even that, which broke the BSD
builds); an offline package build points Cargo at vendored sources
(`cargo vendor`), and where those sources live is decision D4. `nightly-distro-build.sh` and the `pack-*.sh` scripts
need no path changes because meson installs the Rust executables where the C
ones were; their dependency lists gain `cargo`/`rustc` (and `debian/control`,
the RPM spec and `APKBUILD` their build dependencies). The macOS, Windows and
BSD bundles are unaffected beyond the build: Rust links statically.

### 5.7 Pilot

Port `src/datetime.c` (150 lines, no project dependencies, used by three
modules) through the whole pipeline: crate, `southstar-ffi`, meson link,
header check, every CI platform, every package format. Phase 0 is done when
that pilot ships in a nightly on all platforms with no behaviour change, and
`CLAUDE.md` and `SOUTHSTAR.md` describe the Rust build and the rules in §3.

## 6. Phases

Each phase lists its scope, approach and exit criteria. A module is
**ported** when its C file is deleted, the Rust replacement is linked through
`southstar-ffi`, and the parity gate (§8) passes.

### Phase 1 — Helper processes and the sandbox

Scope: `audio/main.c` (1.7k), `videoproc/main.c` (0.7k), `security.c` (1.1k),
`win_launcher.c` (0.4k), `media_shm.h`.

- `southstar-audio` and `southstar-video` become Rust executables that speak
  the same line protocol and lay out the same shared-memory clock and frame
  ring (`media_shm.h`, now a `#[repr(C)]` definition both sides share). The
  shell does not change. These are the first Rust code to ship, and they
  prove the toolchain, the sandbox and packaging on every platform with no
  engine involvement.
- `southstar-launcher` (Windows only) is a self-contained executable; port
  it alongside.
- `security.c` becomes the `sandbox` crate (seccomp via `seccompiler` or the
  `libseccomp` crate, Landlock via the `landlock` crate, the macOS profile and
  Windows mitigation policies via FFI). The C renderer keeps calling it
  through its existing header, and the Rust helpers use it directly.
- **Run every ported process under the real filter.** The seccomp allowlist
  (266 syscalls) already includes what Rust's `std` relies on — `clone3`,
  `statx`, `getrandom` (for `HashMap` seeding), `sigaltstack` and
  `mmap`/`mprotect` (the guard `std` installs on every thread it spawns),
  `futex`, `rseq`. Its default action is `EPERM`, not kill, so a syscall the
  list misses shows up as a failing call rather than a crash; look for those
  in the smoke runs.
- Audio output keeps SDL2 through the `sdl2` crate for parity (it covers the
  BSDs); `cpal` is a later option. MP3 and MPEG-1 decoding keep minimp3 and
  pl_mpeg through FFI; libav through `ffmpeg-sys-next`.

Exit: `docs/media.md` flows (MPEG-1, MP3, WebM, MSE/HLS) play as before on
all platforms; the sandbox is unchanged in behaviour.

**Revised order (October 2026).** The media helpers are less self-contained
than they look: they drive FFmpeg, SDL2, libvorbisfile, libopusfile and
libcurl, and FFmpeg's struct layouts change across the 6.0–8.0 versions
Southstar supports, so a Rust helper needs bindings generated against the
installed headers (`bindgen` and libclang at build time, or the
`ffmpeg-sys-next` crate that runs it). That is a dependency decision, not a
porting step, so the helpers wait until it is made; phase 2's leaf modules,
whose C APIs are opaque pointers and plain scalars, go first. `security.c`
can still be ported on its own.

### Phase 2 — Leaf modules

Scope (8.7k lines), roughly in this order — modules with no project dependencies first:

- `webcrypto.c` (1.4k) over OpenSSL, as today. (RustCrypto's `rsa` crate
  has carried a timing side-channel advisory, RUSTSEC-2023-0071, so it is not
  an automatic replacement.)
- `woff2.c` (0.7k) over libbrotlidec, as today.

C-variadic functions cannot be defined in stable Rust. The engine's headers
declare two: `ns_debug_log_emit`, which became a `static inline` formatting
wrapper in `debuglog.h` around the non-variadic `ns_debug_log_emit_take`,
and one in the original-QuickJS adapter, which will need the same
treatment.

The leaf ports call their C libraries through **hand-written declarations**
(an `unsafe extern "C"` block per module, and the shared `rust/glib` crate
for GLib) rather than crates: OpenSSL's EVP API, libbrotlidec, GLib's
containers and allocator are opaque pointers plus a few stable public
structs, the symbols resolve when meson links the libraries it already
links, and the dependency tree stays empty. Crates come in when they replace
a C library outright (the image decoders below) or when a binding has to
follow a library's changing struct layouts.
- `css_syntax.c`, `mat4.h`, `glctx.c`, `threaddump.c`, `debuglog.c`,
  `i18n.c`, `spellcheck.c` (Enchant via FFI), `safebrowsing.c` (SHA-256 via
  `sha2`), `csp.c`, `bytecode_cache.c`, `config.c`, `history.c`,
  `bookmarks.c`, `proc_limits.h`.
- **Image decoders.** Replace the Wuffs chain (`image_wuffs.c`) and
  `image_webp.c`/`image_ico.c` with the pure-Rust `png`, `gif`, `zune-jpeg`
  and `image-webp` crates (BMP and ICO are small enough to port by hand);
  libavif stays behind FFI. This retires ~97k vendored lines and keeps the
  memory-safety reason Wuffs was chosen for.
- **A JavaScript engine bake-off and pilot.** Build the first
  `js-engine` layer with two backends, QuickJS-ng (over a first
  `quickjs-sys`) and Boa, plus `southstar-jsshell`, and run test262 and the
  shell benchmarks on both (see "JavaScript engines" below). Then port one
  self-contained binding file — `js_perf.c` (1.3k) or `js_date.c` (Temporal,
  1.5k) — against that layer and run it on both engines. This settles the
  binding style (D8) and shows what the layer must express long before
  phase 7 depends on it.

Exit: the modules above are Rust; PNG, GIF and WebP images decode
identically (render-test PNG dumps match pixel for pixel). JPEG decoders round
IDCT and chroma upsampling differently, so JPEGs are compared with a small
tolerance and checked by eye.
The bake-off's test262 and shell-benchmark numbers for QuickJS-ng and Boa are
published, and the pilot binding runs on both.

### Phase 3 — Renderer host and IPC

Scope (5k lines): `ipc_http.c`, `renderer_serve.c`, `renderer_tiles.c`,
`renderer_http.c`, `rproc_http.c`, `rproc_inproc.c`, `embed_shim.c`.

- The `ipc` crate implements both ends of the renderer protocol: spawning
  (`fork`+`exec` with fd 3, inherited pipes on Windows, stdio mode), the
  HTTP/1.1 framing and its size limits, the ~33 POST paths, the `X-*` reply
  headers (`X-Render-RC`, `X-Tiles`, `X-Audio`, `X-Nav`, …), the tiles
  description, and the shared-memory framebuffer (`memfd_create` or
  `shm_open`, passed with `SCM_RIGHTS`).
- The renderer executable becomes a Rust `main` that serves the protocol and
  calls the still-C engine through the `ns_browser_*` API (bindgen over
  `libsouthstar.h`). Single-process mode runs the same server on a thread.

Exit: the shell (still C) drives a Rust renderer host; single-process and
headless modes work; WPT slice and smoke unchanged.

The renderer executable has no C sources left: its entry point is
`ns_renderer_main` in the one Rust library, and because the shell links that
library too and has its own `main`, the linker aliases `main` to it for the
renderer only (`--defsym`, or `-alias` on macOS).

### Phase 4 — GTK shell and headless driver

Scope (13.3k lines): `src/gtk/*` (10.4k), `headless.c` (2.3k) and
`watchdog.c` (0.6k).

- The shell moves to `gtk4-rs`, using the `ipc` crate for renderers and the
  embedding API for single-process mode. It is the least coupled part of the
  application — it never touches engine data structures. (Today it compiles
  the engine sources in directly for single-process mode; it links
  `southstar-ffi` and the engine library instead.)
- The watchdog is a supervisor mode of the same executable (it re-spawns
  itself with `--watchdog-child` and watches for hangs); it moves with the
  shell.
- `macos_dock.m` moves to `objc2`; the Windows-specific calls go through
  `windows-sys`.
- The shell keeps reading `data/i18n/*.lang` and the GResource icon bundle.

Exit: `southstar` is a Rust executable; every UI path in `docs/Controls.md`
works on all platforms.

### Phase 5 — Networking and storage

Scope (13.5k lines): `net.c` (7.2k), `net_http2.c` (2.7k), `net_backend.h`,
`netutil.c`, `cache.c`, `ws.c`, `eventsource.c`, `idb.c` (SQLite via
`rusqlite`), plus the cookie jar, HSTS and CORS logic inside `net.c`.

- `net.h` is the busiest hub (28 includers). Its C API stays as the seam
  until the core is ported.
- The default backend keeps libcurl through the `curl` crate — same
  transport, same behaviour.
- The alternative backend is decision D7: keep libnghttp2 behind FFI, or move
  to `h2` (pure Rust, but it needs an async runtime). If the latter, HTTP/3
  can move from ngtcp2 + nghttp3 + gnutls to `quinn` + `h3`, which removes the
  gnutls requirement that exists only because system OpenSSL 3.0 has no QUIC
  API.
- The network threads keep their shape: one I/O thread for the curl multi
  handle, a fetch pool, per-connection threads.

Exit: both backends fetch byte-identically, as they do today.

### Phase 6 — Engine core: DOM, style, layout, paint

Scope (82.7k lines), in pipeline order:

1. **DOM** (5.8k): `dom.c`, `html.c`, `html_lexbor.c`, `xml.c`, `forms.c`.
2. **Style** (38k): `css_media.c`, `css_prop_syntax.c`, `css.c`, `anim.c`.
3. **Layout** (22k): `layout.c`, `mathml.c`, `svg.c`, `selection.c`,
   `print.c`, `pdf.c`.
4. **Paint and text** (10.4k): `paint.c`, `render.c`, `font.c`,
   `texture.c`, `layers.h`, `ns_pango*.h`, with Cairo through `cairo-sys-rs`
   and text through `ns-pango-sys`. The `pango` crate cannot be used: it
   binds the system Pango's symbol names, and the engine must use ns-pango's
   renamed ones.
5. **Pipeline driver** (6.5k): `engine.c` and `libsouthstar.c` — `struct
   ns_browser`, navigation, settle, relayout and the `ns_browser_*` API the
   renderer host calls — and `version.h`, which goes with the last C file
   that includes it.

Approach:

- **Keep the data layout while C still reads it.** `js.c` (66k lines) reads
  and writes `ns_node`, `ns_style` and `ns_box` fields directly. While it is
  C, those structs stay `#[repr(C)]` with identical layout (§5.3), and Rust
  exports every function their headers declare. The structs become idiomatic
  Rust (an arena with `NodeId` handles instead of raw links, `Rc`-free
  sharing, owned strings) only after phase 7.
- **lexbor memory.** Node names and text borrow lexbor's memory today. The
  DOM port keeps that ownership model (lexbor stays, decision D1) and only
  changes it if lexbor is replaced.
- **Globals.** Each file-scope static becomes a `static` with a `Mutex`,
  `OnceLock` or `Cell`, or a `thread_local!` where C uses `__thread`. The
  `__thread` ones exist because the image pool builds SVG documents off the
  main thread; that stays legal in Rust only if those trees never cross
  threads, which the port must preserve.
- **Order inside style.** `css.c` is 33k lines in one file. Port it by
  section — parser, value types, selector matching, cascade, computed values,
  incremental restyle — each section a set of exported functions, so the file
  shrinks commit by commit rather than flipping at once.

Why the core goes before the JS bindings: the bindings sit on top of the DOM,
style and layout. Porting them first would mean writing 79k lines of Rust
against raw C structs through bindgen and rewriting them once the DOM is
Rust. Porting the core first means the bindings are written once, against a
Rust DOM. The phase 2 pilot keeps the binding approach from being an unknown
until then.

Exit: no C remains in the DOM, style, layout or paint modules; render-test
PNGs match pixel for pixel; WPT slice not lower; layout and paint timings
within noise.

### Phase 7 — JavaScript bindings

Scope (77.7k lines): `js.c` and its siblings (`js_canvas*.c`, `js_intl.c`,
`js_perf.c`, `js_realm.c`, `js_brand.c`, `js_internal.h`, `js_classid.h`),
`ns_quickjs.c`, `webaudio.c`.

- **Bindings target `js-engine`, not an engine.** The ported bindings call
  the engine-neutral layer described in "JavaScript engines" below, never a
  `-sys` crate directly, so every engine backend runs the same DOM, events,
  fetch and worker code.
- **Own `quickjs-sys`, not `rquickjs`.** `rquickjs` bundles its own
  quickjs-ng and has none of the fork's hooks (`get_own_property_receiver`,
  host-function mode, brands, engine-private names, realm queries,
  `JS_RepointArrayBuffer`, …). Southstar binds its in-tree fork directly and
  keeps `-Dquickjs=quickjs` (Bellard's QuickJS) working as a fourth backend
  through the same adapter idea as `ns_quickjs.c`.
- **Binding style** (decision D8): a small declarative layer (macros or
  tables) replacing the ~27 `JSCFunctionListEntry` tables and hand-written
  getters, or a generator driven by WebIDL. Settled by the phase 2 pilot.
- **Port by subsystem**, following js.c's own regions: timers and the event
  loop; DOM and element bindings; events; XHR and fetch; WebSocket and
  EventSource; workers and service workers; observers; custom elements;
  context setup; script and module loading and the bytecode cache.
- **Keep the lifetime model first**: wrappers cached on nodes and pinned,
  invalidated when the node is freed, orphans swept after a GC. Revisit it
  only after the port.
- **Event loop.** Timers, rAF and worker loops are GLib sources today. They
  stay on GLib (through the `glib` crate) during the port; replacing GLib's
  loop with a Rust one is a later step.
- The JavaScript polyfills (`data/js/*.js`) are unchanged.

Exit: no C bindings remain; with the default QuickJS-ng backend, test262 and
WPT slice scores are not lower and the Speedometer runs
(`scripts/speedometer*-bench.sh`) are within noise; the Boa backend builds,
runs the same bindings and has a published row in the comparison table.

#### JavaScript engines

Today every binding is written against the QuickJS-ng fork's C API, so the
engine cannot be changed without rewriting ~78,000 lines. The Rust port is the
one chance to remove that coupling: the bindings are being rewritten anyway,
and writing them against an engine-neutral layer costs little more than
writing them against `quickjs-sys`. The engine then becomes a build option,
the same way the HTTP backend is (`http_backend`), and engines can be compared
on Southstar's real workload instead of on shell benchmarks alone.

**Review (October 2026).** Engines usable from Rust, with test262 results from
test262.fyi's run of 2026-10-07 (test262 revision c8c7988, 53,614 tests) where
available:

| Engine | Language, licence | test262 | Execution | Rust embedding | Fit for Southstar |
|---|---|---:|---|---|---|
| QuickJS-ng (in-tree fork) | C, MIT | 83.5% (upstream; 23,288/23,726 language tests, 23/3,382 intl402) | bytecode interpreter | own `quickjs-sys` | **Default backend.** Keeps the fork's hooks and today's behaviour; Southstar's own `Intl` lives in the bindings |
| QuickJS (Bellard) | C, MIT | 82.2% | bytecode interpreter | own `quickjs-sys` + adapter | Backend for parity with `-Dquickjs=quickjs` |
| [Boa](https://github.com/boa-dev/boa) 0.22 (2026-08-28) | Rust, MIT/Unlicense | 95.6% | register bytecode VM, NaN-boxing, inline caches; no JIT | `Class` trait, `NativeFunction`, `Trace`/`Finalize` for host objects, pluggable job queue and module loader; `Context` is `!Send` | **Optional backend.** Best pure-Rust conformance, native Temporal, `Intl` through ICU4X behind a feature. No asynchronous interrupt hook (only `RuntimeLimits` for loops, recursion and stack), no bytecode serialisation, MSRV 1.91 |
| [Nova](https://github.com/trynova/nova) 1.0 (2026-03-15) | Rust, MPL-2.0 | ~80% (own metrics; not in the current test262.fyi run) | interpreter, data-oriented heap with a compacting safepoint GC | built for embedding; API young | **Experimental backend.** Known gaps: RegExp without lookaround or backreferences, dense-only arrays, Promise subclassing; MSRV 1.95 |
| [Brimstone](https://github.com/Hans-Halverson/brimstone) | Rust, MIT | >97% of language tests (own claim) | Ignition-style bytecode VM | none documented, no releases, "not ready for production" | Watch; revisit when it has an embedding API |
| `rquickjs` 0.14 | Rust bindings, MIT | as QuickJS-ng | — | mature (classes, interrupts, memory limits, windows-gnu) | Not used: bundles stock quickjs-ng without the fork's hooks. Its API is a good model for `js-engine` |
| V8 (`v8` crate), SpiderMonkey (`mozjs`) | C++ | 97.6%, 98.5% | JIT | mature | **Excluded**: upstream browser engine code, which the project rules keep out; neither builds for windows-gnu (MSYS2) |
| Kiesel, yavashark, Starlight | Zig / Rust | 94.6% / — / — | — | — | Not candidates: Kiesel is Zig, yavashark is early, Starlight is abandoned |

No pure-Rust engine has a JIT; every candidate is an interpreter like QuickJS.
Boa is the only pure-Rust engine with both higher conformance than QuickJS-ng
and an embedding API complete enough for a DOM.

**The `js-engine` layer.** A small crate with one backend module per engine,
chosen at build time by a Cargo feature (exactly one enabled), so calls are
static and inline — no trait objects on hot paths. It exposes what the
bindings use today, and nothing engine-specific:

- runtimes and realms (one runtime per thread; workers create their own);
- values, strings, objects, arrays, typed arrays and `ArrayBuffer` storage;
- handles that keep a value alive across calls (the node-wrapper pin of
  today's lifetime model) and weak handles (the orphan sweep);
- host classes with native data and a prototype chosen per instance (the one
  `Element` class with per-kind prototypes), brands, getters and setters,
  functions and constructors, and the receiver-aware property hooks used by
  `WindowProxy`, named properties and legacy platform objects;
- exceptions, including `DOMException`;
- the promise job queue, driven by the event loop, and host promise
  rejection tracking;
- script and module evaluation, a module loader, and an optional bytecode
  cache (a backend without serialisation leaves `bytecode_cache` unused);
- an interrupt check for the watchdog and `js_eval_budget_ms`, and a memory
  limit for `js_memory_cap_mb`.

Each backend implements the layer against its engine: QuickJS-ng and QuickJS
through `quickjs-sys`, Boa and Nova as ordinary crate dependencies. A
capability the engine lacks is emulated or reported: the Boa backend bounds
runaway scripts with `RuntimeLimits` until an interrupt hook exists upstream,
and its realms skip the bytecode cache. Southstar's own Temporal and `Intl`
implementations stay in the bindings so every backend shows the same objects;
using Boa's native ones is a later, measured choice.

**Selecting an engine.** A meson combo option `js_engine` with the values
`quickjs-ng` (default), `quickjs`, `boa` and `nova` replaces `-Dquickjs` (kept
as an alias until phase 7 ends) and maps to the `js-engine` features. Like
WebGPU and wgpu-native, an optional engine is never vendored into the tree:
Boa and Nova come from crates.io through Cargo (vendored only in release
tarballs, D4), and a default build contains no trace of them. They need a
newer compiler than the 1.85 floor (Boa 1.91, Nova 1.95), so selecting them
selects that toolchain; the default build keeps 1.85 (D3). The About page and
`--print-config` report the engine a build uses.

**Comparing engines.** Every backend is measured the same way, and the
results go into a table in `docs/` next to the WPT scoreboard:

| Measure | How |
|---|---|
| Conformance | test262 through `southstar-jsshell` (overall, language, built-ins, intl402, annexB) |
| Web platform | the tracked WPT slice (`scripts/wpt-score.sh`) once phase 7's bindings run |
| Speed | JetStream's shell runner and SunSpider/Kraken-style suites in `southstar-jsshell`; Speedometer in the browser |
| Start-up and memory | time and resident memory to create a realm with the polyfills (`data/js/*.js`) loaded; memory after loading a set of real sites |
| Cost | binary size and build time added by the backend, crates pulled in, minimum Rust version |
| Platforms | builds and smoke-runs on Linux, Windows (MSYS2 MinGW), macOS, FreeBSD, NetBSD and musl |

The shell numbers come first, from the phase 2 bake-off; the browser numbers
follow as phase 7 lands. `scripts/js-engine-compare.py` builds
`southstar-jsshell` once per engine, runs test262 (in worker processes, so a
hang or a crash costs one test, not the run) and the Octane suite, and
rewrites `docs/js-engines.md`. Whether a pure-Rust engine ever becomes the default
is decision D12, made on these numbers, not in advance.

### Phase 8 — Remaining web platform features

Scope (16.4k lines): `webgl.c` (5k), `webgpu.c` (2.9k), `wasm.c` (2.5k),
`video.c`, `video_decode.c`, `camera.c`, `mic.c`, `ext.c` (WebExtensions),
`image.c` (the decode chain and image cache).

- WebGL keeps GL ES through libepoxy (FFI) or moves to `glow`.
- WebGPU can use the `wgpu` crate directly — wgpu-native is itself a C
  wrapper around it — behind an off-by-default Cargo feature, keeping the
  runtime gate.
- Wasm: WAMR (73k vendored lines) can be replaced by `wasmi`, a pure-Rust
  interpreter with no JIT (decision D9).

Some of these can move earlier; they sit at the edge of the engine and only
depend on the JS binding style.

### Phase 9 — Vendored libraries and the end of C

What is left is third-party C. Each library gets its own decision:

| Library | Lines | Recommendation |
|---|---:|---|
| Wuffs | ~97k | Replaced in phase 2 by Rust image crates |
| WAMR | ~73k | Replace with `wasmi` (D9) |
| QuickJS-ng fork | ~101k | Keep as the default `js_engine` backend; Boa and Nova are optional backends behind the same bindings, and D12 decides on the comparison results whether a pure-Rust engine replaces it |
| lexbor | ~289k | Keep for HTML parsing and URLs unless D1 allows writing Southstar's own (an HTML5 tokenizer and tree builder plus a WHATWG URL parser) |
| pl_mpeg, minimp3 | ~6k | Keep, or move to Rust decoders (`symphonia` covers MP1/MP2/MP3 audio; MPEG-1 video has no maintained crate) |
| ns-pango, Cairo, GTK 4, libcurl, OpenSSL, SQLite, FFmpeg, SDL2, uchardet, libpsl, libseccomp | system / subproject | Keep behind FFI |

Then decide whether Cargo becomes the build entry point (D10). Meson can stay
if C libraries remain; Cargo with `build.rs` is simpler once the C is only
vendored libraries.

## 7. Engineering rules during the port

- **One module per commit** (one section per commit for the very large files,
  as for `css.c` in phase 6), behaviour unchanged, C code deleted in the same
  commit that adds its Rust replacement.
- **No redesign inside a port commit.** Better data structures, a display
  list, a Rust-native text stack — later, separately, measured.
- **`unsafe` lives in `ffi` modules and `-sys` crates.** Code above them is
  safe Rust.
- **Strings.** Inside Rust, `String`/`&str`; at the C boundary,
  NUL-terminated GLib-allocated strings as today.
- **Collections.** `Vec` and `HashMap` replace `GPtrArray`, `GArray` and
  `GHashTable` inside Rust; GLib types appear only at the boundary.
- **Threads** keep their current roles and owners; no new runtime unless D7.
- **Performance budget.** Check layout, paint and script timings on every
  core port; bounds checks and reference counting in hot loops are the usual
  regressions.

## 8. Verification

The project has no test suite and this plan does not add one. Every port is
checked with what already exists:

| Check | Tool | Bar |
|---|---|---|
| Build | `meson compile -C builddir`, clippy, rustfmt | No warnings, every CI platform |
| Smoke | `scripts/dev.sh smoke` | All fixtures match `data/baseline/` |
| Rendering | `scripts/render-tests.sh` (53 pages, headless PNG dumps) | Pixel-identical to the build of the previous commit (JPEG decoder swap excepted, phase 2) |
| Web platform | `scripts/wpt-score.sh` | Slice score not lower (70,743 of 71,907 subtests on the 2026-07-13 slice) |
| JavaScript | `scripts/test262-run.sh` | Not lower |
| Performance | `scripts/speedometer-bench.sh`, `speedometer4-bench.sh` | Within noise |
| Manual | Launch the browser, exercise the changed path | As `CLAUDE.md` requires |

The rendering check needs one small addition: a script that dumps the render
tests from two builds and compares the PNGs. That is a comparison script like
the existing ones, not a test suite.

## 9. Dependency map

| Today (C) | During the port | Later option |
|---|---|---|
| GTK 4 | `gtk4-rs` (shell only) | — |
| Cairo | `cairo-sys-rs` | `tiny-skia` or a GPU rasterizer, after a display list exists |
| ns-pango | own `ns-pango-sys` | `rustybuzz` + `swash` (a text-stack redesign) |
| libcurl | `curl` crate | — |
| libnghttp2, ngtcp2, nghttp3, gnutls | FFI (D7) | `h2`, `quinn` + `h3` |
| OpenSSL (WebCrypto) | `openssl` crate | — |
| lexbor (HTML, URL) | own `lexbor-sys` | Southstar's own parser (D1) |
| QuickJS-ng fork | own `quickjs-sys`, behind `js-engine` | Boa (`boa_engine`) or Nova (`nova_vm`) as the backend (D12) |
| Wuffs, libwebp | `png`, `gif`, `zune-jpeg`, `image-webp` | — |
| libavif | FFI | — |
| WAMR | `wasmi` (D9) | — |
| SQLite | `rusqlite` | — |
| uchardet | FFI | Own detector (D1 excludes `chardetng`) |
| libpsl | FFI | `psl` crate |
| libseccomp, Landlock | `seccompiler` or `libseccomp`, `landlock` | — |
| SDL2 | `sdl2` crate | `cpal` |
| FFmpeg | `ffmpeg-sys-next` | — |
| pl_mpeg, minimp3 | FFI | `symphonia` (audio) |
| Enchant | FFI | — |
| libepoxy | FFI | `glow` |
| wgpu-native | `wgpu` (feature, off by default) | — |
| GLib | `glib-sys` at boundaries, `glib` for the event loop | Own event loop |

## 10. Risks

| Risk | Mitigation |
|---|---|
| js.c (66.6k lines, 30% of the project) is ported last, so the biggest work lands at the end | Phase 2 pilot settles the binding style early; phase 7 is split by js.c's own subsystems |
| `#[repr(C)]` structs drift from what C expects | cbindgen-generated headers and size/offset assertions on both sides (§5.3) |
| DOM wrapper lifetime bugs (use-after-free, leaks) during the mixed period | Keep today's pin/invalidate/orphan-sweep model unchanged until phase 7 is done |
| The sandbox denies a syscall Rust code needs (it returns `EPERM`, so this shows up as a failing call) | The allowlist already covers `std`'s needs; every port is smoke-run under the real sandbox |
| Toolchain friction on MSYS2, Alpine/musl, FreeBSD, NetBSD | Phase 0 pilot must ship on every platform before any real module moves |
| Distro compilers older than crate MSRVs | D3; `rust-toolchain.toml`; crate versions chosen against the MSRV |
| Dependency creep | `cargo-deny` licence and source policy; new crates need a reason in the commit message |
| Performance loss (bounds checks, `RefCell`, extra copies at FFI boundaries) | Speedometer and layout timings on every core commit |
| The engine-neutral layer becomes a lowest common denominator or slows the default engine | Static dispatch to one backend; the layer is sized by what the bindings use, measured on QuickJS-ng first; a capability only one engine has is emulated in the others, not dropped |
| An optional engine lacks a hook the browser needs (asynchronous interrupts in Boa, receiver-aware property access, mature RegExp in Nova) | Found by the phase 2 bake-off; emulate in the backend or report upstream; an optional backend may ship with documented gaps |
| A long mixed-language period slows everything else | Phases 1–5 are independent enough to interleave with feature work; the core and bindings phases are the ones to schedule deliberately |

## 11. Decisions needed

| # | Decision | Recommendation |
|---|---|---|
| D1 | Do Servo/Firefox-origin crates (`html5ever`, `cssparser`, `selectors`, `url`, `encoding_rs`, `chardetng`) count as "upstream browser engine code"? | Yes — keep them out; keep lexbor and uchardet behind FFI |
| D2 | Incremental port in place (this plan) or a clean rewrite in a new tree? | Incremental: the C browser already passes 98% of the tracked WPT slice, and a rewrite would have to reach that bar again before it could replace anything |
| D3 | MSRV: 1.85 with `gtk4` 0.10, or ≥ 1.92 with newer gtk-rs and backported distro compilers? | 1.85 — every build environment above can provide it; revisit when the shell port (phase 4) needs GTK 4.22 APIs |
| D4 | Commit vendored crates (`cargo vendor`) to the repo, or ship them only in source tarballs (with OBS's `cargo_vendor` service for the openSUSE build)? | Commit them under `vendor/`: it matches "vendored in-tree", keeps the offline Debian and OBS builds simple, and the dependency budget keeps it small |
| D5 | Keep the no-comments rule in Rust, including no `// SAFETY:` comments? | Keep it; confine `unsafe` to `ffi` modules |
| D6 | May ported Rust carry `#[test]` unit tests? | No, per the existing rule; parity checks in §8 |
| D7 | Alternative HTTP backend: keep libnghttp2/ngtcp2 via FFI, or move to `h2`/`quinn` (needs an async runtime)? | Keep FFI during the port |
| D8 | JS binding style: declarative macros/tables, or WebIDL-driven generation? | Either targets `js-engine`, not an engine; decide after the phase 2 pilot, with a lean toward WebIDL generation now that several backends must be served |
| D9 | Replace WAMR with `wasmi`? | Yes, in phase 8 |
| D10 | After the port: Cargo or meson as the build entry point? | Cargo, once only vendored C remains |
| D11 | Which JavaScript engines get backends? | QuickJS-ng (default) and Bellard's QuickJS, Boa (optional), Nova (optional, experimental). Not V8 or SpiderMonkey (upstream browser engines), not `rquickjs` (lacks the fork's hooks) |
| D12 | Should a pure-Rust engine become the default? | Not decided in advance: decide from the comparison table once phase 7's bindings run on both QuickJS-ng and Boa |

## 12. Tracking

Progress is tracked in this file: every ported module is listed in the
table below, and `Changelog.md` gets an entry per phase.

### JavaScript engine bake-off

The first `js-engine` layer (`rust/js-engine`) has the QuickJS-ng backend (the
in-tree fork, linked from meson's `libqjs`) and the Boa 0.22 backend, and
`southstar-jsshell` (`rust/jsshell`) runs test262 and Octane on both; results
are in `docs/js-engines.md`.

The pilot binding is done: Temporal (`js_date.c`) is `rust/js-temporal`,
written only against `js-engine`, and is the first Rust binding in the
browser. The layer grew the primitives a binding needs (host objects carrying
Rust data, constructors callable with or without `new`, `prototype` and
`constructor` wiring, property attributes, `Symbol.toStringTag`, numeric and
BigInt conversions, Range and Type errors) and an entry point that wraps a C
`JSContext*`, and the QuickJS backend serves Bellard's engine through the
`quickjs-original` feature. The same Temporal code runs unchanged on Boa in
`southstar-jsshell --temporal`. On D8, hand-written natives over a small
registration table read close to the C and were enough here; the next binding
files will show whether WebIDL generation pays for itself.

The WebExtensions host (`ext.c`, now `rust/extensions`) is the second user of
the layer. It needed JSON parsing, indexed property reads, ToBoolean, number
and boolean checks and a string's raw bytes, which both backends now provide;
manifests and rule files are parsed by a private QuickJS runtime with the C's
1 MiB stack budget, so the same JSON is accepted and the same values are read
from it.

Intl (`js_intl.c`, now `rust/js-intl`) is the second binding written only
against the layer. The layer gained arrays, objects with a given prototype,
indexed writes, ToNumber, `new` on an arbitrary constructor and native
functions that carry captured values (Intl's bound `format` and `compare`).
Collation, normalization and case folding stay with GLib and word, sentence and
grapheme breaks with ns-pango, called over FFI, and the C library's `%.*f` and
`%g` output is reproduced exactly, so every formatted string is unchanged.

Offline Web Audio rendering (`webaudio.c`, now `rust/webaudio`) reads the
AudioNode graph the bindings build through the layer, which gained a borrowed
view of a typed array's bytes, element size and offset (the same
`JS_GetTypedArrayBuffer` and `JS_GetArrayBuffer` calls the C made on QuickJS,
`JsTypedArray` on Boa). Every property is read in the C's order, so getters
observe the same sequence, and the DSP gives bit-identical samples.

### Ported

| Module | Lines | Crate | Phase |
|---|---:|---|---|
| `datetime.c` | 150 | `rust/datetime` | 0 |
| `bookmarks.c` | 153 | `rust/bookmarks` | 2 |
| `csp.c` | 500 | `rust/csp` | 2 |
| `css_syntax.c` | 475 | `rust/css-syntax` | 2 |
| `i18n.c` | 120 | `rust/i18n` | 2 |
| `safebrowsing.c` | 253 | `rust/safebrowsing` | 2 |
| `debuglog.c` | 192 | `rust/debuglog` | 2 |
| `woff2.c` | 721 | `rust/woff2` | 2 |
| `history.c` | 385 | `rust/history` | 2 |
| `bytecode_cache.c` | 259 | `rust/bytecode-cache` | 2 |
| `config.c` | 566 | `rust/config` | 2 |
| `spellcheck.c` | 132 | `rust/spellcheck` | 2 |
| `webcrypto.c` | 1,248 | `rust/webcrypto` | 2 |
| `threaddump.c` | 237 | `rust/threaddump` | 2 |
| `glctx.c` | 350 | `rust/glctx` | 2 |
| `mat4.h` | 144 | `rust/mat4` | 2 |
| `image_ico.c`, `image_webp.c` | 377 | `rust/image-decoders` | 2 |
| `ipc_http.c` | 599 | `rust/ipc` | 3 |
| `renderer_http.c` | 313 | `rust/renderer-host` (the renderer's `main`) | 3 |
| `renderer_serve.c` | 1,240 | `rust/renderer-host` | 3 |
| `rproc_http.c` | 1,822 | `rust/renderer-client` | 3 |
| `rproc_inproc.c` | 240 | `rust/renderer-host` | 3 |
| `renderer_tiles.c` | 439 | `rust/renderer-host` | 3 |
| `cache.c` | 788 | `rust/http-cache` (SQLite through the shared `rust/sqlite`) | 5 |
| `eventsource.c` | 438 | `rust/eventsource` | 5 |
| `html.c` | 829 | `rust/html-util` | 6 |
| `mic.c` | 144 | `rust/mic` | 8 |
| `pdf.c` | 172 | `rust/pdf` | 8 |
| `texture.c` | 113 | `rust/texture` | 6 (paint) |
| `ws.c` | 708 | `rust/websocket` | 5 |
| `netutil.c` | 241 | `rust/netutil` | 5 |
| `css_media.c` | 1,339 | `rust/css-media` | 6 (style) |
| `css_prop_syntax.c` | 1,295 | `rust/css-prop-syntax` | 6 (style) |
| `security.c` | 1,107 | `rust/sandbox` (and `rust/helper-ffi` for the media helpers) | 1 |
| `js_date.c` | 1,531 | `rust/js-temporal` | 2 (JavaScript pilot) |
| `watchdog.c` | 583 | `rust/watchdog` | 4 |
| `ext.c` | 1,265 | `rust/extensions` | 8 (WebExtensions) |
| `js_intl.c` | 2,433 | `rust/js-intl` | 7 (JavaScript bindings) |
| `camera.c` | 393 | `rust/camera` | 8 |
| `xml.c` | 456 | `rust/xml` | 6 (DOM and parsing) |
| `forms.c` | 320 | `rust/forms` (reads the C DOM through `rust/dom`, a `#[repr(C)]` prefix of `ns_node` with borrowed node handles) | 6 (DOM and parsing) |
| `font.c` | 597 | `rust/font` (WOFF2 through `rust/woff2` directly) | 6 (paint and text) |
| `webaudio.c` | 351 | `rust/webaudio` | 7 (JavaScript bindings) |
| `mathml.c` | 560 | `rust/mathml` (DOM through `rust/dom`) | 6 (layout) |
| `image.c` | 909 | `rust/image` (`ns_image` stays a `#[repr(C)]` struct the C reads and writes) | 8 |
| `html_lexbor.c` | 852 | `rust/html-parser` (lexbor through its exported `_noi` functions and mirrored node structs) | 6 (DOM and parsing) |
| `dom.c` | 3,028 | `rust/dom` (ported in five sections; the node and attribute memory stays GLib's and the document indexes stay GLib tables, so the C that reads `ns_node` keeps working) | 6 (DOM and parsing) |
| `idb.c` | 1,219 | `rust/idb` (SQLite through `rust/sqlite`, values in QuickJS's object serialization as before) | 5 |
| `print.c` | 218 | `rust/print` (the box tree through `rust/layout`, a `#[repr(C)]` mirror of `ns_box` with borrowed box handles) | 6 (layout) |
| `selection.c` | 546 | `rust/selection` (boxes through `rust/layout`) | 6 (layout) |
| `svg.c` | 2,542 | `rust/svg` (computed styles through the new shared `rust/style`) | 6 (layout) |
| `headless.c` | 2,227 | `rust/headless` (ported in two sections; its standard output still goes through the C runtime's stdout buffer, so it stays ordered with what C writes there) | 4 |
| `engine.c` | 1,500 | `rust/engine` (ported in three sections; style sheets stay C `ns_css_stylesheet`s collected into GLib pointer arrays, and the render context and profile are `#[repr(C)]` mirrors asserted on both sides) | 6 (pipeline driver) |
| `libsouthstar.c` | 4,472 | `rust/browser` (ported in three sections; `struct ns_browser` became a Rust struct with C's field layout and `Cell` fields, so the script callbacks that re-enter a page while it relays out stay sound) | 6 (pipeline driver) |
| `net.c` | 6,925 | `rust/net` (ported in nine sections; `ns_response` stays a `#[repr(C)]` struct the C reads, transfers still run on libcurl's easy and multi handles, the HTTP cache is called through `rust/http-cache` directly, and `net_http2.c` stays C behind the `ns_hop_transport` seam, chosen by the `http-nghttp2` Cargo feature) | 5 |
| `render.c` | 737 | `rust/render` (the render context and profile mirrors moved here from `rust/engine`; computed styles are compared through full `ns_style` and `ns_css_value` mirrors in `rust/style`, whose layouts `css.h` now asserts) | 6 (paint and text) |
| `anim.c` | 2,049 | `rust/anim` (values stay css.c's refcounted `ns_css_value`s, held through handles that dup and free them so the engine's pointer-identity checks behave as before; per-element state is kept in node-address order rather than GLib's pointer-hash order, which already differed from run to run) | 6 (style) |

### Being ported

| Module | Sections so far | Crate |
|---|---|---|
| `css.c` | the colour parser (hex, named and system colours, `rgb()`, `hsl()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color-mix()`, `light-dark()`, `calc()` inside them); lengths and their units, `calc()` and the other math functions, and their canonical serialization; container queries, with the container map, the container stack the cascade pushes and pops, and container units; gradients, `<position>` values, `image-set()`, the `content` property, `unicode-range` and the colour text values serialize to; transforms, `transform-origin` and the `translate`, `rotate` and `scale` properties (the values stay C `ns_css_value`s, built through a `#[repr(C)]` mirror css.h asserts; `src/css_internal.h` declares the Rust functions only css.c calls) | `rust/css` |
