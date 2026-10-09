//! Southstar — the C ABI of cascade layer order: the rank table css.c's gather loop looks each rule's layer up in, built from a style pass's sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use southstar_glib::{self as glib, GHashTable};

use super::selector_view::SheetRef;
use crate::layers;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_layer_ranks_build(
    ua: *const c_void,
    author: *const *const c_void,
    n_author: usize,
) -> *mut GHashTable {
    let sheets = unsafe { SheetRef::list(ua, author, n_author) };
    let ranks = layers::ranks(sheets.iter().flat_map(|sheet| sheet.layer_names()));
    let table = unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            None,
        )
    };
    for (name, rank) in ranks {
        unsafe {
            glib::g_hash_table_insert(table, glib::strdup(name).cast(), (rank + 1) as *mut c_void)
        };
    }
    table
}
