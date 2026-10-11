//! Southstar — where the shell's icons live in the resource bundle and how large they draw.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const RESOURCE_DIR: &[u8] = b"/org/southstar/WebBrowser/icons/scalable/apps/";
pub const DEFAULT_PX: i32 = 16;

pub fn resource_path(name: &[u8]) -> Vec<u8> {
    [RESOURCE_DIR, name, b".svg"].concat()
}

pub fn is_symbolic(name: &[u8]) -> bool {
    name.ends_with(b"-symbolic")
}

pub fn intrinsic_px(has: bool, value: f64) -> i32 {
    if has && (1.0..=1024.0).contains(&value) {
        value as i32
    } else {
        DEFAULT_PX
    }
}
