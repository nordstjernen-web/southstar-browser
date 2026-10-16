//! Southstar — the style engine, ported from css.c: the colour parser, lengths with calc() and the other math functions, container queries, gradients, positions and image values, transforms, grid values, font values, shadows, time values, easing functions and animation lists, var() in @keyframes stops, display, counter and list values, border-image, the property table, the per-property value parser, shorthand expansion, the inline style text behind element.style, serializing, interpolating and comparing values, reading declaration blocks, parsing selectors, flattening nesting, evaluating @supports, parsing style sheets with their at-rules, scoping shadow trees' style sheets to their hosts, turning legacy HTML attributes into presentational hints, the element state behind pseudo-classes, attr() and var() substitution, the custom-property cascade, CSS.registerProperty()'s registry and registered-property values, the user-agent style sheet, caching parsed style sheets and the text <style> elements contribute, gathering the rules that match an element, sharing computed styles between elements, the lifetimes of values, styles and sheets, the fixups computed styles get after the cascade, the walk that computes a document's styles with incremental restyle, answering layout's and paint's computed-style queries, resolving pending var() and attr() declarations, applying matched declarations to a style, resolving units in computed styles, the computed text of grid track lists, cascade layer order, the rule index, matching selectors against elements and the restyle invalidation DOM mutations trigger.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod animation;
mod attr_fn;
mod border_image;
mod calc;
mod cascade;
mod clip_path;
mod color;
mod computed_units;
mod container;
mod content;
mod counter;
mod custom_props;
mod declarations;
mod display;
mod element_state;
mod ffi;
mod fixups;
mod font;
mod gather;
mod gradient;
mod grid;
mod hints;
mod host_scope;
mod image;
mod incremental;
mod initial;
mod inline;
mod keyframes;
mod layers;
mod lex;
mod matcher;
mod math;
mod nesting;
mod pending;
mod position;
mod prop;
mod property;
mod restyle;
mod rule_index;
mod scan;
mod selector;
mod shadow;
mod sheet;
mod shorthand;
mod style_query;
mod style_share;
mod style_text;
mod supports;
mod text;
mod time;
mod timing;
mod track_text;
mod transform;
mod units;
mod values;
mod vars;

pub use animation::{
    Entry as AnimationEntry, range_serialize as animation_range_serialize,
    shorthand_serialize as animation_shorthand_serialize,
};
pub use border_image::{Params as BorderImageParams, params as border_image_params};
pub use calc::{Calc, CalcArg, math_fn_px as calc_math_fn_px};
pub use cascade::currentcolor_bit;
pub use color::parse_color;
pub use container::condition_canonical as container_condition_canonical;
pub use container::{
    features_begin as container_features_begin, features_used as container_features_used,
};
pub use container::{set_unit_dims as set_container_unit_dims, unit_dims as container_unit_dims};
pub use content::append_unescaped;
pub use counter::list_style_serialize;
pub use declarations::{named_declaration_valid, named_property_supported, sizes_resolve};
pub use element_state::node_dir;
pub use element_state::{mark_visited, set_doc_base, set_doc_language, set_target_fragment};
pub use ffi::viewport::get as viewport;
pub use ffi::{
    ContainerMap, Declaration, Declarations, FontFaceRule, FontMetrics, FontOracle, FrameViewport,
    ImportRule, KeyframeStop, Keyframes, MatchContext, NsCssValue, OwnedSheet, OwnedValue,
    PROP_COUNT, PageRule, QueryKey, RawAreaRect as GridAreaRect, RawAreas as GridAreas,
    RawStyle as StyleData, RegisterError, Registration, RetainedStyle, Selector, SelectorList,
    Sheet, ValueKind, animation_effective, animation_lists, clear_registered_properties,
    compute_styles, incremental_exclude, merged_styles_cached, parse_import_cached,
    parse_url_cached, register_property, relayout_enter, relayout_leave, selector_cache_begin,
    selector_cache_end, set_active_node, set_focus_node, set_focus_visible_node, set_font_alias,
    set_font_oracle, set_frame_viewport_measure, set_fullscreen_node, set_hover_node,
    set_render_zoom, style_before_change, style_element_cache_begin, style_element_cache_end,
    style_may_animate, style_retain, style_scale_font_size, style_value_slot,
    stylesheet_from_style_element_cached, tracks_computed_text, value_interpolate,
    value_resolve_font_units, value_serialize, values_equal,
};
pub use font::{
    family_for_pango as font_family_for_pango, shorthand_canonical as font_shorthand_canonical,
    stretch_rank as font_stretch_rank, weight_number as font_weight_number,
};
pub use gradient::{
    Gradient, Stop as GradientStop, angle as gradient_angle, radii as gradient_radii,
};
pub use grid::{
    AUTO_REPEAT_FIT, AUTO_REPEAT_NONE, LINE_NAMES_MAX, LineName as GridLineName, TRACK_AUTO,
    TRACK_FR, TRACK_MAX_CONTENT, TRACK_MIN_CONTENT, TRACK_PERCENT, TRACK_PX, TRACKS_MAX,
    Track as GridTrack, Tracks as GridTracks,
};
pub use grid::{
    grid_compose as grid_shorthand_compose, placement_compose as grid_placement_compose,
    template_compose as grid_template_compose,
};
pub use initial::initial_value;
pub use inline::{
    background_position_zip as background_position_join, columns_serialize,
    get as inline_style_get, longhand_names as inline_style_longhands, mask_shorthand_serialize,
    serialize as inline_style_serialize, set as inline_style_set, shorthand_members,
};
pub use matcher::{clear_defined_elements, register_defined_element};
pub use nesting::has_container_units as text_has_container_units;
pub use prop::Prop;
pub use restyle::{
    attr_may_affect_style, mark_attr_dirty, mark_childlist_dirty, mark_restyle_dirty,
};
pub use scan::strip_important;
pub use shadow::{Shadow, ShadowList};
pub use style_query::{
    alignment_base, dimension_px, overflow_keyword, text_orientation, used_column_count,
    writing_mode_of,
};
pub use style_text::{shadow_adopted_text, style_element_text};
pub use supports::{condition as supports_condition, declaration as supports_declaration};
pub use time::list_serialize as time_list_serialize;
pub use timing::{
    Timing as TimingFunction, easing_canonical, linear_eval as timing_linear_eval,
    list_canonical as timing_list_canonical, parse as timing_parse, serialize as timing_serialize,
};
pub use transform::{
    Op as TransformOp, Transform, effective as effective_transform, individual_property_serialize,
    is_3d as transform_is_3d, to_mat4 as transform_to_mat4,
};
pub use values::specified_canonical;
