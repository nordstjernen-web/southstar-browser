# Southstar — Development Plan

Living plan for a web browser written from scratch in **Rust**,
small enough for one person to audit end-to-end. It began in C and was
ported module by module (`docs/rust-port.md`); the port is complete and the
only C file left is `src/libsouthstar.h`, the embedding API's header. The
engine and all non-toolkit logic are crates of one Cargo workspace under
`rust/`; the GUI is a thin **GTK 4** frontend (`rust/shell`) over them and
the in-tree HTTP client (`rust/http`).
No upstream engine (Gecko / WebKit / Blink / Servo) is read, ported, or imported.
See `README.md` for the product vision and `CLAUDE.md` for working rules.

## Principles

- **One competent human's worth of code** — when work balloons, cut
  scope, not corners. A working subset beats an unfinished superset.
- **Vertical slices that ship** — every task ends in something runnable.
- **Few small auditable deps**, in-tree or pure Rust: the Boa JavaScript
  interpreter (a patched fork in `third_party/`, `docs/boa-fork.md`), rustls
  over ring for TLS, the png/gif/zune-jpeg crates for image decode, and the
  pure-Rust wasmi interpreter for WebAssembly. `deny.toml` and `cargo vet`
  keep the tree small and audited.
- **Tests where they pay** — unit tests for parsers and engine logic
  (`scripts/rust-tests.sh`), conformance tracked by test262
  (`southstar-jsshell test262`) and the WPT scoreboard
  (`docs/wpt-scores.md`); everything else is verified by running the
  browser.
- No JIT, therefore more secure. However, this means the rest of the browser
  engine needs to be super-fast.
- **`unsafe` only at real boundaries** — system FFI, the embedding API, the
  JS/GC binding layer, OS primitives and SIMD, never in parsers or engine
  logic (`docs/unsafe-policy.md`, budgeted per crate in
  `rust/unsafe-budget.tsv`).
- **No code comments** beyond one header block per file (see `CLAUDE.md`).

## Non-goals (won't change)

WebRTC / WebUSB / WebBluetooth / WebMIDI / WebHID;
service-worker push and background sync (registration, lifecycle, and
`FetchEvent` network interception via `respondWith` *are* supported —
`ns_sw_post_fetch_request` routes `fetch()` through the worker);
DRM / EME; **JIT** (Boa is an
interpreter only — W^X holds process-wide); plugins (NPAPI / PPAPI /
WebExtensions); sync / accounts / telemetry /
"studies"; AI-style web APIs.

WebGL is the standard exception to the no-GPU-APIs stance: a minimalist
WebGL 1 / 2 implementation mapped directly onto OpenGL ES (`rust/webgl`),
enabled by default with a global Settings toggle and a visible activity
indicator.

