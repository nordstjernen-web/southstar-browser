//! Southstar — owners of a libav demuxer opened on a file or an in-memory buffer, and borrowed views of its streams and codec parameters.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use std::borrow::Cow;

use crate::decode::{CodecDescription, Packet};
use crate::{
    self as av, AVCodec, AVCodecParameters, AVDictionary, AVFormatContext, AVIOContext, AVStream,
    Field, format_context, io_context, stream,
};

const IO_BUFFER: c_int = 32768;
const SEEK_CUR: c_int = 1;
const SEEK_END: c_int = 2;

pub fn quiet_logging() {
    // SAFETY: sets libav's global log level; no pointers are involved.
    unsafe { av::av_log_set_level(av::AV_LOG_QUIET) };
}

struct Options(*mut AVDictionary);

impl Options {
    fn new(key: &CStr, value: &CStr) -> Options {
        let mut options = Options(ptr::null_mut());
        // SAFETY: options.0 is a null dictionary libav allocates into; key and value are NUL-terminated and copied.
        unsafe { av::av_dict_set(&mut options.0, key.as_ptr(), value.as_ptr(), 0) };
        options
    }
}

impl Drop for Options {
    fn drop(&mut self) {
        // SAFETY: options.0 is null or a dictionary libav allocated; av_dict_free frees it and nulls the pointer.
        unsafe { av::av_dict_free(&mut self.0) };
    }
}

struct Memory<'a> {
    data: Cow<'a, [u8]>,
    pos: usize,
}

impl Memory<'_> {
    fn read(&mut self, out: &mut [u8]) -> c_int {
        let rest = &self.data[self.pos..];
        if rest.is_empty() {
            return av::AVERROR_EOF;
        }
        let n = out.len().min(rest.len());
        out[..n].copy_from_slice(&rest[..n]);
        self.pos += n;
        n as c_int
    }

    fn seek(&mut self, offset: i64, whence: c_int) -> i64 {
        let whence = whence & !av::AVSEEK_FORCE;
        if whence == av::AVSEEK_SIZE {
            return self.data.len() as i64;
        }
        let base = match whence {
            SEEK_CUR => self.pos as i64,
            SEEK_END => self.data.len() as i64,
            _ => 0,
        };
        let np = base.saturating_add(offset);
        if np < 0 || np as u64 > self.data.len() as u64 {
            return -1;
        }
        self.pos = np as usize;
        np
    }
}

unsafe extern "C" fn memory_read(opaque: *mut c_void, buf: *mut u8, size: c_int) -> c_int {
    // SAFETY: opaque is the boxed Memory the Avio owning this context passed, alive and used by this one context only.
    let memory = unsafe { &mut *opaque.cast::<Memory>() };
    // SAFETY: libav passes its I/O buffer with room for size bytes.
    let out = unsafe { core::slice::from_raw_parts_mut(buf, size.max(0) as usize) };
    memory.read(out)
}

unsafe extern "C" fn memory_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    // SAFETY: opaque is the boxed Memory the Avio owning this context passed, alive and used by this one context only.
    let memory = unsafe { &mut *opaque.cast::<Memory>() };
    memory.seek(offset, whence)
}

struct Avio<'a> {
    context: NonNull<AVIOContext>,
    source: NonNull<Memory<'a>>,
}

impl<'a> Avio<'a> {
    fn new(data: Cow<'a, [u8]>) -> Option<Avio<'a>> {
        // SAFETY: allocates an I/O buffer of IO_BUFFER bytes or returns null.
        let buffer = unsafe { av::av_malloc(IO_BUFFER as usize) }.cast::<u8>();
        if buffer.is_null() {
            return None;
        }
        let source = NonNull::from(Box::leak(Box::new(Memory { data, pos: 0 })));
        // SAFETY: buffer holds IO_BUFFER bytes the context takes over; the opaque Memory is boxed and outlives the context, which this Avio frees first.
        let context = unsafe {
            av::avio_alloc_context(
                buffer,
                IO_BUFFER,
                0,
                source.as_ptr().cast(),
                Some(memory_read),
                None,
                Some(memory_seek),
            )
        };
        let Some(context) = NonNull::new(context) else {
            // SAFETY: buffer came from av_malloc above and no context took it over.
            unsafe { av::av_free(buffer.cast()) };
            // SAFETY: source is the Memory leaked above, which no context was given.
            drop(unsafe { Box::from_raw(source.as_ptr()) });
            return None;
        };
        Some(Avio { context, source })
    }

    fn memory(&self) -> &Memory<'a> {
        // SAFETY: source is the boxed Memory this Avio owns; libav only touches it inside calls that borrow the owning Format mutably.
        unsafe { self.source.as_ref() }
    }

