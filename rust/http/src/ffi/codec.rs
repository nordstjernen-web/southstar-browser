//! Southstar — streaming content decoders for response bodies: gzip and deflate over zlib, br over libbrotlidec and zstd over libzstd.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_ulong, c_void};

pub enum Failure {
    Corrupt,
    Stopped,
}

pub type Emit<'a> = dyn FnMut(&[u8]) -> bool + 'a;

const OUT_CHUNK: usize = 16_384;

#[repr(C)]
struct ZStream {
    next_in: *const u8,
    avail_in: c_uint,
    total_in: c_ulong,
    next_out: *mut u8,
    avail_out: c_uint,
    total_out: c_ulong,
    msg: *const c_char,
    state: *mut c_void,
    zalloc: *const c_void,
    zfree: *const c_void,
    opaque: *mut c_void,
    data_type: c_int,
    adler: c_ulong,
    reserved: c_ulong,
}

unsafe extern "C" {
    fn zlibVersion() -> *const c_char;
    fn inflateInit2_(strm: *mut ZStream, bits: c_int, version: *const c_char, size: c_int)
    -> c_int;
    fn inflate(strm: *mut ZStream, flush: c_int) -> c_int;
    fn inflateEnd(strm: *mut ZStream) -> c_int;
}

const Z_OK: c_int = 0;
const Z_STREAM_END: c_int = 1;
const Z_BUF_ERROR: c_int = -5;
const Z_NO_FLUSH: c_int = 0;

pub struct Inflate {
    stream: Box<ZStream>,
    finished: bool,
}

unsafe impl Send for Inflate {}

impl Inflate {
    pub fn new(window_bits: i32) -> Option<Inflate> {
        let mut stream = Box::new(ZStream {
            next_in: core::ptr::null(),
            avail_in: 0,
            total_in: 0,
            next_out: core::ptr::null_mut(),
            avail_out: 0,
            total_out: 0,
            msg: core::ptr::null(),
            state: core::ptr::null_mut(),
            zalloc: core::ptr::null(),
            zfree: core::ptr::null(),
            opaque: core::ptr::null_mut(),
            data_type: 0,
            adler: 0,
            reserved: 0,
        });
        let rc = unsafe {
            inflateInit2_(
                &mut *stream,
                window_bits,
                zlibVersion(),
                core::mem::size_of::<ZStream>() as c_int,
            )
        };
        (rc == Z_OK).then_some(Inflate {
            stream,
            finished: false,
        })
    }

    pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), Failure> {
        if self.finished {
            return Ok(());
        }
        let mut out = [0u8; OUT_CHUNK];
        let mut rest = input;
        loop {
            let take = rest.len().min(c_uint::MAX as usize);
            self.stream.next_in = rest.as_ptr();
            self.stream.avail_in = take as c_uint;
            self.stream.next_out = out.as_mut_ptr();
            self.stream.avail_out = OUT_CHUNK as c_uint;
            let rc = unsafe { inflate(&mut *self.stream, Z_NO_FLUSH) };
            let consumed = take - self.stream.avail_in as usize;
            rest = &rest[consumed..];
            let produced = OUT_CHUNK - self.stream.avail_out as usize;
            if produced > 0 && !emit(&out[..produced]) {
                return Err(Failure::Stopped);
            }
            match rc {
                Z_STREAM_END => {
                    self.finished = true;
                    return Ok(());
                }
                Z_OK | Z_BUF_ERROR => {
                    if rest.is_empty() && produced < OUT_CHUNK {
                        return Ok(());
                    }
                    if consumed == 0 && produced == 0 {
                        return Err(Failure::Corrupt);
                    }
                }
                _ => return Err(Failure::Corrupt),
            }
        }
    }
}

impl Drop for Inflate {
    fn drop(&mut self) {
        unsafe { inflateEnd(&mut *self.stream) };
    }
}

#[cfg(feature = "brotli")]
mod brotli {
    use super::{Emit, Failure, OUT_CHUNK};
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

    unsafe impl Send for Brotli {}

    impl Brotli {
        pub fn new() -> Option<Brotli> {
            let state = unsafe {
                BrotliDecoderCreateInstance(
                    core::ptr::null(),
                    core::ptr::null(),
                    core::ptr::null_mut(),
                )
            };
            (!state.is_null()).then_some(Brotli {
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
            unsafe { BrotliDecoderDestroyInstance(self.state) };
        }
    }
}

#[cfg(feature = "zstd")]
mod zstd {
    use super::{Emit, Failure, OUT_CHUNK};
    use core::ffi::{c_uint, c_void};

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
        fn ZSTD_createDStream() -> *mut c_void;
        fn ZSTD_initDStream(ds: *mut c_void) -> usize;
        fn ZSTD_decompressStream(
            ds: *mut c_void,
            out: *mut OutBuffer,
            input: *mut InBuffer,
        ) -> usize;
        fn ZSTD_isError(code: usize) -> c_uint;
        fn ZSTD_freeDStream(ds: *mut c_void) -> usize;
    }

    pub struct Zstd {
        stream: *mut c_void,
    }

    unsafe impl Send for Zstd {}

    impl Zstd {
        pub fn new() -> Option<Zstd> {
            let stream = unsafe { ZSTD_createDStream() };
            if stream.is_null() {
                return None;
            }
            unsafe { ZSTD_initDStream(stream) };
            Some(Zstd { stream })
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
                let rc = unsafe { ZSTD_decompressStream(self.stream, &mut outb, &mut inb) };
                if outb.pos > 0 && !emit(&out[..outb.pos]) {
                    return Err(Failure::Stopped);
                }
                if unsafe { ZSTD_isError(rc) } != 0 {
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
            unsafe { ZSTD_freeDStream(self.stream) };
        }
    }
}

#[cfg(feature = "brotli")]
pub use brotli::Brotli;
#[cfg(feature = "zstd")]
pub use zstd::Zstd;
