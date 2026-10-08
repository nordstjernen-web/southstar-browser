//! Southstar — the C ABI of the web font loader, as declared in src/font.h, over the network layer, GLib, FreeType, fontconfig and Pango.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean, GError};

use crate::Descriptors;

const FETCH_DEST_FONT: c_int = 4;

#[repr(C)]
pub struct NsFontDescriptors {
    weight: c_int,
    slant: c_int,
}

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct NsResponse {
    status: c_long,
    _headers: [*mut c_char; 10],
    body: *mut GByteArray,
    error: *mut c_char,
}

type ReadyCallback =
    unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, user_data: *mut c_void);
type IdleCallback = unsafe extern "C" fn(user_data: *mut c_void);

unsafe extern "C" {
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_net_request_async(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        extra_headers: *const *const c_char,
        cancellable: *mut c_void,
        callback: Option<ReadyCallback>,
        user_data: *mut c_void,
    );
    fn ns_net_fetch_finish(result: *mut c_void, error: *mut *mut GError) -> *mut NsResponse;
    fn ns_net_accept_headers_for(dest: c_int) -> *const *const c_char;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_paint_register_font_oracle();
    fn g_cancellable_new() -> *mut c_void;
    fn g_cancellable_cancel(cancellable: *mut c_void);
    fn g_object_ref(object: *mut c_void) -> *mut c_void;
    fn g_object_unref(object: *mut c_void);
    fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_get_home_dir() -> *const c_char;
    fn g_compute_checksum_for_data(
        checksum_type: glib::GChecksumType,
        data: *const u8,
        length: usize,
    ) -> *mut c_char;
}

fn take_gstring(s: *mut c_char) -> Option<CString> {
    if s.is_null() {
        return None;
    }
    let owned = unsafe { CStr::from_ptr(s) }.to_owned();
    unsafe { glib::g_free(s.cast()) };
    Some(owned)
}

fn build_filename(parts: &[&CStr]) -> Option<CString> {
    let mut args: Vec<*mut c_char> = parts.iter().map(|p| p.as_ptr().cast_mut()).collect();
    args.push(ptr::null_mut());
    take_gstring(unsafe { glib::g_build_filenamev(args.as_mut_ptr()) })
}

pub fn cache_dir() -> CString {
    let xdg = unsafe { glib::bytes(glib::g_getenv(c"XDG_CACHE_HOME".as_ptr())) }
        .filter(|x| !x.is_empty());
    let base = match xdg {
        Some(xdg) => CString::new(xdg).unwrap_or_default(),
        None => {
            let home = unsafe { CStr::from_ptr(g_get_home_dir()) };
            build_filename(&[home, c".cache"]).unwrap_or_default()
        }
    };
    let dir = build_filename(&[&base, c"southstar", c"webfonts"]).unwrap_or_default();
    unsafe { g_mkdir_with_parents(dir.as_ptr(), 0o700) };
    dir
}

pub fn build_path(dir: &CStr, name: &[u8]) -> Option<CString> {
    let name = CString::new(name).ok()?;
    build_filename(&[dir, &name])
}

pub fn sha256_hex(data: &[u8]) -> Vec<u8> {
    let digest =
        unsafe { g_compute_checksum_for_data(glib::G_CHECKSUM_SHA256, data.as_ptr(), data.len()) };
    take_gstring(digest).map_or_else(|| b"0".to_vec(), CString::into_bytes)
}

pub fn write_file(path: &CStr, data: &[u8]) -> bool {
    let mut error = ptr::null_mut();
    let ok = unsafe {
        g_file_set_contents(
            path.as_ptr(),
            data.as_ptr().cast(),
            data.len() as isize,
            &mut error,
        )
    };
    if !error.is_null() {
        unsafe { glib::g_error_free(error) };
    }
    ok != 0
}

pub fn register_font_oracle() {
    unsafe { ns_paint_register_font_oracle() };
}

pub fn resolve_url(base: Option<&CStr>, href: &CStr) -> Option<CString> {
    match base {
        Some(base) => take_gstring(unsafe { ns_url_resolve(base.as_ptr(), href.as_ptr()) }),
        None => Some(href.to_owned()),
    }
}

pub struct Cancellable(NonNull<c_void>);

unsafe impl Send for Cancellable {}

impl Cancellable {
    pub fn new() -> Self {
        Cancellable(NonNull::new(unsafe { g_cancellable_new() }).expect("g_cancellable_new"))
    }

    pub fn cancel(&self) {
        unsafe { g_cancellable_cancel(self.0.as_ptr()) };
    }
}

impl Clone for Cancellable {
    fn clone(&self) -> Self {
        unsafe { g_object_ref(self.0.as_ptr()) };
        Cancellable(self.0)
    }
}

