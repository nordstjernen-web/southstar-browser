# Security policy

Southstar is a small independent web browser. Security fixes ship from
`main`; only the latest tagged release is supported.

## Reporting

Report security issues by e-mail to: andreas.rosdal@hotmail.com

Please include the version (shown in the About Southstar dialog), your OS,
a minimal reproducer (a URL or self-contained HTML) and your assessment of
the impact.

## Threat model

The attacker controls everything fetched from the network: HTML, CSS,
JavaScript, images, fonts, media and PDFs. The user, the kernel and the
local file system outside the sandbox are trusted.

**In scope**

- Memory-safety bugs in Southstar's `unsafe` Rust (`docs/unsafe-policy.md`
  says where it is allowed) and panics reachable from web content.
- Escapes from the Linux renderer sandbox (Landlock and seccomp).
- Same-origin policy, CORS, cookie and cache partitioning bypass.
- HSTS, mixed-content and CSP bypass.
- Address-bar spoofing.
- A web page reaching the remote-control listener or the embedding protocol.

**Out of scope**

- Bugs in third-party code: GTK, FFmpeg, poppler, HarfBuzz, FreeType,
  libavif, uchardet, OpenSSL libcrypto, GPU drivers and the crates.io
  dependencies. Report those upstream; Southstar updates when fixes ship.
- CPU side channels (Spectre class).
- Attacks that already need code execution as the same user.
- Anything that needs a debugging switch (`NS_NO_SANDBOX` and the like).

## Architecture

- **One renderer process per tab.** `southstar-renderer` parses and runs all
  untrusted content and holds the strongest sandbox. The GTK shell displays
  the renderer's shared-memory framebuffer and forwards input. A tab keeps
  its renderer across sites: there is no per-site process isolation, and the
  renderer does its own networking and storage.
- **Media helpers.** Audio is decoded and played by `southstar-audio`, MSE
  video by `southstar-video`; other video decodes in the renderer.
- **No JIT.** JavaScript runs in Boa (an interpreter, forked in
  `third_party/boa_engine`) and WebAssembly in wasmi (an interpreter), so
  the browser generates no machine code.
- **A fresh JavaScript runtime for every page load.** Nothing in script
  memory survives a navigation. A script that runs past its budget (60 s by
  default) is halted.
- **Frames are realms of one engine.** A cross-origin frame, or a sandboxed
  one without `allow-same-origin`, has its own built-ins and reaches its
  parent, top and child windows only through a proxy made of native
  functions (`postMessage`, navigation, `closed`, `length`, `close`,
  `focus`, `blur`, `frames`); page script never holds the other origin's
  window. This boundary stops script. It does not contain a memory-safety
  bug, because all frames of a tab share one process.

## Process confinement

| | Renderer | Shell | Media helpers |
|---|---|---|---|
| Linux | Landlock, seccomp with argument filters | Landlock (wider), no seccomp | Landlock (wider), seccomp without argument filters |
| macOS | Seatbelt, writes only | Seatbelt, writes only | none |
| Windows | mitigation policies, no child processes | mitigation policies | none |

### Linux (`rust/sandbox`)

Both layers are installed before the renderer handles its first request.
`PR_SET_NO_NEW_PRIVS` is set, and the browser refuses to start as root.

