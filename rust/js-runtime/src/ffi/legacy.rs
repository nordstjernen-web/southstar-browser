//! Southstar — the QuickJS class id of the Element wrapper, which the interface branding on QuickJS still asks for.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::quickjs;

#[unsafe(no_mangle)]
pub extern "C" fn ns_element_class_id() -> u32 {
    quickjs::class_id(&super::wrapper::ELEMENT_CLASS)
}
