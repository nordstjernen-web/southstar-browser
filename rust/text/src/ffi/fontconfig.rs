//! Southstar text — fontconfig: patterns, font sets, matching and sorting, configuration substitution and character sets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_void};
use core::ptr::NonNull;
use std::sync::Once;

use crate::coverage::CharSet;

#[repr(C)]
pub(crate) struct RawPattern {
    _private: [u8; 0],
}

#[repr(C)]
struct RawFontSet {
    nfont: c_int,
    sfont: c_int,
    fonts: *mut *mut RawPattern,
}

#[repr(C)]
struct RawCharSet {
    _private: [u8; 0],
}

#[repr(C)]
struct RawLangSet {
    _private: [u8; 0],
}

#[repr(C)]
struct RawStrSet {
    _private: [u8; 0],
}

#[repr(C)]
struct RawStrList {
    _private: [u8; 0],
}

#[repr(C)]
struct RawObjectSet {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FcMatrix {
    pub xx: f64,
    pub xy: f64,
    pub yx: f64,
    pub yy: f64,
}

impl FcMatrix {
    pub const IDENTITY: FcMatrix = FcMatrix {
        xx: 1.0,
        xy: 0.0,
        yx: 0.0,
        yy: 1.0,
    };
}

const RESULT_MATCH: c_int = 0;
const RESULT_NO_MATCH: c_int = 1;
const MATCH_PATTERN: c_int = 0;
const CHARSET_MAP_SIZE: usize = 8;
const CHARSET_DONE: u32 = u32::MAX;

pub(crate) const SET_SYSTEM: c_int = 0;
pub(crate) const SET_APPLICATION: c_int = 1;

pub(crate) const FC_FAMILY: &CStr = c"family";
pub(crate) const FC_STYLE: &CStr = c"style";
pub(crate) const FC_SLANT: &CStr = c"slant";
pub(crate) const FC_WEIGHT: &CStr = c"weight";
pub(crate) const FC_SIZE: &CStr = c"size";
pub(crate) const FC_PIXEL_SIZE: &CStr = c"pixelsize";
pub(crate) const FC_SPACING: &CStr = c"spacing";
pub(crate) const FC_FILE: &CStr = c"file";
pub(crate) const FC_INDEX: &CStr = c"index";
pub(crate) const FC_DPI: &CStr = c"dpi";
pub(crate) const FC_EMBOLDEN: &CStr = c"embolden";
pub(crate) const FC_MATRIX: &CStr = c"matrix";
pub(crate) const FC_CHARSET: &CStr = c"charset";
pub(crate) const FC_LANG: &CStr = c"lang";
pub(crate) const FC_WIDTH: &CStr = c"width";
pub(crate) const FC_ORDER: &CStr = c"order";
pub(crate) const FC_COLOR: &CStr = c"color";
pub(crate) const FC_VARIABLE: &CStr = c"variable";
pub(crate) const FC_FONTFORMAT: &CStr = c"fontformat";
pub(crate) const FC_FONT_WRAPPER: &CStr = c"fontwrapper";
pub(crate) const FC_PRGNAME: &CStr = c"prgname";
pub(crate) const FC_FONT_FEATURES: &CStr = c"fontfeatures";
pub(crate) const FC_FONT_VARIATIONS: &CStr = c"fontvariations";
pub(crate) const FC_ANTIALIAS: &CStr = c"antialias";
pub(crate) const FC_HINTING: &CStr = c"hinting";
pub(crate) const FC_HINT_STYLE: &CStr = c"hintstyle";
pub(crate) const FC_AUTOHINT: &CStr = c"autohint";
pub(crate) const FC_HINT_FULL: i32 = 3;
pub(crate) const PANGO_FC_GRAVITY: &CStr = c"pangogravity";
pub(crate) const PANGO_FC_VERSION: &CStr = c"pangoversion";

pub(crate) const FC_WEIGHT_MEDIUM: i32 = 100;
pub(crate) const FC_SLANT_ROMAN: i32 = 0;
pub(crate) const FC_SLANT_ITALIC: i32 = 100;
pub(crate) const FC_SLANT_OBLIQUE: i32 = 110;
pub(crate) const FC_WIDTH_ULTRACONDENSED: i32 = 50;
pub(crate) const FC_WIDTH_EXTRACONDENSED: i32 = 63;
pub(crate) const FC_WIDTH_CONDENSED: i32 = 75;
pub(crate) const FC_WIDTH_SEMICONDENSED: i32 = 87;
pub(crate) const FC_WIDTH_SEMIEXPANDED: i32 = 113;
pub(crate) const FC_WIDTH_EXPANDED: i32 = 125;
pub(crate) const FC_WIDTH_EXTRAEXPANDED: i32 = 150;
pub(crate) const FC_WIDTH_ULTRAEXPANDED: i32 = 200;
pub(crate) const FC_PROPORTIONAL: i32 = 0;
pub(crate) const FC_DUAL: i32 = 90;
pub(crate) const FC_MONO: i32 = 100;
pub(crate) const FC_CHARCELL: i32 = 110;
pub(crate) const FC_DONT_CARE: i32 = 2;

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcInit() -> c_int;
    fn FcPatternCreate() -> *mut RawPattern;
    fn FcPatternDuplicate(p: *const RawPattern) -> *mut RawPattern;
    fn FcPatternReference(p: *mut RawPattern);
    fn FcPatternDestroy(p: *mut RawPattern);
    fn FcPatternHash(p: *const RawPattern) -> u32;
    fn FcPatternEqual(a: *const RawPattern, b: *const RawPattern) -> c_int;
    fn FcPatternAddInteger(p: *mut RawPattern, object: *const c_char, i: c_int) -> c_int;
    fn FcPatternAddDouble(p: *mut RawPattern, object: *const c_char, d: c_double) -> c_int;
    fn FcPatternAddString(p: *mut RawPattern, object: *const c_char, s: *const u8) -> c_int;
    fn FcPatternAddBool(p: *mut RawPattern, object: *const c_char, b: c_int) -> c_int;
    fn FcPatternRemove(p: *mut RawPattern, object: *const c_char, id: c_int) -> c_int;
    fn FcPatternGetInteger(
        p: *const RawPattern,
        object: *const c_char,
        n: c_int,
        i: *mut c_int,
    ) -> c_int;
    fn FcPatternGetDouble(
        p: *const RawPattern,
        object: *const c_char,
        n: c_int,
        d: *mut c_double,
    ) -> c_int;
    fn FcPatternGetString(
        p: *const RawPattern,
        object: *const c_char,
        n: c_int,
        s: *mut *mut u8,
    ) -> c_int;
    fn FcPatternGetMatrix(
        p: *const RawPattern,
        object: *const c_char,
        n: c_int,
        m: *mut *mut FcMatrix,
    ) -> c_int;
    fn FcPatternGetCharSet(
        p: *const RawPattern,
        object: *const c_char,
        n: c_int,
        c: *mut *mut RawCharSet,
    ) -> c_int;
    fn FcPatternGetBool(
        p: *const RawPattern,
        object: *const c_char,
        n: c_int,
        b: *mut c_int,
    ) -> c_int;
    fn FcPatternGetLangSet(
        p: *const RawPattern,
        object: *const c_char,
        n: c_int,
        ls: *mut *mut RawLangSet,
    ) -> c_int;
    fn FcCharSetFirstPage(a: *const RawCharSet, map: *mut u32, next: *mut u32) -> u32;
    fn FcCharSetNextPage(a: *const RawCharSet, map: *mut u32, next: *mut u32) -> u32;
    fn FcLangSetGetLangs(ls: *const RawLangSet) -> *mut RawStrSet;
    fn FcStrSetDestroy(set: *mut RawStrSet);
    fn FcStrListCreate(set: *mut RawStrSet) -> *mut RawStrList;
    fn FcStrListFirst(list: *mut RawStrList);
    fn FcStrListNext(list: *mut RawStrList) -> *mut u8;
    fn FcStrListDone(list: *mut RawStrList);
    fn FcFontSetCreate() -> *mut RawFontSet;
    fn FcFontSetDestroy(s: *mut RawFontSet);
    fn FcFontSetAdd(s: *mut RawFontSet, font: *mut RawPattern) -> c_int;
    fn FcObjectSetCreate() -> *mut RawObjectSet;
    fn FcObjectSetAdd(os: *mut RawObjectSet, object: *const c_char) -> c_int;
    fn FcObjectSetDestroy(os: *mut RawObjectSet);
    fn FcConfigReference(config: *mut c_void) -> *mut c_void;
    fn FcConfigDestroy(config: *mut c_void);
    fn FcConfigGetFonts(config: *mut c_void, set: c_int) -> *mut RawFontSet;
    fn FcConfigSubstitute(config: *mut c_void, p: *mut RawPattern, kind: c_int) -> c_int;
    fn FcConfigSetDefaultSubstitute(config: *mut c_void, p: *mut RawPattern);
    fn FcFontSetList(
        config: *mut c_void,
        sets: *mut *mut RawFontSet,
        nsets: c_int,
        p: *mut RawPattern,
        os: *mut RawObjectSet,
    ) -> *mut RawFontSet;
    fn FcFontSetMatch(
        config: *mut c_void,
        sets: *mut *mut RawFontSet,
        nsets: c_int,
        p: *mut RawPattern,
        result: *mut c_int,
    ) -> *mut RawPattern;
    fn FcFontSetSort(
        config: *mut c_void,
        sets: *mut *mut RawFontSet,
        nsets: c_int,
        p: *mut RawPattern,
        trim: c_int,
        csp: *mut *mut RawCharSet,
        result: *mut c_int,
    ) -> *mut RawFontSet;
    fn FcFontRenderPrepare(
        config: *mut c_void,
        pat: *mut RawPattern,
        font: *mut RawPattern,
    ) -> *mut RawPattern;
    fn FcWeightFromOpenTypeDouble(ot_weight: c_double) -> c_double;
    fn FcWeightToOpenTypeDouble(fc_weight: c_double) -> c_double;
    fn FcMatrixMultiply(result: *mut FcMatrix, a: *const FcMatrix, b: *const FcMatrix);
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_get_prgname() -> *const c_char;
}

