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

natives! {
    ns_window_requestAnimationFrame,
    ns_window_cancelAnimationFrame,
}

installers! {
    ns_js_input_install_wpt,
    ns_storage_install_window,
}
