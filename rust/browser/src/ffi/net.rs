//! Southstar — the navigation response a page is opened from and the URL, safe-browsing and HTML calls of opening it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{self, NonNull};

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GBoolean, GError, GStr};

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    _content_disposition: *mut c_char,
    csp_header: *mut c_char,
    _xframe_options: *mut c_char,
    _x_content_type_options: *mut c_char,
    _cors_allow_origin: *mut c_char,
    refresh: *mut c_char,
    content_language: *mut c_char,
    raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
    _tls_warning: *mut c_char,
    remote_ip: *mut c_char,
    _next_hop_protocol: *mut c_char,
    request_start_us: i64,
    request_start_real_ms: f64,
    domain_lookup_ms: f64,
    connect_ms: f64,
    tls_ms: f64,
    pretransfer_ms: f64,
    response_start_ms: f64,
    response_end_ms: f64,
    security: c_int,
    _redirect_count: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsResponse>() == 200 && core::mem::offset_of!(NsResponse, security) == 192
);

#[repr(C)]
#[derive(Default)]
pub struct NavigationTimingData {
    pub origin_us: i64,
    pub origin_real_ms: f64,
    pub domain_lookup_start_ms: f64,
    pub domain_lookup_end_ms: f64,
    pub connect_start_ms: f64,
    pub connect_end_ms: f64,
    pub secure_connection_start_ms: f64,
    pub request_start_ms: f64,
    pub response_start_ms: f64,
    pub response_end_ms: f64,
    pub dom_loading_ms: f64,
    pub dom_interactive_ms: f64,
    pub dom_content_loaded_event_start_ms: f64,
    pub dom_content_loaded_event_end_ms: f64,
    pub dom_complete_ms: f64,
    pub load_event_start_ms: f64,
    pub load_event_end_ms: f64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<NavigationTimingData>() == 136);

unsafe extern "C" {
    fn ns_engine_navigate_blocking(
        url: *const c_char,
        top_url: *const c_char,
        user_activated: GBoolean,
        error: *mut *mut GError,
    ) -> *mut NsResponse;
    fn ns_engine_navigate_post_blocking(
        url: *const c_char,
        top_url: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        user_activated: GBoolean,
        error: *mut *mut GError,
    ) -> *mut NsResponse;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_build_error_page(
        url: *const c_char,
        status: c_long,
        transport_error: *const c_char,
    ) -> *mut c_char;
    fn ns_url_host_from(url: *const c_char) -> *mut c_char;
    fn ns_url_strip_tracking_params(url: *const c_char) -> *mut c_char;
    fn ns_net_https_first_upgrade(url: *const c_char) -> *mut c_char;
    fn ns_safebrowsing_blocked(host: *const c_char) -> GBoolean;
    fn ns_safebrowsing_allow_host(host: *const c_char);
    fn ns_safebrowsing_interstitial(url: *const c_char, host: *const c_char) -> *mut c_char;
    fn ns_html_parse(input: *const c_char, len: isize) -> *mut NsNode;
    fn ns_html_parse_with_scripting(
        input: *const c_char,
        len: isize,
        scripting: GBoolean,
    ) -> *mut NsNode;
    fn ns_html_escape_text(s: *const c_char) -> *mut c_char;
    fn ns_html_decode_body_full(
        body: *const c_char,
        len: usize,
        content_type: *const c_char,
        out_charset: *mut *mut c_char,
    ) -> *mut c_char;
    fn ns_html_image_document(url: *const c_char) -> *mut c_char;
    fn ns_html_json_document(url: *const c_char, json: *const c_char, len: usize) -> *mut c_char;
    fn ns_html_xml_document(url: *const c_char, xml: *const c_char, len: usize) -> *mut c_char;
    fn ns_pdf_document_html(data: *const u8, len: usize, url: *const c_char) -> *mut c_char;
    fn ns_css_set_color_scheme(scheme: c_int);
    fn ns_css_set_reduced_motion(motion: c_int);
    fn g_byte_array_new() -> *mut GByteArray;
    fn g_byte_array_set_size(array: *mut GByteArray, length: c_uint) -> *mut GByteArray;
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
    fn g_clear_error(err: *mut *mut GError);
    fn g_canonicalize_filename(filename: *const c_char, relative_to: *const c_char) -> *mut c_char;
    fn g_filename_to_uri(
        filename: *const c_char,
        hostname: *const c_char,
        error: *mut *mut GError,
    ) -> *mut c_char;
}

