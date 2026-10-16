//! Southstar — cairo surfaces: owned references to image, recording and PDF surfaces, borrowed handles, pixel access and PNG output.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use super::RawSurface;
use super::pixels::PixelBorrow;
use crate::types::{Content, Error, Format, PdfMetadata, Rectangle, STATUS_SUCCESS};

pub struct Surface(NonNull<RawSurface>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceRef<'a> {
    raw: NonNull<RawSurface>,
    _surface: PhantomData<&'a Surface>,
}

impl Surface {
    /// # Safety
    ///
    /// `raw` is a live surface reference that the returned Surface takes over.
    pub(super) unsafe fn own(raw: *mut RawSurface) -> Surface {
        Surface(NonNull::new(raw).expect("cairo returns a surface, never null"))
    }

    /// # Safety
    ///
    /// `raw` is a live surface.
    pub(super) unsafe fn borrowed(raw: *mut RawSurface) -> Surface {
        // SAFETY: the caller passes a live surface; cairo_surface_reference adds the reference the Surface owns.
        let raw = unsafe { super::cairo_surface_reference(raw) };
        // SAFETY: that new reference is handed to the Surface.
        unsafe { Surface::own(raw) }
    }

    pub fn image(format: Format, width: i32, height: i32) -> Option<Surface> {
        let surface = Surface::image_unchecked(format, width, height);
        surface.as_ref().status().is_ok().then_some(surface)
    }

    pub fn image_unchecked(format: Format, width: i32, height: i32) -> Surface {
        // SAFETY: cairo_image_surface_create takes plain values and returns a new reference, an error surface on failure.
        let raw = unsafe { super::cairo_image_surface_create(format as c_int, width, height) };
        // SAFETY: that new reference is handed to the Surface.
        unsafe { Surface::own(raw) }
    }

    /// # Safety
    ///
    /// `data` points to `stride * height` bytes that stay valid, and are touched only through
    /// cairo, for as long as any reference to the surface lives; cairo writes to them when the
    /// surface is drawn on.
    pub unsafe fn for_data(
        data: *mut u8,
        format: Format,
        width: i32,
        height: i32,
        stride: i32,
    ) -> Surface {
        // SAFETY: the caller guarantees the pixel memory for the surface's whole life.
        let raw = unsafe {
            super::cairo_image_surface_create_for_data(data, format as c_int, width, height, stride)
        };
        // SAFETY: cairo_image_surface_create_for_data returns a new reference, handed to the Surface.
        unsafe { Surface::own(raw) }
    }

    pub fn from_png(path: &CStr) -> Surface {
        // SAFETY: the path is NUL-terminated; cairo returns a new reference, an error surface on failure.
        let raw = unsafe { super::cairo_image_surface_create_from_png(path.as_ptr()) };
        // SAFETY: that new reference is handed to the Surface.
        unsafe { Surface::own(raw) }
    }

    pub fn recording(content: Content, extents: Option<&Rectangle>) -> Surface {
        let extents: *const Rectangle = extents.map_or(ptr::null(), ptr::from_ref);
        // SAFETY: extents is null or a rectangle cairo copies before returning a new reference.
        let raw = unsafe { super::cairo_recording_surface_create(content as c_int, extents) };
        // SAFETY: that new reference is handed to the Surface.
        unsafe { Surface::own(raw) }
    }

    pub fn pdf(path: &CStr, width: f64, height: f64) -> Surface {
        // SAFETY: the path is NUL-terminated; cairo returns a new reference, an error surface on failure.
        let raw = unsafe { super::cairo_pdf_surface_create(path.as_ptr(), width, height) };
        // SAFETY: that new reference is handed to the Surface.
        unsafe { Surface::own(raw) }
    }

    /// # Safety
    ///
    /// `raw` is null or a live surface whose reference the caller hands over to the returned
    /// Surface, and it is only used on this thread from now on.
    pub unsafe fn from_raw_full(raw: *mut c_void) -> Option<Surface> {
        NonNull::new(raw.cast()).map(Surface)
    }

    pub fn as_ref(&self) -> SurfaceRef<'_> {
        SurfaceRef {
            raw: self.0,
            _surface: PhantomData,
        }
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.as_raw();
        core::mem::forget(self);
        raw
    }
}

impl Clone for Surface {
    fn clone(&self) -> Surface {
        self.as_ref().reference()
    }
}

impl PartialEq for Surface {
    fn eq(&self, other: &Surface) -> bool {
        self.0 == other.0
    }
}

impl Eq for Surface {}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: the Surface owns one reference to its live surface and releases it once, here.
        unsafe { super::cairo_surface_destroy(self.0.as_ptr()) };
    }
}

struct Pixels {
    data: *mut u8,
    len: usize,
    stride: usize,
}

unsafe extern "C" fn collect_png(closure: *mut c_void, data: *const u8, length: c_uint) -> c_int {
    // SAFETY: closure is the Vec that SurfaceRef::to_png lends cairo for the duration of the write, borrowed by nothing else meanwhile.
    let out = unsafe { &mut *closure.cast::<Vec<u8>>() };
    if !data.is_null() {
        // SAFETY: cairo passes `length` readable bytes at data.
        out.extend_from_slice(unsafe { core::slice::from_raw_parts(data, length as usize) });
    }
    STATUS_SUCCESS
}