    fn memory_mut(&mut self) -> &mut Memory<'a> {
        // SAFETY: source is the boxed Memory this Avio owns; libav only touches it inside calls that borrow the owning Format mutably, so this is the only reference.
        unsafe { self.source.as_mut() }
    }

    fn clear_end(&mut self) {
        let context = self.context.as_ptr();
        // SAFETY: context is the live I/O context this Avio owns and the offset of eof_reached comes from the installed headers.
        unsafe { io_context::EOF_REACHED.set(context, 0) };
        // SAFETY: context is the live I/O context this Avio owns and the offset of error comes from the installed headers.
        unsafe { io_context::ERROR.set(context, 0) };
    }
}

impl Drop for Avio<'_> {
    fn drop(&mut self) {
        let buffer = io_context::BUFFER.ptr(self.context.as_ptr());
        // SAFETY: buffer points at the context's current I/O buffer, which libav allocated; av_freep frees it and nulls the field.
        unsafe { av::av_freep(buffer.cast()) };
        let mut context = self.context.as_ptr();
        // SAFETY: context is the I/O context avio_alloc_context returned, freed exactly once.
        unsafe { av::avio_context_free(&mut context) };
        // SAFETY: source is the Memory leaked in new(), reclaimed once now that no context reads it.
        drop(unsafe { Box::from_raw(self.source.as_ptr()) });
    }
}

pub struct Format<'a> {
    context: NonNull<AVFormatContext>,
    io: Option<Avio<'a>>,
}

impl Format<'static> {
    pub fn open_file(path: &CStr) -> Option<Format<'static>> {
        let mut options = Options::new(c"protocol_whitelist", c"file");
        let mut context = ptr::null_mut();
        // SAFETY: context is null so libav allocates it, and frees it again on failure; path is NUL-terminated; options is a live dictionary libav may replace.
        let opened = unsafe {
            av::avformat_open_input(&mut context, path.as_ptr(), ptr::null(), &mut options.0)
        };
        drop(options);
        if opened < 0 {
            return None;
        }
        Some(Format {
            context: NonNull::new(context)?,
            io: None,
        })
    }

    pub fn open_owned(data: Vec<u8>) -> Option<Format<'static>> {
        Format::open_io(Cow::Owned(data))
    }
}

