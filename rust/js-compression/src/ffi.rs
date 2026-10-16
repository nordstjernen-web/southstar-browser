//! Southstar — zlib's streaming deflate and inflate, and the C ABI of the codec natives.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_ulong, c_void};

use southstar_js_engine::{Scope, Value};

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
    fn deflateInit2_(
        strm: *mut ZStream,
        level: c_int,
        method: c_int,
        bits: c_int,
        mem_level: c_int,
        strategy: c_int,
        version: *const c_char,
        size: c_int,
    ) -> c_int;
    fn inflate(strm: *mut ZStream, flush: c_int) -> c_int;
    fn deflate(strm: *mut ZStream, flush: c_int) -> c_int;
    fn inflateEnd(strm: *mut ZStream) -> c_int;
    fn deflateEnd(strm: *mut ZStream) -> c_int;
}

const Z_OK: c_int = 0;
const Z_STREAM_END: c_int = 1;
const Z_NEED_DICT: c_int = 2;
const Z_STREAM_ERROR: c_int = -2;
const Z_DATA_ERROR: c_int = -3;
const Z_MEM_ERROR: c_int = -4;
const Z_BUF_ERROR: c_int = -5;
const Z_NO_FLUSH: c_int = 0;
const Z_FINISH: c_int = 4;
const Z_DEFAULT_COMPRESSION: c_int = -1;
const Z_DEFLATED: c_int = 8;
const Z_DEFAULT_STRATEGY: c_int = 0;

pub(crate) enum Step {
    Progress,
    StreamEnd,
    BufError,
    Failed,
}

pub(crate) struct Zlib {
    stream: Box<ZStream>,
    decompress: bool,
}

impl Zlib {
    pub fn new(decompress: bool, window_bits: i32) -> Option<Zlib> {
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
        let size = core::mem::size_of::<ZStream>() as c_int;
        let rc = unsafe {
            if decompress {
                inflateInit2_(&mut *stream, window_bits, zlibVersion(), size)
            } else {
                deflateInit2_(
                    &mut *stream,
                    Z_DEFAULT_COMPRESSION,
                    Z_DEFLATED,
                    window_bits,
                    8,
                    Z_DEFAULT_STRATEGY,
                    zlibVersion(),
                    size,
                )
            }
        };
        (rc == Z_OK).then(|| Zlib { stream, decompress })
    }

    pub fn decompress(&self) -> bool {
        self.decompress
    }

    pub fn step(&mut self, input: &mut &[u8], out: &mut [u8], finish: bool) -> (Step, usize) {
        let take = input.len().min(c_uint::MAX as usize);
        self.stream.next_in = input.as_ptr();
        self.stream.avail_in = take as c_uint;
        self.stream.next_out = out.as_mut_ptr();
        self.stream.avail_out = out.len() as c_uint;
        let flush = if finish { Z_FINISH } else { Z_NO_FLUSH };
        let rc = unsafe {
            if self.decompress {
                inflate(&mut *self.stream, flush)
            } else {
                deflate(&mut *self.stream, flush)
            }
        };
        *input = &input[take - self.stream.avail_in as usize..];
        let produced = out.len() - self.stream.avail_out as usize;
        let step = match rc {
            Z_STREAM_ERROR | Z_DATA_ERROR | Z_NEED_DICT | Z_MEM_ERROR => Step::Failed,
            Z_STREAM_END => Step::StreamEnd,
            Z_BUF_ERROR => Step::BufError,
            _ => Step::Progress,
        };
        (step, produced)
    }
}

impl Drop for Zlib {
    fn drop(&mut self) {
        unsafe {
            if self.decompress {
                inflateEnd(&mut *self.stream);
            } else {
                deflateEnd(&mut *self.stream);
            }
        }
    }
}

pub(crate) fn internal_error(scope: &mut Scope<'_>, message: &str) -> Value {
    scope.internal_error(message)
}

#[unsafe(no_mangle)]
pub fn nsr_compression_install(scope: &mut Scope<'_>, global: &Value) {
    crate::install(scope, global);
}