impl<'a> SurfaceRef<'a> {
    /// # Safety
    ///
    /// `raw` is null or a surface that stays alive for `'a` and is only used on this thread.
    pub unsafe fn from_raw(raw: *mut c_void) -> Option<SurfaceRef<'a>> {
        NonNull::new(raw.cast()).map(|raw| SurfaceRef {
            raw,
            _surface: PhantomData,
        })
    }

    pub(super) fn raw(&self) -> *mut RawSurface {
        self.raw.as_ptr()
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.raw().cast()
    }

    pub fn reference(&self) -> Surface {
        // SAFETY: the handle is a live surface.
        unsafe { Surface::borrowed(self.raw()) }
    }

    pub fn status(&self) -> Result<(), Error> {
        // SAFETY: the handle is a live surface.
        Error::check(unsafe { super::cairo_surface_status(self.raw()) })
    }

    pub fn flush(&self) {
        // SAFETY: the handle is a live surface.
        unsafe { super::cairo_surface_flush(self.raw()) };
    }

    pub fn mark_dirty(&self) {
        // SAFETY: the handle is a live surface.
        unsafe { super::cairo_surface_mark_dirty(self.raw()) };
    }

    pub fn width(&self) -> i32 {
        // SAFETY: the handle is a live surface; a non-image surface reports 0.
        unsafe { super::cairo_image_surface_get_width(self.raw()) }
    }

    pub fn height(&self) -> i32 {
        // SAFETY: the handle is a live surface; a non-image surface reports 0.
        unsafe { super::cairo_image_surface_get_height(self.raw()) }
    }

    pub fn stride(&self) -> i32 {
        // SAFETY: the handle is a live surface; a non-image surface reports 0.
        unsafe { super::cairo_image_surface_get_stride(self.raw()) }
    }

    pub fn device_offset(&self) -> (f64, f64) {
        let (mut x, mut y) = (0.0, 0.0);
        // SAFETY: the handle is a live surface, and x and y are writable locals.
        unsafe { super::cairo_surface_get_device_offset(self.raw(), &mut x, &mut y) };
        (x, y)
    }

    pub fn set_pdf_metadata(&self, metadata: PdfMetadata, value: &CStr) {
        // SAFETY: the handle is a live surface and the value is NUL-terminated; a non-PDF surface only records an error.
        unsafe {
            super::cairo_pdf_surface_set_metadata(self.raw(), metadata as c_int, value.as_ptr())
        };
    }

    pub fn write_to_png(&self, path: &CStr) -> Result<(), Error> {
        // SAFETY: the handle is a live surface and the path is NUL-terminated.
        Error::check(unsafe { super::cairo_surface_write_to_png(self.raw(), path.as_ptr()) })
    }

    fn png_stream(&self) -> (Vec<u8>, Result<(), Error>) {
        let mut png: Vec<u8> = Vec::new();
        let closure: *mut Vec<u8> = &mut png;
        // SAFETY: the handle is a live surface, and collect_png only appends to the Vec, which outlives the call.
        let status = unsafe {
            super::cairo_surface_write_to_png_stream(self.raw(), collect_png, closure.cast())
        };
        (png, Error::check(status))
    }

    pub fn to_png(&self) -> Result<Vec<u8>, Error> {
        let (png, status) = self.png_stream();
        status.map(|()| png)
    }

    fn pixels(&self) -> Option<Pixels> {
        // SAFETY: the handle is a live surface; a non-image or failed surface reports null.
        let data = unsafe { super::cairo_image_surface_get_data(self.raw()) };
        let stride = usize::try_from(self.stride()).ok().filter(|&s| s > 0)?;
        let height = usize::try_from(self.height()).ok().filter(|&h| h > 0)?;
        let len = stride.checked_mul(height)?;
        (!data.is_null()).then_some(Pixels { data, len, stride })
    }

    pub fn with_data<R>(&self, f: impl FnOnce(&[u8], usize) -> R) -> Option<R> {
        self.flush();
        let pixels = self.pixels()?;
        let _borrow = PixelBorrow::shared(self.raw())?;
        // SAFETY: an image surface's data holds stride * height bytes that stay allocated while this handle keeps the surface alive, and the borrow rules out a mutable slice of them meanwhile.
        let data = unsafe { core::slice::from_raw_parts(pixels.data, pixels.len) };
        Some(f(data, pixels.stride))
    }

    pub fn with_data_mut<R>(&self, f: impl FnOnce(&mut [u8], usize) -> R) -> Option<R> {
        self.flush();
        let pixels = self.pixels()?;
        let borrow = PixelBorrow::exclusive(self.raw())?;
        // SAFETY: an image surface's data holds stride * height bytes that stay allocated while this handle keeps the surface alive, and the borrow rules out any other slice of them meanwhile.
        let data = unsafe { core::slice::from_raw_parts_mut(pixels.data, pixels.len) };
        let out = f(data, pixels.stride);
        drop(borrow);
        self.mark_dirty();
        Some(out)
    }
}

pub fn encode_png(
    pixels: &[u8],
    format: Format,
    width: i32,
    height: i32,
    stride: i32,
) -> Option<Vec<u8>> {
    let rows = usize::try_from(height).ok()?;
    let row_bytes = usize::try_from(stride).ok()?;
    if rows.checked_mul(row_bytes)? > pixels.len() {
        return None;
    }
    // SAFETY: the pixels hold stride * height bytes, cairo only reads them to encode the PNG, and the surface never leaves this function, so its last reference goes before the borrow of the pixels ends.
    let owned =
        unsafe { Surface::for_data(pixels.as_ptr().cast_mut(), format, width, height, stride) };
    let surface = owned.as_ref();
    surface.status().ok()?;
    Some(surface.png_stream().0)
}
