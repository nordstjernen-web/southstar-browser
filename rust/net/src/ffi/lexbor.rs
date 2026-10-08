//! Southstar — lexbor's WHATWG URL parser behind a per-thread parser: parsing, serializing, the URL setters and the parsed URL's fields.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::cell::Cell;

type Status = c_uint;
type SerializeCb = unsafe extern "C" fn(data: *const u8, len: usize, ctx: *mut c_void) -> Status;

const STATUS_OK: Status = 0;

pub const SCHEME_HTTP: c_int = 2;
pub const SCHEME_HTTPS: c_int = 3;
pub const SCHEME_WS: c_int = 4;
pub const SCHEME_WSS: c_int = 5;
pub const SCHEME_FTP: c_int = 6;
pub const SCHEME_FILE: c_int = 7;

const HOST_UNDEF: c_int = 0;
const HOST_DOMAIN: c_int = 1;
const HOST_OPAQUE: c_int = 2;
const HOST_EMPTY: c_int = 5;

#[repr(C)]
#[derive(Clone, Copy)]
struct LexborStr {
    data: *mut u8,
    length: usize,
}

#[repr(C)]
struct Scheme {
    name: LexborStr,
    kind: c_int,
}

#[repr(C)]
union HostValue {
    ipv6: [u16; 8],
    ipv4: u32,
    text: LexborStr,
}

#[repr(C)]
struct Host {
    kind: c_int,
    u: HostValue,
}

#[repr(C)]
struct Path {
    text: LexborStr,
    length: usize,
    opaque: bool,
}

#[repr(C)]
struct LxbUrl {
    scheme: Scheme,
    host: Host,
    username: LexborStr,
    password: LexborStr,
    port: u16,
    has_port: bool,
    path: Path,
    query: LexborStr,
    fragment: LexborStr,
    mraw: *mut c_void,
}

#[repr(C)]
struct LxbParser {
    _private: [u8; 0],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<LxbUrl>() == 160
        && core::mem::offset_of!(LxbUrl, host) == 24
        && core::mem::offset_of!(LxbUrl, port) == 80
        && core::mem::offset_of!(LxbUrl, has_port) == 82
        && core::mem::offset_of!(LxbUrl, path) == 88
        && core::mem::offset_of!(LxbUrl, query) == 120
        && core::mem::offset_of!(LxbUrl, mraw) == 152
        && core::mem::size_of::<Host>() == 24
        && core::mem::size_of::<Path>() == 32
);

unsafe extern "C" {
    fn lxb_url_parser_create() -> *mut LxbParser;
    fn lxb_url_parser_init(parser: *mut LxbParser, mraw: *mut c_void) -> Status;
    fn lxb_url_parser_clean(parser: *mut LxbParser);
    fn lxb_url_parser_destroy(parser: *mut LxbParser, destroy_self: bool) -> *mut LxbParser;
    fn lxb_url_parser_memory_destroy(parser: *mut LxbParser);
    fn lxb_url_parse(
        parser: *mut LxbParser,
        base: *const LxbUrl,
        data: *const u8,
        length: usize,
    ) -> *mut LxbUrl;
    fn lxb_url_destroy(url: *mut LxbUrl) -> *mut LxbUrl;
    fn lxb_url_serialize(
        url: *const LxbUrl,
        cb: SerializeCb,
        ctx: *mut c_void,
        exclude_fragment: bool,
    ) -> Status;
    fn lxb_url_serialize_scheme(url: *const LxbUrl, cb: SerializeCb, ctx: *mut c_void) -> Status;
    fn lxb_url_serialize_host(host: *const Host, cb: SerializeCb, ctx: *mut c_void) -> Status;
    fn lxb_url_serialize_path(path: *const Path, cb: SerializeCb, ctx: *mut c_void) -> Status;
    fn lxb_url_api_protocol_set(
        url: *mut LxbUrl,
        parser: *mut LxbParser,
        value: *const u8,
        length: usize,
    ) -> Status;
    fn lxb_url_api_username_set(url: *mut LxbUrl, value: *const u8, length: usize) -> Status;
    fn lxb_url_api_password_set(url: *mut LxbUrl, value: *const u8, length: usize) -> Status;
    fn lxb_url_api_host_set(
        url: *mut LxbUrl,
        parser: *mut LxbParser,
        value: *const u8,
        length: usize,
    ) -> Status;
    fn lxb_url_api_hostname_set(
        url: *mut LxbUrl,
        parser: *mut LxbParser,
        value: *const u8,
        length: usize,
    ) -> Status;
    fn lxb_url_api_port_set(
        url: *mut LxbUrl,
        parser: *mut LxbParser,
        value: *const u8,
        length: usize,
    ) -> Status;
    fn lxb_url_api_pathname_set(
        url: *mut LxbUrl,
        parser: *mut LxbParser,
        value: *const u8,
        length: usize,
    ) -> Status;
    fn lxb_url_api_search_set(
        url: *mut LxbUrl,
        parser: *mut LxbParser,
        value: *const u8,
        length: usize,
    ) -> Status;
    fn lxb_url_api_hash_set(
        url: *mut LxbUrl,
        parser: *mut LxbParser,
        value: *const u8,
        length: usize,
    ) -> Status;
}

struct ParserSlot(Cell<*mut LxbParser>);

impl Drop for ParserSlot {
    fn drop(&mut self) {
        let parser = self.0.get();
        if !parser.is_null() {
            unsafe {
                lxb_url_parser_memory_destroy(parser);
                lxb_url_parser_destroy(parser, true);
            }
        }
    }
}

thread_local! {
    static PARSER: ParserSlot = const { ParserSlot(Cell::new(ptr::null_mut())) };
}

