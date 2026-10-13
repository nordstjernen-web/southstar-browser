//! Southstar — the raw-QuickJS natives and installers the window and worker realms still bind; each moves to an nsr_ Rust declaration when its crate is converted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{JSContext, JSValue};

macro_rules! natives {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(
                pub(crate) fn $name(
                    ctx: *mut JSContext,
                    this_val: JSValue,
                    argc: c_int,
                    argv: *mut JSValue,
                ) -> JSValue;
            )*
        }
    };
}

macro_rules! installers {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(pub(crate) fn $name(ctx: *mut JSContext, target: JSValue);)*
        }
    };
}

pub(crate) use super::legacy::ns_illegal_constructor;

macro_rules! context_installers {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(pub(crate) fn $name(ctx: *mut JSContext);)*
        }
    };
}

natives! {
    ns_window_requestAnimationFrame,
    ns_window_cancelAnimationFrame,
    ns_window_url_update_object,
    ns_window_btoa,
    ns_window_atob,
    ns_window_url_ctor,
    ns_window_url_can_parse,
    ns_window_url_parse_static,
    ns_window_url_create_object,
    ns_window_url_revoke_object,
    ns_window_usp_ctor,
    ns_window_text_encoder_ctor,
    ns_window_text_decoder_ctor,
    ns_window_filereader_ctor,
}

installers! {
    ns_js_input_install_wpt,
    ns_net_install_text_codecs,
    ns_storage_install_window,
    ns_idb_install,
}

context_installers! {
    ns_usp_install_interface,
    ns_url_install_interface,
}