impl Drop for Cancellable {
    fn drop(&mut self) {
        unsafe { g_object_unref(self.0.as_ptr()) };
    }
}

pub struct IdleWaiter {
    callback: IdleCallback,
    user_data: *mut c_void,
}

unsafe impl Send for IdleWaiter {}

impl PartialEq for IdleWaiter {
    fn eq(&self, other: &Self) -> bool {
        self.callback as usize == other.callback as usize && self.user_data == other.user_data
    }
}

impl IdleWaiter {
    pub fn call(&self) {
        unsafe { (self.callback)(self.user_data) };
    }
}

pub fn fetch(url: &CStr, base_url: Option<&CStr>, cancel: &Cancellable) {
    let context = Box::into_raw(Box::new(url.to_owned()));
    unsafe {
        ns_net_request_async(
            url.as_ptr(),
            base_url.map_or(ptr::null(), CStr::as_ptr),
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            ns_net_accept_headers_for(FETCH_DEST_FONT),
            cancel.0.as_ptr(),
            Some(on_fetched),
            context.cast(),
        )
    };
}

unsafe fn response_font_body<'a>(resp: *const NsResponse) -> Option<&'a [u8]> {
    let resp = unsafe { resp.as_ref() }?;
    if !resp.error.is_null() || resp.status >= 400 {
        return None;
    }
    let body = unsafe { resp.body.as_ref() }?;
    (body.len > 0).then(|| unsafe { glib::slice(body.data, body.len as usize) })
}

unsafe extern "C" fn on_fetched(_source: *mut c_void, result: *mut c_void, user_data: *mut c_void) {
    let url = unsafe { Box::from_raw(user_data.cast::<CString>()) };
    let mut error = ptr::null_mut();
    let resp = unsafe { ns_net_fetch_finish(result, &mut error) };
    crate::fetched(&url, unsafe { response_font_body(resp) });
    unsafe { ns_response_free(resp) };
    if !error.is_null() {
        unsafe { glib::g_error_free(error) };
    }
    drop(url);
    crate::notify_idle();
}

#[cfg(feature = "freetype")]
mod freetype {
    use core::ffi::{c_int, c_long, c_uint, c_ulong, c_void};

    pub type Error = c_int;

    unsafe extern "C" {
        pub fn FT_Init_FreeType(library: *mut *mut c_void) -> Error;
        pub fn FT_Done_FreeType(library: *mut c_void) -> Error;
        pub fn FT_New_Memory_Face(
            library: *mut c_void,
            file_base: *const u8,
            file_size: c_long,
            face_index: c_long,
            face: *mut *mut c_void,
        ) -> Error;
        pub fn FT_Done_Face(face: *mut c_void) -> Error;
        pub fn FT_Sfnt_Table_Info(
            face: *mut c_void,
            table_index: c_uint,
            tag: *mut c_ulong,
            length: *mut c_ulong,
        ) -> Error;
        pub fn FT_Load_Sfnt_Table(
            face: *mut c_void,
            tag: c_ulong,
            offset: c_long,
            buffer: *mut u8,
            length: *mut c_ulong,
        ) -> Error;
    }
}

#[cfg(feature = "freetype")]
pub struct Face<'a> {
    library: *mut c_void,
    face: *mut c_void,
    data: core::marker::PhantomData<&'a [u8]>,
}

#[cfg(feature = "freetype")]
impl<'a> Face<'a> {
    pub fn open(data: &'a [u8]) -> Option<Self> {
        let mut library = ptr::null_mut();
        if unsafe { freetype::FT_Init_FreeType(&mut library) } != 0 {
            return None;
        }
        let mut face = ptr::null_mut();
        if unsafe {
            freetype::FT_New_Memory_Face(library, data.as_ptr(), data.len() as c_long, 0, &mut face)
        } != 0
        {
            unsafe { freetype::FT_Done_FreeType(library) };
            return None;
        }
        Some(Face {
            library,
            face,
            data: core::marker::PhantomData,
        })
    }

    pub fn table_count(&self) -> Option<core::ffi::c_ulong> {
        let mut count = 0;
        let error =
            unsafe { freetype::FT_Sfnt_Table_Info(self.face, 0, ptr::null_mut(), &mut count) };
        (error == 0).then_some(count)
    }

    pub fn table_info(&self, index: c_uint) -> Option<(core::ffi::c_ulong, core::ffi::c_ulong)> {
        let (mut tag, mut len) = (0, 0);
        let error = unsafe { freetype::FT_Sfnt_Table_Info(self.face, index, &mut tag, &mut len) };
        (error == 0).then_some((tag, len))
    }

