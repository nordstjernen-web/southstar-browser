//! Southstar text — FriBiDi: per-character bidi types, paired brackets and the paragraph embedding levels of UAX #9.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::c_int;

#[link(name = "fribidi")]
unsafe extern "C" {
    fn fribidi_get_bidi_type(ch: u32) -> u32;
    fn fribidi_get_bracket(ch: u32) -> u32;
    fn fribidi_get_par_embedding_levels_ex(
        bidi_types: *const u32,
        bracket_types: *const u32,
        len: c_int,
        pbase_dir: *mut u32,
        embedding_levels: *mut i8,
    ) -> i8;
}

pub(crate) fn bidi_type(ch: u32) -> u32 {
    // SAFETY: fribidi_get_bidi_type is a pure table lookup defined for every u32.
    unsafe { fribidi_get_bidi_type(ch) }
}

pub(crate) fn bracket(ch: u32) -> u32 {
    // SAFETY: fribidi_get_bracket is a pure table lookup defined for every u32.
    unsafe { fribidi_get_bracket(ch) }
}

pub(crate) fn par_embedding_levels(
    bidi_types: &[u32],
    bracket_types: &[u32],
    base_dir: &mut u32,
    levels: &mut [u8],
) -> i8 {
    let len = bidi_types.len().min(bracket_types.len()).min(levels.len());
    let Ok(count) = c_int::try_from(len) else {
        return 0;
    };
    // SAFETY: the three arrays hold at least `count` elements each, `base_dir` is a valid
    // u32 and FriBidiLevel is a signed char with the layout of u8.
    unsafe {
        fribidi_get_par_embedding_levels_ex(
            bidi_types.as_ptr(),
            bracket_types.as_ptr(),
            count,
            base_dir,
            levels.as_mut_ptr().cast::<i8>(),
        )
    }
}
