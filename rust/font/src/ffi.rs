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
use std::ffi::{CString, OsStr};
use std::io::Write;
use std::path::{Path, PathBuf};

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

#[cfg(unix)]
fn path_of(path: &CStr) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(path.to_bytes()))
}

#[cfg(not(unix))]
fn path_of(path: &CStr) -> PathBuf {
    PathBuf::from(path.to_string_lossy().into_owned())
}

#[cfg(unix)]
fn os_bytes(text: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    text.as_bytes().to_vec()
}

#[cfg(not(unix))]
fn os_bytes(text: &OsStr) -> Vec<u8> {
    text.to_string_lossy().into_owned().into_bytes()
}

fn make_private_dir(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir);
    }
    #[cfg(not(unix))]
    let _ = std::fs::create_dir_all(dir);
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
    make_private_dir(&path_of(&dir));
    dir
}

pub fn build_path(dir: &CStr, name: &[u8]) -> Option<CString> {
    let name = CString::new(name).ok()?;
    build_filename(&[dir, &name])
}

fn write_atomically(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut temp_name = path.as_os_str().to_owned();
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = PathBuf::from(temp_name);
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(data)?;
        file.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&temp, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

pub fn write_file(path: &CStr, data: &[u8]) -> bool {
    write_atomically(&path_of(path), data).is_ok()
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
    let web_font_object = southstar_text::WEB_FONT_OBJECT;
    unsafe {
        let before = FcConfigGetFonts(ptr::null_mut(), SET_APPLICATION);
        let first_added = before.as_ref().map_or(0, |set| set.nfont.max(0) as usize);
        FcConfigAppFontAddFile(ptr::null_mut(), path.as_ptr());
        let app_fonts = FcConfigGetFonts(ptr::null_mut(), SET_APPLICATION);
        if app_fonts.is_null() {
            return refresh_text_font_map();
        }
        for &pattern in font_set_patterns(app_fonts).iter().skip(first_added) {
            FcPatternAddBool(pattern, web_font_object.as_ptr(), 1);
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
                FcPatternAddBool(pattern, web_font_object.as_ptr(), 1);
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

pub fn refresh_fonts() {
    #[cfg(feature = "fontconfig")]
    refresh_text_font_map();
}

#[cfg(feature = "fontconfig")]
fn refresh_text_font_map() {
    southstar_text::fontmap::FontMap::default_for_thread().config_changed();
}
