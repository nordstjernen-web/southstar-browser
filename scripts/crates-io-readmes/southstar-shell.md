# southstar-shell

The GTK 4 user interface of [Southstar](https://github.com/nordstjernen-web/southstar-browser).
It provides:
- the window, tabs, address bar and toolbar;
- the menus, downloads, task manager and settings;
- the compositor that shows each tab's renderer output.

**`shell`** holds the interface itself and the browser's entry point,
`southstar_shell::run`. It parses the command line, sets up the sandbox,
starts the watchdog and runs the GTK application.

**`gtk`** wraps the GTK 4 widgets the shell builds in safe,
reference-counted handles. Shell code therefore creates and connects
widgets without `unsafe`.

**`gtk_ffi`** has hand-written declarations of the GTK 4, GDK, GObject,
cairo and graphene calls the shell makes. There is no gtk-rs and no
bindgen.

The interface talks to pages only through the renderer protocol of
[southstar-runtime](https://crates.io/crates/southstar-runtime). The engine
itself runs in separate sandboxed processes.

Part of the browser that `cargo install southstar` builds. The API follows
the browser's needs and is not stable.

License: `LicenseRef-NSL-1.0 OR GPL-3.0-or-later`.