unsafe extern "C" {
    fn qsort(
        base: *mut c_void,
        count: usize,
        size: usize,
        compare: unsafe extern "C" fn(*const c_void, *const c_void) -> c_int,
    );
}

pub(crate) fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // SAFETY: FcInit takes no arguments and loads the default configuration once.
        unsafe { FcInit() };
    });
}

pub(crate) fn prgname() -> Option<String> {
    // SAFETY: g_get_prgname takes no arguments and returns GLib's own string or NULL.
    let raw = unsafe { g_get_prgname() };
    if raw.is_null() {
        return None;
    }
    // SAFETY: raw is a NUL-terminated string GLib keeps alive, copied out at once.
    Some(
        unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned(),
    )
}

pub(crate) fn weight_from_opentype(weight: f64) -> f64 {
    // SAFETY: a pure numeric conversion.
    unsafe { FcWeightFromOpenTypeDouble(weight) }
}

pub(crate) fn weight_to_opentype(weight: f64) -> f64 {
    // SAFETY: a pure numeric conversion.
    unsafe { FcWeightToOpenTypeDouble(weight) }
}

pub(crate) fn matrix_multiply(a: &FcMatrix, b: &FcMatrix) -> FcMatrix {
    let mut result = FcMatrix::IDENTITY;
    // SAFETY: all three matrices are valid locals; fontconfig reads a and b before writing result.
    unsafe { FcMatrixMultiply(&mut result, a, b) };
    result
}