fn thread_parser() -> Option<NonNull<LxbParser>> {
    PARSER
        .try_with(|slot| {
            if let Some(parser) = NonNull::new(slot.0.get()) {
                unsafe { lxb_url_parser_clean(parser.as_ptr()) };
                return Some(parser);
            }
            let parser = NonNull::new(unsafe { lxb_url_parser_create() })?;
            if unsafe { lxb_url_parser_init(parser.as_ptr(), ptr::null_mut()) } != STATUS_OK {
                unsafe { lxb_url_parser_destroy(parser.as_ptr(), true) };
                return None;
            }
            slot.0.set(parser.as_ptr());
            Some(parser)
        })
        .ok()
        .flatten()
}

unsafe extern "C" fn append(data: *const u8, len: usize, ctx: *mut c_void) -> Status {
    let out = unsafe { &mut *ctx.cast::<Vec<u8>>() };
    if len > 0 {
        out.extend_from_slice(unsafe { core::slice::from_raw_parts(data, len) });
    }
    STATUS_OK
}

fn serialized(write: impl FnOnce(*mut c_void) -> Status) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    (write((&mut out as *mut Vec<u8>).cast()) == STATUS_OK).then_some(out)
}

fn text(s: &LexborStr) -> &[u8] {
    if s.data.is_null() || s.length == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(s.data, s.length) }
    }
}

pub struct Parser(NonNull<LxbParser>);

impl Parser {
    pub fn open() -> Option<Parser> {
        thread_parser().map(Parser)
    }

    pub fn parse(&self, base: Option<&Url>, input: &[u8]) -> Option<Url> {
        let base = base.map_or(ptr::null(), |b| b.0.as_ptr().cast_const());
        let url = unsafe { lxb_url_parse(self.0.as_ptr(), base, input.as_ptr(), input.len()) };
        NonNull::new(url).map(Url)
    }

    pub fn clean(&self) {
        unsafe { lxb_url_parser_clean(self.0.as_ptr()) };
    }
}

impl Drop for Parser {
    fn drop(&mut self) {
        self.clean();
    }
}

pub struct Url(NonNull<LxbUrl>);

impl Drop for Url {
    fn drop(&mut self) {
        unsafe { lxb_url_destroy(self.0.as_ptr()) };
    }
}

pub enum Setter {
    Protocol,
    Username,
    Password,
    Host,
    Hostname,
    Port,
    Pathname,
    Search,
    Hash,
}

impl Url {
    fn raw(&self) -> &LxbUrl {
        unsafe { self.0.as_ref() }
    }

    pub fn serialize(&self) -> Option<Vec<u8>> {
        let url = self.0.as_ptr().cast_const();
        serialized(|ctx| unsafe { lxb_url_serialize(url, append, ctx, false) })
    }

    pub fn serialize_scheme(&self) -> Option<Vec<u8>> {
        let url = self.0.as_ptr().cast_const();
        serialized(|ctx| unsafe { lxb_url_serialize_scheme(url, append, ctx) })
    }

    pub fn serialize_host(&self) -> Option<Vec<u8>> {
        let host = &self.raw().host as *const Host;
        serialized(|ctx| unsafe { lxb_url_serialize_host(host, append, ctx) })
    }

    pub fn serialize_path(&self) -> Option<Vec<u8>> {
        let path = &self.raw().path as *const Path;
        serialized(|ctx| unsafe { lxb_url_serialize_path(path, append, ctx) })
    }

    pub fn scheme_name(&self) -> &[u8] {
        text(&self.raw().scheme.name)
    }

    pub fn scheme_type(&self) -> c_int {
        self.raw().scheme.kind
    }

    pub fn has_host(&self) -> bool {
        !matches!(self.raw().host.kind, HOST_UNDEF | HOST_EMPTY)
    }

    pub fn host_is_empty(&self) -> bool {
        let host = &self.raw().host;
        match host.kind {
            HOST_UNDEF | HOST_EMPTY => true,
            HOST_DOMAIN | HOST_OPAQUE => (unsafe { host.u.text.length }) == 0,
            _ => false,
        }
    }

    pub fn port(&self) -> Option<u16> {
        let raw = self.raw();
        raw.has_port.then_some(raw.port)
    }

    pub fn username(&self) -> &[u8] {
        text(&self.raw().username)
    }

    pub fn password(&self) -> &[u8] {
        text(&self.raw().password)
    }

    pub fn query(&self) -> &[u8] {
        text(&self.raw().query)
    }

    pub fn fragment(&self) -> &[u8] {
        text(&self.raw().fragment)
    }

    pub fn set(&mut self, parser: &Parser, setter: Setter, value: &[u8]) -> bool {
        let url = self.0.as_ptr();
        let p = parser.0.as_ptr();
        let (v, n) = (value.as_ptr(), value.len());
        let status = unsafe {
            match setter {
                Setter::Protocol => lxb_url_api_protocol_set(url, p, v, n),
                Setter::Username => lxb_url_api_username_set(url, v, n),
                Setter::Password => lxb_url_api_password_set(url, v, n),
                Setter::Host => lxb_url_api_host_set(url, p, v, n),
                Setter::Hostname => lxb_url_api_hostname_set(url, p, v, n),
                Setter::Port => lxb_url_api_port_set(url, p, v, n),
                Setter::Pathname => lxb_url_api_pathname_set(url, p, v, n),
                Setter::Search => lxb_url_api_search_set(url, p, v, n),
                Setter::Hash => lxb_url_api_hash_set(url, p, v, n),
            }
        };
        status == STATUS_OK
    }
}
