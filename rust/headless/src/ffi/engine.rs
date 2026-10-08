//! Southstar — the engine, network, document, style, paint, animation, image and video calls of the in-process run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::mem::size_of;
use core::ptr::{self, NonNull};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GHashTable, GPtrArray, GStr};
use southstar_layout::{BoxRef, NsBox};

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
pub struct GString {
    str: *mut c_char,
    len: usize,
    allocated_len: usize,
}

#[repr(C)]
struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    _content_disposition: *mut c_char,
    _csp_header: *mut c_char,
    _xframe_options: *mut c_char,
    _x_content_type_options: *mut c_char,
    _cors_allow_origin: *mut c_char,
    _refresh: *mut c_char,
    content_language: *mut c_char,
    _raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
    _tls_warning: *mut c_char,
    _remote_ip: *mut c_char,
    _next_hop_protocol: *mut c_char,
    request_start_us: i64,
    request_start_real_ms: f64,
    domain_lookup_ms: f64,
    connect_ms: f64,
    tls_ms: f64,
    pretransfer_ms: f64,
    response_start_ms: f64,
    response_end_ms: f64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::offset_of!(NsResponse, response_end_ms) == 184);

#[repr(C)]
struct NsBoxMedia {
    _image_src: *mut c_char,
    _image: *mut c_void,
    _bg_image_src: *mut c_char,
    _bg_image: *mut c_void,
    _marker_image_src: *mut c_char,
    _marker_image: *mut c_void,
    _border_image_src: *mut c_char,
    _border_image: *mut c_void,
    _bg_layer_srcs: *mut GPtrArray,
    _bg_layer_images: *mut GPtrArray,
    video_src: *mut c_char,
    video_poster: *mut c_char,
    _video_audio_src: *mut c_char,
    video: *mut NsVideo,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::offset_of!(NsBoxMedia, video) == 104);

#[repr(C)]
struct NsVideo {
    url: *mut c_char,
    natural_width: c_int,
    natural_height: c_int,
    poster_texture: *mut c_void,
    _poster_url: *mut c_char,
    frame_texture: *mut c_void,
    _loaded: GBoolean,
    _load_events_fired: GBoolean,
    _failed: GBoolean,
    player: *mut c_void,
    _dom_node: *const c_void,
    _is_camera: GBoolean,
    _autoplay: GBoolean,
    _loop: GBoolean,
    _controls: GBoolean,
    _muted: GBoolean,
    _has_audio: GBoolean,
    _audio_opened: GBoolean,
    _audio_url: *mut c_char,
    _audio_file: *mut c_char,
    _audio_file_len: usize,
    _audio_file_gen: c_uint,
    _audio_timeline_start: f64,
    _pending_audio_file: *mut c_char,
    _pending_audio_file_len: usize,
    _pending_audio_file_gen: c_uint,
    _pending_audio_start: f64,
    _video_file: *mut c_char,
    _video_file_len: usize,
    _video_file_gen: c_uint,
    _pending_video_file: *mut c_char,
    _pending_video_file_len: usize,
    _pending_video_file_gen: c_uint,
    _pending_video_start: f64,
    _video_opened: GBoolean,
    _rect: [f64; 4],
    _clip: [f64; 4],
    _rect_fit: c_int,
    _last_paint_us: i64,
    _rect_dirty: GBoolean,
    _rect_sent_us: i64,
    _sent_rect: [f64; 4],
    _sent_clip: [f64; 4],
    _sent_rect_fit: c_int,
    _sent_rect_page: GBoolean,
    _token: *mut c_char,
    _playing: GBoolean,
    _ended: GBoolean,
    _volume: f64,
    _meta_sent: GBoolean,
    _buf_sent: GBoolean,
    _sent_buffered_end: f64,
    _seq: c_uint,
    _base_us: i64,
    _last_refresh_us: i64,
    _stalled: GBoolean,
    _stall_since_us: i64,
    _mse_id: c_uint,
    _cur_time: f64,
    _prev_tick_time: f64,
    _last_emit_time: f64,
    duration: f64,
    _cues: *mut GPtrArray,
    _track_requested: GBoolean,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<NsVideo>() == 544
        && core::mem::offset_of!(NsVideo, player) == 56
        && core::mem::offset_of!(NsVideo, duration) == 520
);