fn c_string(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text.bytes().take_while(|&b| b != 0).collect();
    bytes.push(0);
    bytes
}

pub(crate) struct Pattern(NonNull<RawPattern>);

// SAFETY: a shared Pattern is never modified, and fontconfig counts its references atomically.
unsafe impl Send for Pattern {}
// SAFETY: as above; the getters only read.
unsafe impl Sync for Pattern {}

impl Clone for Pattern {
    fn clone(&self) -> Pattern {
        // SAFETY: the pattern is live; the new reference is released by the clone's Drop.
        unsafe { FcPatternReference(self.raw()) };
        Pattern(self.0)
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        // SAFETY: this Pattern holds one reference, released exactly once here.
        unsafe { FcPatternDestroy(self.raw()) };
    }
}

impl Pattern {
    fn from_owned(raw: *mut RawPattern) -> Option<Pattern> {
        NonNull::new(raw).map(Pattern)
    }

    pub(crate) fn raw(&self) -> *mut RawPattern {
        self.0.as_ptr()
    }

    pub(crate) fn id(&self) -> usize {
        self.0.as_ptr() as usize
    }

    pub(crate) fn duplicate(&self) -> PatternBuilder {
        // SAFETY: the pattern is live; the duplicate is a new pattern the builder owns.
        let raw = unsafe { FcPatternDuplicate(self.raw()) };
        PatternBuilder(Pattern::from_owned(raw).expect("fontconfig duplicates a pattern"))
    }