WebGPU (`navigator.gpu`) is an **experimental** feature layered on the
external [wgpu-native](https://github.com/gfx-rs/wgpu-native) library. The
`webgpu` build feature is `auto`: it is compiled in whenever wgpu-native is
present and skipped otherwise, so a machine without the library still gets a
WebGPU-free binary. At runtime it stays off until the browser is started with
`--enable-webgpu` (equivalently `NS_WEBGPU_ALLOW=1`).
wgpu-native is a large dependency that does not fit the minimalism the rest of
the engine is built around, so it is never made a hard or default dependency.

## Current state

A usable HTML5 + modern-CSS + modern-JavaScript browser. `rust/html5` parses
to a DOM (`rust/dom`); the engine does the CSS cascade and selectors,
block/inline/flex/grid/table/multicol/float/positioned layout, and Cairo +
Pango paint (text shaped through the `ns-pango` fork; gradients incl.
`repeating-*` and gradient masks, filters, transforms, transitions /
`@keyframes`). JavaScript runs on Boa and passes all of test262 (52,133 of
52,133 outside `test/staging`), including ES2025/ES2026 and late-stage
proposals such as explicit resource management, iterator chunking and joint
iteration, immutable ArrayBuffers, decorators, `ShadowRealm`, `import defer`,
source phase imports and Temporal; `Intl` is the ICU-free `rust/js-intl`. Bindings
(`rust/js-*`) cover DOM, Shadow DOM, `fetch`/XHR, canvas 2D, storage
(`localStorage`/`sessionStorage`, IndexedDB over SQLite),
`WebSocket`/`EventSource`, dedicated and module workers, service workers, the
Resize/Intersection/Mutation observers, and `crypto.subtle` (WebCrypto over
OpenSSL libcrypto, `rust/webcrypto`); forms support submission and
constraint validation; `overflow` boxes scroll. The full `WebAssembly` JS API
runs on wasmi (`rust/js-wasm`), and WebGL 1 / 2 maps onto OpenGL ES. PNG/APNG,
GIF and animated WebP play; MPEG-1 and, with libav, WebM (VP9/VP8 +
Opus/Vorbis) play inline (`docs/media.md`). Restyles are incremental and
painting skips off-screen boxes (viewport culling). The tracked WPT slice
passes 95% of its subtests.

Runs on Linux, Windows (MSYS2) and macOS. CI builds Linux on every push and
pull request, FreeBSD and NetBSD nightly, and Windows, macOS and musl on
dispatch. The GTK frontend is a tabbed, **process-per-tab** browser:
each tab drives its own sandboxed renderer process over the engine and
shows full-fidelity output. The shell is a thin display/input client —
it carries no bespoke in-process renderer (the optional
`--single-process` mode runs the same renderer on a thread). The
`about:start` new-tab page provides a compact search-focused home page.

The release version lives in the meson project definition.

## Architecture & frontends

The codebase is layered so the GUI stays thin and the engine stays
toolkit-agnostic:

- **`rust/` — the engine crates.** HTML parse (`html5`, `html-parser`),
  DOM (`dom`), CSS (`css`, `style`), layout (`layout-*`), Cairo/Pango paint
  (`paint`, `render`), image decode (`image`, `image-decoders`), URL parsing
  (`url`), networking (`http`, `http-wire`, `net`), JavaScript bindings
  (`js-*` over the `js-engine` API) and media (`video`, `plmpeg`,
  `minimp3`). Frontend-agnostic pieces serve alternative GUIs: the headless
  driver (`headless`) and the `ns_browser` embedding API (`browser`,
  declared in `src/libsouthstar.h`, linked as `libsouthstar`). Crates call
  each other through ordinary Rust APIs; the `ns_*` C ABI left over from
  the port is being removed, and `extern "C"` stays only at the real
  boundaries (the embedding API, GTK/Cairo/Pango, FFmpeg, OpenSSL, SDL and
  the other system libraries).
- **`rust/shell` — GTK 4 frontend.** A thin process-per-tab
  shell (`procwindow`/`procview` over the renderer IPC) that spawns one
  sandboxed `southstar-renderer` process per tab and blits its framebuffer.
  It carries the browser chrome — navigation, tabs, history, zoom,
  selection, `:hover`, find-in-page, a context menu, the DevTools console,
  save/export, the media helper plumbing, an app menu, settings, and
  bookmarks.

**Process-per-tab renderer boundary — shipped.** Each tab runs in its own
sandboxed *process* that runs the engine and produces a rendered surface;
the GUI process is reduced to hosting a widget per tab, blitting the
shared-memory framebuffer, and forwarding input over the IPC
protocol (render/viewport, click/key/hover/select, find, export, media,
console/eval). The GTK shell is a thin display-plus-input layer showing the
engine's output (so it needs no separate renderer), and isolation is
real — `southstar-renderer` applies Landlock + seccomp confinement at
startup (`ns_browser_sandbox`, called right after `ns_browser_init` and
before any page is opened), so a renderer crash is a per-tab failure and
untrusted content always runs under a loaded syscall filter. The shell
parses no untrusted bytes but must `fork`/`execv` renderers and create POSIX
shm, so it runs under a widened Landlock with seccomp skipped. The mechanics
live in `rust/sandbox`; `SECURITY.md` describes the boundary.

**The plan from here:**

1. **Browser-process broker services** (networking, cookies, cache,
   storage) so the renderer can be credential-less rather than fetching and
   persisting on its own — the true security payoff.
2. A shell-specific seccomp profile (or a renderer zygote) and re-enabled
   shell watchdog supervision.
3. **Retire the internal C ABI** — replace the remaining `ns_*`/`nsr_*`
   exports between crates with Rust APIs, lowering `rust/unsafe-budget.tsv`
   as they go.

## Priorities

**Now**
- **18 · CSS cascade performance** — the biggest "feels slow" lever.
  Incremental restyle shipped (2026-06-26): only dirtied subtrees are
  re-cascaded and clean ones clone their previous style
  (`NS_NO_INCR_RESTYLE=1` forces the full cascade). Next: a conservative
  computed-style sharing cache between siblings, and keeping the
  incremental path for pages that use `:has()` or `@container`, which still
  fall back to a full cascade. Land as small, measured commits — profile
  before/after; this area regresses easily (prior per-element class-hashset
  attempts both regressed).
- **20 · test262 to 100%** — done: 52,133 of 52,133 outside
  `test/staging` (`southstar-jsshell test262`). Keep it there: run the
  suite after engine changes.

**Next**
- **19 · Incremental paint** — viewport culling shipped; the real win is
  dirty-region partial repaint (re-rasterize only changed rects), which
  also unblocks animated smooth scrolling. A retained full-document
  surface cache was tried and reverted (re-rastered the whole page on
  every change); smooth wheel scrolling was reverted for the same reason
  (full repaint × ~15 ease-frames saturated CPU). Both need dirty rects.
- **14 · YouTube watch-page playback** — baseline shipped: a streaming
  `<video>` (MSE/`blob:`, no file URL) plays *inline* — the renderer
  materializes the growing stream and the `southstar-video` helper
  (`rust/video-helper`, built when libav is present) decodes frames
  into a shm ring the shell composites over the page (see
  `docs/media.md`). Next: widen codec/site coverage and surface clearer
  in-page feedback when the helper is unavailable.
- **8 · Sign the Windows build** — biggest distribution-side ROI. Wire
  signtool + timestamp now; flip on once a cert is procured.

**Later**
- 5 · Reader mode · 9 · macOS notarized DMG.

## Future focus areas

Candidate directions once the performance work above lands — none
committed, listed to keep the long view in one place:

- **Layout & paint throughput** — the dirty-region partial repaint in
  *Next* is the headline win; it unblocks smooth animated scrolling, which
  has been reverted twice for want of it. Treat it as the gating
  performance work.
- **Frame-loop idling / animation throttling** *(partially landed)* —
  quiet-page idling shipped: the renderer reports `X-Anim` from
  `ns_browser_animating`, the shell stops requesting frames once a page
  is quiet (6a519a6, 6d598fc), and a tick that ran no work reuses the
  previous frame (`X-Unchanged`, staged-surface reuse in the shell) —
  safe because it skips only when *nothing* ran, unlike the reverted
  coarse `frame_dirty` flag that skipped after no-op rAFs and caused
  judder; don't re-attempt that version. The reflow-loop dampener now
  *defers* suppressed relayouts instead of discarding them (51659ba) —
  pending JS work must always count as animating or the page freezes.
  What remains is the rAF-pinned SPA case (the full `duckduckgo.com`
  spinner still burns CPU because its rAF genuinely runs every frame):
  that needs per-region paint invalidation (the dirty-region work above)
  and, for transform/opacity spinners, compositor-thread animation — a
  larger architectural effort. The re-entrant-relayout leak was a
  distinct bug, fixed in 80ca0d5.
- **Worker maturity** — dedicated, module and service workers run;
  `SharedWorker`, worklets, transferable `MessagePort`s and nested workers
  do not yet, and structured clone and transferables need rounding out so
  wasm-bindgen + worker bundles run unmodified.
- **`iframe` rendering** — `src`/`srcdoc` load and `sandbox` is parsed and
  enforced, but nested document rendering is still limited. Real
  child-document layout/paint would close one of the larger remaining HTML
  gaps.
- **Accessibility** — the chrome labels its controls, but no accessibility
  tree is exposed for page content yet; a minimal accessible-name + role
  surface would be a high-value, self-contained slice.
- **WebGL extensions** — `getExtension()` implements
  `WEBGL_debug_renderer_info` and `EXT_texture_filter_anisotropic` and
  returns `null` for the rest; a small allow-list of further widely-used,
  safe extensions (e.g. instancing, `OES_*` float textures) would
  unblock more content without re-architecting `rust/webgl`.
- **Packaging reach** — a signed Windows build and a notarized macOS
  DMG are the distribution-side levers.

**Done:** process-per-tab renderers behind the IPC +
shared-memory-framebuffer boundary (the GTK shell is a thin display
client — see *Architecture & frontends*),
the C-to-Rust port (`docs/rust-port.md`),
6 APNG and animated-WebP playback,
10 embeddable `libsouthstar` (built and header-installed
from meson, `src/libsouthstar.h`),
15 Debian/Ubuntu `.deb` packaging (`scripts/pack-deb.sh`, built nightly by
`scripts/nightly.sh`),
the incremental restyle half of 18.

**Ongoing — security hardening passes.** Recurring source audits of the
attacker-reachable surface — network parsing, cookie scoping, layout
allocation, the renderer/IPC boundary — with each fix landed as its own
commit (the running detail lives in the git history). The fundamentals stay
strong: memory-safe Rust parsers and TLS (rustls, with hybrid post-quantum
key exchange), http/https-only scheme allow-listing with no HTTPS→HTTP
redirect downgrade, public-suffix cookie scoping via libpsl, CRLF-filtered
request headers, credentials never replayed across a redirect host,
clamped/overflow-checked sizes on the untrusted IPC and layout paths,
depth-limited recursive CSS parsers, and per-renderer Landlock + seccomp
confinement applied before any page opens.

**Mostly done / partial:** 12 downloads under Landlock (path 2),
13 fragment navigation, 16 silent-stub honesty, 17 GTK renderer
selection.

## How we work

meson + ninja build the Cargo workspace (`rust/meson.build`); `cargo check`
and a `-Dbuildtype=debug` tree keep the edit loop fast. `cargo fmt`,
`scripts/unsafe-budget.py --clippy`, `cargo deny` and `cargo vet` stay
clean. Build and smoke-launch locally before pushing — the local machine is
the build and run oracle. Commit small; push logical units to `origin/main`
as they land, with a `Changelog.md` entry.