#[repr(C)]
struct NsLinkRange {
    _start: usize,
    _len: usize,
    href: *mut c_char,
    _target: *mut c_char,
    dom: *const NsNode,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PrintSetup {
    pub width: f64,
    pub height: f64,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
}

#[repr(C)]
struct PageRule {
    _private: [u8; 0],
}

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
    fn ns_engine_fetch_blocking(
        url: *const c_char,
        top_url: *const c_char,
        error: *mut *mut GError,
    ) -> *mut NsResponse;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_build_error_page(
        url: *const c_char,
        status: c_long,
        transport_error: *const c_char,
    ) -> *mut c_char;
    fn ns_html_image_document(url: *const c_char) -> *mut c_char;
    fn ns_html_json_document(url: *const c_char, json: *const c_char, len: usize) -> *mut c_char;
    fn ns_html_xml_document(url: *const c_char, xml: *const c_char, len: usize) -> *mut c_char;
    fn ns_html_decode_body_full(
        body: *const c_char,
        len: usize,
        content_type: *const c_char,
        charset_out: *mut *mut c_char,
    ) -> *mut c_char;
    fn ns_html_parse(input: *const c_char, len: isize) -> *mut NsNode;
    fn ns_html_parse_with_scripting(
        input: *const c_char,
        len: isize,
        scripting: GBoolean,
    ) -> *mut NsNode;
    fn ns_node_free(node: *mut NsNode);
    fn ns_node_dump(node: *const NsNode) -> *mut GString;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_print_setup_default(setup: *mut PrintSetup);
    fn ns_print_setup_apply_page_rule(setup: *mut PrintSetup, rule: *const PageRule);
    fn ns_render_page_rule() -> *const PageRule;
    fn ns_css_set_print_media(printing: GBoolean);
    fn ns_css_set_viewport(vw: f64, vh: f64);
    fn ns_css_set_target_fragment(fragment: *const c_char);
    fn ns_css_set_doc_language(lang: *const c_char);
    fn ns_css_set_active_node(node: *const NsNode) -> *const NsNode;
    fn ns_engine_compute_cascade(
        doc: *mut NsNode,
        base_url: *const c_char,
        css_cache: *mut GHashTable,
        anim: *mut c_void,
    ) -> *mut GHashTable;
    fn ns_engine_relayout(
        doc: *mut NsNode,
        base_url: *const c_char,
        viewport_width: c_int,
        viewport_height: f64,
        images: *mut c_void,
        anim: *mut c_void,
        js: *mut c_void,
        css_cache: *mut GHashTable,
        focused: *const NsNode,
        hover: *const NsNode,
        caret_byte: usize,
        sel_anchor_byte: usize,
        out_layout: *mut *mut NsBox,
    ) -> *mut GHashTable;
    fn ns_engine_load_keyframes(
        anim: *mut c_void,
        doc: *mut NsNode,
        base_url: *const c_char,
        css_cache: *mut GHashTable,
    );
    fn ns_engine_anim_observe(anim: *mut c_void, styles: *mut GHashTable, now_us: i64);
    fn ns_engine_dump_text(root: *const NsBox, out: *mut GString);
    fn ns_engine_dump_layout(root: *const NsBox, indent: c_int, out: *mut GString);
    fn ns_engine_fetch_images(root: *mut NsBox, base_url: *const c_char, cache: *mut c_void);
    fn ns_engine_suffix_before_ext(path: *const c_char, suffix: *const c_char) -> *mut c_char;
    fn ns_engine_write_png(root: *const NsBox, path: *const c_char) -> c_int;
    fn ns_engine_write_pdf(root: *const NsBox, path: *const c_char) -> c_int;
    fn ns_engine_write_pdf_paged(
        root: *const NsBox,
        path: *const c_char,
        setup: *const PrintSetup,
    ) -> c_int;
    fn ns_box_free(b: *mut NsBox);
    fn ns_box_hit_link_range(root: *const NsBox, x: f64, y: f64) -> *const NsLinkRange;
    fn ns_box_hit_form_dom(root: *const NsBox, x: f64, y: f64) -> *const NsNode;
    fn ns_box_hit_inline_dom(root: *const NsBox, x: f64, y: f64) -> *const NsNode;
    fn ns_layout_collect_videos(root: *const NsBox, out: *mut GPtrArray);
    fn ns_paint_3d_invalidate();
    fn ns_paint_set_anim(anim: *mut c_void);
    fn ns_paint_set_js(js: *mut c_void);
    fn ns_anim_new() -> *mut c_void;
    fn ns_anim_free(anim: *mut c_void);
    fn ns_anim_tick(anim: *mut c_void, now_us: i64) -> GBoolean;
    fn ns_anim_rebase(anim: *mut c_void, base_us: i64);
    fn ns_anim_needs_layout(anim: *const c_void) -> GBoolean;
    fn ns_image_cache_new() -> *mut c_void;
    fn ns_image_cache_free(cache: *mut c_void);
    fn ns_image_cache_tick(cache: *mut c_void, now_us: i64) -> GBoolean;
    fn ns_image_decode_bytes(
        data: *const u8,
        len: usize,
        w: *mut c_int,
        h: *mut c_int,
    ) -> *mut c_void;
    fn ns_texture_ref(texture: *mut c_void) -> *mut c_void;
    fn ns_texture_unref(texture: *mut c_void);
    fn ns_video_cache_new() -> *mut c_void;
    fn ns_video_cache_free(cache: *mut c_void);
    fn ns_video_cache_tick(cache: *mut c_void, now_us: i64) -> GBoolean;
    fn ns_video_cache_discover(
        cache: *mut c_void,
        root: *const NsBox,
        doc: *const NsNode,
        now_us: i64,
    );
    fn ns_video_cache_set_base(cache: *mut c_void, base_url: *const c_char);
    fn ns_video_player_new(bytes: *const u8, len: usize) -> *mut c_void;
    fn ns_video_player_free(player: *mut c_void);
    fn ns_video_player_width(player: *const c_void) -> c_int;
    fn ns_video_player_height(player: *const c_void) -> c_int;
    fn ns_video_player_duration(player: *const c_void) -> f64;
    fn ns_video_player_frame_at(
        player: *mut c_void,
        seconds: f64,
        looped: GBoolean,
        ended: *mut GBoolean,
    ) -> *mut c_void;
    fn g_hash_table_new_full(
        hash: unsafe extern "C" fn(*const c_void) -> c_uint,
        equal: unsafe extern "C" fn(*const c_void, *const c_void) -> GBoolean,
        key_destroy: glib::GDestroyNotify,
        value_destroy: glib::GDestroyNotify,
    ) -> *mut GHashTable;
    fn g_bytes_unref(bytes: *mut c_void);
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_free(s: *mut GString, free_segment: GBoolean) -> *mut c_char;
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn ptr_or_null(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

pub fn take_string(p: *mut c_char) -> Option<Vec<u8>> {
    unsafe { GStr::take(p) }.map(|s| s.to_bytes().to_vec())
}

pub struct Timing {
    pub request_start_us: i64,
    pub request_start_real_ms: f64,
    pub domain_lookup_ms: f64,
    pub connect_ms: f64,
    pub tls_ms: f64,
    pub pretransfer_ms: f64,
    pub response_start_ms: f64,
    pub response_end_ms: f64,
}

pub struct Response {
    pub status: c_long,
    pub final_url: Option<Vec<u8>>,
    pub content_type: Option<Vec<u8>>,
    pub content_language: Option<Vec<u8>>,
    pub body: Option<Vec<u8>>,
    pub error: Option<Vec<u8>>,
    pub timing: Timing,
}

fn owned(p: *const c_char) -> Option<Vec<u8>> {
    c_str(p).map(|s| s.to_bytes().to_vec())
}

unsafe fn take_response(resp: *mut NsResponse) -> Option<Response> {
    let r = unsafe { resp.as_ref() }?;
    let body = unsafe { r.body.as_ref() }.map(|b| {
        if b.data.is_null() || b.len == 0 {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(b.data, b.len as usize) }.to_vec()
        }
    });
    let out = Response {
        status: r.status,
        final_url: owned(r.final_url),
        content_type: owned(r.content_type),
        content_language: owned(r.content_language),
        body,
        error: owned(r.error),
        timing: Timing {
            request_start_us: r.request_start_us,
            request_start_real_ms: r.request_start_real_ms,
            domain_lookup_ms: r.domain_lookup_ms,
            connect_ms: r.connect_ms,
            tls_ms: r.tls_ms,
            pretransfer_ms: r.pretransfer_ms,
            response_start_ms: r.response_start_ms,
            response_end_ms: r.response_end_ms,
        },
    };
    unsafe { ns_response_free(resp) };
    Some(out)
}

fn error_message(err: *mut GError) -> Option<Vec<u8>> {
    let e = unsafe { err.as_ref() }?;
    let msg = owned(e.message);
    unsafe { glib::g_error_free(err) };
    msg
}

pub fn navigate(
    url: &CStr,
    top_url: Option<&CStr>,
    post: Option<(&[u8], &CStr)>,
    user_activated: bool,
) -> Result<Response, Option<Vec<u8>>> {
    let mut err: *mut GError = ptr::null_mut();
    let resp = unsafe {
        match post {
            Some((body, content_type)) => ns_engine_navigate_post_blocking(
                url.as_ptr(),
                ptr_or_null(top_url),
                body.as_ptr().cast(),
                body.len(),
                content_type.as_ptr(),
                glib::boolean(user_activated),
                &mut err,
            ),
            None => ns_engine_navigate_blocking(
                url.as_ptr(),
                ptr_or_null(top_url),
                glib::boolean(user_activated),
                &mut err,
            ),
        }
    };
    match unsafe { take_response(resp) } {
        Some(r) => {
            drop(error_message(err));
            Ok(r)
        }
        None => Err(error_message(err)),
    }
}

pub fn fetch(url: &CStr, top_url: Option<&CStr>) -> Option<Response> {
    unsafe {
        take_response(ns_engine_fetch_blocking(
            url.as_ptr(),
            ptr_or_null(top_url),
            ptr::null_mut(),
        ))
    }
}

pub fn error_page(url: Option<&CStr>, status: c_long, transport_error: Option<&CStr>) -> Vec<u8> {
    take_string(unsafe {
        ns_build_error_page(ptr_or_null(url), status, ptr_or_null(transport_error))
    })
    .unwrap_or_default()
}

pub fn image_document(url: Option<&CStr>) -> Vec<u8> {
    take_string(unsafe { ns_html_image_document(ptr_or_null(url)) }).unwrap_or_default()
}

pub fn json_document(url: Option<&CStr>, json: Option<&CStr>) -> Option<Vec<u8>> {
    let len = json.map_or(0, |j| j.to_bytes().len());
    take_string(unsafe { ns_html_json_document(ptr_or_null(url), ptr_or_null(json), len) })
}

pub fn xml_document(url: Option<&CStr>, xml: Option<&CStr>) -> Option<Vec<u8>> {
    let len = xml.map_or(0, |x| x.to_bytes().len());
    take_string(unsafe { ns_html_xml_document(ptr_or_null(url), ptr_or_null(xml), len) })
}

pub fn decode_body(
    body: Option<&[u8]>,
    content_type: Option<&CStr>,
) -> (Option<GStr>, Option<GStr>) {
    let (data, len) = body.map_or((ptr::null(), 0), |b| (b.as_ptr().cast::<c_char>(), b.len()));
    let mut charset: *mut c_char = ptr::null_mut();
    let decoded =
        unsafe { ns_html_decode_body_full(data, len, ptr_or_null(content_type), &mut charset) };
    unsafe { (GStr::take(decoded), GStr::take(charset)) }
}

pub fn decode_body_text(body: &[u8], content_type: Option<&CStr>) -> Option<GStr> {
    let decoded = unsafe {
        ns_html_decode_body_full(
            body.as_ptr().cast(),
            body.len(),
            ptr_or_null(content_type),
            ptr::null_mut(),
        )
    };
    unsafe { GStr::take(decoded) }
}

pub fn url_resolve(base: &CStr, href: &CStr) -> Option<Vec<u8>> {
    take_string(unsafe { ns_url_resolve(base.as_ptr(), href.as_ptr()) })
}

pub fn suffix_before_ext(path: Option<&CStr>, suffix: &CStr) -> Option<GStr> {
    unsafe {
        GStr::take(ns_engine_suffix_before_ext(
            ptr_or_null(path),
            suffix.as_ptr(),
        ))
    }
}

pub struct Document(NonNull<NsNode>);

impl Document {
    pub fn parse(html: Option<&CStr>, scripting: bool) -> Option<Document> {
        let (input, len) = html.map_or((c"".as_ptr(), 0), |h| {
            (h.as_ptr(), h.to_bytes().len() as isize)
        });
        let doc = unsafe {
            if scripting {
                ns_html_parse(input, len)
            } else {
                ns_html_parse_with_scripting(input, len, glib::FALSE)
            }
        };
        NonNull::new(doc).map(Document)
    }

