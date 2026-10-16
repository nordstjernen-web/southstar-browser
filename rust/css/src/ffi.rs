//! Southstar — the C ABI of the style engine, and the GLib calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod animation;
mod cascade;
mod compute;
mod computed_units;
mod container;
mod custom_props;
mod declarations;
mod element_state;
mod fixups;
mod font;
mod gather;
mod hints;
mod host_scope;
mod inline;
mod keyframes;
mod layers;
mod matcher;
mod nesting;
mod pending;
mod property;
mod query;
mod registry;
mod rule_index;
mod selector;
mod selector_view;
mod sheet;
mod sheet_cache;
mod shorthand;
mod style_alloc;
mod style_query;
mod style_share;
mod ua;
mod value;
mod values;
mod vars;
pub(crate) mod viewport;

use core::ffi::{CStr, c_double};

use southstar_glib::{self as glib, GBoolean};

use crate::{calc, units};

pub(crate) use computed_units::{ComputedStyle, PROP_COUNT, Slot, SlotMut, StyleView};
pub(crate) use container::{Container, container_map};
pub(crate) use declarations::{media_query_matches, syntax_def_valid};
pub(crate) use element_state::{
    regex_matches_whole, unichar_is_alpha, unichar_is_rtl_script, url_is_valid_absolute,
    url_resolve,
};
pub(crate) use font::relative_unit_px as font_relative_px;
pub use font::set_font_alias;
pub(crate) use font::{font_alias, font_available, font_generation, font_oracle_serial};
pub(crate) use hints::image_supports_mime;
pub(crate) use host_scope::scoped as scoped_css;
pub(crate) use inline::{SheetDecl, first_rule_declares, sheet_declarations};
pub(crate) use matcher::{
    active_node, focus_node, focus_visible_node, fullscreen_node, hover_node, match_scope,
    set_match_scope,
};
pub use matcher::{
    set_active_node, set_focus_node, set_focus_visible_node, set_fullscreen_node, set_hover_node,
};
pub use query::{MatchContext, QueryKey, Selector, SelectorList};
pub use registry::{RegisterError, Registration, clear_registered_properties, register_property};
pub(crate) use selector_view::{
    AttrRef, CompoundRef, GroupRef, PseudoRef, RuleRef, ScopeRef, SelectorRef, SheetRef,
};
pub(crate) use sheet::{PageRule, SheetBuilder, SyntaxDef};
pub(crate) use shorthand::prop_named;
pub(crate) use value::RawCalc;
pub use value::{NsCssValue, RawAreaRect, RawAreas};

pub(crate) fn prefers_dark() -> bool {
    southstar_css_media::prefers_dark()
}

pub(crate) fn viewport() -> (f64, f64) {
    viewport::get()
}

pub(crate) fn parse_int(text: &[u8], default: i32, min: i32, max: i32) -> i32 {
    let text = std::ffi::CString::new(text).unwrap_or_default();
    southstar_dom::controls::parse_int(Some(&text), default, min, max)
}

pub(crate) fn strtod(text: &CStr, pos: usize) -> (f64, usize) {
    glib::ascii_strtod_at(text, pos)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_length_or(v: *const NsCssValue, fallback: c_double) -> c_double {
    if let Some(length) = unsafe { value::length_of(v) }
        && (length.unit == units::PX || length.unit == units::NUMBER)
    {
        return length.v;
    }
    unsafe { value::calc_of(v) }.map_or(fallback, |calc| calc.px)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_calc_is_math_fn(v: *const NsCssValue) -> GBoolean {
    let math_fn =
        unsafe { value::calc_of(v) }.is_some_and(|calc| calc.func != 0 && calc.n_args > 0);
    glib::boolean(math_fn)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_calc_math_fn_px(v: *const NsCssValue, basis: c_double) -> c_double {
    let Some(raw) = (unsafe { v.as_ref() }) else {
        return 0.0;
    };
    calc::math_fn_px(&unsafe { raw.u.calc }.to_calc(), basis)
}
