# southstar-platform

The platform layer of [Southstar](https://github.com/nordstjernen-web/southstar-browser):
the small pieces between the browser and the operating system and its C
libraries. Each module was a crate of its own in the Southstar workspace.

**System libraries**
- `glib`, `cairo`, `sqlite`: the GLib, cairo and SQLite calls the browser shares, as safe owned types where possible
- `glctx`: the offscreen GL context WebGL renders into (WGL, CGL or EGL)
- `spellcheck`: optional spell-checking over Enchant
- `mic`: microphone capture through SDL for `getUserMedia` and Web Audio

**Security and processes**
- `sandbox`: Landlock and seccomp on Linux, Seatbelt on macOS, Windows mitigation policies, refusing to run as root, SRI checks, download marking
- `ipc`: framing for the renderer protocol over a socket or pipe, with file-descriptor passing
- `media-shm`: the shared-memory audio clock and video frame ring
- `helper-ffi`: what the media helper processes link
- `threaddump`: per-process thread dumps on `SIGQUIT`

**Data and formats**
- `config`, `keyfile`, `bookmarks`, `i18n`: settings, localStorage files, bookmarks and UI translations
- `digest`: SHA digests (over ring), hex and base64
- `numfmt`: locale-independent `%g`/`%f` formatting
- `datetime`: civil-date math and HTML date/time strings
- `mime`: sniffing a file's media type
- `webvtt`: WebVTT cues
- `raster`: PNG/APNG, GIF, JPEG and BMP decoding in safe Rust
- `texture`: the decoded-image buffer
- `mat4`: 3D transform matrices
- `debuglog`, `netutil`, `about-style`: smaller helpers

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