    pub fn node(&self) -> Node<'_> {
        unsafe { Node::from_ptr(self.0.as_ptr()) }.expect("document node")
    }

    pub fn as_ptr(&self) -> *mut NsNode {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_node_free(self.0.as_ptr()) };
    }
}

pub fn node_dump(node: Node) -> Vec<u8> {
    gstring_take(unsafe { ns_node_dump(node.as_ptr()) })
}

fn gstring_take(s: *mut GString) -> Vec<u8> {
    let Some(g) = (unsafe { s.as_ref() }) else {
        return Vec::new();
    };
    let bytes = if g.str.is_null() {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(g.str.cast::<u8>(), g.len) }.to_vec()
    };
    unsafe { g_string_free(s, glib::TRUE) };
    bytes
}

pub fn with_gstring(fill: impl FnOnce(*mut GString)) -> Vec<u8> {
    let s = unsafe { g_string_new(ptr::null()) };
    fill(s);
    gstring_take(s)
}

pub fn dump_text(root: *const NsBox) -> Vec<u8> {
    with_gstring(|s| unsafe { ns_engine_dump_text(root, s) })
}

pub fn dump_layout(root: *const NsBox) -> Vec<u8> {
    with_gstring(|s| unsafe { ns_engine_dump_layout(root, 0, s) })
}

