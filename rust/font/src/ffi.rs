//! Southstar — the file helpers and the fontconfig and southstar-text calls behind the web font loader.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
#[cfg(feature = "fontconfig")]
use core::ffi::{c_int, c_uint, c_void};
#[cfg(feature = "fontconfig")]
use core::ptr;
#[cfg(feature = "fontconfig")]
use core::ptr::NonNull;
use std::ffi::CString;
use std::path::PathBuf;

use southstar_glib::os_bytes;

use crate::Descriptors;

#[cfg(windows)]
const DIR_SEPARATOR: u8 = b'\\';
#[cfg(not(windows))]
const DIR_SEPARATOR: u8 = b'/';

fn is_dir_separator(c: u8) -> bool {
    c == b'/' || (cfg!(windows) && c == b'\\')
}

fn build_filename(parts: &[&CStr]) -> Option<CString> {
    let mut result = Vec::new();
    let mut separator = DIR_SEPARATOR;
    let mut is_first = true;
    let mut have_leading = false;
    let mut single_element: Option<&[u8]> = None;
    let mut last_trailing: &[u8] = &[];
    for element in parts.iter().map(|part| part.to_bytes()) {
        if element.is_empty() {
            continue;
        }
        let start = element
            .iter()
            .position(|&c| !is_dir_separator(c))
            .unwrap_or(element.len());
        if cfg!(windows) && start > 0 {
            separator = element[start - 1];
        }
        let end = element[start..]
            .iter()
            .rposition(|&c| !is_dir_separator(c))
            .map_or(start, |i| start + i + 1);
        if cfg!(windows) && end < element.len() {
            separator = element[end];
        }
        last_trailing = &element[if end > start { end } else { 0 }..];
        if !have_leading {
            if end == start {
                single_element = Some(element);
            }
            result.extend_from_slice(&element[..start]);
            have_leading = true;
        } else {
            single_element = None;
        }
        if end == start {
            continue;
        }
        if !is_first {
            result.push(separator);
        }
        result.extend_from_slice(&element[start..end]);
        is_first = false;
    }
    let joined = match single_element {
        Some(element) => element.to_vec(),
        None => [result.as_slice(), last_trailing].concat(),
    };
    CString::new(joined).ok()
}

fn path_of(path: &CStr) -> PathBuf {
    southstar_glib::path_from_bytes_lossy(path.to_bytes())
}

fn home_dir() -> CString {
    CString::new(southstar_glib::home_dir()).unwrap_or_default()
}

pub fn cache_dir() -> CString {
    let xdg = std::env::var_os("XDG_CACHE_HOME")
        .map(|value| os_bytes(&value))
        .filter(|x| !x.is_empty());
    let base = match xdg {
        Some(xdg) => CString::new(xdg).unwrap_or_default(),
        None => build_filename(&[&home_dir(), c".cache"]).unwrap_or_default(),
    };
    let dir = build_filename(&[&base, c"southstar", c"webfonts"]).unwrap_or_default();
    let _ = southstar_config::util::make_dirs(&path_of(&dir), 0o700);
    dir
}

pub fn build_path(dir: &CStr, name: &[u8]) -> Option<CString> {
    let name = CString::new(name).ok()?;
    build_filename(&[dir, &name])
}

pub fn write_file(path: &CStr, data: &[u8]) -> bool {
    southstar_config::util::write_replacing(&path_of(path), data).is_ok()
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
        pub fn FcPatternAddBool(pattern: *mut c_void, object: *const c_char, value: c_int)
        -> c_int;
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
    // SAFETY: `pattern` is a live FcPattern, the object name is a NUL-terminated literal and `internal` is a valid out-pointer.
    let found =
        unsafe { fontconfig::FcPatternGetString(pattern, c"family".as_ptr(), 0, &mut internal) };
    if found != fontconfig::RESULT_MATCH || internal.is_null() {
        return false;
    }
    // SAFETY: FcPatternGetString returned a NUL-terminated string owned by `pattern`, which outlives this borrow.
    let name = unsafe { CStr::from_ptr(internal) };
    name.to_bytes().eq_ignore_ascii_case(family)
}