    pub(crate) fn hash_value(&self) -> u32 {
        // SAFETY: the pattern is live and only read.
        unsafe { FcPatternHash(self.raw()) }
    }

    pub(crate) fn equal(&self, other: &Pattern) -> bool {
        // SAFETY: both patterns are live and only read.
        unsafe { FcPatternEqual(self.raw(), other.raw()) != 0 }
    }

    pub(crate) fn get_integer(&self, object: &CStr, n: i32) -> Option<i32> {
        let mut value: c_int = 0;
        // SAFETY: the pattern is live, the object name is NUL-terminated and value is a writable local.
        let result = unsafe { FcPatternGetInteger(self.raw(), object.as_ptr(), n, &mut value) };
        (result == RESULT_MATCH).then_some(value)
    }

    pub(crate) fn get_double(&self, object: &CStr, n: i32) -> Option<f64> {
        let mut value: c_double = 0.0;
        // SAFETY: the pattern is live, the object name is NUL-terminated and value is a writable local.
        let result = unsafe { FcPatternGetDouble(self.raw(), object.as_ptr(), n, &mut value) };
        (result == RESULT_MATCH).then_some(value)
    }

    pub(crate) fn get_bool(&self, object: &CStr, n: i32) -> Option<bool> {
        let mut value: c_int = 0;
        // SAFETY: the pattern is live, the object name is NUL-terminated and value is a writable local.
        let result = unsafe { FcPatternGetBool(self.raw(), object.as_ptr(), n, &mut value) };
        (result == RESULT_MATCH).then_some(value != 0)
    }

    pub(crate) fn get_cstr(&self, object: &CStr, n: i32) -> Option<&CStr> {
        let mut value: *mut u8 = core::ptr::null_mut();
        // SAFETY: the pattern is live, the object name is NUL-terminated and value is a writable local.
        let result = unsafe { FcPatternGetString(self.raw(), object.as_ptr(), n, &mut value) };
        if result != RESULT_MATCH || value.is_null() {
            return None;
        }
        // SAFETY: the string belongs to this pattern, which is never modified while shared and outlives &self.
        Some(unsafe { CStr::from_ptr(value.cast_const().cast()) })
    }

    pub(crate) fn get_string(&self, object: &CStr, n: i32) -> Option<String> {
        self.get_cstr(object, n)
            .map(|s| s.to_string_lossy().into_owned())
    }

    pub(crate) fn get_matrix(&self, object: &CStr, n: i32) -> Option<FcMatrix> {
        let mut value: *mut FcMatrix = core::ptr::null_mut();
        // SAFETY: the pattern is live, the object name is NUL-terminated and value is a writable local.
        let result = unsafe { FcPatternGetMatrix(self.raw(), object.as_ptr(), n, &mut value) };
        if result != RESULT_MATCH || value.is_null() {
            return None;
        }
        // SAFETY: the matrix belongs to this live pattern and is copied out at once.
        Some(unsafe { *value })
    }

    pub(crate) fn matrix_product(&self) -> FcMatrix {
        let mut product = FcMatrix::IDENTITY;
        let mut i = 0;
        while let Some(matrix) = self.get_matrix(FC_MATRIX, i) {
            product = matrix_multiply(&product, &matrix);
            i += 1;
        }
        product
    }

