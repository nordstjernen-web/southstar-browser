//! Southstar — the image-format check presentational hints make through rust/image.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_glib::GBoolean;

unsafe extern "C" {
    fn ns_image_supports_mime(mime: *const c_char) -> GBoolean;
}

pub(crate) fn image_supports_mime(mime: &CStr) -> bool {
    unsafe { ns_image_supports_mime(mime.as_ptr()) != 0 }
}
