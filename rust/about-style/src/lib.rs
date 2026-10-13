//! Southstar — the about: pages' base stylesheet, embedded from base.css.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub fn base_css() -> &'static str {
    include_str!("base.css")
}