    fn get_charset(&self) -> Option<*mut RawCharSet> {
        let mut value: *mut RawCharSet = core::ptr::null_mut();
        // SAFETY: the pattern is live, the object name is NUL-terminated and value is a writable local.
        let result = unsafe { FcPatternGetCharSet(self.raw(), FC_CHARSET.as_ptr(), 0, &mut value) };
        (result == RESULT_MATCH && !value.is_null()).then_some(value)
    }

    pub(crate) fn charset(&self) -> Option<CharSet> {
        let charset = self.get_charset()?;
        let mut set = CharSet::new();
        let mut map = [0u32; CHARSET_MAP_SIZE];
        let mut next: u32 = 0;
        // SAFETY: the charset belongs to this live pattern; map holds FC_CHARSET_MAP_SIZE words and next is a local.
        let mut base = unsafe { FcCharSetFirstPage(charset, map.as_mut_ptr(), &mut next) };
        while base != CHARSET_DONE {
            for (word_index, &word) in map.iter().enumerate() {
                let mut bits = word;
                while bits != 0 {
                    let bit = bits.trailing_zeros();
                    set.insert(base + word_index as u32 * 32 + bit);
                    bits &= bits - 1;
                }
            }
            // SAFETY: as above; next carries the iteration state fontconfig returned.
            base = unsafe { FcCharSetNextPage(charset, map.as_mut_ptr(), &mut next) };
        }
        Some(set)
    }

    pub(crate) fn languages(&self) -> Option<Vec<String>> {
        let mut langset: *mut RawLangSet = core::ptr::null_mut();
        // SAFETY: the pattern is live, the object name is NUL-terminated and langset is a writable local.
        let result = unsafe { FcPatternGetLangSet(self.raw(), FC_LANG.as_ptr(), 0, &mut langset) };
        if result != RESULT_MATCH || langset.is_null() {
            return None;
        }
        // SAFETY: the langset belongs to this live pattern; the returned set is ours to destroy.
        let strset = unsafe { FcLangSetGetLangs(langset) };
        // SAFETY: strset is the live set just created.
        let list = unsafe { FcStrListCreate(strset) };
        // SAFETY: list is the live iterator just created.
        unsafe { FcStrListFirst(list) };
        let mut languages = Vec::new();
        loop {
            // SAFETY: list is live; each string stays valid until the set is destroyed below.
            let s = unsafe { FcStrListNext(list) };
            if s.is_null() {
                break;
            }
            // SAFETY: s is a NUL-terminated string owned by the live set, copied out at once.
            let text = unsafe { CStr::from_ptr(s.cast_const().cast()) };
            languages.push(text.to_string_lossy().into_owned());
        }
        // SAFETY: the iterator is released once, here.
        unsafe { FcStrListDone(list) };
        // SAFETY: the set is released once, here, after its strings were copied.
        unsafe { FcStrSetDestroy(strset) };
        Some(languages)
    }

    pub(crate) fn render_prepare(&self, font: &Pattern) -> Option<Pattern> {
        // SAFETY: both patterns are live and only read; the result is a new pattern we own.
        let raw = unsafe { FcFontRenderPrepare(core::ptr::null_mut(), self.raw(), font.raw()) };
        Pattern::from_owned(raw)
    }

    pub(crate) fn with_order(&self, order: i32) -> Pattern {
        if self.get_integer(FC_ORDER, 0) == Some(order) {
            return self.clone();
        }
        let mut pattern = self.duplicate();
        pattern.remove(FC_ORDER, 0);
        pattern.add_integer(FC_ORDER, order);
        pattern.finish()
    }
}

pub(crate) struct PatternBuilder(Pattern);

impl PatternBuilder {
    pub(crate) fn new() -> PatternBuilder {
        // SAFETY: FcPatternCreate takes nothing and returns a new pattern we own.
        let raw = unsafe { FcPatternCreate() };
        PatternBuilder(Pattern::from_owned(raw).expect("fontconfig creates a pattern"))
    }

    pub(crate) fn raw(&self) -> *mut RawPattern {
        self.0.raw()
    }

    pub(crate) fn finish(self) -> Pattern {
        self.0
    }

    pub(crate) fn add_integer(&mut self, object: &CStr, value: i32) {
        // SAFETY: the pattern is uniquely owned by this builder and the object name is NUL-terminated.
        unsafe { FcPatternAddInteger(self.raw(), object.as_ptr(), value) };
    }