    pub fn load_table(
        &self,
        tag: core::ffi::c_ulong,
        buffer: &mut [u8],
        len: core::ffi::c_ulong,
    ) -> bool {
        if buffer.len() < len as usize {
            return false;
        }
        let mut length = len;
        unsafe {
            freetype::FT_Load_Sfnt_Table(self.face, tag, 0, buffer.as_mut_ptr(), &mut length) == 0
        }
    }
}

#[cfg(feature = "freetype")]
impl Drop for Face<'_> {
    fn drop(&mut self) {
        unsafe {
            freetype::FT_Done_Face(self.face);
            freetype::FT_Done_FreeType(self.library);
        }
    }
}

#[cfg(feature = "fontconfig")]
mod fontconfig {
    use core::ffi::{c_char, c_double, c_int, c_uint, c_void};

    pub const SET_APPLICATION: c_int = 1;
    pub const RESULT_MATCH: c_int = 0;
    pub const SLANT_ROMAN: c_int = 0;
    pub const SLANT_ITALIC: c_int = 100;
    pub const SLANT_OBLIQUE: c_int = 110;

    #[repr(C)]
    pub struct FontSet {
        pub nfont: c_int,
        pub sfont: c_int,
        pub fonts: *mut *mut c_void,
    }

    unsafe extern "C" {
        pub fn FcConfigGetFonts(config: *mut c_void, set: c_int) -> *mut FontSet;
        pub fn FcConfigAppFontAddFile(config: *mut c_void, file: *const c_char) -> c_int;
        pub fn FcFontSetCreate() -> *mut FontSet;
        pub fn FcFontSetAdd(set: *mut FontSet, font: *mut c_void) -> c_int;
        pub fn FcFontSetDestroy(set: *mut FontSet);
        pub fn FcFreeTypeQueryAll(
            file: *const c_char,
            id: c_uint,
            blanks: *mut c_void,
            count: *mut c_int,
            set: *mut FontSet,
        ) -> c_uint;
        pub fn FcPatternGetBool(
            pattern: *const c_void,
            object: *const c_char,
            n: c_int,
            value: *mut c_int,
        ) -> c_int;
        pub fn FcPatternGetString(
            pattern: *const c_void,
            object: *const c_char,
            n: c_int,
            value: *mut *const c_char,
        ) -> c_int;
        pub fn FcPatternDel(pattern: *mut c_void, object: *const c_char) -> c_int;
        pub fn FcPatternAddDouble(
            pattern: *mut c_void,
            object: *const c_char,
            value: c_double,
        ) -> c_int;
        pub fn FcPatternAddInteger(
            pattern: *mut c_void,
            object: *const c_char,
            value: c_int,
        ) -> c_int;
        pub fn FcPatternAddString(
            pattern: *mut c_void,
            object: *const c_char,
            value: *const c_char,
        ) -> c_int;
        pub fn FcPatternDestroy(pattern: *mut c_void);
        pub fn FcWeightFromOpenTypeDouble(ot_weight: c_double) -> c_double;
    }
}

#[cfg(feature = "pangofc")]
mod pango {
    use core::ffi::c_void;

    use southstar_glib::GBoolean;

    unsafe extern "C" {
        #[cfg_attr(
            feature = "ns-pango",
            link_name = "ns_pango_cairo_font_map_get_default"
        )]
        pub fn pango_cairo_font_map_get_default() -> *mut c_void;
        #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_fc_font_map_get_type")]
        pub fn pango_fc_font_map_get_type() -> usize;
        #[cfg_attr(
            feature = "ns-pango",
            link_name = "ns_pango_fc_font_map_config_changed"
        )]
        pub fn pango_fc_font_map_config_changed(fontmap: *mut c_void);
        pub fn g_type_check_instance_is_a(instance: *mut c_void, iface_type: usize) -> GBoolean;
    }
}

#[cfg(feature = "fontconfig")]
fn family_is(pattern: *mut c_void, family: &[u8]) -> bool {
    let mut internal = ptr::null();
    let found =
        unsafe { fontconfig::FcPatternGetString(pattern, c"family".as_ptr(), 0, &mut internal) };
    found == fontconfig::RESULT_MATCH
        && !internal.is_null()
        && unsafe { CStr::from_ptr(internal) }
            .to_bytes()
            .eq_ignore_ascii_case(family)
}

#[cfg(feature = "fontconfig")]
fn apply_descriptors(pattern: *mut c_void, descriptors: Descriptors) {
    let mut variable = 0;
    unsafe { fontconfig::FcPatternGetBool(pattern, c"variable".as_ptr(), 0, &mut variable) };
    if descriptors.weight > 0 && variable == 0 {
        unsafe {
            fontconfig::FcPatternDel(pattern, c"weight".as_ptr());
            let weight = fontconfig::FcWeightFromOpenTypeDouble(descriptors.weight.into());
            fontconfig::FcPatternAddDouble(pattern, c"weight".as_ptr(), weight);
        }
    }
    let slant = match descriptors.slant {
        0 => return,
        2 => fontconfig::SLANT_ITALIC,
        3 => fontconfig::SLANT_OBLIQUE,
        _ => fontconfig::SLANT_ROMAN,
    };
    unsafe {
        fontconfig::FcPatternDel(pattern, c"slant".as_ptr());
        fontconfig::FcPatternAddInteger(pattern, c"slant".as_ptr(), slant);
    }
}

