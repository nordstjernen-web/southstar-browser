//! Southstar — the streaming zstd decoder for response bodies, over libzstd.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "zstd")]
mod zstd {
    use crate::inflate::{Emit, Failure, OUT_CHUNK};
    use core::ffi::{c_int, c_uint, c_void};

    #[repr(C)]
    struct InBuffer {
        src: *const u8,
        size: usize,
        pos: usize,
    }

    #[repr(C)]
    struct OutBuffer {
        dst: *mut u8,
        size: usize,
        pos: usize,
    }

    unsafe extern "C" {
        safe fn ZSTD_createDStream() -> *mut c_void;
        fn ZSTD_DCtx_setParameter(ds: *mut c_void, param: c_int, value: c_int) -> usize;
        fn ZSTD_decompressStream(
            ds: *mut c_void,
            out: *mut OutBuffer,
            input: *mut InBuffer,
        ) -> usize;
        safe fn ZSTD_isError(code: usize) -> c_uint;
        fn ZSTD_freeDStream(ds: *mut c_void) -> usize;
    }

    const ZSTD_D_WINDOW_LOG_MAX: c_int = 100;
    const MAX_WINDOW_LOG: c_int = 23;

    pub struct Zstd {
        stream: *mut c_void,
    }

    // SAFETY: the stream is owned by this value, reached only through `&mut Zstd`, and libzstd
    // keeps no thread affinity.
    unsafe impl Send for Zstd {}

    impl Zstd {
        pub fn new() -> Option<Zstd> {
            let stream = ZSTD_createDStream();
            if stream.is_null() {
                return None;
            }
            let zstd = Zstd { stream };
            // SAFETY: `stream` is the non-null stream just created, owned by `zstd`.
            let rc = unsafe {
                ZSTD_DCtx_setParameter(zstd.stream, ZSTD_D_WINDOW_LOG_MAX, MAX_WINDOW_LOG)
            };
            (ZSTD_isError(rc) == 0).then_some(zstd)
        }

        pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), Failure> {
            let mut inb = InBuffer {
                src: input.as_ptr(),
                size: input.len(),
                pos: 0,
            };
            let mut out = [0u8; OUT_CHUNK];
            loop {
                let mut outb = OutBuffer {
                    dst: out.as_mut_ptr(),
                    size: OUT_CHUNK,
                    pos: 0,
                };
                // SAFETY: `stream` is live, `inb` describes `input` and `outb` describes `out`,
                // both of which outlive the call.
                let rc = unsafe { ZSTD_decompressStream(self.stream, &mut outb, &mut inb) };
                if outb.pos > 0 && !emit(&out[..outb.pos]) {
                    return Err(Failure::Stopped);
                }
                if ZSTD_isError(rc) != 0 {
                    return Err(Failure::Corrupt);
                }
                if inb.pos >= inb.size && outb.pos < OUT_CHUNK {
                    return Ok(());
                }
            }
        }
    }

    impl Drop for Zstd {
        fn drop(&mut self) {
            // SAFETY: `stream` is the non-null stream `new` created, freed exactly once, here.
            unsafe { ZSTD_freeDStream(self.stream) };
        }
    }
}

#[cfg(feature = "zstd")]
pub use zstd::Zstd;