    pub(crate) fn add_double(&mut self, object: &CStr, value: f64) {
        // SAFETY: the pattern is uniquely owned by this builder and the object name is NUL-terminated.
        unsafe { FcPatternAddDouble(self.raw(), object.as_ptr(), value) };
    }

    pub(crate) fn add_bool(&mut self, object: &CStr, value: i32) {
        // SAFETY: the pattern is uniquely owned by this builder and the object name is NUL-terminated.
        unsafe { FcPatternAddBool(self.raw(), object.as_ptr(), value) };
    }

    pub(crate) fn add_string(&mut self, object: &CStr, value: &str) {
        let value = c_string(value);
        // SAFETY: the pattern is uniquely owned, both strings are NUL-terminated and fontconfig copies value.
        unsafe { FcPatternAddString(self.raw(), object.as_ptr(), value.as_ptr()) };
    }

    pub(crate) fn remove(&mut self, object: &CStr, id: i32) {
        // SAFETY: the pattern is uniquely owned by this builder and the object name is NUL-terminated.
        unsafe { FcPatternRemove(self.raw(), object.as_ptr(), id) };
    }

    pub(crate) fn config_substitute(&mut self) {
        // SAFETY: the pattern is uniquely owned; a NULL configuration means the current one.
        unsafe { FcConfigSubstitute(core::ptr::null_mut(), self.raw(), MATCH_PATTERN) };
    }

    pub(crate) fn default_substitute(&mut self) {
        // SAFETY: the pattern is uniquely owned; a NULL configuration means the current one.
        unsafe { FcConfigSetDefaultSubstitute(core::ptr::null_mut(), self.raw()) };
    }
}

struct CurrentConfig(*mut c_void);

impl CurrentConfig {
    fn new() -> CurrentConfig {
        // SAFETY: a NULL argument asks for a new reference to the current configuration.
        CurrentConfig(unsafe { FcConfigReference(core::ptr::null_mut()) })
    }
}

impl Drop for CurrentConfig {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the reference taken in new is released once, here.
            unsafe { FcConfigDestroy(self.0) };
        }
    }
}

pub(crate) struct FontSet(NonNull<RawFontSet>);

// SAFETY: a FontSet is only read once built, and its patterns are Send + Sync.
unsafe impl Send for FontSet {}
// SAFETY: as above.
unsafe impl Sync for FontSet {}

impl Drop for FontSet {
    fn drop(&mut self) {
        // SAFETY: the set and its pattern references are owned here and released once.
        unsafe { FcFontSetDestroy(self.0.as_ptr()) };
    }
}

impl FontSet {
    pub(crate) fn new() -> FontSet {
        // SAFETY: FcFontSetCreate takes nothing and returns a new set we own.
        let raw = unsafe { FcFontSetCreate() };
        FontSet(NonNull::new(raw).expect("fontconfig creates a font set"))
    }

    fn from_owned(raw: *mut RawFontSet) -> Option<FontSet> {
        NonNull::new(raw).map(FontSet)
    }

    pub(crate) fn push(&mut self, pattern: Pattern) {
        let raw = pattern.raw();
        core::mem::forget(pattern);
        // SAFETY: the set is uniquely borrowed and takes over the pattern reference just released by forget.
        let added = unsafe { FcFontSetAdd(self.0.as_ptr(), raw) };
        if added == 0 {
            drop(Pattern(NonNull::new(raw).expect("pattern is not null")));
        }
    }

    pub(crate) fn len(&self) -> usize {
        // SAFETY: the set is live; nfont is a plain field.
        let n = unsafe { (*self.0.as_ptr()).nfont };
        usize::try_from(n).unwrap_or(0)
    }

    fn raw_at(&self, i: usize) -> *mut RawPattern {
        // SAFETY: the set is live; fonts holds nfont pointers and i is checked against len by callers.
        let fonts = unsafe { (*self.0.as_ptr()).fonts };
        // SAFETY: i < nfont, so the slot is inside the fonts array.
        let slot = unsafe { fonts.add(i) };
        // SAFETY: slot points at one initialized pattern pointer of the live set.
        unsafe { *slot }
    }