pub fn print_setup_default() -> PrintSetup {
    let mut setup = PrintSetup::default();
    unsafe { ns_print_setup_default(&mut setup) };
    setup
}

pub fn apply_render_page_rule(setup: &mut PrintSetup) {
    unsafe { ns_print_setup_apply_page_rule(setup, ns_render_page_rule()) };
}

pub fn css_set_print_media(printing: bool) {
    unsafe { ns_css_set_print_media(glib::boolean(printing)) };
}

pub fn css_set_viewport(vw: f64, vh: f64) {
    unsafe { ns_css_set_viewport(vw, vh) };
}

pub fn css_set_target_fragment(fragment: Option<&CStr>) {
    unsafe { ns_css_set_target_fragment(ptr_or_null(fragment)) };
}

pub fn css_set_doc_language(lang: Option<&CStr>) {
    unsafe { ns_css_set_doc_language(ptr_or_null(lang)) };
}

pub fn css_set_active_node(node: Option<Node>) {
    unsafe { ns_css_set_active_node(Node::ptr_or_null(node)) };
}

unsafe extern "C" fn bytes_unref(p: *mut c_void) {
    unsafe { g_bytes_unref(p) };
}

pub fn new_css_cache() -> *mut GHashTable {
    unsafe {
        g_hash_table_new_full(
            glib::g_str_hash,
            glib::g_str_equal,
            Some(glib::g_free),
            Some(bytes_unref),
        )
    }
}

