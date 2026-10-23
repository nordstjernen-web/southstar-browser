//! Southstar text — libthai: TIS-620 conversion and the dictionary-based Thai word break finder.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{c_char, c_int, c_void};
use std::sync::Mutex;

#[cfg(windows)]
type WChar = u16;
#[cfg(not(windows))]
type WChar = i32;

#[link(name = "thai")]
unsafe extern "C" {
    fn th_uni2tis(wc: WChar) -> u8;
    fn th_brk_new(dictpath: *const c_char) -> *mut c_void;
    fn th_brk_find_breaks(brk: *mut c_void, s: *const u8, pos: *mut c_int, pos_sz: usize) -> c_int;
}

struct Breaker(*mut c_void);

// SAFETY: the ThBrk handle is only ever used while THAI_BRK's mutex is held, which is
// how the C library serialized it too.
unsafe impl Send for Breaker {}

static THAI_BRK: Mutex<Breaker> = Mutex::new(Breaker(core::ptr::null_mut()));

pub(crate) fn uni2tis(ch: u32) -> u8 {
    // SAFETY: th_uni2tis is a pure table lookup; the code point is truncated to wchar_t exactly
    // as the C caller's implicit conversion does.
    unsafe { th_uni2tis(ch as WChar) }
}

pub(crate) fn find_breaks(tis_text: &[u8], positions: &mut [c_int]) -> usize {
    let mut breaker = THAI_BRK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if breaker.0.is_null() {
        // SAFETY: a null dictionary path asks libthai for its default dictionary.
        breaker.0 = unsafe { th_brk_new(core::ptr::null()) };
    }
    if tis_text.last() != Some(&0) {
        return 0;
    }
    // SAFETY: the handle is a ThBrk guarded by the mutex, or null, which libthai replaces by
    // its shared breaker; `tis_text` is NUL-terminated and `positions` holds
    // `positions.len()` writable ints.
    let found = unsafe {
        th_brk_find_breaks(
            breaker.0,
            tis_text.as_ptr(),
            positions.as_mut_ptr(),
            positions.len(),
        )
    };
    usize::try_from(found).unwrap_or(0).min(positions.len())
}