- **Landlock, renderer.** Read access to the system directories (`/usr`,
  `/lib`, `/etc`), `/proc`, `/sys`, `/run`, fonts, themes and
  `~/.config/southstar`. Write access only to
  `~/.config/southstar/cookies`, `~/.local/share/southstar`,
  `~/.cache/southstar` and `$XDG_RUNTIME_DIR/southstar`. The configuration
  file, bookmarks and the local block list are read-only, so a compromised
  renderer cannot change settings. No access to the rest of `$HOME`
  (`~/.ssh`, documents, other applications' data), to the Downloads
  directory, to `$XAUTHORITY` or to `/dev/snd`. No directory is both
  writable and executable. The GPU device nodes are writable while WebGL is
  enabled (the default) or WebGPU was turned on.
- **seccomp, renderer.** A default-deny list of about 260 system calls;
  everything else returns `EPERM`. Not on the list: `execve`, `fork`,
  `ptrace`, `bpf`, `mount`, `unshare`, `setns`, `keyctl`, `userfaultfd`,
  `io_uring_*`, `perf_event_open`, `bind`, `listen` and `accept`. Argument
  filters refuse memory that is writable and executable at once, namespace
  flags on `clone` (`clone3` answers `ENOSYS`), signals to any other
  process, System V shared memory and `socket(AF_UNIX)`, which keeps the
  renderer off the D-Bus session bus.
- **Shell.** It parses no page content but must start the renderers and
  talk to the display server, so it has a wider Landlock rule set
  (Downloads, the whole runtime directory, the GPU) and no seccomp filter.
- **Media helpers.** They apply the wider Landlock rules and the system
  call list, without the renderer's argument filters, before reading the
  first command.

The sandbox is best-effort: on a kernel without Landlock or seccomp the
process logs a warning and runs unconfined. `NS_REQUIRE_SANDBOX=1` makes
that fatal. `NS_NO_SANDBOX=1` and `NS_NO_SECCOMP=1` turn the layers off for
debugging, and `NS_ALLOW_ROOT=1` allows running as root. A switch counts as
set for any value other than empty, `0`, `false`, `no` or `off`.

Executables are linked with full RELRO, a non-executable stack and separate
code segments, and export no symbols.

### macOS

The renderer and the shell apply a Seatbelt profile (`sandbox_init`) that
denies file writes outside Southstar's own directories, Downloads and the
temporary directories. Reads, network access and `exec` are not restricted,
there is no system-call filter, and the profile fails open with a warning.
The App Sandbox is not used because it forbids starting the renderer
processes (`docs/macOS.md`).

### Windows

The renderer and the shell set these process mitigation policies at
startup: forced image relocation, strict handle checks, extension points
disabled, and no image loads from remote or low-integrity locations. The
renderer also may not create child processes. There is no file-system
sandbox, no restricted token and no AppContainer. `NS_NO_WIN32_MITIGATIONS=1`
turns the policies off for debugging.

When started elevated, the browser relaunches itself with the desktop
shell's unelevated token and exits. If that fails it asks whether to quit
(the default) or continue.

## Network

All requests go through the in-tree HTTP client (`rust/http`).

- **TLS** is rustls: TLS 1.2 and 1.3, certificate and host-name
  verification always on, roots from the CA bundle or the system store.
- **HTTPS first.** A top-level `http://` navigation is tried over HTTPS
  first. HSTS is recorded and persisted.
- **Certificate errors.** The `tls_allow_insecure_override` setting (off by
  default) lets the user continue past a certificate error for the top-level
  page only, never for an HSTS host. That request carries and stores no
  cookies, and its connection is not reused. Subresources are never fetched
  without verification.
- **Redirects** go to `http` or `https` only, at most ten, and are resolved
  by the same URL parser as every other check.
- **Malformed traffic is refused**: request targets with spaces or control
  characters, the Fetch standard's blocked ports, header lines with a bare
  CR or LF, and HTTP/1.1 responses that end early.
- **Mixed content.** An HTTPS page cannot load scripts, modules or workers,
  or use `fetch`, `XMLHttpRequest` or WebSocket, over plain HTTP.
- **DNS over HTTPS** is off unless the user names a resolver.
- **Safe Browsing** is active only in builds given an API key. It downloads
  hash-prefix lists and sends a 4-byte hash prefix on a match, never the
  address. It can be turned off in Settings.

## Web security model

- **Partitioning.** Cookies and the HTTP cache are partitioned by top-level
  site (scheme, registrable domain from the Public Suffix List, port).
  Third-party cookies are blocked by default. IndexedDB is keyed by the top
  page's origin.
- **Cookies** honour `HttpOnly`, `Secure`, `SameSite`, `Path`, `Domain`, the
  `__Host-` and `__Secure-` prefixes and expiry. `HttpOnly` cookies are
  never visible to script, and `document.cookie` cannot set `Secure` from an
  insecure page or a `Domain` outside the document's host.
- **`file:` documents** are opaque origins: a local page cannot read other
  files, and a web page cannot load `file:` URLs.
- **CORS** applies to `fetch`, `XMLHttpRequest`, module scripts and
  `crossorigin` resources. Opaque responses expose no body, status, headers
  or redirect flag. Errors from cross-origin scripts are reported as
  "Script error." while the script is first evaluated.
- **CSP** enforces `default-src`, `script-src`, `style-src`, `img-src`,
  `media-src`, `connect-src`, `font-src`, `frame-src`, `child-src`,
  `worker-src`, `object-src`, `base-uri`, `form-action` and
  `frame-ancestors`, with nonces, hashes and `'strict-dynamic'`.
- **Also enforced:** Subresource Integrity, `X-Frame-Options`,
  `X-Content-Type-Options: nosniff`, and the MIME type and status of module
  scripts.
- **`<iframe sandbox>`** is enforced, and nested frames inherit the
  intersection: scripts, same-origin, forms, modals, popups, downloads and
  top navigation.
- **Permissions.** Camera and microphone need the user's answer to a prompt
  for each origin. Geolocation is always denied. WebRTC and EME are stubs
  that connect to nothing and offer no key system.
- **Address bar.** Hosts are shown in Punycode, so an internationalized
  look-alike domain appears as `xn--…`.

## Parsers and decoders

Everything below runs in the sandboxed renderer unless noted.

| Input | Decoder |
|---|---|
| HTML, CSS, URLs, XML | in-tree Rust (`rust/html5`, `rust/css`, `rust/url`) |
| PNG, GIF, BMP, ICO, JPEG, WebP, SVG | Rust (`png`, `gif`, `zune-jpeg`, `image-webp`, in-tree) |
| AVIF | libavif (C), when built |
| Web fonts | Rust (`read-fonts`, `skrifa`, HarfRust) |
| System fonts | FreeType, HarfBuzz (C) |
| MPEG-1, MP2, MP3 | in-tree Rust (`rust/plmpeg`, `rust/minimp3`) |
| WebM, Opus, Vorbis, MP4 | FFmpeg `libav*` (C); H.264 and AAC by Media Foundation on Windows |
| PDF | poppler (C), when built |
| Character set detection | uchardet (C) |

No image format falls back to a plugin loader. Image sizes are capped at
16384 pixels per side and 64 megapixels. The CSS parsers and the layout
tree walkers have fixed depth limits, so nested input cannot exhaust the
stack.

## Local interfaces and state

- **Remote control** (`southstar --mcp`) is off by default. When the user
  turns it on, the shell listens on a random loopback port and requires a
  random token, kept in a file only the user can read.
- **Embedding.** The renderer protocol runs over the renderer's own pipes
  and is not reachable from web content. An embedder's resource policy is
  checked on every request, upgrade and preconnect and on the resolved
  address of direct connections.
- **Extensions** are loaded only from the user's `extensions` directory;
  there is no store and no install prompt. They can run content scripts,
  block requests and keep storage.
- **Files.** Southstar's directories are created `0700` and its files
  `0600` on Unix. Cache, cookie-jar and storage file names are SHA-256
  hashes, never bytes chosen by a site.
- **Downloads** are named safely, capped at 1 GiB, and executable types
  wait for the user's consent.

## Known limits

- **No site isolation.** Every renderer does its own networking and can
  read and write the cookie jars, HTTP cache, `localStorage`, IndexedDB,
  history, extensions and Safe Browsing lists of the whole profile. A
  compromised renderer therefore reaches other sites' data and can persist
  into later sessions. A broker in the shell is the planned fix.
- **Frames share a process.** Cross-origin frame isolation is enforced by
  the script engine only.
- **The Linux renderer reads more than it needs:** all of `/proc`, `/sys`
  and `/run`, and the camera device nodes whatever the user answered.
  The camera and microphone decision is kept in the renderer.
- **The media helpers are weaker than the renderer** on Linux and
  unconfined on macOS and Windows. The audio helper fetches its media
  itself. With a distribution FFmpeg, media bytes are probed against every
  demuxer that library was built with.
- **macOS and Windows have no read or system-call confinement.** A
  compromised renderer there can read the user's files.
- **Passive mixed content is not blocked.** An HTTPS page can load images
  and style sheets over HTTP.
- **`localStorage` is keyed by origin only**, not by top-level site.
- **The address bar shows the URL's user-name part** and decodes
  percent-escapes for display.
- **Not enforced:** the CSP `sandbox` directive, and the `js_memory_cap_mb`
  setting.
- **Cross-origin script errors** thrown later, from a handler or a timer,
  are reported in full.
- **Extension content scripts run in the page's realm**, so a page can
  interfere with them.
- **Downloads are fetched by the shell**, outside the renderer sandbox.
