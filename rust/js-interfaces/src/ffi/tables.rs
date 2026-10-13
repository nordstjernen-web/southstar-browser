//! Southstar — the property tables of the DOM prototypes: element, media, anchor, document and the other interfaces' native methods and accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_canvas as canvas;
use southstar_js_attrs as attrs;
use southstar_js_bind as bind;
use southstar_js_collections as collections;
use southstar_js_cssom as cssom;
use southstar_js_dispatch as dispatch;
use southstar_js_document as document;
use southstar_js_dom_factory as dom_factory;
use southstar_js_elements as elements;
use southstar_js_engine::{
    NativeFn, Prop, Scope, Value, accessor, accessor_magic, method, method_enumerable,
};
use southstar_js_focus as focus;
use southstar_js_forms as forms;
use southstar_js_frames as frames;
use southstar_js_geometry as geometry;
use southstar_js_html as html;
use southstar_js_input as input;
use southstar_js_lifecycle as lifecycle;
use southstar_js_media as media;
use southstar_js_node as node;
use southstar_js_reflect as reflect;
use southstar_js_services as services;
use southstar_js_shadow as shadow;
use southstar_js_text as text;
use southstar_js_top_layer as top_layer;
use southstar_js_traversal as traversal;

use super::{adapt, natives};

