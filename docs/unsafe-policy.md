# Unsafe code policy

Southstar is written in Rust so that memory safety is checked by the
compiler, not by review. Every `unsafe` block gives that up for the code
inside it, so `unsafe` is allowed only where the browser genuinely has to
leave safe Rust, and the amount of it may only go down.

## Where `unsafe` is allowed

1. **System-library FFI.** Calls into the C libraries the browser links:
   GLib/GTK/Cairo/Pango, OpenSSL (TLS and WebCrypto), FFmpeg's libav\*,
   SDL2, zlib/brotli/zstd, libwebp/libavif, SQLite, libseccomp,
   epoxy/OpenGL, wgpu-native, uchardet, libpsl and enchant. The
   declarations and the calls live in the crate's `ffi.rs` or `ffi/`
   module, which hands the rest of the crate a safe API: owned Rust types
   in, owned Rust types out, no raw pointer crossing the module boundary.
2. **The embedding API.** The `ns_*` functions `src/libsouthstar.h`
   declares for C embedders, and callbacks the C libraries above call back
   into (GTK signal handlers, GSource callbacks, libav I/O callbacks).
3. **The JavaScript binding layer.** `rust/js-engine` and the Boa fork,
   where DOM objects meet the garbage collector. The aim is to replace raw
   pointers stashed in JS objects with Boa's `Trace`/`Finalize` and GC
   handles, so this category shrinks too.
4. **Operating-system primitives** that have no safe standard-library
   wrapper: process spawning and the seccomp sandbox (`rust/sandbox`),
   signals and the watchdog, shared memory and `mmap` (`rust/media-shm`,
   the renderer framebuffer). Shared memory is wrapped in a type that
   checks every offset and length against the mapping's size.
5. **SIMD intrinsics**, behind a safe function with a scalar fallback.

## Where it is not allowed

- **Code that reads untrusted input**: the HTML, CSS, URL, HTTP/1.1,
  HTTP/2/HPACK, WebVTT and IPC parsers, and the image, font and media
  decoders. These crates carry `#![forbid(unsafe_code)]`, or are moving
  there; a raw pointer in a parser is the bug class this rewrite exists
  to remove.
- **Engine logic**: style, the cascade, layout math, painting decisions,
  the HTTP cache, cookies, HSTS, CSP, history and bookmarks.
- **Calls between Southstar's own crates.** Crates that still talk
  through `ns_*`/`nsr_*` symbol exports (`#[unsafe(no_mangle)]` plus an
  `extern` block in the caller) do so because modules were ported from C
  one at a time. That internal C ABI is scaffolding to remove: a new
  cross-crate call is an ordinary Rust function reached through a Cargo
  dependency, with typed arguments.
- `static mut`. Use `OnceLock`, `LazyLock`, `thread_local!`, atomics, or
  pass a context value.
- `transmute` between pointer types, and `as` casts between pointer types
  (clippy's `transmute_ptr_to_ptr` and `ptr_as_ptr` are denied); use
  `.cast()`.

A crate with no `unsafe` at all carries `#![forbid(unsafe_code)]` so it
stays that way.

## Rules for every `unsafe` block

- One unsafe operation per block (`clippy::multiple_unsafe_ops_per_block`).
- A `// SAFETY:` comment directly above it saying which invariant makes
  that operation sound — not what the code does
  (`clippy::undocumented_unsafe_blocks`). This is the one comment the
  no-comments rule asks for.
- An `unsafe fn` documents its caller's obligations in a `# Safety` doc
  section (`clippy::missing_safety_doc`). Prefer a safe function that
  checks its input over an `unsafe fn` that trusts it.
- Keep the block in the `ffi` module and as small as the operation; the
  logic around it is safe Rust.

## Measuring it

`scripts/unsafe-budget.py --clippy` counts, for every crate, the `unsafe`
keywords, the functions imported through `extern` blocks, the
`no_mangle`/`export_name` exports, `static mut` items and the hits of the
three documentation lints above, and compares them with
`rust/unsafe-budget.tsv`. CI runs it on every push and pull request and
publishes the crates that changed in the job summary.

- A count above its budget fails the build.
- A count below its budget also fails until the budget is lowered with
  `scripts/unsafe-budget.py --clippy --update` in the same commit, so the
  file only ever moves down.
- Raising a budget line needs an `Unsafe-Budget-Increase: <reason>`
  trailer in a commit of the push or pull request.

## Review

A change that adds an `unsafe` block — every budget increase — needs a
reviewer to read its `SAFETY` argument against the code it guards, and
to check that the unsafety could not have been avoided with a safe API
or by moving the code into an existing `ffi` module. Removing `unsafe`
needs no such ceremony: lower the budget and say what replaced it.
