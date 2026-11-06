//! Southstar — streaming content decoders for response bodies: br over libbrotlidec and zstd over libzstd.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "brotli")]
mod brotli {
    use crate::inflate::{Emit, Failure, OUT_CHUNK};
    use core::ffi::{c_int, c_void};

    unsafe extern "C" {
        fn BrotliDecoderCreateInstance(
            alloc: *const c_void,
            free: *const c_void,
            opaque: *mut c_void,
        ) -> *mut c_void;
        fn BrotliDecoderDecompressStream(
            state: *mut c_void,
            avail_in: *mut usize,
            next_in: *mut *const u8,
            avail_out: *mut usize,
            next_out: *mut *mut u8,
            total_out: *mut usize,
        ) -> c_int;
        fn BrotliDecoderDestroyInstance(state: *mut c_void);
    }

    const SUCCESS: c_int = 1;
    const NEEDS_MORE_INPUT: c_int = 2;
    const NEEDS_MORE_OUTPUT: c_int = 3;

    pub struct Brotli {
        state: *mut c_void,
        finished: bool,
    }

    // SAFETY: the decoder state is owned by this value, reached only through `&mut Brotli`, and
    // libbrotlidec keeps no thread affinity.
    unsafe impl Send for Brotli {}

    impl Brotli {
        pub fn new() -> Option<Brotli> {
            // SAFETY: null allocator functions and opaque select brotli's default allocator.
            let state = unsafe {
                BrotliDecoderCreateInstance(
                    core::ptr::null(),
                    core::ptr::null(),
                    core::ptr::null_mut(),
                )
            };
            (!state.is_null()).then(|| Brotli {
                state,
                finished: false,
            })
        }

        pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), Failure> {
            let mut next_in = input.as_ptr();
            let mut avail_in = input.len();
            let mut out = [0u8; OUT_CHUNK];
            while !self.finished {
                let mut next_out = out.as_mut_ptr();
                let mut avail_out = OUT_CHUNK;
                // SAFETY: `state` is the live decoder, next_in/avail_in describe the rest of
                // `input` and next_out/avail_out the rest of `out`, and total_out may be null.
                let rc = unsafe {
                    BrotliDecoderDecompressStream(
                        self.state,
                        &mut avail_in,
                        &mut next_in,
                        &mut avail_out,
                        &mut next_out,
                        core::ptr::null_mut(),
                    )
                };
                let produced = OUT_CHUNK - avail_out;
                if produced > 0 && !emit(&out[..produced]) {
                    return Err(Failure::Stopped);
                }
                match rc {
                    SUCCESS => self.finished = true,
                    NEEDS_MORE_INPUT => return Ok(()),
                    NEEDS_MORE_OUTPUT => {}
                    _ => return Err(Failure::Corrupt),
                }
            }
            Ok(())
        }
    }

    impl Drop for Brotli {
        fn drop(&mut self) {
            // SAFETY: `state` is the non-null decoder `new` created, destroyed exactly once, here.
            unsafe { BrotliDecoderDestroyInstance(self.state) };
        }
    }
}

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

#[cfg(feature = "brotli")]
pub use brotli::Brotli;
#[cfg(feature = "zstd")]
pub use zstd::Zstd;