unsafe extern "Rust" {
    safe fn nsr_tlist_contains(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_tlist_add(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_tlist_remove(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_tlist_toggle(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_tlist_replace(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_tlist_item(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_tlist_supports(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    safe fn nsr_tlist_toString(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    safe fn nsr_tlist_get_length(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value>;
    safe fn nsr_tlist_get_value(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value>;
    safe fn nsr_tlist_set_value(
        scope: &mut Scope<'_>,
        this: &Value,
        value: &Value,
    ) -> Result<(), Value>;
    safe fn nsr_element_attr_getter(
        scope: &mut Scope<'_>,
        this: &Value,
        magic: i32,
    ) -> Result<Value, Value>;
    safe fn nsr_element_attr_setter(
        scope: &mut Scope<'_>,
        this: &Value,
        value: &Value,
        magic: i32,
    ) -> Result<(), Value>;
}

pub(crate) static TLIST_PROTO: [Prop; 10] = [
    method("contains", 1, nsr_tlist_contains),
    method("add", 1, nsr_tlist_add),
    method("remove", 1, nsr_tlist_remove),
    method("toggle", 1, nsr_tlist_toggle),
    method("replace", 2, nsr_tlist_replace),
    method("item", 1, nsr_tlist_item),
    method("supports", 1, nsr_tlist_supports),
    method("toString", 0, nsr_tlist_toString),
    accessor("length", Some(nsr_tlist_get_length), None),
    accessor(
        "value",
        Some(nsr_tlist_get_value),
        Some(nsr_tlist_set_value),
    ),
];

pub(crate) static VALIDITY_PROTO_FUNCS: [Prop; 1] =
    [accessor("valid", Some(adapt::validity_get_valid), None)];

pub(crate) static ELEMENT_PROTO_FUNCS: [Prop; 497] = [
    method("getNumberOfChars", 0, natives::element_get_number_of_chars),
    method("getSVGDocument", 0, frames::owner::svg_document),
    accessor(
        "contentDocument",
        Some(frames::owner::content_document),
        Some(elements::element::ignore),
    ),
    accessor(
        "contentWindow",
        Some(frames::owner::content_window),
        Some(elements::element::ignore),
    ),
    accessor(
        "tagName",
        Some(node::owner::tag_name),
        Some(elements::element::ignore),
    ),
    accessor(
        "accessKeyLabel",
        Some(natives::element_get_access_key_label),
        Some(elements::element::ignore),
    ),
    accessor(
        "localName",
        Some(adapt::element_get_local_name),
        Some(elements::element::ignore),
    ),
    accessor(
        "prefix",
        Some(adapt::element_get_prefix),
        Some(elements::element::ignore),
    ),
    accessor(
        "textContent",
        Some(html::text::text_content_get),
        Some(html::text::text_content_set),
    ),
    accessor(
        "innerText",
        Some(adapt::element_get_inner_text),
        Some(adapt::element_set_inner_text),
    ),
    accessor(
        "outerText",
        Some(adapt::element_get_inner_text),
        Some(adapt::element_set_outer_text),
    ),
    accessor(
        "id",
        Some(reflect::strings::id_get),
        Some(reflect::strings::id_set),
    ),
    accessor(
        "className",
        Some(reflect::strings::class_name_get),
        Some(reflect::strings::class_name_set),
    ),
    accessor(
        "innerHTML",
        Some(html::markup::inner_html_get),
        Some(html::markup::inner_html_set),
    ),
    accessor(
        "outerHTML",
        Some(html::markup::outer_html_get),
        Some(html::markup::outer_html_set),
    ),
    accessor(
        "style",
        Some(natives::element_get_style),
        Some(adapt::element_set_style),
    ),
    accessor(
        "classList",
        Some(adapt::element_get_class_list),
        Some(adapt::element_set_class_list),
    ),
    accessor(
        "relList",
        Some(adapt::element_get_rel_list),
        Some(adapt::element_set_rel_list),
    ),
    accessor(
        "itemScope",
        Some(html::microdata::item_scope_get),
        Some(html::microdata::item_scope_set),
    ),
    accessor(
        "itemId",
        Some(html::microdata::item_id_get),
        Some(html::microdata::item_id_set),
    ),
    accessor(
        "itemType",
        Some(html::microdata::item_type_get),
        Some(elements::element::ignore),
    ),
    accessor(
        "itemProp",
        Some(html::microdata::item_prop_get),
        Some(elements::element::ignore),
    ),
    accessor(
        "itemRef",
        Some(html::microdata::item_ref_get),
        Some(elements::element::ignore),
    ),
    accessor(
        "itemValue",
        Some(html::microdata::item_value_get),
        Some(html::microdata::item_value_set),
    ),
    accessor(
        "properties",
        Some(html::microdata::properties_get),
        Some(elements::element::ignore),
    ),
    accessor(
        "parentElement",
        Some(adapt::element_get_parent_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "parentNode",
        Some(adapt::element_get_parent_node),
        Some(elements::element::ignore),
    ),
    accessor(
        "firstElementChild",
        Some(adapt::element_get_first_element_child),
        Some(elements::element::ignore),
    ),
    accessor(
        "lastElementChild",
        Some(adapt::element_get_last_element_child),
        Some(elements::element::ignore),
    ),
    accessor(
        "nextElementSibling",
        Some(adapt::element_get_next_element_sibling),
        Some(elements::element::ignore),
    ),
    accessor(
        "previousElementSibling",
        Some(adapt::element_get_previous_element_sibling),
        Some(elements::element::ignore),
    ),
    accessor(
        "firstChild",
        Some(adapt::element_get_first_child),
        Some(elements::element::ignore),
    ),
    accessor(
        "lastChild",
        Some(adapt::element_get_last_child),
        Some(elements::element::ignore),
    ),
    accessor(
        "nextSibling",
        Some(adapt::element_get_next_sibling),
        Some(elements::element::ignore),
    ),
    accessor(
        "previousSibling",
        Some(adapt::element_get_previous_sibling),
        Some(elements::element::ignore),
    ),
    accessor(
        "childNodes",
        Some(node::owner::child_nodes),
        Some(elements::element::ignore),
    ),
    accessor(
        "childElementCount",
        Some(natives::element_get_child_element_count),
        Some(elements::element::ignore),
    ),
    accessor(
        "children",
        Some(node::owner::children),
        Some(elements::element::ignore),
    ),
    method("getAttribute", 1, natives::element_get_attribute),
    method("hasAttribute", 1, natives::element_has_attribute),
    method("setAttribute", 2, adapt::element_set_attribute),
    method("removeAttribute", 1, adapt::element_remove_attribute),
    method("showPopover", 0, top_layer::popover::show_popover),
    method("hidePopover", 0, top_layer::popover::hide_popover),
    method("togglePopover", 0, top_layer::popover::toggle_popover),
    method("toggleAttribute", 1, adapt::element_toggle_attribute),
    method("getAttributeNS", 2, adapt::element_get_attribute_ns),
    method("hasAttributeNS", 2, adapt::element_has_attribute_ns),
    method("setAttributeNS", 3, adapt::element_set_attribute_ns),
    method("removeAttributeNS", 2, adapt::element_remove_attribute_ns),
    method(
        "requestFullscreen",
        0,
        focus::fullscreen::request_fullscreen,
    ),
    method(
        "webkitRequestFullscreen",
        0,
        focus::fullscreen::request_fullscreen,
    ),
    method(
        "webkitRequestFullScreen",
        0,
        focus::fullscreen::request_fullscreen,
    ),
    method(
        "mozRequestFullScreen",
        0,
        focus::fullscreen::request_fullscreen,
    ),
    method(
        "msRequestFullscreen",
        0,
        focus::fullscreen::request_fullscreen,
    ),
    method("getAnimations", 0, bind::natives::empty_array),
    method("animate", 2, cssom::animate::element_animate),
    method("getRootNode", 1, shadow::root::get_root_node),
    method("isEqualNode", 1, natives::element_is_equal_node),
    method("isSameNode", 1, adapt::element_is_same_node),
    method(
        "compareDocumentPosition",
        1,
        natives::element_compare_document_position,
    ),
    method("lookupPrefix", 1, dom_factory::namespaces::lookup_prefix),
    method(
        "lookupNamespaceURI",
        1,
        dom_factory::namespaces::lookup_namespace_uri,
    ),
    method(
        "isDefaultNamespace",
        1,
        dom_factory::namespaces::is_default_namespace,
    ),
    method("getClientRects", 0, geometry::metrics::get_client_rects),
    method("scrollBy", 2, geometry::scroll::element_scroll_by),
    method("scrollTo", 2, geometry::scroll::element_scroll_to),
    method("scroll", 2, geometry::scroll::element_scroll_to),
    method(
        "scrollIntoViewIfNeeded",
        1,
        geometry::scroll::scroll_into_view,
    ),
    method(
        "requestPointerLock",
        0,
        focus::fullscreen::request_pointer_lock,
    ),
    method("releasePointerLock", 0, bind::natives::noop),
    method("releaseCapture", 0, bind::natives::noop),
    method("setCapture", 0, bind::natives::noop),
    method("associateForm", 1, bind::natives::noop),
    method("setMessage", 2, bind::natives::noop),
    method("syncInputValidity", 1, bind::natives::noop),
    method("_connect", 0, bind::natives::noop),
    method("_disconnect", 0, bind::natives::noop),
    accessor(
        "length",
        Some(elements::node_value::text_length),
        Some(elements::element::ignore),
    ),
    method("substringData", 2, text::cdata::substring_data),
    method("appendData", 1, text::cdata::append_data),
    method("deleteData", 2, text::cdata::delete_data),
    method("insertData", 2, text::cdata::insert_data),
    method("replaceData", 3, text::cdata::replace_data),
    method("splitText", 1, text::cdata::split_text),
    method("select", 0, forms::selection::select),
    method(
        "setSelectionRange",
        3,
        forms::selection::set_selection_range,
    ),
    method("setRangeText", 1, forms::selection::set_range_text),
    method("stepUp", 0, forms::selection::step_up),
    method("stepDown", 0, forms::selection::step_down),
    method("showPicker", 0, forms::picker::show_picker),
    method("play", 0, media::element::play),
    method("pause", 0, media::element::pause),
    method("load", 0, media::element::load),
    method("canPlayType", 1, media::support::can_play_type),
    method("fastSeek", 1, media::element::fast_seek),
    method("addTextTrack", 3, media::tracks::add_text_track),
    method("setMediaKeys", 1, media::eme::set_media_keys),
    method(
        "getVideoPlaybackQuality",
        0,
        media::element::video_playback_quality,
    ),
    method(
        "requestVideoFrameCallback",
        1,
        lifecycle::frames::request_video_frame_callback,
    ),
    method(
        "cancelVideoFrameCallback",
        1,
        lifecycle::frames::cancel_animation_frame,
    ),
    accessor(
        "validity",
        Some(adapt::element_get_validity),
        Some(elements::element::ignore),
    ),
    accessor(
        "validationMessage",
        Some(adapt::element_get_validation_message),
        Some(elements::element::ignore),
    ),
    accessor(
        "willValidate",
        Some(adapt::element_get_will_validate),
        Some(elements::element::ignore),
    ),
    accessor(
        "labels",
        Some(elements::element::labels),
        Some(elements::element::ignore),
    ),
    accessor(
        "files",
        Some(adapt::input_get_files),
        Some(elements::element::ignore),
    ),
    accessor(
        "indeterminate",
        Some(adapt::element_get_indeterminate),
        Some(adapt::element_set_indeterminate),
    ),
    accessor(
        "selectionStart",
        Some(adapt::element_get_selection_start),
        Some(adapt::element_set_selection_start),
    ),
    accessor(
        "selectionEnd",
        Some(adapt::element_get_selection_end),
        Some(adapt::element_set_selection_end),
    ),
    accessor(
        "selectionDirection",
        Some(adapt::element_get_selection_dir),
        Some(adapt::element_set_selection_dir),
    ),
    accessor(
        "textLength",
        Some(adapt::text_control_get_text_length),
        Some(elements::element::ignore),
    ),
    accessor(
        "defaultValue",
        Some(adapt::element_get_default_value),
        Some(adapt::element_set_default_value),
    ),
    accessor(
        "defaultChecked",
        Some(adapt::element_get_default_checked),
        Some(adapt::element_set_default_checked),
    ),
    accessor(
        "defaultSelected",
        Some(adapt::element_get_default_selected),
        Some(adapt::element_set_default_selected),
    ),
    accessor(
        "currentTime",
        Some(adapt::media_get_current_time),
        Some(adapt::media_set_current_time),
    ),
    accessor(
        "duration",
        Some(adapt::media_get_duration),
        Some(elements::element::ignore),
    ),
    accessor(
        "paused",
        Some(adapt::media_get_paused),
        Some(elements::element::ignore),
    ),
    accessor(
        "ended",
        Some(adapt::media_get_ended),
        Some(elements::element::ignore),
    ),
    accessor(
        "seeking",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
    accessor(
        "volume",
        Some(adapt::media_get_volume),
        Some(adapt::media_set_volume),
    ),
    accessor(
        "playbackRate",
        Some(adapt::media_get_playback_rate),
        Some(adapt::media_set_playback_rate),
    ),
    accessor(
        "defaultPlaybackRate",
        Some(adapt::media_get_default_playback_rate),
        Some(adapt::media_set_default_playback_rate),
    ),
    accessor(
        "error",
        Some(adapt::media_get_error),
        Some(elements::element::ignore),
    ),
    accessor(
        "muted",
        Some(adapt::media_get_muted),
        Some(adapt::media_set_muted),
    ),
    accessor(
        "readyState",
        Some(adapt::media_get_ready_state),
        Some(elements::element::ignore),
    ),
    accessor(
        "networkState",
        Some(adapt::media_get_network_state),
        Some(elements::element::ignore),
    ),
    accessor(
        "seekable",
        Some(adapt::media_get_seekable_ranges),
        Some(elements::element::ignore),
    ),
    accessor(
        "buffered",
        Some(adapt::media_get_buffered_ranges),
        Some(elements::element::ignore),
    ),
    accessor(
        "played",
        Some(adapt::media_get_played_ranges),
        Some(elements::element::ignore),
    ),
    accessor(
        "textTracks",
        Some(adapt::media_get_text_tracks),
        Some(elements::element::ignore),
    ),
    accessor(
        "videoTracks",
        Some(elements::element::empty_array),
        Some(elements::element::ignore),
    ),
    accessor(
        "audioTracks",
        Some(elements::element::empty_array),
        Some(elements::element::ignore),
    ),
    accessor(
        "valueAsNumber",
        Some(adapt::element_get_value_as_number),
        Some(adapt::element_set_value_as_number),
    ),
    accessor(
        "valueAsDate",
        Some(adapt::element_get_value_as_date),
        Some(adapt::element_set_value_as_date),
    ),
    accessor(
        "position",
        Some(adapt::element_get_progress_position),
        Some(elements::element::ignore),
    ),
    accessor(
        "encoding",
        Some(elements::element::form_enctype),
        Some(elements::element::set_form_enctype),
    ),
    accessor(
        "isContentEditable",
        Some(elements::element::is_content_editable),
        Some(elements::element::ignore),
    ),
    accessor(
        "translate",
        Some(reflect::strings::translate_get),
        Some(reflect::strings::translate_set),
    ),
    accessor(
        "offsetParent",
        Some(adapt::element_get_offset_parent),
        Some(elements::element::ignore),
    ),
    accessor(
        "videoWidth",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
    accessor(
        "videoHeight",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
    accessor(
        "srcObject",
        Some(adapt::media_get_src_object),
        Some(adapt::media_set_src_object),
    ),
    accessor(
        "clientInformation",
        Some(elements::element::null),
        Some(elements::element::ignore),
    ),
    method("decode", 0, bind::natives::resolved_undefined),
    method("toBlob", 1, canvas::context::element_to_blob),
    method("attachInternals", 0, forms::internals::attach_internals),
    accessor(
        "_internals",
        Some(adapt::element_get_internals),
        Some(elements::element::ignore),
    ),
    accessor(
        "index",
        Some(elements::element::option_index),
        Some(elements::element::ignore),
    ),
    accessor(
        "rows",
        Some(elements::table::rows),
        Some(elements::table::set_rows),
    ),
    accessor(
        "caption",
        Some(elements::table::caption),
        Some(elements::element::ignore),
    ),
    accessor(
        "tHead",
        Some(elements::table::thead),
        Some(elements::element::ignore),
    ),
    accessor(
        "tFoot",
        Some(elements::table::tfoot),
        Some(elements::element::ignore),
    ),
    accessor(
        "tBodies",
        Some(elements::table::tbodies),
        Some(elements::element::ignore),
    ),
    accessor(
        "cells",
        Some(elements::table::cells),
        Some(elements::element::ignore),
    ),
    accessor(
        "rowIndex",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
    accessor(
        "sectionRowIndex",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
    accessor(
        "cellIndex",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
    accessor_magic(
        "colSpan",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        6,
    ),
    accessor_magic(
        "rowSpan",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        7,
    ),
    accessor(
        "returnValue",
        Some(adapt::dialog_get_return_value),
        Some(adapt::dialog_set_return_value),
    ),
    method("createCaption", 0, elements::table::create_caption),
    method("createTHead", 0, elements::table::create_thead),
    method("createTFoot", 0, elements::table::create_tfoot),
    method("createTBody", 0, elements::table::create_tbody),
    method("deleteCaption", 0, elements::table::delete_caption),
    method("deleteTHead", 0, elements::table::delete_thead),
    method("deleteTFoot", 0, elements::table::delete_tfoot),
    method("insertRow", 1, elements::table::insert_row),
    method("deleteRow", 1, elements::table::delete_row),
    method("insertCell", 1, elements::table::insert_cell),
    method("deleteCell", 1, elements::table::delete_cell),
    method("add", 2, forms::select::add),
    method("appendChild", 1, node::child::append_child),
    method("removeChild", 1, node::child::remove_child),
    method("insertBefore", 2, node::child::insert_before),
    method("moveBefore", 2, node::child::move_before),
    method("replaceChild", 2, node::child::replace_child),
    method("insertAdjacentHTML", 2, html::markup::insert_adjacent_html),
    method("getHTML", 0, html::markup::get_html),
    method("setHTMLUnsafe", 1, html::markup::set_html_unsafe),
    method(
        "insertAdjacentElement",
        2,
        html::text::insert_adjacent_element,
    ),
    method("insertAdjacentText", 2, html::text::insert_adjacent_text),
    method("replaceChildren", 0, node::sequence::replace_children),
    method("getAttributeNames", 0, adapt::element_get_attribute_names),
    method("getAttributeNode", 1, attrs::attr::get_attribute_node),
    method("getAttributeNodeNS", 2, attrs::attr::get_attribute_node_ns),
    method("removeAttributeNode", 1, attrs::attr::remove_attribute_node),
    method("setAttributeNode", 1, attrs::attr::set_attribute_node),
    method("setAttributeNodeNS", 1, attrs::attr::set_attribute_node),
    method("hasAttributes", 0, adapt::element_has_attributes),
    method("remove", 0, node::sequence::remove),
    method("cloneNode", 1, dom_factory::adopt::clone_node),
    method("normalize", 0, text::rendered::normalize),
    method("append", 0, node::sequence::append),
    method("prepend", 0, node::sequence::prepend),
    method("before", 0, node::sequence::before),
    method("after", 0, node::sequence::after),
    method("replaceWith", 0, node::sequence::replace_with),
    method("addEventListener", 2, dispatch::listeners::element_add),
    method(
        "removeEventListener",
        2,
        dispatch::listeners::element_remove,
    ),
    method("getElementsByTagName", 1, collections::live::element_by_tag),
    method(
        "getElementsByTagNameNS",
        2,
        collections::live::element_by_tag_ns,
    ),
    method(
        "getElementById",
        1,
        collections::query::element_get_element_by_id,
    ),
    method(
        "getElementsByClassName",
        1,
        collections::live::element_by_class,
    ),
    method(
        "querySelector",
        1,
        collections::query::element_query_selector,
    ),
    method(
        "querySelectorAll",
        1,
        collections::query::element_query_selector_all,
    ),
    method("matches", 1, collections::query::matches),
    method("webkitMatchesSelector", 1, collections::query::matches),
    method("closest", 1, collections::query::closest),
    method("contains", 1, natives::element_contains),
    method("hasChildNodes", 0, natives::element_has_child_nodes),
    method(
        "getBoundingClientRect",
        0,
        geometry::metrics::get_bounding_client_rect,
    ),
    method("getBBox", 0, geometry::svg::get_bbox),
    method("getCTM", 0, geometry::svg::get_ctm),
    method("getScreenCTM", 0, geometry::svg::get_screen_ctm),
    method("getTotalLength", 0, geometry::svg::get_total_length),
    method("getPointAtLength", 1, geometry::svg::get_point_at_length),
    method("createSVGPoint", 0, geometry::svg::create_svg_point),
    method("createSVGRect", 0, geometry::svg::create_svg_rect),
    method("createSVGMatrix", 0, geometry::svg::create_svg_matrix),
    method("createSVGTransform", 0, geometry::svg::create_svg_transform),
    accessor(
        "ownerSVGElement",
        Some(adapt::element_get_owner_svg_element),
        Some(elements::element::ignore),
    ),
    method("focus", 0, focus::focus::focus_method),
    method("blur", 0, focus::focus::blur_method),
    method("click", 0, focus::click::click_method),
    method("submit", 0, forms::submit::submit_method),
    method("requestSubmit", 0, forms::submit::request_submit_method),
    method("reset", 0, forms::submit::reset_method),
    method("checkValidity", 0, forms::validity::check_validity),
    method("reportValidity", 0, forms::validity::check_validity),
    method("setCustomValidity", 1, forms::validity::set_custom_validity),
    method("cancelAsync", 1, services::timers::clear),
    method("announce", 1, bind::natives::noop),
    method("scrollIntoView", 0, geometry::scroll::scroll_into_view),
    method("checkVisibility", 0, geometry::visibility::check_visibility),
    method("setPointerCapture", 1, input::capture::set),
    method("releasePointerCapture", 1, input::capture::release),
    method("hasPointerCapture", 1, input::capture::has),
    method("show", 0, top_layer::dialog::show),
    method("showModal", 0, top_layer::dialog::show_modal_method),
    method("close", 0, top_layer::dialog::close_method),
    method("requestClose", 0, top_layer::dialog::request_close_method),
    method("dispatchEvent", 1, dispatch::target::element_dispatch),
    method("assignedNodes", 0, shadow::slot::assigned_nodes),
    method("assignedElements", 0, shadow::slot::assigned_elements),
    method("getContext", 1, canvas::context::element_get_context),
    method("toDataURL", 0, canvas::context::element_to_data_url),
    accessor(
        "nodeType",
        Some(natives::element_get_node_type),
        Some(elements::element::ignore),
    ),
    accessor(
        "nodeValue",
        Some(elements::node_value::node_value),
        Some(elements::node_value::set_node_value),
    ),
    accessor(
        "wholeText",
        Some(adapt::element_get_whole_text),
        Some(elements::element::ignore),
    ),
    method("replaceWholeText", 1, text::cdata::replace_whole_text),
    accessor(
        "nodeName",
        Some(adapt::element_get_node_name),
        Some(elements::element::ignore),
    ),
    accessor(
        "dataset",
        Some(natives::element_get_dataset),
        Some(elements::element::ignore),
    ),
    accessor(
        "offsetTop",
        Some(adapt::element_get_offset_top),
        Some(elements::element::ignore),
    ),
    accessor(
        "offsetLeft",
        Some(adapt::element_get_offset_left),
        Some(elements::element::ignore),
    ),
    accessor(
        "offsetWidth",
        Some(adapt::element_get_offset_width),
        Some(elements::element::ignore),
    ),
    accessor(
        "offsetHeight",
        Some(adapt::element_get_offset_height),
        Some(elements::element::ignore),
    ),
    accessor(
        "clientTop",
        Some(adapt::element_get_client_top),
        Some(elements::element::ignore),
    ),
    accessor(
        "clientLeft",
        Some(adapt::element_get_client_left),
        Some(elements::element::ignore),
    ),
    accessor(
        "clientWidth",
        Some(adapt::element_get_client_width),
        Some(elements::element::ignore),
    ),
    accessor(
        "clientHeight",
        Some(adapt::element_get_client_height),
        Some(elements::element::ignore),
    ),
    accessor(
        "scrollTop",
        Some(adapt::element_get_scroll_top),
        Some(adapt::element_set_scroll_top),
    ),
    accessor(
        "scrollLeft",
        Some(adapt::element_get_scroll_left),
        Some(adapt::element_set_scroll_left),
    ),
    accessor(
        "scrollWidth",
        Some(adapt::element_get_scroll_width),
        Some(elements::element::ignore),
    ),
    accessor(
        "scrollHeight",
        Some(adapt::element_get_scroll_height),
        Some(elements::element::ignore),
    ),
    accessor(
        "attributes",
        Some(adapt::element_get_attributes),
        Some(elements::element::ignore),
    ),
    accessor(
        "naturalWidth",
        Some(elements::image::natural_width),
        Some(elements::element::ignore),
    ),
    accessor(
        "naturalHeight",
        Some(elements::image::natural_height),
        Some(elements::element::ignore),
    ),
    accessor(
        "complete",
        Some(elements::image::complete),
        Some(elements::element::ignore),
    ),
    accessor(
        "currentSrc",
        Some(elements::image::current_src),
        Some(elements::element::ignore),
    ),
    accessor(
        "content",
        Some(node::owner::template_content),
        Some(elements::element::set_content),
    ),
    accessor(
        "hidden",
        Some(elements::element::hidden),
        Some(elements::element::set_hidden),
    ),
    accessor_magic(
        "title",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        0,
    ),
    accessor_magic(
        "name",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        1,
    ),
    accessor_magic(
        "alt",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        2,
    ),
    accessor_magic(
        "src",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        3,
    ),
    accessor_magic(
        "href",
        Some(natives::element_anchor_part_get),
        Some(natives::element_anchor_href_set),
        0,
    ),
    accessor_magic(
        "protocol",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        1,
    ),
    accessor_magic(
        "host",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        2,
    ),
    accessor_magic(
        "hostname",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        3,
    ),
    accessor_magic(
        "port",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        4,
    ),
    accessor_magic(
        "pathname",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        5,
    ),
    accessor_magic(
        "search",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        6,
    ),
    accessor_magic(
        "hash",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        7,
    ),
    accessor_magic(
        "origin",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        8,
    ),
    accessor_magic(
        "username",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        9,
    ),
    accessor_magic(
        "password",
        Some(natives::element_anchor_part_get),
        Some(natives::element_url_part_set),
        10,
    ),
    accessor(
        "type",
        Some(reflect::strings::type_get),
        Some(natives::element_set_type),
    ),
    accessor_magic(
        "placeholder",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        6,
    ),
    accessor_magic(
        "lang",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        7,
    ),
    accessor(
        "dir",
        Some(reflect::strings::dir_get),
        Some(reflect::strings::dir_set),
    ),
    accessor_magic(
        "action",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        9,
    ),
    accessor_magic(
        "method",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        2,
    ),
    accessor_magic(
        "enctype",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        11,
    ),
    accessor_magic(
        "target",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        12,
    ),
    accessor_magic(
        "rel",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        13,
    ),
    accessor_magic(
        "accept",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        14,
    ),
    accessor_magic(
        "acceptCharset",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        15,
    ),
    accessor(
        "autocomplete",
        Some(reflect::strings::autocomplete_get),
        Some(reflect::strings::autocomplete_set),
    ),
    accessor(
        "list",
        Some(elements::element::list),
        Some(elements::element::set_list),
    ),
    accessor_magic(
        "min",
        Some(natives::element_range_number_getter),
        Some(natives::element_range_number_setter),
        0,
    ),
    accessor_magic(
        "max",
        Some(natives::element_range_number_getter),
        Some(natives::element_range_number_setter),
        1,
    ),
    accessor_magic(
        "low",
        Some(natives::element_range_number_getter),
        Some(natives::element_range_number_setter),
        2,
    ),
    accessor_magic(
        "high",
        Some(natives::element_range_number_getter),
        Some(natives::element_range_number_setter),
        3,
    ),
    accessor_magic(
        "optimum",
        Some(natives::element_range_number_getter),
        Some(natives::element_range_number_setter),
        4,
    ),
    accessor_magic(
        "step",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        20,
    ),
    accessor_magic(
        "pattern",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        21,
    ),
    accessor(
        "spellcheck",
        Some(reflect::strings::spellcheck_get),
        Some(reflect::strings::spellcheck_set),
    ),
    accessor_magic(
        "crossOrigin",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        3,
    ),
    accessor_magic(
        "referrerPolicy",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        4,
    ),
    accessor_magic(
        "decoding",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        1,
    ),
    accessor_magic(
        "loading",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        0,
    ),
    accessor_magic(
        "fetchPriority",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        27,
    ),
    accessor(
        "sizes",
        Some(adapt::element_get_sizes_list),
        Some(reflect::strings::sizes_set),
    ),
    accessor(
        "sandbox",
        Some(adapt::element_get_sandbox_list),
        Some(reflect::strings::sandbox_set),
    ),
    accessor_magic(
        "srcset",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        29,
    ),
    accessor_magic(
        "useMap",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        30,
    ),
    accessor_magic(
        "inputMode",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        31,
    ),
    accessor_magic(
        "size",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        2,
    ),
    accessor_magic(
        "cols",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        3,
    ),
    accessor_magic(
        "allowFullscreen",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        0,
    ),
    accessor_magic(
        "declare",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        1,
    ),
    accessor_magic(
        "defaultMuted",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        2,
    ),
    accessor_magic(
        "default",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        3,
    ),
    accessor_magic(
        "noHref",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        4,
    ),
    accessor_magic(
        "noShade",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        5,
    ),
    accessor_magic(
        "compact",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        6,
    ),
    accessor_magic(
        "noWrap",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        7,
    ),
    accessor_magic(
        "trueSpeed",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        8,
    ),
    accessor_magic(
        "noResize",
        Some(reflect::numeric::plain_bool_get),
        Some(reflect::numeric::plain_bool_set),
        9,
    ),
    accessor_magic(
        "face",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        130,
    ),
    accessor_magic(
        "text",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        131,
    ),
    accessor_magic(
        "hspace",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        11,
    ),
    accessor_magic(
        "vspace",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        12,
    ),
    accessor_magic(
        "scrollAmount",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        13,
    ),
    accessor_magic(
        "scrollDelay",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        14,
    ),
    accessor_magic(
        "maxLength",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        0,
    ),
    accessor_magic(
        "minLength",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        1,
    ),
    accessor_magic(
        "span",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        5,
    ),
    accessor_magic(
        "width",
        Some(natives::element_dimension_getter),
        Some(natives::element_dimension_setter),
        8,
    ),
    accessor_magic(
        "height",
        Some(natives::element_dimension_getter),
        Some(natives::element_dimension_setter),
        9,
    ),
    method("beginElement", 0, geometry::svg::begin_element),
    method("setCurrentTime", 1, geometry::svg::set_current_time),
    accessor_magic(
        "start",
        Some(reflect::numeric::int_get),
        Some(reflect::numeric::int_set),
        10,
    ),
    accessor_magic(
        "coords",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        37,
    ),
    accessor_magic(
        "shape",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        38,
    ),
    accessor_magic(
        "formAction",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        39,
    ),
    accessor_magic(
        "formMethod",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        40,
    ),
    accessor_magic(
        "formEnctype",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        41,
    ),
    accessor_magic(
        "formTarget",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        42,
    ),
    accessor_magic(
        "integrity",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        43,
    ),
    accessor_magic(
        "kind",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        44,
    ),
    accessor_magic(
        "hreflang",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        46,
    ),
    accessor_magic(
        "charset",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        47,
    ),
    accessor_magic(
        "ping",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        90,
    ),
    accessor_magic(
        "rev",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        91,
    ),
    accessor_magic(
        "as",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        92,
    ),
    accessor_magic(
        "align",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        93,
    ),
    accessor_magic(
        "vAlign",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        94,
    ),
    accessor_magic(
        "ch",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        95,
    ),
    accessor_magic(
        "chOff",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        96,
    ),
    accessor_magic(
        "bgColor",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        97,
    ),
    accessor_magic(
        "background",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        98,
    ),
    accessor_magic(
        "link",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        99,
    ),
    accessor_magic(
        "vLink",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        100,
    ),
    accessor_magic(
        "aLink",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        101,
    ),
    accessor_magic(
        "color",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        102,
    ),
    accessor_magic(
        "clear",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        103,
    ),
    accessor_magic(
        "summary",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        104,
    ),
    accessor_magic(
        "frame",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        105,
    ),
    accessor_magic(
        "rules",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        106,
    ),
    accessor_magic(
        "border",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        107,
    ),
    accessor_magic(
        "cellPadding",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        108,
    ),
    accessor_magic(
        "cellSpacing",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        109,
    ),
    accessor_magic(
        "axis",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        110,
    ),
    accessor_magic(
        "abbr",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        111,
    ),
    accessor_magic(
        "headers",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        112,
    ),
    accessor_magic(
        "scheme",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        113,
    ),
    accessor_magic(
        "standby",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        114,
    ),
    accessor_magic(
        "codeType",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        115,
    ),
    accessor_magic(
        "codeBase",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        116,
    ),
    accessor_magic(
        "code",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        117,
    ),
    accessor_magic(
        "archive",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        118,
    ),
    accessor_magic(
        "scrolling",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        119,
    ),
    accessor_magic(
        "frameBorder",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        120,
    ),
    accessor_magic(
        "marginWidth",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        121,
    ),
    accessor_magic(
        "marginHeight",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        122,
    ),
    accessor_magic(
        "longDesc",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        123,
    ),
    accessor_magic(
        "lowsrc",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        124,
    ),
    accessor_magic(
        "version",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        125,
    ),
    accessor_magic(
        "event",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        126,
    ),
    accessor_magic(
        "valueType",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        127,
    ),
    accessor_magic(
        "srclang",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        128,
    ),
    accessor_magic(
        "dirName",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        129,
    ),
    accessor_magic(
        "httpEquiv",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        49,
    ),
    accessor(
        "contentEditable",
        Some(reflect::strings::content_editable_get),
        Some(reflect::strings::content_editable_set),
    ),
    accessor_magic(
        "slot",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        51,
    ),
    accessor_magic(
        "role",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        0,
    ),
    accessor_magic(
        "ariaLabel",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        1,
    ),
    accessor_magic(
        "ariaBrailleLabel",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        2,
    ),
    accessor_magic(
        "ariaBrailleRoleDescription",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        3,
    ),
    accessor_magic(
        "ariaColCount",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        4,
    ),
    accessor_magic(
        "ariaColIndex",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        5,
    ),
    accessor_magic(
        "ariaColIndexText",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        6,
    ),
    accessor_magic(
        "ariaColSpan",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        7,
    ),
    accessor_magic(
        "ariaDescription",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        8,
    ),
    accessor_magic(
        "ariaKeyShortcuts",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        9,
    ),
    accessor_magic(
        "ariaLevel",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        10,
    ),
    accessor_magic(
        "ariaPlaceholder",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        11,
    ),
    accessor_magic(
        "ariaPosInSet",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        12,
    ),
    accessor_magic(
        "ariaRelevant",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        13,
    ),
    accessor_magic(
        "ariaRoleDescription",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        14,
    ),
    accessor_magic(
        "ariaRowCount",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        15,
    ),
    accessor_magic(
        "ariaRowIndex",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        16,
    ),
    accessor_magic(
        "ariaRowIndexText",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        17,
    ),
    accessor_magic(
        "ariaRowSpan",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        18,
    ),
    accessor_magic(
        "ariaSetSize",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        19,
    ),
    accessor_magic(
        "ariaValueMax",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        20,
    ),
    accessor_magic(
        "ariaValueMin",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        21,
    ),
    accessor_magic(
        "ariaValueNow",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        22,
    ),
    accessor_magic(
        "ariaValueText",
        Some(reflect::strings::aria_get),
        Some(reflect::strings::aria_set),
        23,
    ),
    accessor_magic(
        "ariaHidden",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        14,
    ),
    accessor_magic(
        "ariaDisabled",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        11,
    ),
    accessor_magic(
        "ariaPressed",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        21,
    ),
    accessor_magic(
        "ariaExpanded",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        12,
    ),
    accessor_magic(
        "ariaControls",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        59,
    ),
    accessor_magic(
        "ariaDescribedBy",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        60,
    ),
    accessor_magic(
        "ariaLabelledBy",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        61,
    ),
    accessor_magic(
        "ariaLive",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        16,
    ),
    accessor_magic(
        "ariaBusy",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        8,
    ),
    accessor_magic(
        "ariaChecked",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        9,
    ),
    accessor_magic(
        "ariaCurrent",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        10,
    ),
    accessor_magic(
        "ariaSelected",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        24,
    ),
    accessor_magic(
        "ariaAtomic",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        6,
    ),
    accessor_magic(
        "ariaAutoComplete",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        7,
    ),
    accessor_magic(
        "ariaHasPopup",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        13,
    ),
    accessor_magic(
        "ariaInvalid",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        15,
    ),
    accessor_magic(
        "ariaModal",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        17,
    ),
    accessor_magic(
        "ariaMultiLine",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        18,
    ),
    accessor_magic(
        "ariaMultiSelectable",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        19,
    ),
    accessor_magic(
        "ariaOrientation",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        20,
    ),
    accessor_magic(
        "ariaReadOnly",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        22,
    ),
    accessor_magic(
        "ariaRequired",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        23,
    ),
    accessor_magic(
        "ariaSort",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        25,
    ),
    accessor_magic(
        "nonce",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        72,
    ),
    accessor_magic(
        "accessKey",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        73,
    ),
    accessor_magic(
        "dateTime",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        74,
    ),
    accessor_magic(
        "srcdoc",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        75,
    ),
    accessor_magic(
        "popoverTargetAction",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        77,
    ),
    accessor(
        "autocapitalize",
        Some(reflect::strings::autocapitalize_get),
        Some(reflect::strings::autocapitalize_set),
    ),
    accessor_magic(
        "enterKeyHint",
        Some(reflect::strings::enum_get),
        Some(reflect::strings::enum_set),
        5,
    ),
    accessor_magic(
        "charSet",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        47,
    ),
    accessor_magic(
        "poster",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        83,
    ),
    accessor_magic(
        "preload",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        84,
    ),
    accessor_magic(
        "wrap",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        85,
    ),
    accessor_magic(
        "scope",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        86,
    ),
    accessor_magic(
        "cite",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        87,
    ),
    accessor_magic(
        "media",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        88,
    ),
    accessor_magic(
        "download",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        89,
    ),
    accessor_magic(
        "open",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        0,
    ),
    accessor(
        "selected",
        Some(adapt::element_get_selected),
        Some(adapt::element_set_selected),
    ),
    accessor_magic(
        "multiple",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        2,
    ),
    accessor_magic(
        "readOnly",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        3,
    ),
    accessor_magic(
        "autofocus",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        4,
    ),
    accessor_magic(
        "controls",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        5,
    ),
    accessor_magic(
        "loop",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        6,
    ),
    accessor_magic(
        "autoplay",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        8,
    ),
    accessor_magic(
        "defer",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        9,
    ),
    accessor_magic(
        "async",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        10,
    ),
    accessor_magic(
        "noValidate",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        11,
    ),
    accessor_magic(
        "isMap",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        12,
    ),
    accessor(
        "draggable",
        Some(reflect::strings::draggable_get),
        Some(reflect::strings::draggable_set),
    ),
    accessor_magic(
        "reversed",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        14,
    ),
    accessor_magic(
        "playsInline",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        15,
    ),
    accessor_magic(
        "inert",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        17,
    ),
    accessor_magic(
        "noModule",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        18,
    ),
    accessor_magic(
        "formNoValidate",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        19,
    ),
    accessor_magic(
        "required",
        Some(reflect::numeric::bool_get),
        Some(reflect::numeric::bool_set),
        20,
    ),
    accessor(
        "htmlFor",
        Some(reflect::strings::html_for_get),
        Some(reflect::strings::html_for_set),
    ),
    accessor(
        "control",
        Some(adapt::element_get_label_control),
        Some(elements::element::ignore),
    ),
    accessor(
        "popoverTargetElement",
        Some(adapt::element_get_popover_target_element),
        Some(adapt::element_set_popover_target_element),
    ),
    accessor(
        "tabIndex",
        Some(reflect::numeric::tab_index_get),
        Some(reflect::numeric::tab_index_set),
    ),
    accessor(
        "isConnected",
        Some(natives::element_get_is_connected),
        Some(elements::element::ignore),
    ),
    accessor(
        "baseURI",
        Some(node::owner::base_uri),
        Some(elements::element::ignore),
    ),
    accessor(
        "ownerDocument",
        Some(node::owner::owner_document),
        Some(elements::element::ignore),
    ),
    accessor(
        "namespaceURI",
        Some(adapt::element_get_namespace_uri),
        Some(elements::element::ignore),
    ),
    accessor(
        "shadowRoot",
        Some(adapt::element_get_shadow_root),
        Some(elements::element::ignore),
    ),
    accessor(
        "assignedSlot",
        Some(adapt::element_get_assigned_slot),
        Some(elements::element::ignore),
    ),
    method("attachShadow", 1, shadow::root::attach_shadow),
    accessor(
        "disabled",
        Some(elements::element::disabled),
        Some(elements::element::set_disabled),
    ),
    accessor(
        "checked",
        Some(adapt::element_get_checked),
        Some(adapt::element_set_checked),
    ),
    accessor(
        "label",
        Some(adapt::element_get_label_prop),
        Some(adapt::element_set_label_prop),
    ),
    accessor(
        "selectedIndex",
        Some(adapt::element_get_selected_index),
        Some(adapt::element_set_selected_index),
    ),
    accessor(
        "options",
        Some(adapt::element_get_options),
        Some(elements::element::ignore),
    ),
    accessor(
        "selectedOptions",
        Some(adapt::element_get_selected_options),
        Some(elements::element::ignore),
    ),
    accessor(
        "elements",
        Some(adapt::element_get_form_elements),
        Some(elements::element::ignore),
    ),
    accessor(
        "form",
        Some(elements::element::form),
        Some(elements::element::ignore),
    ),
];

pub(crate) static SRC_PROTO: [Prop; 1] = [accessor_magic(
    "src",
    Some(nsr_element_attr_getter),
    Some(nsr_element_attr_setter),
    3,
)];

pub(crate) static IMAGE_PROTO_FUNCS: [Prop; 6] = [
    accessor_magic(
        "src",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        3,
    ),
    accessor_magic(
        "srcset",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        29,
    ),
    accessor(
        "currentSrc",
        Some(elements::image::current_src),
        Some(elements::element::ignore),
    ),
    accessor(
        "naturalWidth",
        Some(elements::image::natural_width),
        Some(elements::element::ignore),
    ),
    accessor(
        "naturalHeight",
        Some(elements::image::natural_height),
        Some(elements::element::ignore),
    ),
    accessor(
        "complete",
        Some(elements::image::complete),
        Some(elements::element::ignore),
    ),
];

pub(crate) static SOURCE_PROTO: [Prop; 2] = [
    accessor_magic(
        "src",
        Some(nsr_element_attr_getter),
        Some(nsr_element_attr_setter),
        3,
    ),
    accessor_magic(
        "srcset",
        Some(nsr_element_attr_getter),
        Some(nsr_element_attr_setter),
        29,
    ),
];

pub(crate) static MEDIA_PROTO_FUNCS: [Prop; 21] = [
    method("play", 0, media::element::play),
    method("pause", 0, media::element::pause),
    method("load", 0, media::element::load),
    method("canPlayType", 1, media::support::can_play_type),
    method("fastSeek", 1, media::element::fast_seek),
    accessor_magic(
        "src",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        3,
    ),
    accessor(
        "currentSrc",
        Some(elements::image::current_src),
        Some(elements::element::ignore),
    ),
    accessor(
        "currentTime",
        Some(adapt::media_get_current_time),
        Some(adapt::media_set_current_time),
    ),
    accessor(
        "duration",
        Some(adapt::media_get_duration),
        Some(elements::element::ignore),
    ),
    accessor(
        "paused",
        Some(adapt::media_get_paused),
        Some(elements::element::ignore),
    ),
    accessor(
        "ended",
        Some(adapt::media_get_ended),
        Some(elements::element::ignore),
    ),
    accessor(
        "seeking",
        Some(adapt::media_get_seeking),
        Some(elements::element::ignore),
    ),
    accessor(
        "volume",
        Some(adapt::media_get_volume),
        Some(adapt::media_set_volume),
    ),
    accessor(
        "muted",
        Some(adapt::media_get_muted),
        Some(adapt::media_set_muted),
    ),
    accessor(
        "playbackRate",
        Some(adapt::media_get_playback_rate),
        Some(adapt::media_set_playback_rate),
    ),
    accessor(
        "defaultPlaybackRate",
        Some(adapt::media_get_default_playback_rate),
        Some(adapt::media_set_default_playback_rate),
    ),
    accessor(
        "readyState",
        Some(adapt::media_get_ready_state),
        Some(elements::element::ignore),
    ),
    accessor(
        "networkState",
        Some(adapt::media_get_network_state),
        Some(elements::element::ignore),
    ),
    accessor(
        "buffered",
        Some(adapt::media_get_buffered_ranges),
        Some(elements::element::ignore),
    ),
    accessor(
        "played",
        Some(adapt::media_get_played_ranges),
        Some(elements::element::ignore),
    ),
    accessor(
        "seekable",
        Some(adapt::media_get_seekable_ranges),
        Some(elements::element::ignore),
    ),
];

pub(crate) static VIDEO_PROTO_FUNCS: [Prop; 5] = [
    method(
        "getVideoPlaybackQuality",
        0,
        media::element::video_playback_quality,
    ),
    method(
        "requestVideoFrameCallback",
        1,
        lifecycle::frames::request_video_frame_callback,
    ),
    method(
        "cancelVideoFrameCallback",
        1,
        lifecycle::frames::cancel_animation_frame,
    ),
    accessor(
        "videoWidth",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
    accessor(
        "videoHeight",
        Some(elements::element::zero),
        Some(elements::element::ignore),
    ),
];

pub(crate) static IFRAME_PROTO_FUNCS: [Prop; 2] = [
    accessor_magic(
        "src",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        3,
    ),
    accessor_magic(
        "srcdoc",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        75,
    ),
];

pub(crate) static ANCHOR_PROTO_FUNCS: [Prop; 3] = [
    accessor_magic(
        "href",
        Some(natives::element_anchor_part_get),
        Some(natives::element_anchor_href_set),
        0,
    ),
    accessor_magic(
        "ping",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        90,
    ),
    accessor_magic(
        "download",
        Some(reflect::strings::attr_get),
        Some(reflect::strings::attr_set),
        89,
    ),
];

pub(crate) static HREF_PROTO_FUNCS: [Prop; 1] = [accessor_magic(
    "href",
    Some(natives::element_anchor_part_get),
    Some(natives::element_anchor_href_set),
    0,
)];

pub(crate) static NAMEDMAP_PROTO_FUNCS: [Prop; 8] = [
    method("getNamedItem", 1, attrs::named_map::get_named_item),
    method("getNamedItemNS", 2, attrs::named_map::get_named_item_ns),
    method("setNamedItem", 1, attrs::named_map::set_named_item),
    method("setNamedItemNS", 1, attrs::named_map::set_named_item),
    method("removeNamedItem", 1, attrs::named_map::remove_named_item),
    method(
        "removeNamedItemNS",
        2,
        attrs::named_map::remove_named_item_ns,
    ),
    method("item", 1, attrs::named_map::item),
    accessor(
        "length",
        Some(adapt::namedmap_get_length),
        Some(elements::element::ignore),
    ),
];

pub(crate) static DOM_IMPLEMENTATION_PROTO_FUNCS: [Prop; 4] = [
    method("hasFeature", 2, natives::event_true),
    method(
        "createHTMLDocument",
        1,
        dom_factory::implementation::create_html_document,
    ),
    method(
        "createDocument",
        3,
        dom_factory::implementation::create_document,
    ),
    method(
        "createDocumentType",
        3,
        dom_factory::implementation::create_document_type,
    ),
];

pub(crate) static DOCUMENT_FUNCS: [Prop; 87] = [
    accessor_magic(
        "fgColor",
        Some(natives::document_get_color),
        Some(natives::document_set_color),
        0,
    ),
    accessor_magic(
        "bgColor",
        Some(natives::document_get_color),
        Some(natives::document_set_color),
        1,
    ),
    accessor_magic(
        "linkColor",
        Some(natives::document_get_color),
        Some(natives::document_set_color),
        2,
    ),
    accessor_magic(
        "vlinkColor",
        Some(natives::document_get_color),
        Some(natives::document_set_color),
        3,
    ),
    accessor_magic(
        "alinkColor",
        Some(natives::document_get_color),
        Some(natives::document_set_color),
        4,
    ),
    method(
        "getElementById",
        1,
        collections::query::document_get_element_by_id,
    ),
    method("createElement", 1, dom_factory::factories::create_element),
    method(
        "createElementNS",
        2,
        dom_factory::factories::create_element_ns,
    ),
    method(
        "createTextNode",
        1,
        dom_factory::factories::create_text_node,
    ),
    method("createComment", 1, dom_factory::factories::create_comment),
    method(
        "createCDATASection",
        1,
        dom_factory::factories::create_cdata_section,
    ),
    method(
        "createProcessingInstruction",
        2,
        dom_factory::factories::create_processing_instruction,
    ),
    method(
        "createAttribute",
        1,
        dom_factory::factories::create_attribute,
    ),
    method(
        "createAttributeNS",
        2,
        dom_factory::factories::create_attribute_ns,
    ),
    method("createEvent", 1, dom_factory::factories::create_event),
    method(
        "createDocumentFragment",
        0,
        dom_factory::factories::create_document_fragment,
    ),
    method(
        "getElementsByTagName",
        1,
        collections::live::document_by_tag,
    ),
    method(
        "getElementsByTagNameNS",
        2,
        collections::live::document_by_tag_ns,
    ),
    method(
        "getElementsByClassName",
        1,
        collections::live::document_by_class,
    ),
    method(
        "querySelector",
        1,
        collections::query::document_query_selector,
    ),
    method(
        "querySelectorAll",
        1,
        collections::query::document_query_selector_all,
    ),
    accessor(
        "documentElement",
        Some(adapt::document_get_document_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "body",
        Some(adapt::document_get_body),
        Some(adapt::document_set_body),
    ),
    accessor(
        "head",
        Some(adapt::document_get_head),
        Some(elements::element::ignore),
    ),
    accessor(
        "activeElement",
        Some(adapt::document_get_active_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "forms",
        Some(natives::document_get_forms),
        Some(elements::element::ignore),
    ),
    accessor(
        "images",
        Some(natives::document_get_images),
        Some(elements::element::ignore),
    ),
    accessor(
        "links",
        Some(natives::document_get_links),
        Some(elements::element::ignore),
    ),
    accessor(
        "scripts",
        Some(adapt::document_get_scripts),
        Some(elements::element::ignore),
    ),
    accessor(
        "embeds",
        Some(adapt::document_get_embeds),
        Some(elements::element::ignore),
    ),
    accessor(
        "plugins",
        Some(adapt::document_get_plugins),
        Some(elements::element::ignore),
    ),
    accessor(
        "designMode",
        Some(adapt::document_get_design_mode),
        Some(elements::element::ignore),
    ),
    accessor(
        "lastModified",
        Some(adapt::document_get_last_modified),
        Some(elements::element::ignore),
    ),
    accessor(
        "all",
        Some(natives::document_get_all),
        Some(elements::element::ignore),
    ),
    accessor(
        "anchors",
        Some(adapt::document_get_anchors),
        Some(elements::element::ignore),
    ),
    accessor(
        "applets",
        Some(adapt::document_get_applets),
        Some(elements::element::ignore),
    ),
    accessor(
        "fonts",
        Some(natives::document_get_fonts),
        Some(elements::element::ignore),
    ),
    accessor(
        "implementation",
        Some(natives::document_implementation),
        None,
    ),
    method("write", 1, document::write::write),
    method("writeln", 1, document::write::writeln),
    method("open", 0, document::write::open),
    method("close", 0, document::write::close),
    method("execCommand", 3, services::commands::exec_command),
    method("hasFocus", 0, focus::focus::has_focus),
    method("elementFromPoint", 2, geometry::hit::element_from_point),
    method("elementsFromPoint", 2, geometry::hit::elements_from_point),
    method("createRange", 0, traversal::range::create_range),
    method("createTreeWalker", 3, traversal::walker::create),
    method("createNodeIterator", 3, traversal::iterator::create),
    method("getSelection", 0, traversal::selection::get_selection),
    method("adoptNode", 1, dom_factory::adopt::adopt_node),
    method("importNode", 2, dom_factory::adopt::import_node),
    method("exitFullscreen", 0, focus::fullscreen::exit_fullscreen),
    method(
        "webkitExitFullscreen",
        0,
        focus::fullscreen::exit_fullscreen,
    ),
    method(
        "webkitCancelFullScreen",
        0,
        focus::fullscreen::exit_fullscreen,
    ),
    method("mozCancelFullScreen", 0, focus::fullscreen::exit_fullscreen),
    method("msExitFullscreen", 0, focus::fullscreen::exit_fullscreen),
    method("exitPointerLock", 0, focus::fullscreen::exit_pointer_lock),
    method(
        "queryCommandSupported",
        1,
        services::commands::query_command_supported,
    ),
    method(
        "queryCommandEnabled",
        1,
        services::commands::query_command_enabled,
    ),
    method("queryCommandState", 1, natives::event_false),
    method("queryCommandValue", 1, bind::natives::noop),
    accessor(
        "currentScript",
        Some(adapt::document_get_current_script),
        Some(elements::element::ignore),
    ),
    accessor(
        "rootElement",
        Some(adapt::document_get_document_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "fullscreenElement",
        Some(adapt::document_get_fullscreen_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "webkitFullscreenElement",
        Some(adapt::document_get_fullscreen_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "mozFullScreenElement",
        Some(adapt::document_get_fullscreen_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "msFullscreenElement",
        Some(adapt::document_get_fullscreen_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "webkitIsFullScreen",
        Some(adapt::document_get_is_fullscreen),
        Some(elements::element::ignore),
    ),
    accessor(
        "mozFullScreen",
        Some(adapt::document_get_is_fullscreen),
        Some(elements::element::ignore),
    ),
    accessor(
        "pointerLockElement",
        Some(adapt::document_get_pointer_lock_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "fullscreenEnabled",
        Some(adapt::document_get_fullscreen_enabled),
        Some(elements::element::ignore),
    ),
    accessor(
        "webkitFullscreenEnabled",
        Some(adapt::document_get_fullscreen_enabled),
        Some(elements::element::ignore),
    ),
    accessor(
        "scrollingElement",
        Some(adapt::document_get_scrolling_element),
        Some(elements::element::ignore),
    ),
    method("addEventListener", 2, dispatch::listeners::document_add),
    method(
        "removeEventListener",
        2,
        dispatch::listeners::document_remove,
    ),
    method("dispatchEvent", 1, dispatch::target::document_dispatch),
    method("getElementsByName", 1, collections::live::document_by_name),
    method("getItems", 1, html::microdata::get_items),
    accessor(
        "title",
        Some(adapt::document_get_title),
        Some(adapt::document_set_title),
    ),
    accessor(
        "cookie",
        Some(adapt::document_get_cookie),
        Some(adapt::document_set_cookie),
    ),
    accessor(
        "referrer",
        Some(adapt::document_get_referrer),
        Some(elements::element::ignore),
    ),
    accessor(
        "readyState",
        Some(adapt::document_get_ready_state),
        Some(elements::element::ignore),
    ),
    accessor(
        "hidden",
        Some(adapt::document_get_hidden),
        Some(elements::element::ignore),
    ),
    accessor(
        "visibilityState",
        Some(adapt::document_get_visibility_state),
        Some(elements::element::ignore),
    ),
    accessor(
        "compatMode",
        Some(adapt::document_get_compat_mode),
        Some(elements::element::ignore),
    ),
    accessor(
        "dir",
        Some(adapt::document_get_dir),
        Some(adapt::document_set_dir),
    ),
];

pub(crate) static DOCUMENT_PROTO_ACCESSORS: [Prop; 9] = [
    accessor(
        "documentElement",
        Some(adapt::document_get_document_element),
        Some(elements::element::ignore),
    ),
    accessor(
        "body",
        Some(adapt::document_get_body),
        Some(adapt::document_set_body),
    ),
    accessor(
        "head",
        Some(adapt::document_get_head),
        Some(elements::element::ignore),
    ),
    accessor(
        "currentScript",
        Some(adapt::document_get_current_script),
        Some(elements::element::ignore),
    ),
    accessor(
        "defaultView",
        Some(natives::document_get_default_view),
        Some(elements::element::ignore),
    ),
    accessor(
        "implementation",
        Some(natives::document_implementation),
        None,
    ),
    accessor(
        "cookie",
        Some(adapt::document_get_cookie),
        Some(adapt::document_set_cookie),
    ),
    accessor(
        "readyState",
        Some(adapt::document_get_ready_state),
        Some(elements::element::ignore),
    ),
    accessor(
        "xmlVersion",
        Some(adapt::document_get_xml_version),
        Some(elements::element::ignore),
    ),
];

pub(crate) static DOCUMENT_PROTO_METHODS: [Prop; 5] = [
    method("adoptNode", 1, dom_factory::adopt::adopt_node),
    method(
        "createDocumentFragment",
        0,
        dom_factory::factories::create_document_fragment,
    ),
    method("createElement", 1, dom_factory::factories::create_element),
    method(
        "createElementNS",
        2,
        dom_factory::factories::create_element_ns,
    ),
    method("createTreeWalker", 3, traversal::walker::create),
];

pub(crate) static TRACK_ACCESSORS: [Prop; 1] = [accessor_magic(
    "default",
    Some(reflect::numeric::plain_bool_get),
    Some(reflect::numeric::plain_bool_set),
    3,
)];

pub(crate) static DATASET_MEMBERS: [Prop; 1] = [accessor(
    "dataset",
    Some(natives::element_get_dataset),
    None,
)];

pub(crate) static HTML_ELEMENT_MEMBERS: [Prop; 2] = [
    accessor("dataset", Some(natives::element_get_dataset), None),
    accessor(
        "popover",
        Some(adapt::element_get_popover),
        Some(adapt::element_set_popover),
    ),
];

pub(crate) static ASSIGNED_SLOT_MEMBERS: [Prop; 1] = [accessor(
    "assignedSlot",
    Some(adapt::element_get_assigned_slot),
    Some(elements::element::ignore),
)];

pub(crate) static DIALOG_MEMBERS: [Prop; 1] = [accessor(
    "closedBy",
    Some(adapt::dialog_get_closed_by),
    Some(adapt::dialog_set_closed_by),
)];

pub(crate) static BUTTON_MEMBERS: [Prop; 2] = [
    accessor(
        "command",
        Some(adapt::button_get_command),
        Some(adapt::button_set_command),
    ),
    accessor(
        "commandForElement",
        Some(adapt::button_get_command_for_element),
        Some(adapt::button_set_command_for_element),
    ),
];

pub(crate) static TEXT_PROPERTY_MEMBERS: [Prop; 1] = [accessor(
    "text",
    Some(html::text::text_get),
    Some(html::text::text_set),
)];

pub(crate) static DATA_PROPERTY_MEMBERS: [Prop; 1] = [accessor(
    "data",
    Some(elements::node_value::data),
    Some(elements::node_value::set_data),
)];

pub(crate) static VALUE_PROPERTY_MEMBERS: [Prop; 1] = [accessor(
    "value",
    Some(adapt::element_get_value_prop),
    Some(adapt::element_set_value_prop),
)];

pub(crate) static OPTIONS_MEMBERS: [Prop; 2] = [
    method_enumerable("item", 1, forms::select::item, false),
    method_enumerable("namedItem", 1, forms::select::named_item, false),
];

pub(crate) static ELEMENT_CLASS_FUNCTIONS: [(&str, NativeFn); 3] = [
    ("matches", collections::query::matches),
    ("webkitMatchesSelector", collections::query::matches),
    ("closest", collections::query::closest),
];

pub(crate) static FRAGMENT_FUNCTIONS: [(&str, NativeFn); 2] = [
    ("querySelector", collections::query::element_query_selector),
    (
        "querySelectorAll",
        collections::query::element_query_selector_all,
    ),
];
