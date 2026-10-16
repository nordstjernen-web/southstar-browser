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
mod style_share;
mod ua;
mod value;
mod values;
mod vars;
pub(crate) mod viewport;

use core::ffi::CStr;

use southstar_glib::{self as glib};

pub use animation::{animation_effective, animation_lists, style_may_animate};
pub use compute::{compute_styles, incremental_exclude, set_render_zoom, style_before_change};
pub(crate) use computed_units::{ComputedStyle, Slot, SlotMut, StyleView, slot};
pub use computed_units::{
    PROP_COUNT, RawStyle, style_retain, style_scale_font_size, style_value_slot,
    tracks_computed_text,
};
pub use container::ContainerMap;
pub(crate) use container::{Container, container_map_installed};
pub use declarations::{Declaration, Declarations};
pub(crate) use declarations::{media_query_matches, syntax_def_valid};
pub(crate) use element_state::{
    regex_matches_whole, unichar_is_alpha, unichar_is_rtl_script, url_is_valid_absolute,
    url_resolve,
};
pub(crate) use font::relative_unit_px as font_relative_px;
pub use font::{FontMetrics, FontOracle, set_font_alias, set_font_oracle};
pub(crate) use font::{font_alias, font_available, font_generation, font_oracle_serial};
pub use gather::{selector_cache_begin, selector_cache_end};
pub(crate) use hints::image_supports_mime;
pub(crate) use host_scope::scoped as scoped_css;
pub(crate) use inline::{SheetDecl, first_rule_declares, sheet_declarations};
pub use keyframes::{KeyframeStop, Keyframes};
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
pub use sheet::{FontFaceRule, ImportRule, OwnedSheet, PageRule, Sheet};
pub(crate) use sheet::{SheetBuilder, SyntaxDef};
pub use sheet_cache::{
    merged_styles_cached, parse_import_cached, parse_url_cached, relayout_enter, relayout_leave,
    style_element_cache_begin, style_element_cache_end, stylesheet_from_style_element_cached,
};
pub(crate) use shorthand::prop_named;
pub use style_alloc::RetainedStyle;
pub(crate) use value::RawCalc;
pub use value::{NsCssValue, OwnedValue, RawAreaRect, RawAreas, ValueKind};
pub use values::{value_interpolate, value_serialize, values_equal};
pub use viewport::{FrameViewport, set_frame_viewport_measure};

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