#[cfg(feature = "fontconfig")]
fn apply_descriptors(pattern: *mut c_void, descriptors: Descriptors) {
    let mut variable = 0;
    // SAFETY: `pattern` is a live FcPattern, the object name is a NUL-terminated literal and `variable` is a valid out-pointer.
    unsafe { fontconfig::FcPatternGetBool(pattern, c"variable".as_ptr(), 0, &mut variable) };
    if descriptors.weight > 0 && variable == 0 {
        // SAFETY: `pattern` is a live FcPattern and the object name is a NUL-terminated literal.
        unsafe { fontconfig::FcPatternDel(pattern, c"weight".as_ptr()) };
        // SAFETY: FcWeightFromOpenTypeDouble only converts its argument.
        let weight = unsafe { fontconfig::FcWeightFromOpenTypeDouble(descriptors.weight.into()) };
        // SAFETY: `pattern` is a live FcPattern and the object name is a NUL-terminated literal.
        unsafe { fontconfig::FcPatternAddDouble(pattern, c"weight".as_ptr(), weight) };
    }
    let slant = match descriptors.slant {
        0 => return,
        2 => fontconfig::SLANT_ITALIC,
        3 => fontconfig::SLANT_OBLIQUE,
        _ => fontconfig::SLANT_ROMAN,
    };
    // SAFETY: `pattern` is a live FcPattern and the object name is a NUL-terminated literal.
    unsafe { fontconfig::FcPatternDel(pattern, c"slant".as_ptr()) };
    // SAFETY: `pattern` is a live FcPattern and the object name is a NUL-terminated literal.
    unsafe { fontconfig::FcPatternAddInteger(pattern, c"slant".as_ptr(), slant) };
}

/// # Safety
///
/// `set` must point to a live FcFontSet that nothing else accesses while the returned slice is in use.
#[cfg(feature = "fontconfig")]
unsafe fn font_set_patterns<'a>(set: *mut fontconfig::FontSet) -> &'a mut [*mut c_void] {
    // SAFETY: the caller guarantees `set` is a live FcFontSet with no other access while the slice lives.
    let set = unsafe { &mut *set };
    if set.fonts.is_null() || set.nfont <= 0 {
        return &mut [];
    }
    // SAFETY: a font set's `fonts` array holds `nfont` initialized pattern pointers.
    unsafe { core::slice::from_raw_parts_mut(set.fonts, set.nfont as usize) }
}

#[cfg(feature = "fontconfig")]
fn application_fonts() -> *mut fontconfig::FontSet {
    // SAFETY: a null config selects the current fontconfig configuration.
    unsafe { fontconfig::FcConfigGetFonts(ptr::null_mut(), fontconfig::SET_APPLICATION) }
}

#[cfg(feature = "fontconfig")]
fn font_count(set: *mut fontconfig::FontSet) -> usize {
    // SAFETY: `set` is null or a live FcFontSet owned by fontconfig.
    unsafe { set.as_ref() }.map_or(0, |set| set.nfont.max(0) as usize)
}

#[cfg(feature = "fontconfig")]
fn mark_added_fonts(
    app_fonts: *mut fontconfig::FontSet,
    first_added: usize,
    family: &[u8],
    descriptors: Descriptors,
) {
    let web_font_object = southstar_text::WEB_FONT_OBJECT;
    // SAFETY: `app_fonts` is the configuration's live application font set, used only here.
    let patterns = unsafe { font_set_patterns(app_fonts) };
    for &pattern in patterns.iter().skip(first_added) {
        // SAFETY: `pattern` belongs to the application font set and the object name is NUL-terminated.
        unsafe { fontconfig::FcPatternAddBool(pattern, web_font_object.as_ptr(), 1) };
        if family_is(pattern, family) {
            apply_descriptors(pattern, descriptors);
        }
    }
}

