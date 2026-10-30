# southstar-runtime

The browser runtime of [Southstar](https://github.com/nordstjernen-web/southstar-browser):
it turns the engine into running pages, in sandboxed processes.

- **`browser`:** a page as an object you open, lay out, paint, drive with
  input and query. It is the core of Southstar's C embedding API
  (`libsouthstar.h`), and what everything below calls.
- **`renderer_host`:** the renderer process each tab runs in. It serves the
  page over the renderer protocol and paints into shared-memory tiles and
  viewport layers for the window's compositor. `cargo install southstar`
  builds it as `southstar-renderer`.
- **`renderer_client`:** the window's side of that protocol. It spawns
  renderer processes (or in-process sessions), shares the framebuffer, and
  collects task-manager statistics.
- **`headless`:** `southstar --headless`. Pages can be rendered to PNG, text
  or a layout dump, driven by scripted input, and run against
  web-platform-tests.
- **`watchdog`:** the supervisor that restarts the browser on a crash or hang.
- **`ffi`:** the gathering point that links every engine module into one
  binary.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
