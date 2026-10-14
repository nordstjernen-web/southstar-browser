//! Southstar — links GLib, which southstar-css-syntax calls into, so the fuzz targets link outside meson.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

fn main() {
    println!("cargo:rustc-link-lib=glib-2.0");
}