impl<'a> Format<'a> {
    pub fn open_memory(data: &'a [u8]) -> Option<Format<'a>> {
        Format::open_io(Cow::Borrowed(data))
    }

    fn open_io(data: Cow<'a, [u8]>) -> Option<Format<'a>> {
        let io = Avio::new(data)?;
        // SAFETY: allocates an empty format context or returns null.
        let mut context = unsafe { av::avformat_alloc_context() };
        if context.is_null() {
            return None;
        }
        // SAFETY: context is a fresh format context and the offset of its pb field comes from the installed headers; io outlives the opened context.
        unsafe { format_context::PB.set(context, io.context.as_ptr()) };
        // SAFETY: context carries the custom I/O context, so libav reads through it and leaves it to io; on failure libav frees context.
        let opened = unsafe {
            av::avformat_open_input(&mut context, ptr::null(), ptr::null(), ptr::null_mut())
        };
        if opened < 0 {
            return None;
        }
        Some(Format {
            context: NonNull::new(context)?,
            io: Some(io),
        })
    }

    pub fn buffered(&self) -> Option<&[u8]> {
        self.io.as_ref().map(|io| &*io.memory().data)
    }

    pub fn append(&mut self, extra: &[u8]) -> bool {
        let Some(io) = self.io.as_mut() else {
            return false;
        };
        let Cow::Owned(data) = &mut io.memory_mut().data else {
            return false;
        };
        if data.try_reserve(extra.len()).is_err() {
            return false;
        }
        data.extend_from_slice(extra);
        io.clear_end();
        true
    }

    fn get<T: Copy>(&self, field: &Field<AVFormatContext, T>) -> T {
        // SAFETY: self.context is an open format context and the field's offset and type come from the installed libav headers.
        unsafe { field.get(self.context.as_ptr()) }
    }

    pub fn find_stream_info(&mut self) -> bool {
        // SAFETY: self.context is an open format context; no per-stream options are passed.
        unsafe { av::avformat_find_stream_info(self.context.as_ptr(), ptr::null_mut()) >= 0 }
    }

    pub fn best_stream(&mut self, media_type: c_int) -> Option<c_int> {
        // SAFETY: self.context is an open format context; no decoder is asked for.
        let index = unsafe {
            av::av_find_best_stream(
                self.context.as_ptr(),
                media_type,
                -1,
                -1,
                ptr::null_mut(),
                0,
            )
        };
        (index >= 0).then_some(index)
    }

    pub fn best_stream_and_decoder(
        &mut self,
        media_type: c_int,
    ) -> Option<(c_int, CodecDescription)> {
        let mut decoder: *const AVCodec = ptr::null();
        // SAFETY: self.context is an open format context; decoder is a writable pointer libav sets to a static codec description.
        let index = unsafe {
            av::av_find_best_stream(self.context.as_ptr(), media_type, -1, -1, &mut decoder, 0)
        };
        if index < 0 {
            return None;
        }
        Some((index, CodecDescription::from_libav(decoder)?))
    }

    pub fn stream(&self, index: c_int) -> Option<Stream<'_>> {
        let count = self.get(&format_context::NB_STREAMS);
        let index = u32::try_from(index).ok().filter(|&index| index < count)?;
        let entry = self
            .get(&format_context::STREAMS)
            .wrapping_add(index as usize);
        // SAFETY: streams is libav's array of nb_streams stream pointers and index is below that count.
        let stream = unsafe { entry.read() };
        NonNull::new(stream).map(|stream| Stream(stream, PhantomData))
    }

    pub fn duration(&self) -> i64 {
        self.get(&format_context::DURATION)
    }

    pub fn read_packet(&mut self, packet: &mut Packet) -> bool {
        // SAFETY: self.context is an open format context and packet an allocated, unreferenced packet libav fills.
        unsafe { av::av_read_frame(self.context.as_ptr(), packet.as_ptr()) >= 0 }
    }

    pub fn seek(&mut self, stream: c_int, timestamp: i64, flags: c_int) -> bool {
        // SAFETY: self.context is an open format context; libav checks the stream index.
        unsafe { av::av_seek_frame(self.context.as_ptr(), stream, timestamp, flags) >= 0 }
    }
}

impl Drop for Format<'_> {
    fn drop(&mut self) {
        let mut context = self.context.as_ptr();
        // SAFETY: context is the format context avformat_open_input opened, closed exactly once and before its custom I/O context drops.
        unsafe { av::avformat_close_input(&mut context) };
    }
}

pub struct Stream<'a>(NonNull<AVStream>, PhantomData<&'a ()>);

impl Stream<'_> {
    pub fn get<T: Copy>(&self, field: &Field<AVStream, T>) -> T {
        // SAFETY: self.0 is a stream of a format context that outlives this borrow; the field's offset and type come from the installed headers.
        unsafe { field.get(self.0.as_ptr()) }
    }

    pub fn time_base(&self) -> av::AVRational {
        self.get(&stream::TIME_BASE)
    }

    pub fn parameters(&self) -> Option<Parameters<'_>> {
        NonNull::new(self.get(&stream::CODECPAR))
            .map(|parameters| Parameters(parameters, PhantomData))
    }
}

pub struct Parameters<'a>(NonNull<AVCodecParameters>, PhantomData<&'a ()>);

impl Parameters<'_> {
    pub fn get<T: Copy>(&self, field: &Field<AVCodecParameters, T>) -> T {
        // SAFETY: self.0 is the codec parameters of a stream that outlives this borrow; the field's offset and type come from the installed headers.
        unsafe { field.get(self.0.as_ptr()) }
    }

    pub(crate) fn as_ptr(&self) -> *const AVCodecParameters {
        self.0.as_ptr()
    }
}