#[cfg(feature = "fontconfig")]
unsafe fn font_set_patterns<'a>(set: *mut fontconfig::FontSet) -> &'a mut [*mut c_void] {
    let set = unsafe { &mut *set };
    if set.fonts.is_null() || set.nfont <= 0 {
        return &mut [];
    }
    unsafe { core::slice::from_raw_parts_mut(set.fonts, set.nfont as usize) }
}

#[cfg(feature = "fontconfig")]
pub fn install_font_file(path: &CStr, family: &[u8], descriptors: Descriptors) {
    use fontconfig::*;
    let Ok(css_family) = CString::new(family) else {
        return;
    };
    unsafe {
        let before = FcConfigGetFonts(ptr::null_mut(), SET_APPLICATION);
        let first_added = before.as_ref().map_or(0, |set| set.nfont.max(0) as usize);
        FcConfigAppFontAddFile(ptr::null_mut(), path.as_ptr());
        let app_fonts = FcConfigGetFonts(ptr::null_mut(), SET_APPLICATION);
        if app_fonts.is_null() {
            return refresh_pango_font_map();
        }
        for &pattern in font_set_patterns(app_fonts).iter().skip(first_added) {
            if family_is(pattern, family) {
                apply_descriptors(pattern, descriptors);
            }
        }
        let faces = if family.is_empty() {
            ptr::null_mut()
        } else {
            FcFontSetCreate()
        };
        if !faces.is_null() {
            let mut count = 0;
            FcFreeTypeQueryAll(
                path.as_ptr(),
                c_uint::MAX,
                ptr::null_mut(),
                &mut count,
                faces,
            );
            for slot in font_set_patterns(faces) {
                let pattern = core::mem::replace(slot, ptr::null_mut());
                if family_is(pattern, family) {
                    FcPatternDestroy(pattern);
                    continue;
                }
                FcPatternDel(pattern, c"family".as_ptr());
                FcPatternDel(pattern, c"familylang".as_ptr());
                FcPatternAddString(pattern, c"family".as_ptr(), css_family.as_ptr());
                apply_descriptors(pattern, descriptors);
                if FcFontSetAdd(app_fonts, pattern) == 0 {
                    FcPatternDestroy(pattern);
                }
            }
            (*faces).nfont = 0;
            FcFontSetDestroy(faces);
        }
    }
    refresh_pango_font_map();
}

#[cfg(not(feature = "fontconfig"))]
pub fn install_font_file(_path: &CStr, _family: &[u8], _descriptors: Descriptors) {}

#[cfg(feature = "fontconfig")]
fn refresh_pango_font_map() {
    #[cfg(feature = "pangofc")]
    unsafe {
        let font_map = pango::pango_cairo_font_map_get_default();
        if !font_map.is_null()
            && pango::g_type_check_instance_is_a(font_map, pango::pango_fc_font_map_get_type()) != 0
        {
            pango::pango_fc_font_map_config_changed(font_map);
        }
    }
}

fn descriptors(d: NsFontDescriptors) -> Descriptors {
    Descriptors {
        weight: d.weight,
        slant: d.slant,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_font_init() {
    crate::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_font_shutdown() {
    crate::shutdown();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_font_available() -> GBoolean {
    glib::boolean(crate::available())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_font_generation() -> c_uint {
    crate::generation()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_font_pending_count() -> c_uint {
    crate::pending_count()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_font_family_loaded(family: *const c_char) -> GBoolean {
    let family = unsafe { glib::bytes(family) };
    glib::boolean(family.is_some_and(crate::family_loaded))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_font_request(
    family: *const c_char,
    src_url: *const c_char,
    base_url: *const c_char,
    d: NsFontDescriptors,
) {
    let cstr = |p: *const c_char| (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) });
    crate::request(cstr(family), cstr(src_url), cstr(base_url), descriptors(d));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_font_add_idle_cb(
    callback: Option<IdleCallback>,
    user_data: *mut c_void,
) {
    if let Some(callback) = callback {
        crate::add_idle_waiter(IdleWaiter {
            callback,
            user_data,
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_font_remove_idle_cb(
    callback: Option<IdleCallback>,
    user_data: *mut c_void,
) {
    if let Some(callback) = callback {
        crate::remove_idle_waiter(&IdleWaiter {
            callback,
            user_data,
        });
    }
}
