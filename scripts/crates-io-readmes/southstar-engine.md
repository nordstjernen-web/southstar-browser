# southstar-engine

The browser engine of [Southstar](https://github.com/nordstjernen-web/southstar-browser):
everything between a parsed document and pixels on the screen. In the
Southstar repository it is about 85 small crates. Their dependencies
interlock, so they are published together, each as a module.

**JavaScript.** The `js_*` modules bind the web platform to
[Boa](https://github.com/boa-dev/boa), through Southstar's fork
(`southstar-boa-engine`). `js_engine` is the layer every binding uses, with
`Scope`, `Value` and `HostClass`. The other modules implement:
- the DOM interfaces and events, the HTML elements and forms;
- the CSSOM, observers, custom elements and shadow DOM;
- `fetch`, `XMLHttpRequest`, WebSockets and `EventSource`;
- storage, IndexedDB, workers, `Intl` and Temporal;
- WebAssembly through [wasmi](https://crates.io/crates/wasmi), with the
  `wasm` feature.

The engine passes all of test262.

**Layout.** The `layout*` modules implement:
- block, inline, float and positioned layout;
- flexbox, grid and tables;
- intrinsic sizing, hit testing and layout queries.

**Rendering.** The rendering modules are:
- `style`: computed style per element
- `paint`: painting through cairo
- `render`: the frame pipeline
- `anim`: CSS animations and transitions
- `svg` and `mathml`
- `canvas` 2D, `webgl` (WebGL 1 and 2 over OpenGL ES), and `webgpu`
  (experimental, behind a runtime flag)
- `print`

**Network and media.**
- `net`: the page fetch path. Redirects, cookies, HSTS, the cache, CSP,
  partitioning and `about:` pages.
- `video`: inline MPEG-1 and WebM playback.
- `webaudio`, `camera` and `idb`.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
