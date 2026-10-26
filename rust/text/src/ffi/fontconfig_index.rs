//! Southstar text — the fontconfig calls that index a web font without FreeType: character sets, language sets, ranges and the application font set.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int};
use core::ptr::NonNull;

use super::{
    CHARSET_DONE, CHARSET_MAP_SIZE, FcCharSetFirstPage, FcCharSetNextPage, FcConfigGetFonts,
    FcFontSetAdd, FcPatternDestroy, FcPatternReference, FcStrListCreate, FcStrListDone,
    FcStrListFirst, FcStrListNext, FcStrSetDestroy, Pattern, PatternBuilder, RawCharSet,
    RawLangSet, RawStrSet, SET_APPLICATION,
};

#[repr(C)]
struct RawRange {
    _private: [u8; 0],
}

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcCharSetCreate() -> *mut RawCharSet;
    fn FcCharSetDestroy(fcs: *mut RawCharSet);
    fn FcCharSetAddChar(fcs: *mut RawCharSet, ucs4: u32) -> c_int;
    fn FcCharSetSubtractCount(a: *const RawCharSet, b: *const RawCharSet) -> u32;
    fn FcPatternAddCharSet(
        p: *mut super::RawPattern,
        object: *const c_char,
        c: *const RawCharSet,
    ) -> c_int;
    fn FcLangSetCreate() -> *mut RawLangSet;
    fn FcLangSetDestroy(ls: *mut RawLangSet);
    fn FcLangSetAdd(ls: *mut RawLangSet, lang: *const u8) -> c_int;
    fn FcPatternAddLangSet(
        p: *mut super::RawPattern,
        object: *const c_char,
        ls: *const RawLangSet,
    ) -> c_int;
    fn FcGetLangs() -> *mut RawStrSet;
    fn FcLangGetCharSet(lang: *const u8) -> *const RawCharSet;
    fn FcRangeCreateDouble(begin: c_double, end: c_double) -> *mut RawRange;
    fn FcRangeDestroy(range: *mut RawRange);
    fn FcPatternAddRange(
        p: *mut super::RawPattern,
        object: *const c_char,
        r: *const RawRange,
    ) -> c_int;
    fn FcConfigAppFontAddFile(config: *mut core::ffi::c_void, file: *const u8) -> c_int;
    fn FcPatternDel(p: *mut super::RawPattern, object: *const c_char) -> c_int;
}

pub(crate) struct CharSetBuilder(NonNull<RawCharSet>);

// SAFETY: the charset is owned by this value alone and fontconfig charsets have no thread affinity.
unsafe impl Send for CharSetBuilder {}

impl Drop for CharSetBuilder {
    fn drop(&mut self) {
        // SAFETY: this value holds one reference to the charset, released once here.
        unsafe { FcCharSetDestroy(self.0.as_ptr()) };
    }
}

impl CharSetBuilder {
    pub(crate) fn new() -> Option<CharSetBuilder> {
        // SAFETY: FcCharSetCreate takes nothing and returns a new charset we own.
        NonNull::new(unsafe { FcCharSetCreate() }).map(CharSetBuilder)
    }

    pub(crate) fn add(&mut self, ch: u32) {
        // SAFETY: the charset is live and exclusively ours.
        unsafe { FcCharSetAddChar(self.0.as_ptr(), ch) };
    }

    fn missing_from(&self, language: &CStr) -> Option<(u32, usize)> {
        // SAFETY: the language name is NUL-terminated; the charset returned stays owned by fontconfig.
        let wanted = unsafe { FcLangGetCharSet(language.as_ptr().cast()) };
        if wanted.is_null() {
            return None;
        }
        // SAFETY: both charsets are live and only read.
        let missing = unsafe { FcCharSetSubtractCount(wanted, self.0.as_ptr()) };
        Some((missing, page_count(wanted)))
    }

    pub(crate) fn languages(&self, exclusive: Option<&CStr>) -> Vec<String> {
        let exclusive_pages = exclusive.and_then(|lang| self.page_count_of(lang));
        let mut found = Vec::new();
        for language in all_languages() {
            let Ok(name) = std::ffi::CString::new(language.clone()) else {
                continue;
            };
            let Some((missing, pages)) = self.missing_from(&name) else {
                continue;
            };
            if let Some(exclusive_pages) = exclusive_pages
                && is_exclusive_language(&language)
                && pages != exclusive_pages
            {
                continue;
            }
            if missing == 0 {
                found.push(language);
            }
        }
        found
    }

    fn page_count_of(&self, language: &CStr) -> Option<usize> {
        // SAFETY: the language name is NUL-terminated; the charset returned stays owned by fontconfig.
        let charset = unsafe { FcLangGetCharSet(language.as_ptr().cast()) };
        (!charset.is_null()).then(|| page_count(charset))
    }
}

fn is_exclusive_language(language: &str) -> bool {
    matches!(language, "ja" | "zh-cn" | "ko" | "zh-tw")
}

fn page_count(charset: *const RawCharSet) -> usize {
    let mut map = [0u32; CHARSET_MAP_SIZE];
    let mut next = 0u32;
    let mut pages = 0;
    // SAFETY: charset is a live charset owned by fontconfig; map and next are writable locals.
    let mut page = unsafe { FcCharSetFirstPage(charset, map.as_mut_ptr(), &mut next) };
    while page != CHARSET_DONE {
        pages += 1;
        // SAFETY: as above.
        page = unsafe { FcCharSetNextPage(charset, map.as_mut_ptr(), &mut next) };
    }
    pages
}

