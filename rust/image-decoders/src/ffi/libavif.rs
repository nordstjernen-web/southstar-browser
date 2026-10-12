//! Southstar — libavif's decoder and YUV-to-RGB entry points, with struct offsets meson computes from the installed avif.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr::NonNull;
use southstar_av::layout::{Field, lookup};

const LAYOUT: &str = match option_env!("NS_AVIF_LAYOUT") {
    Some(layout) => layout,
    None => "",
};

const fn value(key: &str) -> i64 {
    if LAYOUT.is_empty() {
        0
    } else {
        lookup(LAYOUT, key)
    }
}

const fn field<S, T: Copy>(key: &str) -> Field<S, T> {
    Field::at(value(key))
}

#[repr(C)]
struct AvifDecoder {
    _private: [u8; 0],
}

#[repr(C)]
struct AvifImage {
    _private: [u8; 0],
}

#[repr(C)]
struct AvifRgbImage {
    _private: [u8; 0],
}

const RESULT_OK: c_int = value("AVIF_RESULT_OK") as c_int;
const RGB_FORMAT_BGRA: c_int = value("AVIF_RGB_FORMAT_BGRA") as c_int;
const STRICT_DISABLED: u32 = value("AVIF_STRICT_DISABLED") as u32;
const RGB_IMAGE_SIZE: usize = value("sizeof.avifRGBImage") as usize;

const MAX_THREADS: Field<AvifDecoder, c_int> = field("avifDecoder.maxThreads");
const IMAGE_SIZE_LIMIT: Field<AvifDecoder, u32> = field("avifDecoder.imageSizeLimit");
const IMAGE_DIMENSION_LIMIT: Field<AvifDecoder, u32> = field("avifDecoder.imageDimensionLimit");
const STRICT_FLAGS: Field<AvifDecoder, u32> = field("avifDecoder.strictFlags");
const IMAGE: Field<AvifDecoder, *mut AvifImage> = field("avifDecoder.image");
const IMAGE_WIDTH: Field<AvifImage, u32> = field("avifImage.width");
const IMAGE_HEIGHT: Field<AvifImage, u32> = field("avifImage.height");
const RGB_FORMAT: Field<AvifRgbImage, c_int> = field("avifRGBImage.format");
const RGB_DEPTH: Field<AvifRgbImage, u32> = field("avifRGBImage.depth");
const RGB_ALPHA_PREMULTIPLIED: Field<AvifRgbImage, c_int> =
    field("avifRGBImage.alphaPremultiplied");
const RGB_PIXELS: Field<AvifRgbImage, *mut u8> = field("avifRGBImage.pixels");
const RGB_ROW_BYTES: Field<AvifRgbImage, u32> = field("avifRGBImage.rowBytes");

unsafe extern "C" {
    fn avifDecoderCreate() -> *mut AvifDecoder;
    fn avifDecoderDestroy(decoder: *mut AvifDecoder);
    fn avifDecoderSetIOMemory(decoder: *mut AvifDecoder, data: *const u8, size: usize) -> c_int;
    fn avifDecoderParse(decoder: *mut AvifDecoder) -> c_int;
    fn avifDecoderNextImage(decoder: *mut AvifDecoder) -> c_int;
    fn avifRGBImageSetDefaults(rgb: *mut AvifRgbImage, image: *const AvifImage);
    fn avifRGBImageAllocatePixels(rgb: *mut AvifRgbImage) -> c_int;
    fn avifRGBImageFreePixels(rgb: *mut AvifRgbImage);
    fn avifImageYUVToRGB(image: *const AvifImage, rgb: *mut AvifRgbImage) -> c_int;
}

pub struct Limits {
    pub max_pixels: u32,
    pub max_dimension: u32,
}

pub struct Decoder<'a> {
    raw: NonNull<AvifDecoder>,
    _data: core::marker::PhantomData<&'a [u8]>,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8], limits: &Limits) -> Option<Decoder<'a>> {
        if LAYOUT.is_empty() {
            return None;
        }
        let raw = NonNull::new(unsafe { avifDecoderCreate() })?;
        let decoder = Decoder {
            raw,
            _data: core::marker::PhantomData,
        };
        let p = raw.as_ptr();
        unsafe {
            MAX_THREADS.set(p, 1);
            IMAGE_SIZE_LIMIT.set(p, limits.max_pixels);
            IMAGE_DIMENSION_LIMIT.set(p, limits.max_dimension);
            STRICT_FLAGS.set(p, STRICT_DISABLED);
            if avifDecoderSetIOMemory(p, data.as_ptr(), data.len()) != RESULT_OK
                || avifDecoderParse(p) != RESULT_OK
                || avifDecoderNextImage(p) != RESULT_OK
            {
                return None;
            }
        }
        Some(decoder)
    }

    pub fn size(&self) -> (u32, u32) {
        let image = unsafe { IMAGE.get(self.raw.as_ptr()) };
        if image.is_null() {
            return (0, 0);
        }
        unsafe { (IMAGE_WIDTH.get(image), IMAGE_HEIGHT.get(image)) }
    }

    pub fn to_bgra_premultiplied(&self) -> Option<RgbImage> {
        let image = unsafe { IMAGE.get(self.raw.as_ptr()) };
        if image.is_null() {
            return None;
        }
        let mut rgb = RgbImage {
            storage: vec![0u64; RGB_IMAGE_SIZE.div_ceil(8)],
            allocated: false,
        };
        let p = rgb.raw();
        unsafe {
            avifRGBImageSetDefaults(p, image);
            RGB_FORMAT.set(p, RGB_FORMAT_BGRA);
            RGB_DEPTH.set(p, 8);
            RGB_ALPHA_PREMULTIPLIED.set(p, 1);
            if avifRGBImageAllocatePixels(p) != RESULT_OK {
                return None;
            }
            rgb.allocated = true;
            if avifImageYUVToRGB(image, p) != RESULT_OK {
                return None;
            }
        }
        Some(rgb)
    }
}

impl Drop for Decoder<'_> {
    fn drop(&mut self) {
        unsafe { avifDecoderDestroy(self.raw.as_ptr()) };
    }
}

pub struct RgbImage {
    storage: Vec<u64>,
    allocated: bool,
}

impl RgbImage {
    fn raw(&mut self) -> *mut AvifRgbImage {
        self.storage.as_mut_ptr().cast()
    }

    fn raw_const(&self) -> *const AvifRgbImage {
        self.storage.as_ptr().cast()
    }

    pub fn row_bytes(&self) -> usize {
        unsafe { RGB_ROW_BYTES.get(self.raw_const()) as usize }
    }

    pub fn pixels(&self, height: usize) -> Option<&[u8]> {
        let pixels = unsafe { RGB_PIXELS.get(self.raw_const()) };
        let len = self.row_bytes().checked_mul(height)?;
        if pixels.is_null() {
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts(pixels, len) })
    }
}

impl Drop for RgbImage {
    fn drop(&mut self) {
        if self.allocated {
            let p = self.raw();
            unsafe { avifRGBImageFreePixels(p) };
        }
    }
}
