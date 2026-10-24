//! Southstar — the libbrotlidec call behind the WOFF2 decoder.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

const BROTLI_DECODER_RESULT_SUCCESS: c_int = 1;

#[link(name = "brotlidec")]
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