#[cfg(feature = "fontconfig")]
fn add_renamed_faces(
    path: &CStr,
    app_fonts: *mut fontconfig::FontSet,
    family: &[u8],
    css_family: &CStr,
    descriptors: Descriptors,
) {
    use fontconfig::*;
    let web_font_object = southstar_text::WEB_FONT_OBJECT;
    // SAFETY: FcFontSetCreate has no preconditions.
    let faces = unsafe { FcFontSetCreate() };
    if faces.is_null() {
        return;
    }
    let mut count = 0;
    // SAFETY: `path` is NUL-terminated, a null blanks cache is allowed, `count` is a valid out-pointer and `faces` is the live set created above.
    unsafe {
        FcFreeTypeQueryAll(
            path.as_ptr(),
            c_uint::MAX,
            ptr::null_mut(),
            &mut count,
            faces,
        )
    };
    // SAFETY: `faces` is the live set created above and nothing else accesses it.
    let slots = unsafe { font_set_patterns(faces) };
    for slot in slots {
        let pattern = core::mem::replace(slot, ptr::null_mut());
        if family_is(pattern, family) {
            // SAFETY: `pattern` was taken out of `faces`, so this function owns it.
            unsafe { FcPatternDestroy(pattern) };
            continue;
        }
        // SAFETY: `pattern` is a live pattern this function owns and the object name is NUL-terminated.
        unsafe { FcPatternDel(pattern, c"family".as_ptr()) };
        // SAFETY: `pattern` is a live pattern this function owns and the object name is NUL-terminated.
        unsafe { FcPatternDel(pattern, c"familylang".as_ptr()) };
        // SAFETY: `pattern` is a live pattern and both strings are NUL-terminated; fontconfig copies the value.
        unsafe { FcPatternAddString(pattern, c"family".as_ptr(), css_family.as_ptr()) };
        // SAFETY: `pattern` is a live pattern and the object name is NUL-terminated.
        unsafe { FcPatternAddBool(pattern, web_font_object.as_ptr(), 1) };
        apply_descriptors(pattern, descriptors);
        // SAFETY: `app_fonts` is the live application font set, which takes ownership of `pattern` on success.
        if unsafe { FcFontSetAdd(app_fonts, pattern) } == 0 {
            // SAFETY: the set refused `pattern`, so this function still owns it.
            unsafe { FcPatternDestroy(pattern) };
        }
    }
    // SAFETY: every pattern was moved out of `faces` above, so emptying it before destroying it frees nothing twice.
    unsafe { (*faces).nfont = 0 };
    // SAFETY: `faces` is the live set created above and holds no patterns now.
    unsafe { FcFontSetDestroy(faces) };
}

#[cfg(feature = "fontconfig")]
pub fn install_font_file(path: &CStr, family: &[u8], descriptors: Descriptors) {
    let Ok(css_family) = CString::new(family) else {
        return;
    };
    let first_added = font_count(application_fonts());
    // SAFETY: a null config selects the current configuration and `path` is NUL-terminated.
    unsafe { fontconfig::FcConfigAppFontAddFile(ptr::null_mut(), path.as_ptr()) };
    let app_fonts = application_fonts();
    if app_fonts.is_null() {
        return refresh_text_font_map();
    }
    mark_added_fonts(app_fonts, first_added, family, descriptors);
    if !family.is_empty() {
        add_renamed_faces(path, app_fonts, family, &css_family, descriptors);
    }
    refresh_text_font_map();
}

#[cfg(not(feature = "fontconfig"))]
pub fn install_font_file(_path: &CStr, _family: &[u8], _descriptors: Descriptors) {}

pub fn refresh_fonts() {
    #[cfg(feature = "fontconfig")]
    refresh_text_font_map();
}

#[cfg(feature = "fontconfig")]
fn refresh_text_font_map() {
    southstar_text::fontmap::FontMap::default_for_thread().config_changed();
}