fn opt(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

fn take(p: *mut c_char) -> Option<GStr> {
    unsafe { GStr::take(p) }
}

fn text<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub struct Post<'a> {
    pub body: *const c_void,
    pub len: usize,
    pub content_type: Option<&'a CStr>,
}

pub struct Response(NonNull<NsResponse>);

impl Response {
    pub fn navigate(
        url: &CStr,
        referrer: Option<&CStr>,
        user_activated: bool,
        post: Option<&Post<'_>>,
    ) -> Option<Response> {
        let mut error: *mut GError = ptr::null_mut();
        let resp = match post {
            Some(post) => unsafe {
                ns_engine_navigate_post_blocking(
                    url.as_ptr(),
                    opt(referrer),
                    post.body,
                    post.len,
                    opt(post.content_type),
                    glib::boolean(user_activated),
                    &mut error,
                )
            },
            None => unsafe {
                ns_engine_navigate_blocking(
                    url.as_ptr(),
                    opt(referrer),
                    glib::boolean(user_activated),
                    &mut error,
                )
            },
        };
        unsafe { g_clear_error(&mut error) };
        NonNull::new(resp).map(Response)
    }

    fn raw(&self) -> &NsResponse {
        unsafe { self.0.as_ref() }
    }

    fn raw_mut(&mut self) -> &mut NsResponse {
        unsafe { self.0.as_mut() }
    }

    pub fn status(&self) -> c_long {
        self.raw().status
    }

    pub fn error(&self) -> Option<&CStr> {
        text(self.raw().error)
    }

    pub fn has_body(&self) -> bool {
        !self.raw().body.is_null()
    }

    pub fn body(&self) -> &[u8] {
        match unsafe { self.raw().body.as_ref() } {
            Some(body) => unsafe { glib::slice(body.data, body.len as usize) },
            None => &[],
        }
    }

    pub fn final_url(&self) -> Option<&CStr> {
        text(self.raw().final_url)
    }

    pub fn content_type(&self) -> Option<&CStr> {
        text(self.raw().content_type)
    }

    pub fn refresh(&self) -> Option<&CStr> {
        text(self.raw().refresh)
    }

    pub fn content_language(&self) -> Option<&CStr> {
        text(self.raw().content_language)
    }

    pub fn csp_header(&self) -> Option<&CStr> {
        text(self.raw().csp_header)
    }

    pub fn raw_headers(&self) -> Option<&CStr> {
        text(self.raw().raw_headers)
    }

    pub fn remote_ip(&self) -> Option<&CStr> {
        text(self.raw().remote_ip)
    }

    pub fn security(&self) -> c_int {
        self.raw().security
    }

    pub fn timing(&self) -> NavigationTimingData {
        let r = self.raw();
        NavigationTimingData {
            origin_us: r.request_start_us,
            origin_real_ms: r.request_start_real_ms,
            domain_lookup_end_ms: r.domain_lookup_ms,
            connect_start_ms: r.domain_lookup_ms,
            connect_end_ms: r.connect_ms,
            secure_connection_start_ms: if r.connect_ms < r.tls_ms {
                r.connect_ms
            } else {
                0.0
            },
            request_start_ms: r.pretransfer_ms,
            response_start_ms: r.response_start_ms,
            response_end_ms: r.response_end_ms,
            ..NavigationTimingData::default()
        }
    }

    fn body_parts(&self) -> (*const u8, usize) {
        match unsafe { self.raw().body.as_ref() } {
            Some(body) => (body.data, body.len as usize),
            None => (ptr::null(), 0),
        }
    }

    pub fn decode_body(&self, want_charset: bool) -> (Option<GStr>, Option<GStr>) {
        let (data, len) = self.body_parts();
        let mut charset: *mut c_char = ptr::null_mut();
        let out = unsafe {
            ns_html_decode_body_full(
                data.cast(),
                len,
                opt(self.content_type()),
                if want_charset {
                    &mut charset
                } else {
                    ptr::null_mut()
                },
            )
        };
        (take(out), take(charset))
    }

    pub fn pdf_document(&self, url: &CStr) -> Option<GStr> {
        let (data, len) = self.body_parts();
        take(unsafe { ns_pdf_document_html(data, len, url.as_ptr()) })
    }

