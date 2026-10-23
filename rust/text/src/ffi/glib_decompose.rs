//! Southstar text — GLib's one-step canonical decomposition of a character, which font coverage falls back on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_unichar_decompose(ch: u32, a: *mut u32, b: *mut u32) -> i32;
}

pub(crate) fn unichar_decompose(ch: u32) -> Option<(u32, u32)> {
    let mut a = 0u32;
    let mut b = 0u32;
    // SAFETY: g_unichar_decompose writes one u32 through each pointer, both of which point at live locals.
    let decomposed = unsafe { g_unichar_decompose(ch, &mut a, &mut b) };
    (decomposed != 0).then_some((a, b))
}
