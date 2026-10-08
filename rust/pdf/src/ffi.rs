//! Southstar — the C ABI of the PDF viewer, as declared in src/pdf.h, over poppler-glib and Cairo.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use southstar_glib as glib;

unsafe extern "C" {
    fn ns_html_escape_text(s: *const c_char) -> *mut c_char;
    fn g_path_get_basename(file_name: *const c_char) -> *mut c_char;
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

unsafe fn take(p: *mut c_char) -> Vec<u8> {
    let bytes = unsafe { glib::bytes(p) }.unwrap_or_default().to_vec();
    unsafe { glib::g_free(p.cast()) };
    bytes
}

pub(crate) fn html_escape(text: &[u8]) -> Vec<u8> {
    let text = cstring(text);
    unsafe { take(ns_html_escape_text(text.as_ptr())) }
}

#[cfg_attr(not(feature = "poppler"), allow(dead_code))]
pub(crate) fn path_basename(path: &[u8]) -> Vec<u8> {
    let path = cstring(path);
    unsafe { take(g_path_get_basename(path.as_ptr())) }
}

#[cfg(feature = "poppler")]
mod poppler {
    use core::ffi::{c_char, c_int, c_uint, c_void};
    use core::ptr::{self, NonNull};

    use southstar_glib as glib;

    #[repr(C)]
    struct GBytes {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct GError {
        domain: u32,
        code: c_int,
        message: *mut c_char,
    }

    #[repr(C)]
    struct PopplerDocument {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct PopplerPage {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct CairoSurface {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct Cairo {
        _private: [u8; 0],
    }

    type WriteFunc = unsafe extern "C" fn(*mut c_void, *const u8, c_uint) -> c_int;

    const CAIRO_STATUS_SUCCESS: c_int = 0;
    const CAIRO_FORMAT_RGB24: c_int = 1;

    unsafe extern "C" {
        fn g_bytes_new(data: *const c_void, size: usize) -> *mut GBytes;
        fn g_bytes_unref(bytes: *mut GBytes);
        fn g_error_free(error: *mut GError);
        fn g_object_unref(object: *mut c_void);
        fn poppler_document_new_from_bytes(
            bytes: *mut GBytes,
            password: *const c_char,
            error: *mut *mut GError,
        ) -> *mut PopplerDocument;
        fn poppler_document_get_n_pages(document: *mut PopplerDocument) -> c_int;
        fn poppler_document_get_page(
            document: *mut PopplerDocument,
            index: c_int,
        ) -> *mut PopplerPage;
        fn poppler_page_get_size(page: *mut PopplerPage, width: *mut f64, height: *mut f64);
        fn poppler_page_render(page: *mut PopplerPage, cairo: *mut Cairo);
        fn cairo_image_surface_create(
            format: c_int,
            width: c_int,
            height: c_int,
        ) -> *mut CairoSurface;
        fn cairo_surface_status(surface: *mut CairoSurface) -> c_int;
        fn cairo_surface_destroy(surface: *mut CairoSurface);
        fn cairo_surface_flush(surface: *mut CairoSurface);
        fn cairo_surface_write_to_png_stream(
            surface: *mut CairoSurface,
            write: WriteFunc,
            closure: *mut c_void,
        ) -> c_int;
        fn cairo_create(surface: *mut CairoSurface) -> *mut Cairo;
        fn cairo_destroy(cairo: *mut Cairo);
        fn cairo_set_source_rgb(cairo: *mut Cairo, r: f64, g: f64, b: f64);
        fn cairo_paint(cairo: *mut Cairo);
        fn cairo_scale(cairo: *mut Cairo, sx: f64, sy: f64);
    }

    pub(crate) struct Document(NonNull<PopplerDocument>);

    impl Drop for Document {
        fn drop(&mut self) {
            unsafe { g_object_unref(self.0.as_ptr().cast()) };
        }
    }

    unsafe extern "C" fn png_write(closure: *mut c_void, data: *const u8, length: c_uint) -> c_int {
        let out = unsafe { &mut *closure.cast::<Vec<u8>>() };
        out.extend_from_slice(unsafe { glib::slice(data, length as usize) });
        CAIRO_STATUS_SUCCESS
    }

    impl Document {
        pub(crate) fn open(data: &[u8]) -> Result<Document, Option<Vec<u8>>> {
            let bytes = unsafe { g_bytes_new(data.as_ptr().cast(), data.len()) };
            let mut error: *mut GError = ptr::null_mut();
            let doc = unsafe { poppler_document_new_from_bytes(bytes, ptr::null(), &mut error) };
            unsafe { g_bytes_unref(bytes) };
            let message = unsafe { error.as_ref() }.map(|e| {
                unsafe { glib::bytes(e.message) }
                    .unwrap_or_default()
                    .to_vec()
            });
            if !error.is_null() {
                unsafe { g_error_free(error) };
            }
            NonNull::new(doc).map(Document).ok_or(message)
        }

        pub(crate) fn pages(&self) -> i32 {
            unsafe { poppler_document_get_n_pages(self.0.as_ptr()) }
        }

        pub(crate) fn page_data_uri(&self, index: i32) -> Option<Vec<u8>> {
            let page = unsafe { poppler_document_get_page(self.0.as_ptr(), index) };
            if page.is_null() {
                return None;
            }
            let png = unsafe { render_page(page) };
            unsafe { g_object_unref(page.cast()) };
            let png = png?;
            let encoded = unsafe { glib::g_base64_encode(png.as_ptr(), png.len()) };
            let encoded = unsafe { super::take(encoded) };
            Some([b"data:image/png;base64,".as_slice(), &encoded].concat())
        }
    }

    unsafe fn render_page(page: *mut PopplerPage) -> Option<Vec<u8>> {
        let (mut pw, mut ph) = (0.0, 0.0);
        unsafe { poppler_page_get_size(page, &mut pw, &mut ph) };
        let (scale, w, h) = crate::render_size(pw, ph)?;
        unsafe {
            let surface = cairo_image_surface_create(CAIRO_FORMAT_RGB24, w, h);
            if cairo_surface_status(surface) != CAIRO_STATUS_SUCCESS {
                cairo_surface_destroy(surface);
                return None;
            }
            let cr = cairo_create(surface);
            cairo_set_source_rgb(cr, 1.0, 1.0, 1.0);
            cairo_paint(cr);
            cairo_scale(cr, scale, scale);
            poppler_page_render(page, cr);
            cairo_destroy(cr);
            cairo_surface_flush(surface);
            let mut png: Vec<u8> = Vec::new();
            let status =
                cairo_surface_write_to_png_stream(surface, png_write, (&raw mut png).cast());
            cairo_surface_destroy(surface);
            (status == CAIRO_STATUS_SUCCESS).then_some(png)
        }
    }
}

#[cfg(feature = "poppler")]
pub(crate) use poppler::Document;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_pdf_document_html(
    data: *const u8,
    len: usize,
    url: *const c_char,
) -> *mut c_char {
    let url = (!url.is_null()).then(|| unsafe { CStr::from_ptr(url) }.to_bytes());
    let data = unsafe { glib::slice(data, len) };
    glib::strdup(&crate::document_html(data, url))
}