fn all_languages() -> Vec<String> {
    // SAFETY: FcGetLangs takes nothing and returns a new string set we own.
    let set = unsafe { FcGetLangs() };
    if set.is_null() {
        return Vec::new();
    }
    // SAFETY: set is live; the list is ours to free with FcStrListDone.
    let list = unsafe { FcStrListCreate(set) };
    let mut languages = Vec::new();
    if !list.is_null() {
        // SAFETY: list is live.
        unsafe { FcStrListFirst(list) };
        loop {
            // SAFETY: list is live; the returned string stays owned by the set.
            let entry = unsafe { FcStrListNext(list) };
            if entry.is_null() {
                break;
            }
            // SAFETY: entry is a NUL-terminated string owned by the live set.
            let text = unsafe { CStr::from_ptr(entry.cast()) };
            languages.push(text.to_string_lossy().into_owned());
        }
        // SAFETY: list was created above and is released once.
        unsafe { FcStrListDone(list) };
    }
    // SAFETY: set was returned by FcGetLangs and is released once.
    unsafe { FcStrSetDestroy(set) };
    languages
}

impl PatternBuilder {
    pub(crate) fn delete(&mut self, object: &CStr) {
        // SAFETY: the pattern is uniquely owned by this builder and the object name is NUL-terminated.
        unsafe { FcPatternDel(self.raw(), object.as_ptr()) };
    }

    pub(crate) fn add_charset(&mut self, object: &CStr, charset: &CharSetBuilder) {
        // SAFETY: the pattern is uniquely owned; fontconfig takes its own reference to the charset.
        unsafe { FcPatternAddCharSet(self.raw(), object.as_ptr(), charset.0.as_ptr()) };
    }

    pub(crate) fn add_languages(&mut self, object: &CStr, languages: &[String]) {
        // SAFETY: FcLangSetCreate takes nothing and returns a new language set we own.
        let Some(set) = NonNull::new(unsafe { FcLangSetCreate() }) else {
            return;
        };
        for language in languages {
            let Ok(name) = std::ffi::CString::new(language.as_str()) else {
                continue;
            };
            // SAFETY: the set is live and exclusively ours; fontconfig copies the name.
            unsafe { FcLangSetAdd(set.as_ptr(), name.as_ptr().cast()) };
        }
        // SAFETY: the pattern is uniquely owned; fontconfig copies the language set.
        unsafe { FcPatternAddLangSet(self.raw(), object.as_ptr(), set.as_ptr()) };
        // SAFETY: the set was created above and is released once.
        unsafe { FcLangSetDestroy(set.as_ptr()) };
    }

    pub(crate) fn add_range(&mut self, object: &CStr, begin: f64, end: f64) {
        // SAFETY: plain values in, a new range we own out.
        let range = unsafe { FcRangeCreateDouble(begin, end) };
        if range.is_null() {
            return;
        }
        // SAFETY: the pattern is uniquely owned; fontconfig copies the range.
        unsafe { FcPatternAddRange(self.raw(), object.as_ptr(), range) };
        // SAFETY: the range was created above and is released once.
        unsafe { FcRangeDestroy(range) };
    }
}

pub(crate) fn add_application_fonts(patterns: Vec<Pattern>) -> bool {
    // SAFETY: a NULL configuration means the current one; the set stays owned by fontconfig.
    let mut set = unsafe { FcConfigGetFonts(core::ptr::null_mut(), SET_APPLICATION) };
    if set.is_null() {
        // SAFETY: an empty path names no font, so fontconfig only creates the application set.
        unsafe { FcConfigAppFontAddFile(core::ptr::null_mut(), c"".as_ptr().cast()) };
        // SAFETY: as above.
        set = unsafe { FcConfigGetFonts(core::ptr::null_mut(), SET_APPLICATION) };
    }
    if set.is_null() {
        return false;
    }
    for pattern in patterns {
        let raw = pattern.raw();
        // SAFETY: the pattern is live; the extra reference is handed to the set below.
        unsafe { FcPatternReference(raw) };
        // SAFETY: set is fontconfig's live application set, which takes over that reference.
        if unsafe { FcFontSetAdd(set, raw) } == 0 {
            // SAFETY: the set refused the pattern, so the reference taken above is still ours.
            unsafe { FcPatternDestroy(raw) };
        }
    }
    true
}

#[cfg(test)]
#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcFreeTypeQueryAll(
        file: *const u8,
        id: core::ffi::c_uint,
        blanks: *mut core::ffi::c_void,
        count: *mut c_int,
        set: *mut super::RawFontSet,
    ) -> core::ffi::c_uint;
    fn FcPatternFormat(p: *mut super::RawPattern, format: *const u8) -> *mut u8;
    fn FcStrFree(s: *mut u8);
}

#[cfg(test)]
pub(crate) fn freetype_query_all(path: &CStr) -> Vec<Pattern> {
    let set = super::FontSet::new();
    let mut count = 0;
    // SAFETY: path is NUL-terminated and the set is live and exclusively ours.
    unsafe {
        FcFreeTypeQueryAll(
            path.as_ptr().cast(),
            core::ffi::c_uint::MAX,
            core::ptr::null_mut(),
            &mut count,
            set.0.as_ptr(),
        )
    };
    set.patterns()
}

#[cfg(test)]
pub(crate) fn format(pattern: &Pattern, format: &CStr) -> String {
    // SAFETY: the pattern is live and format is NUL-terminated; the result is a new string we free.
    let text = unsafe { FcPatternFormat(pattern.raw(), format.as_ptr().cast()) };
    if text.is_null() {
        return String::new();
    }
    // SAFETY: text is a NUL-terminated string owned by us until FcStrFree.
    let value = unsafe { CStr::from_ptr(text.cast()) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: text was returned by FcPatternFormat and is freed once.
    unsafe { FcStrFree(text) };
    value
}
