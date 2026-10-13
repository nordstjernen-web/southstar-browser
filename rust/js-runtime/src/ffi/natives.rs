//! Southstar — the raw-QuickJS natives the window and worker realms still bind; each moves to an nsr_ Rust declaration when its crate is converted.
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

pub(crate) use super::legacy::ns_illegal_constructor;

natives! {
}