pub fn compute_cascade(
    doc: &Document,
    base: Option<&CStr>,
    css_cache: *mut GHashTable,
) -> *mut GHashTable {
    unsafe {
        ns_engine_compute_cascade(doc.as_ptr(), ptr_or_null(base), css_cache, ptr::null_mut())
    }
}

pub fn destroy_table(table: *mut GHashTable) {
    unsafe { glib::g_hash_table_destroy(table) };
}

#[derive(Clone, Copy)]
pub struct Anim(NonNull<c_void>);

impl Anim {
    pub fn new() -> Option<Anim> {
        NonNull::new(unsafe { ns_anim_new() }).map(Anim)
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_anim_free(self.raw()) };
    }

    pub fn tick(self, now_us: i64) -> bool {
        unsafe { ns_anim_tick(self.raw(), now_us) != 0 }
    }

    pub fn rebase(self, base_us: i64) {
        unsafe { ns_anim_rebase(self.raw(), base_us) };
    }

    pub fn needs_layout(self) -> bool {
        unsafe { ns_anim_needs_layout(self.raw()) != 0 }
    }

    pub fn load_keyframes(self, doc: &Document, base: Option<&CStr>, css_cache: *mut GHashTable) {
        unsafe { ns_engine_load_keyframes(self.raw(), doc.as_ptr(), ptr_or_null(base), css_cache) };
    }

    pub fn observe(self, styles: *mut GHashTable, now_us: i64) {
        unsafe { ns_engine_anim_observe(self.raw(), styles, now_us) };
    }
}

