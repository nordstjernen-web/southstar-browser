//! Southstar — adapters that give the natives whose shapes differ from a Prop table entry the getter, setter or method signature the tables expect.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_js_attrs as attrs;
use southstar_js_document as document;
use southstar_js_engine::{Scope, Value};
use southstar_js_focus as focus;
use southstar_js_forms as forms;
use southstar_js_geometry as geometry;
use southstar_js_media as media;
use southstar_js_node as node;
use southstar_js_runtime::api;
use southstar_js_shadow as shadow;
use southstar_js_text as text;
use southstar_js_top_layer as top_layer;

const MAX_FIXED_ARGS: usize = 3;

macro_rules! native_getters {
    ($($name:ident => $f:path,)*) => {
        $(
            pub(crate) fn $name(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
                $f(scope, this, &[])
            }
        )*
    };
}

macro_rules! native_setters {
    ($($name:ident => $f:path,)*) => {
        $(
            pub(crate) fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> Result<(), Value> {
                $f(scope, this, core::slice::from_ref(value)).map(drop)
            }
        )*
    };
}

macro_rules! fixed_methods {
    ($($name:ident => $f:path,)*) => {
        $(
            pub(crate) fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
                $f(scope, this, &args[..args.len().min(MAX_FIXED_ARGS)])
            }
        )*
    };
}

macro_rules! node_getters {
    ($($name:ident => $f:path,)*) => {
        $(
            pub(crate) fn $name(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
                api::make_element(scope, Node::ptr_or_null($f(api::unwrap_element(this))))
            }
        )*
    };
}

macro_rules! text_getters {
    ($($name:ident => $f:path,)*) => {
        $(
            pub(crate) fn $name(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
                let text = $f(api::unwrap_element(this));
                Ok(node::names::to_value(scope, text))
            }
        )*
    };
}

native_getters! {
    validity_get_valid => forms::validity::valid,
    element_get_inner_text => text::rendered::get_inner_text,
    element_get_class_list => attrs::element::class_list,
    element_get_rel_list => attrs::element::rel_list,
    element_get_validity => forms::validity::validity,
    element_get_validation_message => forms::validity::validation_message,
    element_get_will_validate => forms::validity::will_validate_getter,
    input_get_files => forms::files::get_files,
    element_get_indeterminate => forms::checkable::indeterminate,
    element_get_selection_start => forms::selection::selection_start,
    element_get_selection_end => forms::selection::selection_end,
    element_get_selection_dir => forms::selection::selection_direction,
    text_control_get_text_length => forms::selection::text_length,
    element_get_default_value => forms::value::default_value,
    element_get_default_checked => forms::checkable::default_checked,
    element_get_default_selected => forms::select::default_selected,
    media_get_current_time => media::element::current_time,
    media_get_duration => media::element::duration,
    media_get_paused => media::element::paused,
    media_get_ended => media::element::ended,
    media_get_volume => media::element::volume,
    media_get_playback_rate => media::element::playback_rate,
    media_get_default_playback_rate => media::element::default_playback_rate,
    media_get_error => media::element::error,
    media_get_muted => media::element::muted,
    media_get_ready_state => media::element::ready_state,
    media_get_network_state => media::element::network_state,
    media_get_seekable_ranges => media::element::seekable,
    media_get_buffered_ranges => media::element::buffered,
    media_get_played_ranges => media::element::played,
    media_get_text_tracks => media::tracks::text_tracks,
    element_get_value_as_number => forms::value::value_as_number,
    element_get_value_as_date => forms::value::value_as_date,
    element_get_progress_position => forms::value::progress_position,
    element_get_offset_parent => geometry::metrics::get_offset_parent,
    media_get_src_object => media::element::src_object,
    element_get_internals => forms::internals::internals,
    dialog_get_return_value => top_layer::dialog::get_return_value,
    element_get_owner_svg_element => geometry::svg::get_owner_svg_element,
    element_get_whole_text => text::rendered::get_whole_text,
    element_get_offset_top => geometry::metrics::get_offset_top,
    element_get_offset_left => geometry::metrics::get_offset_left,
    element_get_offset_width => geometry::metrics::get_offset_width,
    element_get_offset_height => geometry::metrics::get_offset_height,
    element_get_client_top => geometry::metrics::get_client_top,
    element_get_client_left => geometry::metrics::get_client_left,
    element_get_client_width => geometry::metrics::get_client_width,
    element_get_client_height => geometry::metrics::get_client_height,
    element_get_scroll_top => geometry::metrics::get_scroll_top,
    element_get_scroll_left => geometry::metrics::get_scroll_left,
    element_get_scroll_width => geometry::metrics::get_scroll_width,
    element_get_scroll_height => geometry::metrics::get_scroll_height,
    element_get_attributes => attrs::element::attributes,
    element_get_sizes_list => attrs::element::sizes_list,
    element_get_sandbox_list => attrs::element::sandbox_list,
    element_get_selected => forms::select::selected,
    element_get_label_control => forms::labels::control,
    element_get_popover_target_element => top_layer::invoker::get_popover_target,
    element_get_shadow_root => shadow::root::get_shadow_root,
    element_get_assigned_slot => shadow::slot::get_assigned_slot,
    element_get_checked => forms::checkable::checked,
    element_get_label_prop => forms::value::label,
    element_get_selected_index => forms::select::selected_index,
    element_get_options => forms::select::options_getter,
    element_get_selected_options => forms::select::selected_options,
    element_get_form_elements => forms::collection::form_elements,
    media_get_seeking => media::element::seeking,
    namedmap_get_length => attrs::named_map::get_length,
    document_get_document_element => document::elements::get_document_element,
    document_get_body => document::elements::get_body,
    document_get_head => document::elements::get_head,
    document_get_active_element => document::elements::get_active_element,
    document_get_scripts => document::elements::get_scripts,
    document_get_embeds => document::elements::empty_list,
    document_get_plugins => document::elements::empty_list,
    document_get_design_mode => document::props::get_design_mode,
    document_get_last_modified => document::props::get_last_modified,
    document_get_anchors => document::elements::get_anchors,
    document_get_applets => document::elements::empty_list,
    document_get_current_script => document::elements::get_current_script,
    document_get_fullscreen_element => focus::fullscreen::get_fullscreen_element,
    document_get_is_fullscreen => focus::fullscreen::get_is_fullscreen,
    document_get_pointer_lock_element => focus::fullscreen::get_pointer_lock_element,
    document_get_fullscreen_enabled => focus::fullscreen::get_fullscreen_enabled,
    document_get_scrolling_element => document::elements::get_scrolling_element,
    document_get_title => document::props::get_title,
    document_get_cookie => document::cookie::get_cookie,
    document_get_referrer => document::props::get_referrer,
    document_get_ready_state => document::props::get_ready_state,
    document_get_hidden => document::props::get_hidden,
    document_get_visibility_state => document::props::get_visibility_state,
    document_get_compat_mode => document::props::get_compat_mode,
    document_get_dir => document::props::get_dir,
    document_get_xml_version => document::props::get_xml_version,
    element_get_popover => top_layer::popover::get_popover,
    dialog_get_closed_by => top_layer::dialog::get_closed_by,
    button_get_command => top_layer::invoker::get_command,
    button_get_command_for_element => top_layer::invoker::get_command_for,
    element_get_value_prop => forms::value::get_value,
}

