//! Southstar — the C ABI of bookmarks storage, as declared in src/bookmarks.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint};
use core::ptr;
use southstar_glib::{self as glib, FALSE, GBoolean, GError};

const APP_DIR_NAME: &[u8] = b"southstar\0";
const BOOKMARKS_FILE: &[u8] = b"bookmarks.txt\0";
const SET_CONTENTS_CONSISTENT: c_int = 1;
const LOG_LEVEL_WARNING: c_int = 1 << 4;

unsafe extern "C" {
    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_file_set_contents_full(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        flags: c_int,
        mode: c_int,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_log(log_domain: *const c_char, log_level: c_int, format: *const c_char, ...);
}

#[repr(C)]
pub struct NsBookmark {
    url: *mut c_char,
    title: *mut c_char,
}

pub struct NsBookmarks {
    items: Vec<NsBookmark>,
    path: *mut c_char,
}

impl NsBookmark {
    fn new(url: &[u8], title: &[u8]) -> Self {
        NsBookmark {
            url: glib::strdup(url),
            title: glib::strdup(title),
        }
    }

    fn url(&self) -> Option<&[u8]> {
        unsafe { glib::bytes(self.url) }
    }

    fn title(&self) -> Option<&[u8]> {
        unsafe { glib::bytes(self.title) }
    }
}

impl Drop for NsBookmark {
    fn drop(&mut self) {
        unsafe {
            glib::g_free(self.url.cast());
            glib::g_free(self.title.cast());
        }
    }
}

impl NsBookmarks {
    fn position(&self, url: &[u8]) -> Option<usize> {
        self.items.iter().position(|b| b.url() == Some(url))
    }

    fn save(&self) {
        let out = crate::serialize(self.items.iter().map(|b| (b.url(), b.title())));
        unsafe {
            let mut err: *mut GError = ptr::null_mut();
            if g_file_set_contents_full(
                self.path,
                out.as_ptr().cast(),
                out.len() as isize,
                SET_CONTENTS_CONSISTENT,
                0o600,
                &mut err,
            ) == FALSE
            {
                g_log(
                    ptr::null(),
                    LOG_LEVEL_WARNING,
                    c"bookmarks: failed to write %s: %s".as_ptr(),
                    self.path,
                    (*err).message,
                );
                glib::g_error_free(err);
            }
        }
    }
}

impl Drop for NsBookmarks {
    fn drop(&mut self) {
        unsafe { glib::g_free(self.path.cast()) };
    }
}

unsafe fn bookmarks_path() -> *mut c_char {
    unsafe {
        let dir = g_build_filename(
            glib::g_get_user_config_dir(),
            APP_DIR_NAME.as_ptr().cast::<c_char>(),
            ptr::null::<c_char>(),
        );
        g_mkdir_with_parents(dir, 0o700);
        let path = g_build_filename(
            dir,
            BOOKMARKS_FILE.as_ptr().cast::<c_char>(),
            ptr::null::<c_char>(),
        );
        glib::g_free(dir.cast());
        path
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bookmarks_load() -> *mut NsBookmarks {
    let path = unsafe { bookmarks_path() };
    let mut items = Vec::new();
    unsafe {
        let mut contents: *mut c_char = ptr::null_mut();
        let mut len = 0usize;
        if glib::g_file_get_contents(path, &mut contents, &mut len, ptr::null_mut()) != FALSE
            && !contents.is_null()
        {
            let bytes = glib::slice(contents.cast_const().cast(), len);
            items.extend(
                crate::parse(bytes)
                    .into_iter()
                    .map(|(url, title)| NsBookmark::new(url, title)),
            );
            glib::g_free(contents.cast());
        }
    }
    Box::into_raw(Box::new(NsBookmarks { items, path }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bookmarks_free(bm: *mut NsBookmarks) {
    if !bm.is_null() {
        drop(unsafe { Box::from_raw(bm) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bookmarks_count(bm: *const NsBookmarks) -> c_uint {
    unsafe { bm.as_ref() }.map_or(0, |bm| bm.items.len() as c_uint)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bookmarks_get(bm: *const NsBookmarks, i: c_uint) -> *const NsBookmark {
    unsafe { bm.as_ref() }
        .and_then(|bm| bm.items.get(i as usize))
        .map_or(ptr::null(), |b| b as *const NsBookmark)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bookmarks_contains(
    bm: *const NsBookmarks,
    url: *const c_char,
) -> GBoolean {
    match (unsafe { bm.as_ref() }, unsafe { glib::bytes(url) }) {
        (Some(bm), Some(url)) => glib::boolean(bm.position(url).is_some()),
        _ => FALSE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bookmarks_add(
    bm: *mut NsBookmarks,
    url: *const c_char,
    title: *const c_char,
) {
    let (Some(bm), Some(url)) = (unsafe { bm.as_mut() }, unsafe { glib::bytes(url) }) else {
        return;
    };
    if bm.position(url).is_some() {
        return;
    }
    let title = unsafe { glib::bytes(title) }.unwrap_or(url);
    bm.items.push(NsBookmark::new(url, title));
    bm.save();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bookmarks_remove(bm: *mut NsBookmarks, url: *const c_char) {
    let (Some(bm), Some(url)) = (unsafe { bm.as_mut() }, unsafe { glib::bytes(url) }) else {
        return;
    };
    if let Some(i) = bm.position(url) {
        bm.items.remove(i);
        bm.save();
    }
}