pub fn anim_raw(anim: Option<Anim>) -> *mut c_void {
    anim.map_or(ptr::null_mut(), Anim::raw)
}

pub fn paint_set_anim(anim: Option<Anim>) {
    unsafe { ns_paint_set_anim(anim_raw(anim)) };
}

pub fn paint_set_js(js: *mut c_void) {
    unsafe { ns_paint_set_js(js) };
}

#[derive(Clone, Copy)]
pub struct ImageCache(NonNull<c_void>);

impl ImageCache {
    pub fn new() -> Option<ImageCache> {
        NonNull::new(unsafe { ns_image_cache_new() }).map(ImageCache)
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_image_cache_free(self.raw()) };
    }

    pub fn tick(self, now_us: i64) {
        unsafe { ns_image_cache_tick(self.raw(), now_us) };
    }
}

pub fn image_cache_raw(cache: Option<ImageCache>) -> *mut c_void {
    cache.map_or(ptr::null_mut(), ImageCache::raw)
}

#[derive(Clone, Copy)]
pub struct VideoCache(NonNull<c_void>);

impl VideoCache {
    pub fn new() -> Option<VideoCache> {
        NonNull::new(unsafe { ns_video_cache_new() }).map(VideoCache)
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub fn free(self) {
        unsafe { ns_video_cache_free(self.raw()) };
    }

    pub fn tick(self, now_us: i64) {
        unsafe { ns_video_cache_tick(self.raw(), now_us) };
    }

    pub fn discover(self, root: *const NsBox, doc: *const NsNode, now_us: i64) {
        unsafe { ns_video_cache_discover(self.raw(), root, doc, now_us) };
    }

    pub fn set_base(self, base: Option<&CStr>) {
        unsafe { ns_video_cache_set_base(self.raw(), ptr_or_null(base)) };
    }
}

pub struct Relayout<'a> {
    pub doc: *mut NsNode,
    pub base: Option<&'a CStr>,
    pub vw: c_int,
    pub vh: f64,
    pub images: *mut c_void,
    pub anim: *mut c_void,
    pub js: *mut c_void,
    pub css_cache: *mut GHashTable,
    pub focused: *const NsNode,
    pub caret: usize,
    pub anchor: usize,
}

pub fn relayout(r: &Relayout, out_layout: *mut *mut NsBox) -> *mut GHashTable {
    unsafe {
        ns_engine_relayout(
            r.doc,
            ptr_or_null(r.base),
            r.vw,
            r.vh,
            r.images,
            r.anim,
            r.js,
            r.css_cache,
            r.focused,
            ptr::null(),
            r.caret,
            r.anchor,
            out_layout,
        )
    }
}

pub fn free_layout(layout: *mut NsBox) {
    unsafe {
        ns_paint_3d_invalidate();
        ns_box_free(layout);
    }
}

pub fn fetch_images(root: *mut NsBox, base: Option<&CStr>, cache: Option<ImageCache>) {
    unsafe { ns_engine_fetch_images(root, ptr_or_null(base), image_cache_raw(cache)) };
}

pub fn write_png(root: *const NsBox, path: Option<&CStr>) -> c_int {
    unsafe { ns_engine_write_png(root, ptr_or_null(path)) }
}

pub fn write_pdf(root: *const NsBox, path: Option<&CStr>) -> c_int {
    unsafe { ns_engine_write_pdf(root, ptr_or_null(path)) }
}

pub fn write_pdf_paged(root: *const NsBox, path: Option<&CStr>, setup: &PrintSetup) -> c_int {
    unsafe { ns_engine_write_pdf_paged(root, ptr_or_null(path), setup) }
}