text_getters! {
    element_get_local_name => node::names::local_name_of,
    element_get_prefix => node::names::prefix_of,
    element_get_node_name => node::names::node_name,
    element_get_namespace_uri => node::names::namespace_uri_of,
}

native_setters! {
    element_set_inner_text => text::rendered::set_inner_text,
    element_set_outer_text => text::rendered::set_outer_text,
    element_set_style => attrs::element::set_style,
    element_set_class_list => attrs::element::set_class_list,
    element_set_rel_list => attrs::element::set_rel_list,
    element_set_indeterminate => forms::checkable::set_indeterminate_prop,
    element_set_selection_start => forms::selection::set_selection_start,
    element_set_selection_end => forms::selection::set_selection_end,
    element_set_selection_dir => forms::selection::set_selection_direction,
    element_set_default_value => forms::value::set_default_value,
    element_set_default_checked => forms::checkable::set_default_checked,
    element_set_default_selected => forms::select::set_default_selected,
    media_set_current_time => media::element::set_current_time,
    media_set_volume => media::element::set_volume,
    media_set_playback_rate => media::element::set_playback_rate,
    media_set_default_playback_rate => media::element::set_default_playback_rate,
    media_set_muted => media::element::set_muted,
    element_set_value_as_number => forms::value::set_value_as_number,
    element_set_value_as_date => forms::value::set_value_as_date,
    media_set_src_object => media::element::set_src_object,
    dialog_set_return_value => top_layer::dialog::set_return_value,
    element_set_scroll_top => geometry::scroll::set_scroll_top,
    element_set_scroll_left => geometry::scroll::set_scroll_left,
    element_set_selected => forms::select::set_selected,
    element_set_popover_target_element => top_layer::invoker::set_popover_target,
    element_set_checked => forms::checkable::set_checked,
    element_set_label_prop => forms::value::set_label,
    element_set_selected_index => forms::select::set_selected_index,
    document_set_body => document::elements::set_body,
    document_set_title => document::props::set_title,
    document_set_cookie => document::cookie::set_cookie,
    document_set_dir => document::props::set_dir,
    element_set_popover => top_layer::popover::set_popover,
    dialog_set_closed_by => top_layer::dialog::set_closed_by,
    button_set_command => top_layer::invoker::set_command,
    button_set_command_for_element => top_layer::invoker::set_command_for,
    element_set_value_prop => forms::value::set_value,
}

node_getters! {
    element_get_parent_element => node::tree::parent_element,
    element_get_parent_node => node::tree::parent_node,
    element_get_first_element_child => node::tree::first_element_child,
    element_get_last_element_child => node::tree::last_element_child,
    element_get_next_element_sibling => node::tree::next_element_sibling,
    element_get_previous_element_sibling => node::tree::previous_element_sibling,
    element_get_first_child => node::tree::first_child,
    element_get_last_child => node::tree::last_child,
    element_get_next_sibling => node::tree::next_sibling,
    element_get_previous_sibling => node::tree::previous_sibling,
}

fixed_methods! {
    element_set_attribute => node::attributes::set_attribute,
    element_remove_attribute => node::attributes::remove_attribute,
    element_toggle_attribute => node::attributes::toggle_attribute,
    element_get_attribute_ns => node::attributes::get_attribute_ns,
    element_has_attribute_ns => node::attributes::has_attribute_ns,
    element_set_attribute_ns => node::attributes::set_attribute_ns,
    element_remove_attribute_ns => node::attributes::remove_attribute_ns,
    element_is_same_node => node::attributes::is_same_node,
    element_get_attribute_names => node::attributes::get_attribute_names,
    element_has_attributes => node::attributes::has_attributes,
}