    pub fn set_body(&mut self, bytes: &[u8]) {
        let r = self.raw_mut();
        if r.body.is_null() {
            r.body = unsafe { g_byte_array_new() };
        }
        unsafe {
            g_byte_array_set_size(r.body, 0);
            g_byte_array_append(r.body, bytes.as_ptr(), bytes.len() as c_uint);
        }
    }

    fn replace_text(slot: &mut *mut c_char, value: Option<&CStr>) {
        let new = value.map_or(ptr::null_mut(), |v| unsafe { glib::g_strdup(v.as_ptr()) });
        let old = core::mem::replace(slot, new);
        unsafe { glib::g_free(old.cast()) };
    }

    pub fn set_content_type(&mut self, value: &CStr) {
        Response::replace_text(&mut self.raw_mut().content_type, Some(value));
    }

    pub fn set_final_url(&mut self, value: &CStr) {
        Response::replace_text(&mut self.raw_mut().final_url, Some(value));
    }

    pub fn clear_error(&mut self) {
        Response::replace_text(&mut self.raw_mut().error, None);
    }

    pub fn set_security(&mut self, security: c_int) {
        self.raw_mut().security = security;
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe { ns_response_free(self.0.as_ptr()) };
    }
}

pub fn build_error_page(
    url: &CStr,
    status: c_long,
    transport_error: Option<&CStr>,
) -> Option<GStr> {
    take(unsafe { ns_build_error_page(url.as_ptr(), status, opt(transport_error)) })
}

pub fn url_host_from(url: &CStr) -> Option<GStr> {
    take(unsafe { ns_url_host_from(url.as_ptr()) })
}

pub fn url_strip_tracking_params(url: &CStr) -> Option<GStr> {
    take(unsafe { ns_url_strip_tracking_params(url.as_ptr()) })
}

pub fn https_first_upgrade(url: &CStr) -> Option<GStr> {
    take(unsafe { ns_net_https_first_upgrade(url.as_ptr()) })
}

pub fn safebrowsing_blocked(host: &CStr) -> bool {
    unsafe { ns_safebrowsing_blocked(host.as_ptr()) != 0 }
}

pub fn safebrowsing_allow_host(host: &CStr) {
    unsafe { ns_safebrowsing_allow_host(host.as_ptr()) };
}

pub fn safebrowsing_interstitial(url: &CStr, host: &CStr) -> Option<GStr> {
    take(unsafe { ns_safebrowsing_interstitial(url.as_ptr(), host.as_ptr()) })
}

pub struct ParsedDoc(pub(super) *mut NsNode);

pub fn html_parse(input: Option<&CStr>, scripting: bool) -> ParsedDoc {
    let (p, len) = input.map_or((ptr::null(), 0), |s| {
        (s.as_ptr(), s.to_bytes().len() as isize)
    });
    ParsedDoc(if scripting {
        unsafe { ns_html_parse(p, len) }
    } else {
        unsafe { ns_html_parse_with_scripting(p, len, 0) }
    })
}

pub fn html_escape_text(s: &CStr) -> Option<GStr> {
    take(unsafe { ns_html_escape_text(s.as_ptr()) })
}

pub fn image_document(url: &CStr) -> Option<GStr> {
    take(unsafe { ns_html_image_document(url.as_ptr()) })
}

pub fn json_document(url: &CStr, json: Option<&CStr>) -> Option<GStr> {
    let len = json.map_or(0, |j| j.to_bytes().len());
    take(unsafe { ns_html_json_document(url.as_ptr(), opt(json), len) })
}

pub fn xml_document(url: &CStr, xml: Option<&CStr>) -> Option<GStr> {
    let len = xml.map_or(0, |x| x.to_bytes().len());
    take(unsafe { ns_html_xml_document(url.as_ptr(), opt(xml), len) })
}

pub fn css_set_color_scheme(dark: bool) {
    unsafe { ns_css_set_color_scheme(c_int::from(dark)) };
}

pub fn css_set_reduced_motion(reduce: bool) {
    unsafe { ns_css_set_reduced_motion(c_int::from(reduce)) };
}

pub fn file_exists(path: &CStr) -> bool {
    unsafe { glib::g_file_test(path.as_ptr(), glib::FILE_TEST_EXISTS) != 0 }
}

pub fn local_file_uri(path: &CStr) -> Option<GStr> {
    let abs = take(unsafe { g_canonicalize_filename(path.as_ptr(), ptr::null()) })?;
    take(unsafe { g_filename_to_uri(abs.as_ptr(), ptr::null(), ptr::null_mut()) })
}
