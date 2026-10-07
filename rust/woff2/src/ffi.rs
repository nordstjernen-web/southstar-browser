//! Southstar — the C ABI of the WOFF2 decoder, as declared in src/woff2.h, and its libbrotlidec call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;
use southstar_glib::{GBoolean, boolean, g_malloc, slice};

const BROTLI_DECODER_RESULT_SUCCESS: c_int = 1;

unsafe extern "C" {
    fn BrotliDecoderDecompress(
        encoded_size: usize,
        encoded_buffer: *const u8,
        decoded_size: *mut usize,
        decoded_buffer: *mut u8,
    ) -> c_int;
}

pub(crate) fn brotli_decompress(input: &[u8], output: &mut [u8]) -> Option<usize> {
    let mut decoded = output.len();
    let result = unsafe {
        BrotliDecoderDecompress(
            input.len(),
            input.as_ptr(),
            &mut decoded,
            output.as_mut_ptr(),
        )
    };
    (result == BROTLI_DECODER_RESULT_SUCCESS).then_some(decoded)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_woff2_is_woff2(data: *const u8, len: usize) -> GBoolean {
    boolean(crate::is_woff2(unsafe { slice(data, len) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_woff2_to_sfnt(
    data: *const u8,
    len: usize,
    out_len: *mut usize,
    out_cff: *mut GBoolean,
) -> *mut u8 {
    let Some(sfnt) = crate::to_sfnt(unsafe { slice(data, len) }) else {
        return ptr::null_mut();
    };
    unsafe {
        let out = g_malloc(sfnt.bytes.len()).cast::<u8>();
        ptr::copy_nonoverlapping(sfnt.bytes.as_ptr(), out, sfnt.bytes.len());
        if !out_len.is_null() {
            *out_len = sfnt.bytes.len();
        }
        if !out_cff.is_null() {
            *out_cff = boolean(sfnt.cff);
        }
        out
    }
}
