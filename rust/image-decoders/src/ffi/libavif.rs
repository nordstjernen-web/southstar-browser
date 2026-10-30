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
        // SAFETY: `avifDecoderCreate` takes no arguments; the decoder it returns is owned by `Decoder` and destroyed in its `Drop`.
        let raw = NonNull::new(unsafe { avifDecoderCreate() })?;
        let decoder = Decoder {
            raw,
            _data: core::marker::PhantomData,
        };
        decoder.set(MAX_THREADS, 1);
        decoder.set(IMAGE_SIZE_LIMIT, limits.max_pixels);
        decoder.set(IMAGE_DIMENSION_LIMIT, limits.max_dimension);
        decoder.set(STRICT_FLAGS, STRICT_DISABLED);
        let p = raw.as_ptr();
        // SAFETY: the decoder reads `data` only while it lives, and `Decoder<'a>` borrows `data` for that long.
        if unsafe { avifDecoderSetIOMemory(p, data.as_ptr(), data.len()) } != RESULT_OK {
            return None;
        }
        // SAFETY: `p` is a live decoder with its input set.
        if unsafe { avifDecoderParse(p) } != RESULT_OK {
            return None;
        }
        // SAFETY: `p` is a live decoder that parsed successfully.
        if unsafe { avifDecoderNextImage(p) } != RESULT_OK {
            return None;
        }
        Some(decoder)
    }

    fn set<T: Copy>(&self, field: Field<AvifDecoder, T>, value: T) {
        // SAFETY: the decoder is live while `self` is, and the field's offset and type come from the installed avif.h.
        unsafe { field.set(self.raw.as_ptr(), value) };
    }

    fn image(&self) -> Option<NonNull<AvifImage>> {
        // SAFETY: the decoder is live while `self` is, and the field's offset and type come from the installed avif.h.
        NonNull::new(unsafe { IMAGE.get(self.raw.as_ptr()) })
    }

    fn image_field(&self, field: Field<AvifImage, u32>) -> u32 {
        self.image().map_or(0, |image| {
            // SAFETY: `image` is the decoder's current image, owned by the live decoder, and the field's offset and type come from the installed avif.h.
            unsafe { field.get(image.as_ptr()) }
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (
            self.image_field(IMAGE_WIDTH),
            self.image_field(IMAGE_HEIGHT),
        )
    }

    pub fn to_bgra_premultiplied(&self) -> Option<RgbImage> {
        let image = self.image()?.as_ptr();
        let mut rgb = RgbImage {
            storage: vec![0u64; RGB_IMAGE_SIZE.div_ceil(8)],
            allocated: false,
            height: self.image_field(IMAGE_HEIGHT) as usize,
        };
        // SAFETY: the storage is `sizeof(avifRGBImage)` 8-byte-aligned writable bytes and `image` is the decoder's live current image.
        unsafe { avifRGBImageSetDefaults(rgb.raw(), image) };
        rgb.set(RGB_FORMAT, RGB_FORMAT_BGRA);
        rgb.set(RGB_DEPTH, 8);
        rgb.set(RGB_ALPHA_PREMULTIPLIED, 1);
        // SAFETY: the storage holds an initialised `avifRGBImage` without pixels; `RgbImage`'s `Drop` frees what this allocates.
        if unsafe { avifRGBImageAllocatePixels(rgb.raw()) } != RESULT_OK {
            return None;
        }
        rgb.allocated = true;
        // SAFETY: `image` is the decoder's live current image and the RGB image has pixels allocated for its size.
        if unsafe { avifImageYUVToRGB(image, rgb.raw()) } != RESULT_OK {
            return None;
        }
        Some(rgb)
    }
}

impl Drop for Decoder<'_> {
    fn drop(&mut self) {
        // SAFETY: the decoder came from `avifDecoderCreate`, is owned by this value and is destroyed only here.
        unsafe { avifDecoderDestroy(self.raw.as_ptr()) };
    }
}

pub struct RgbImage {
    storage: Vec<u64>,
    allocated: bool,
    height: usize,
}

impl RgbImage {
    fn raw(&mut self) -> *mut AvifRgbImage {
        self.storage.as_mut_ptr().cast()
    }

    fn set<T: Copy>(&mut self, field: Field<AvifRgbImage, T>, value: T) {
        let p = self.raw();
        // SAFETY: the storage spans `sizeof(avifRGBImage)` bytes, and the field's offset and type come from the installed avif.h.
        unsafe { field.set(p, value) };
    }

    fn get<T: Copy>(&self, field: Field<AvifRgbImage, T>) -> T {
        // SAFETY: the storage spans `sizeof(avifRGBImage)` initialised bytes, and the field's offset and type come from the installed avif.h.
        unsafe { field.get(self.storage.as_ptr().cast()) }
    }

    pub fn row_bytes(&self) -> usize {
        self.get(RGB_ROW_BYTES) as usize
    }

    pub fn pixels(&self) -> Option<&[u8]> {
        let pixels = self.get(RGB_PIXELS);
        let len = self.row_bytes().checked_mul(self.height)?;
        if !self.allocated || pixels.is_null() {
            return None;
        }
        // SAFETY: `avifRGBImageAllocatePixels` allocated `rowBytes * height` bytes at `pixels`, `height` being the image height `avifRGBImageSetDefaults` copied, and they live until `Drop` frees them.
        Some(unsafe { core::slice::from_raw_parts(pixels, len) })
    }
}

impl Drop for RgbImage {
    fn drop(&mut self) {
        if self.allocated {
            let p = self.raw();
            // SAFETY: the pixels were allocated by `avifRGBImageAllocatePixels` and are freed only here.
            unsafe { avifRGBImageFreePixels(p) };
        }
    }
}