    pub(crate) fn get(&self, i: usize) -> Option<Pattern> {
        if i >= self.len() {
            return None;
        }
        let raw = self.raw_at(i);
        // SAFETY: the slot holds a live pattern; the new reference belongs to the returned Pattern.
        unsafe { FcPatternReference(raw) };
        Pattern::from_owned(raw)
    }

    pub(crate) fn patterns(&self) -> Vec<Pattern> {
        (0..self.len()).filter_map(|i| self.get(i)).collect()
    }

    pub(crate) fn copy(&self) -> FontSet {
        let mut copy = FontSet::new();
        for pattern in self.patterns() {
            copy.push(pattern);
        }
        copy
    }

    pub(crate) fn config_fonts(set: c_int) -> Option<FontSet> {
        // SAFETY: a NULL configuration means the current one; the set stays owned by fontconfig.
        let raw = unsafe { FcConfigGetFonts(core::ptr::null_mut(), set) };
        if raw.is_null() {
            return None;
        }
        let borrowed = core::mem::ManuallyDrop::new(FontSet(NonNull::new(raw)?));
        Some(borrowed.copy())
    }

    pub(crate) fn font_match(&self, pattern: &Pattern) -> (Option<Pattern>, bool) {
        let config = CurrentConfig::new();
        let mut sets = [self.0.as_ptr()];
        let mut result: c_int = RESULT_MATCH;
        // SAFETY: the config, set and pattern are live and only read; the result is a new pattern we own.
        let raw =
            unsafe { FcFontSetMatch(config.0, sets.as_mut_ptr(), 1, pattern.raw(), &mut result) };
        (Pattern::from_owned(raw), result == RESULT_NO_MATCH)
    }

    pub(crate) fn sort(&self, pattern: &Pattern) -> Option<FontSet> {
        let config = CurrentConfig::new();
        let mut sets = [self.0.as_ptr()];
        let mut result: c_int = RESULT_MATCH;
        // SAFETY: the config, set and pattern are live and only read; the result is a new set we own.
        let raw = unsafe {
            FcFontSetSort(
                config.0,
                sets.as_mut_ptr(),
                1,
                pattern.raw(),
                1,
                core::ptr::null_mut(),
                &mut result,
            )
        };
        FontSet::from_owned(raw)
    }

    pub(crate) fn list(&self, pattern: &Pattern, objects: &[&CStr]) -> Option<FontSet> {
        // SAFETY: FcObjectSetCreate takes nothing and returns a new object set we own.
        let os = unsafe { FcObjectSetCreate() };
        for object in objects {
            // SAFETY: os is live and the object name is a static NUL-terminated string.
            unsafe { FcObjectSetAdd(os, object.as_ptr()) };
        }
        let mut sets = [self.0.as_ptr()];
        // SAFETY: the set, pattern and object set are live and only read; the result is ours.
        let raw = unsafe {
            FcFontSetList(
                core::ptr::null_mut(),
                sets.as_mut_ptr(),
                1,
                pattern.raw(),
                os,
            )
        };
        // SAFETY: the object set is released once, here.
        unsafe { FcObjectSetDestroy(os) };
        FontSet::from_owned(raw)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FaceOrder {
    pub slant: i32,
    pub weight: i32,
    pub index: u32,
}

unsafe extern "C" fn compare_face_order(a: *const c_void, b: *const c_void) -> c_int {
    // SAFETY: qsort passes pointers to two FaceOrder elements of the slice being sorted.
    let a = unsafe { &*a.cast::<FaceOrder>() };
    // SAFETY: as above.
    let b = unsafe { &*b.cast::<FaceOrder>() };
    if a.slant != b.slant {
        return a.slant - b.slant;
    }
    a.weight - b.weight
}

pub(crate) fn sort_faces_like_qsort(order: &mut [FaceOrder]) {
    // SAFETY: the slice is valid and exclusively borrowed; the comparator only reads two elements.
    unsafe {
        qsort(
            order.as_mut_ptr().cast(),
            order.len(),
            core::mem::size_of::<FaceOrder>(),
            compare_face_order,
        )
    };
}
