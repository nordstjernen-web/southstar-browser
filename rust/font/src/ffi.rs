//! Southstar — the C ABI of the web font loader, over the network layer, GLib, FreeType, fontconfig and southstar-text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "freetype")]
use core::ffi::c_long;
use core::ffi::{CStr, c_char, c_int};
#[cfg(any(feature = "freetype", feature = "fontconfig"))]
use core::ffi::{c_uint, c_void};
use core::ptr;
#[cfg(feature = "fontconfig")]
use core::ptr::NonNull;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean, GError};

use crate::Descriptors;

unsafe extern "C" {
    fn ns_paint_register_font_oracle();
    fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_get_home_dir() -> *const c_char;

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
    pub const MATCH_PATTERN: c_int = 0;
    pub const BINDING_WEAK: c_int = 0;

    #[repr(C)]
    pub struct Value {
        _kind: c_int,
        _u: c_double,
    }

    impl Value {
        pub const fn empty() -> Value {
            Value { _kind: 0, _u: 0.0 }
        }
    }

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
        pub fn FcPatternCreate() -> *mut c_void;
        pub fn FcConfigSubstitute(config: *mut c_void, pattern: *mut c_void, kind: c_int) -> c_int;
        pub fn FcDefaultSubstitute(pattern: *mut c_void);
        pub fn FcFontMatch(
            config: *mut c_void,
            pattern: *mut c_void,
            result: *mut c_int,
        ) -> *mut c_void;
        pub fn FcPatternGetWithBinding(
            pattern: *const c_void,
            object: *const c_char,
            n: c_int,
            value: *mut Value,
            binding: *mut c_int,
        ) -> c_int;
    }
}

#[cfg(feature = "fontconfig")]
struct Pattern(NonNull<c_void>);

#[cfg(feature = "fontconfig")]
impl Pattern {
    fn new(raw: *mut c_void) -> Option<Pattern> {
        NonNull::new(raw).map(Pattern)
    }

    fn raw(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    fn family(&self, n: c_int) -> Option<Vec<u8>> {
        let mut family = ptr::null();
        // SAFETY: the pattern is live and family receives a pointer into it.
        let found = unsafe {
            fontconfig::FcPatternGetString(self.raw(), c"family".as_ptr(), n, &mut family)
        };
        if found != fontconfig::RESULT_MATCH || family.is_null() {
            return None;
        }
        // SAFETY: fontconfig returned a NUL-terminated string owned by the live pattern.
        Some(unsafe { CStr::from_ptr(family) }.to_bytes().to_vec())
    }

    fn families(&self) -> Vec<Vec<u8>> {
        (0..).map_while(|n| self.family(n)).collect()
    }

    fn family_bound(&self, n: c_int) -> Option<bool> {
        let mut value = fontconfig::Value::empty();
        let mut binding = fontconfig::BINDING_WEAK;
        // SAFETY: the pattern is live and value and binding are writable locals of the C types.
        let found = unsafe {
            fontconfig::FcPatternGetWithBinding(
                self.raw(),
                c"family".as_ptr(),
                n,
                &mut value,
                &mut binding,
            )
        };
        (found == fontconfig::RESULT_MATCH).then_some(binding != fontconfig::BINDING_WEAK)
    }

    fn bound_families(&self) -> Vec<Vec<u8>> {
        let bound = (0..)
            .map_while(|n| self.family_bound(n))
            .enumerate()
            .filter_map(|(i, strong)| strong.then_some(i + 1))
            .last()
            .unwrap_or(0);
        self.families().into_iter().take(bound).collect()
    }
}

#[cfg(feature = "fontconfig")]
impl Drop for Pattern {
    fn drop(&mut self) {
        // SAFETY: the wrapper owns the pattern and destroys it exactly once.
        unsafe { fontconfig::FcPatternDestroy(self.raw()) };
    }
}

#[cfg(feature = "fontconfig")]
pub fn metric_alias(family: &[u8]) -> Option<Vec<u8>> {
    let family = CString::new(family).ok().filter(|f| !f.is_empty())?;
    // SAFETY: FcPatternCreate takes no arguments and returns a new pattern or null.
    let pattern = Pattern::new(unsafe { fontconfig::FcPatternCreate() })?;
    // SAFETY: the pattern is live and fontconfig copies the NUL-terminated family.
    unsafe { fontconfig::FcPatternAddString(pattern.raw(), c"family".as_ptr(), family.as_ptr()) };
    // SAFETY: a null config selects the current configuration and the pattern is live.
    unsafe {
        fontconfig::FcConfigSubstitute(ptr::null_mut(), pattern.raw(), fontconfig::MATCH_PATTERN)
    };
    // SAFETY: the pattern is live.
    unsafe { fontconfig::FcDefaultSubstitute(pattern.raw()) };
    let wanted = pattern.bound_families();
    let mut result = 0;
    // SAFETY: a null config selects the current configuration, the pattern is live and result is a writable local.
    let matched = Pattern::new(unsafe {
        fontconfig::FcFontMatch(ptr::null_mut(), pattern.raw(), &mut result)
    })?;
    matched
        .families()
        .into_iter()
        .find(|got| wanted.iter().any(|want| want.eq_ignore_ascii_case(got)))
}

#[cfg(not(feature = "fontconfig"))]
pub fn metric_alias(_family: &[u8]) -> Option<Vec<u8>> {
    None
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
            return refresh_text_font_map();
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
    refresh_text_font_map();
}

#[cfg(not(feature = "fontconfig"))]
pub fn install_font_file(_path: &CStr, _family: &[u8], _descriptors: Descriptors) {}

#[cfg(feature = "fontconfig")]
fn refresh_text_font_map() {
    southstar_text::fontmap::FontMap::default_for_thread().config_changed();
}