pub struct LinkHit<'a> {
    pub dom: Option<Node<'a>>,
    pub href: Option<&'a CStr>,
}

pub fn hit_link(root: BoxRef<'_>, x: f64, y: f64) -> Option<LinkHit<'_>> {
    let link = unsafe { ns_box_hit_link_range(root.as_ptr(), x, y).as_ref() }?;
    Some(LinkHit {
        dom: unsafe { Node::from_ptr(link.dom) },
        href: c_str(link.href),
    })
}

pub fn hit_form_dom(root: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(ns_box_hit_form_dom(root.as_ptr(), x, y)) }
}

pub fn hit_inline_dom(root: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(ns_box_hit_inline_dom(root.as_ptr(), x, y)) }
}

pub fn collect_videos(root: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    let array = unsafe { glib::g_ptr_array_new() };
    unsafe { ns_layout_collect_videos(root.as_ptr(), array) };
    let a = unsafe { &*array };
    let boxes = (0..a.len as usize)
        .filter_map(|i| unsafe { BoxRef::from_ptr((*a.pdata.add(i)).cast()) })
        .collect();
    unsafe { glib::g_ptr_array_free(array, glib::TRUE) };
    boxes
}

fn media(b: BoxRef<'_>) -> Option<&mut NsBoxMedia> {
    unsafe { b.media_ptr().cast::<NsBoxMedia>().as_mut() }
}

pub fn video_sources(b: BoxRef<'_>) -> Option<(Option<&CStr>, Option<&CStr>)> {
    let m = media(b)?;
    m.video
        .is_null()
        .then(|| (c_str(m.video_src), c_str(m.video_poster)))
}

pub struct Video(NonNull<NsVideo>);

impl Video {
    fn alloc(url: &CStr) -> Video {
        let v = unsafe { glib::g_malloc0(size_of::<NsVideo>()) }.cast::<NsVideo>();
        let v = NonNull::new(v).expect("g_malloc0");
        unsafe { (*v.as_ptr()).url = glib::g_strdup(url.as_ptr()) };
        Video(v)
    }

    pub fn from_player(url: &CStr, body: &[u8]) -> Option<Video> {
        let player = unsafe { ns_video_player_new(body.as_ptr(), body.len()) };
        if player.is_null() {
            return None;
        }
        let mut ended: GBoolean = glib::FALSE;
        let frame = unsafe { ns_video_player_frame_at(player, 0.0, glib::FALSE, &mut ended) };
        let video = Video::alloc(url);
        let v = unsafe { &mut *video.0.as_ptr() };
        v.player = player;
        v.natural_width = unsafe { ns_video_player_width(player) };
        v.natural_height = unsafe { ns_video_player_height(player) };
        v.duration = unsafe { ns_video_player_duration(player) };
        if !frame.is_null() {
            v.frame_texture = unsafe { ns_texture_ref(frame) };
        }
        Some(video)
    }

    pub fn from_poster(url: &CStr, body: &[u8]) -> Option<Video> {
        let (mut w, mut h): (c_int, c_int) = (0, 0);
        let texture = unsafe { ns_image_decode_bytes(body.as_ptr(), body.len(), &mut w, &mut h) };
        if texture.is_null() {
            return None;
        }
        let video = Video::alloc(url);
        let v = unsafe { &mut *video.0.as_ptr() };
        v.poster_texture = texture;
        v.natural_width = w;
        v.natural_height = h;
        Some(video)
    }

    pub fn attach(self, b: BoxRef) -> Result<(), Video> {
        match media(b).filter(|m| m.video.is_null()) {
            Some(m) => {
                m.video = self.0.as_ptr();
                Ok(())
            }
            None => Err(self),
        }
    }

    pub fn discard(self) {
        let v = unsafe { &mut *self.0.as_ptr() };
        unsafe {
            if !v.player.is_null() {
                ns_video_player_free(v.player);
            }
            if !v.frame_texture.is_null() {
                ns_texture_unref(v.frame_texture);
            }
            if !v.poster_texture.is_null() {
                ns_texture_unref(v.poster_texture);
            }
            glib::g_free(v.url.cast());
            glib::g_free(self.0.as_ptr().cast());
        }
    }
}
