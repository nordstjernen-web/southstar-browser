//! Southstar — the QuickJS function tables of the DOM prototypes: element, media, anchor, document and the other interfaces' native methods and accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{
    FunctionListEntry, JSContext, JSValue, cfunc, getset, getset_magic,
};
use southstar_js_engine::{Prop, Scope, Value, accessor, accessor_magic, method};

use super::natives::{
    ns_document_get_all, ns_document_get_forms, ns_document_get_images, ns_document_get_links,
    ns_element_get_accessKeyLabel, ns_element_getNumberOfChars, ns_element_set_type,
    ns_event_false, ns_event_true,
};

unsafe extern "C" {
    pub(crate) fn ns_dialog_get_returnValue(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_dialog_set_returnValue(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_addEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_adopt_node(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_close(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createAttribute(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createAttributeNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createCDATASection(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createComment(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createDocumentFragment(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createElement(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createElementNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createEvent(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createProcessingInstruction(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_createTextNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_create_node_iterator(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_create_range(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_create_tree_walker(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_dispatchEvent(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_element_from_point(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_elements_from_point(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_exitPointerLock(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_exit_fullscreen(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_getElementById(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_getElementsByClassName(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_getElementsByName(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_getElementsByTagName(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_getElementsByTagNameNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_getItems(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_get_activeElement(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_anchors(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_applets(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_body(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_color(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_document_get_compatMode(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_cookie(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_currentScript(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_defaultView(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_designMode(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_dir(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_documentElement(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_embeds(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_fonts(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_fullscreen_element(ctx: *mut JSContext, this: JSValue)
    -> JSValue;
    pub(crate) fn ns_document_get_fullscreen_enabled(ctx: *mut JSContext, this: JSValue)
    -> JSValue;
    pub(crate) fn ns_document_get_head(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_hidden(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_is_fullscreen(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_lastModified(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_plugins(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_pointerLockElement(ctx: *mut JSContext, this: JSValue)
    -> JSValue;
    pub(crate) fn ns_document_get_readyState(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_referrer(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_scripts(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_scrollingElement(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_title(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_visibilityState(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_get_xmlVersion(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_has_focus(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_implementation(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_document_import_node(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_open(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_querySelector(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_querySelectorAll(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_removeEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_set_body(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_set_color(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_document_set_cookie(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_set_dir(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_set_title(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_write(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_document_writeln(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_addEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_after(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_anchor_href_set(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_anchor_part_get(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_animate(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_append(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_appendChild(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_append_data(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_aria_string_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_aria_string_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_assignedElements(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_assignedNodes(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_attachInternals(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_attachShadow(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_attr_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_attr_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_attr_setter_sandbox(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_attr_setter_sizes(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_before(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_blur(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_bool_attr_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_bool_attr_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_boolattr_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_boolattr_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_checkVisibility(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_check_validity(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_click(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_cloneNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_close(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_closest(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_compareDocumentPosition(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_contains(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_createSVGMatrix(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_createSVGPoint(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_createSVGRect(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_createSVGTransform(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_delete_data(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_dimension_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_dimension_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_dispatchEvent(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_enum_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_enum_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_focus(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_form_requestSubmit(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_form_reset(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_form_submit(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getAttribute(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getAttributeNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getAttributeNames(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getAttributeNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getAttributeNodeNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getBBox(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getBoundingClientRect(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getCTM(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getClientRects(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getContext(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getElementById(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getElementsByClassName(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getElementsByTagName(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getElementsByTagNameNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getHTML(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getPointAtLength(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getRootNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getSVGDocument(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getScreenCTM(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_getTotalLength(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_get_assignedSlot(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_attributes(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_autocapitalize(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_autocomplete(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_baseURI(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_checked(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_childElementCount(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_childNodes(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_children(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_classList(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_className(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_clientHeight(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_clientLeft(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_clientTop(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_clientWidth(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_contentDocument(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_contentEditable(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_contentWindow(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_dataset(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_default_checked(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_default_selected(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_default_value(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_dir(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_disabled(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_draggable(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_empty_array_prop(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_firstChild(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_firstElementChild(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_form(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_form_elements(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_form_enctype(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_hidden(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_htmlFor(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_id(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_indeterminate(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_innerHTML(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_innerText(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_internals(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_isConnected(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_isContentEditable(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_itemId(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_itemProp(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_itemRef(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_itemScope(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_itemType(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_itemValue(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_label_control(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_label_prop(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_labels(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_lastChild(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_lastElementChild(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_list_ref(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_localName(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_namespaceURI(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_nextElementSibling(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_nextSibling(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_nodeName(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_nodeType(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_nodeValue(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_null(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_offsetHeight(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_offsetLeft(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_offsetParent(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_offsetTop(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_offsetWidth(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_option_index(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_options(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_outerHTML(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_ownerDocument(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_ownerSVGElement(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_parentElement(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_parentNode(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_popoverTargetElement(
        ctx: *mut JSContext,
        this: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_get_prefix(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_previousElementSibling(
        ctx: *mut JSContext,
        this: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_get_previousSibling(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_progress_position(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_properties(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_relList(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_sandbox_list(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_scrollHeight(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_scrollLeft(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_scrollTop(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_scrollWidth(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_selected(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_selectedIndex(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_selectedOptions(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_selection_dir(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_selection_end(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_selection_start(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_shadowRoot(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_sizes_list(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_spellcheck(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_style(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_tabIndex(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_tagName(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_textContent(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_text_length(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_translate(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_type(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_validation_message(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_validity(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_value_as_date(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_value_as_number(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_wholeText(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_will_validate(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_get_zero_int(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_hasAttribute(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_hasAttributeNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_hasAttributes(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_hasChildNodes(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_hasPointerCapture(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_hidePopover(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_img_complete(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_img_current_src(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_img_natural_height(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_img_natural_width(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_insertAdjacentElement(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_insertAdjacentHTML(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_insertAdjacentText(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_insertBefore(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_insert_data(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_int_attr_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_int_attr_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_isDefaultNamespace(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_isEqualNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_isSameNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_list_set(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_lookupNamespaceURI(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_lookupPrefix(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_matches(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_moveBefore(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_noop_set(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_normalize(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_prepend(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_querySelector(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_querySelectorAll(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_range_number_getter(
        ctx: *mut JSContext,
        this: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_range_number_setter(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_element_releasePointerCapture(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_removeAttribute(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_removeAttributeNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_removeAttributeNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_removeChild(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_removeEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_remove_self(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_replaceChild(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_replaceChildren(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_replaceWith(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_replace_data(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_requestClose(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_requestPointerLock(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_request_fullscreen(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_scrollIntoView(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_scroll_by(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_scroll_to(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_setAttribute(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_setAttributeNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_setAttributeNode(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_setCustomValidity(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_setHTMLUnsafe(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_setPointerCapture(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_autocapitalize(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_autocomplete(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_checked(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_classList(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_className(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_content(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_contentEditable(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_default_checked(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_default_selected(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_default_value(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_dir(ctx: *mut JSContext, this: JSValue, value: JSValue)
    -> JSValue;
    pub(crate) fn ns_element_set_disabled(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_draggable(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_form_enctype(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_hidden(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_htmlFor(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_id(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;
    pub(crate) fn ns_element_set_indeterminate(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_innerHTML(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_innerText(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_itemId(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_itemScope(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_itemValue(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_label_prop(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_nodeValue(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_outerHTML(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_outerText(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_popoverTargetElement(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_relList(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_rows(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_scrollLeft(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_scrollTop(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_selected(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_selectedIndex(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_selection_dir(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_selection_end(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_selection_start(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_spellcheck(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_style(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_tabIndex(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_textContent(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_translate(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_value_as_date(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_set_value_as_number(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_show(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_showModal(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_showPopover(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_show_picker(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_split_text(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_substring_data(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_table_caption(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_table_rows(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_table_tbodies(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_table_tfoot(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_table_thead(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_template_content(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_toBlob(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_toDataURL(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_toggleAttribute(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_togglePopover(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_element_tr_cells(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_element_url_part_set(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
        magic: c_int,
    ) -> JSValue;
    pub(crate) fn ns_event_empty_array(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_event_noop(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_impl_create_document(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_impl_create_document_type(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_impl_create_html_document(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_input_get_files(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_input_select(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_input_setRangeText(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_input_setSelectionRange(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_input_stepDown(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_input_stepUp(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_addTextTrack(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_canPlayType(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_fast_seek(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_get_buffered_ranges(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_current_time(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_defaultPlaybackRate(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_duration(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_ended(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_error(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_muted(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_networkState(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_paused(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_playbackRate(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_played_ranges(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_readyState(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_seekable_ranges(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_seeking(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_srcObject(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_textTracks(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_get_video_playback_quality(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_get_volume(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_media_load(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_pause(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_play(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_request_video_frame_callback(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_set_current_time(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_set_defaultPlaybackRate(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_set_media_keys(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_set_muted(ctx: *mut JSContext, this: JSValue, value: JSValue)
    -> JSValue;
    pub(crate) fn ns_media_set_playbackRate(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_set_srcObject(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_media_set_volume(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    pub(crate) fn ns_namedmap_getNamedItem(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_namedmap_getNamedItemNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_namedmap_get_length(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_namedmap_item(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_namedmap_removeNamedItem(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_namedmap_removeNamedItemNS(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_namedmap_setNamedItem(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_returns_resolved_undefined(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_select_add(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_services_clear_timer(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_services_exec_command(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_services_query_command_enabled(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_services_query_command_supported(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_svg_beginElement(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_svg_setCurrentTime(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_createCaption(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_createTBody(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_createTFoot(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_createTHead(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_deleteCaption(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_deleteRow(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_deleteTFoot(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_deleteTHead(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_table_insertRow(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_text_control_get_text_length(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_text_replaceWholeText(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_tr_deleteCell(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_tr_insertCell(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_validity_get_valid(ctx: *mut JSContext, this: JSValue) -> JSValue;
    pub(crate) fn ns_window_cancelAnimationFrame(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    pub(crate) fn ns_window_get_selection(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
}

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

pub(crate) static VALIDITY_PROTO_FUNCS: [FunctionListEntry; 1] =
    [getset(c"valid", Some(ns_validity_get_valid), None)];

pub(crate) static ELEMENT_PROTO_FUNCS: [FunctionListEntry; 497] = [
    cfunc(c"getNumberOfChars", 0, ns_element_getNumberOfChars),
    cfunc(c"getSVGDocument", 0, ns_element_getSVGDocument),
    getset(
        c"contentDocument",
        Some(ns_element_get_contentDocument),
        Some(ns_element_noop_set),
    ),
    getset(
        c"contentWindow",
        Some(ns_element_get_contentWindow),
        Some(ns_element_noop_set),
    ),
    getset(
        c"tagName",
        Some(ns_element_get_tagName),
        Some(ns_element_noop_set),
    ),
    getset(
        c"accessKeyLabel",
        Some(ns_element_get_accessKeyLabel),
        Some(ns_element_noop_set),
    ),
    getset(
        c"localName",
        Some(ns_element_get_localName),
        Some(ns_element_noop_set),
    ),
    getset(
        c"prefix",
        Some(ns_element_get_prefix),
        Some(ns_element_noop_set),
    ),
    getset(
        c"textContent",
        Some(ns_element_get_textContent),
        Some(ns_element_set_textContent),
    ),
    getset(
        c"innerText",
        Some(ns_element_get_innerText),
        Some(ns_element_set_innerText),
    ),
    getset(
        c"outerText",
        Some(ns_element_get_innerText),
        Some(ns_element_set_outerText),
    ),
    getset(c"id", Some(ns_element_get_id), Some(ns_element_set_id)),
    getset(
        c"className",
        Some(ns_element_get_className),
        Some(ns_element_set_className),
    ),
    getset(
        c"innerHTML",
        Some(ns_element_get_innerHTML),
        Some(ns_element_set_innerHTML),
    ),
    getset(
        c"outerHTML",
        Some(ns_element_get_outerHTML),
        Some(ns_element_set_outerHTML),
    ),
    getset(
        c"style",
        Some(ns_element_get_style),
        Some(ns_element_set_style),
    ),
    getset(
        c"classList",
        Some(ns_element_get_classList),
        Some(ns_element_set_classList),
    ),
    getset(
        c"relList",
        Some(ns_element_get_relList),
        Some(ns_element_set_relList),
    ),
    getset(
        c"itemScope",
        Some(ns_element_get_itemScope),
        Some(ns_element_set_itemScope),
    ),
    getset(
        c"itemId",
        Some(ns_element_get_itemId),
        Some(ns_element_set_itemId),
    ),
    getset(
        c"itemType",
        Some(ns_element_get_itemType),
        Some(ns_element_noop_set),
    ),
    getset(
        c"itemProp",
        Some(ns_element_get_itemProp),
        Some(ns_element_noop_set),
    ),
    getset(
        c"itemRef",
        Some(ns_element_get_itemRef),
        Some(ns_element_noop_set),
    ),
    getset(
        c"itemValue",
        Some(ns_element_get_itemValue),
        Some(ns_element_set_itemValue),
    ),
    getset(
        c"properties",
        Some(ns_element_get_properties),
        Some(ns_element_noop_set),
    ),
    getset(
        c"parentElement",
        Some(ns_element_get_parentElement),
        Some(ns_element_noop_set),
    ),
    getset(
        c"parentNode",
        Some(ns_element_get_parentNode),
        Some(ns_element_noop_set),
    ),
    getset(
        c"firstElementChild",
        Some(ns_element_get_firstElementChild),
        Some(ns_element_noop_set),
    ),
    getset(
        c"lastElementChild",
        Some(ns_element_get_lastElementChild),
        Some(ns_element_noop_set),
    ),
    getset(
        c"nextElementSibling",
        Some(ns_element_get_nextElementSibling),
        Some(ns_element_noop_set),
    ),
    getset(
        c"previousElementSibling",
        Some(ns_element_get_previousElementSibling),
        Some(ns_element_noop_set),
    ),
    getset(
        c"firstChild",
        Some(ns_element_get_firstChild),
        Some(ns_element_noop_set),
    ),
    getset(
        c"lastChild",
        Some(ns_element_get_lastChild),
        Some(ns_element_noop_set),
    ),
    getset(
        c"nextSibling",
        Some(ns_element_get_nextSibling),
        Some(ns_element_noop_set),
    ),
    getset(
        c"previousSibling",
        Some(ns_element_get_previousSibling),
        Some(ns_element_noop_set),
    ),
    getset(
        c"childNodes",
        Some(ns_element_get_childNodes),
        Some(ns_element_noop_set),
    ),
    getset(
        c"childElementCount",
        Some(ns_element_get_childElementCount),
        Some(ns_element_noop_set),
    ),
    getset(
        c"children",
        Some(ns_element_get_children),
        Some(ns_element_noop_set),
    ),
    cfunc(c"getAttribute", 1, ns_element_getAttribute),
    cfunc(c"hasAttribute", 1, ns_element_hasAttribute),
    cfunc(c"setAttribute", 2, ns_element_setAttribute),
    cfunc(c"removeAttribute", 1, ns_element_removeAttribute),
    cfunc(c"showPopover", 0, ns_element_showPopover),
    cfunc(c"hidePopover", 0, ns_element_hidePopover),
    cfunc(c"togglePopover", 0, ns_element_togglePopover),
    cfunc(c"toggleAttribute", 1, ns_element_toggleAttribute),
    cfunc(c"getAttributeNS", 2, ns_element_getAttributeNS),
    cfunc(c"hasAttributeNS", 2, ns_element_hasAttributeNS),
    cfunc(c"setAttributeNS", 3, ns_element_setAttributeNS),
    cfunc(c"removeAttributeNS", 2, ns_element_removeAttributeNS),
    cfunc(c"requestFullscreen", 0, ns_element_request_fullscreen),
    cfunc(c"webkitRequestFullscreen", 0, ns_element_request_fullscreen),
    cfunc(c"webkitRequestFullScreen", 0, ns_element_request_fullscreen),
    cfunc(c"mozRequestFullScreen", 0, ns_element_request_fullscreen),
    cfunc(c"msRequestFullscreen", 0, ns_element_request_fullscreen),
    cfunc(c"getAnimations", 0, ns_event_empty_array),
    cfunc(c"animate", 2, ns_element_animate),
    cfunc(c"getRootNode", 1, ns_element_getRootNode),
    cfunc(c"isEqualNode", 1, ns_element_isEqualNode),
    cfunc(c"isSameNode", 1, ns_element_isSameNode),
    cfunc(
        c"compareDocumentPosition",
        1,
        ns_element_compareDocumentPosition,
    ),
    cfunc(c"lookupPrefix", 1, ns_element_lookupPrefix),
    cfunc(c"lookupNamespaceURI", 1, ns_element_lookupNamespaceURI),
    cfunc(c"isDefaultNamespace", 1, ns_element_isDefaultNamespace),
    cfunc(c"getClientRects", 0, ns_element_getClientRects),
    cfunc(c"scrollBy", 2, ns_element_scroll_by),
    cfunc(c"scrollTo", 2, ns_element_scroll_to),
    cfunc(c"scroll", 2, ns_element_scroll_to),
    cfunc(c"scrollIntoViewIfNeeded", 1, ns_element_scrollIntoView),
    cfunc(c"requestPointerLock", 0, ns_element_requestPointerLock),
    cfunc(c"releasePointerLock", 0, ns_event_noop),
    cfunc(c"releaseCapture", 0, ns_event_noop),
    cfunc(c"setCapture", 0, ns_event_noop),
    cfunc(c"associateForm", 1, ns_event_noop),
    cfunc(c"setMessage", 2, ns_event_noop),
    cfunc(c"syncInputValidity", 1, ns_event_noop),
    cfunc(c"_connect", 0, ns_event_noop),
    cfunc(c"_disconnect", 0, ns_event_noop),
    getset(
        c"length",
        Some(ns_element_get_text_length),
        Some(ns_element_noop_set),
    ),
    cfunc(c"substringData", 2, ns_element_substring_data),
    cfunc(c"appendData", 1, ns_element_append_data),
    cfunc(c"deleteData", 2, ns_element_delete_data),
    cfunc(c"insertData", 2, ns_element_insert_data),
    cfunc(c"replaceData", 3, ns_element_replace_data),
    cfunc(c"splitText", 1, ns_element_split_text),
    cfunc(c"select", 0, ns_input_select),
    cfunc(c"setSelectionRange", 3, ns_input_setSelectionRange),
    cfunc(c"setRangeText", 1, ns_input_setRangeText),
    cfunc(c"stepUp", 0, ns_input_stepUp),
    cfunc(c"stepDown", 0, ns_input_stepDown),
    cfunc(c"showPicker", 0, ns_element_show_picker),
    cfunc(c"play", 0, ns_media_play),
    cfunc(c"pause", 0, ns_media_pause),
    cfunc(c"load", 0, ns_media_load),
    cfunc(c"canPlayType", 1, ns_media_canPlayType),
    cfunc(c"fastSeek", 1, ns_media_fast_seek),
    cfunc(c"addTextTrack", 3, ns_media_addTextTrack),
    cfunc(c"setMediaKeys", 1, ns_media_set_media_keys),
    cfunc(
        c"getVideoPlaybackQuality",
        0,
        ns_media_get_video_playback_quality,
    ),
    cfunc(
        c"requestVideoFrameCallback",
        1,
        ns_media_request_video_frame_callback,
    ),
    cfunc(
        c"cancelVideoFrameCallback",
        1,
        ns_window_cancelAnimationFrame,
    ),
    getset(
        c"validity",
        Some(ns_element_get_validity),
        Some(ns_element_noop_set),
    ),
    getset(
        c"validationMessage",
        Some(ns_element_get_validation_message),
        Some(ns_element_noop_set),
    ),
    getset(
        c"willValidate",
        Some(ns_element_get_will_validate),
        Some(ns_element_noop_set),
    ),
    getset(
        c"labels",
        Some(ns_element_get_labels),
        Some(ns_element_noop_set),
    ),
    getset(
        c"files",
        Some(ns_input_get_files),
        Some(ns_element_noop_set),
    ),
    getset(
        c"indeterminate",
        Some(ns_element_get_indeterminate),
        Some(ns_element_set_indeterminate),
    ),
    getset(
        c"selectionStart",
        Some(ns_element_get_selection_start),
        Some(ns_element_set_selection_start),
    ),
    getset(
        c"selectionEnd",
        Some(ns_element_get_selection_end),
        Some(ns_element_set_selection_end),
    ),
    getset(
        c"selectionDirection",
        Some(ns_element_get_selection_dir),
        Some(ns_element_set_selection_dir),
    ),
    getset(
        c"textLength",
        Some(ns_text_control_get_text_length),
        Some(ns_element_noop_set),
    ),
    getset(
        c"defaultValue",
        Some(ns_element_get_default_value),
        Some(ns_element_set_default_value),
    ),
    getset(
        c"defaultChecked",
        Some(ns_element_get_default_checked),
        Some(ns_element_set_default_checked),
    ),
    getset(
        c"defaultSelected",
        Some(ns_element_get_default_selected),
        Some(ns_element_set_default_selected),
    ),
    getset(
        c"currentTime",
        Some(ns_media_get_current_time),
        Some(ns_media_set_current_time),
    ),
    getset(
        c"duration",
        Some(ns_media_get_duration),
        Some(ns_element_noop_set),
    ),
    getset(
        c"paused",
        Some(ns_media_get_paused),
        Some(ns_element_noop_set),
    ),
    getset(
        c"ended",
        Some(ns_media_get_ended),
        Some(ns_element_noop_set),
    ),
    getset(
        c"seeking",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
    getset(
        c"volume",
        Some(ns_media_get_volume),
        Some(ns_media_set_volume),
    ),
    getset(
        c"playbackRate",
        Some(ns_media_get_playbackRate),
        Some(ns_media_set_playbackRate),
    ),
    getset(
        c"defaultPlaybackRate",
        Some(ns_media_get_defaultPlaybackRate),
        Some(ns_media_set_defaultPlaybackRate),
    ),
    getset(
        c"error",
        Some(ns_media_get_error),
        Some(ns_element_noop_set),
    ),
    getset(c"muted", Some(ns_media_get_muted), Some(ns_media_set_muted)),
    getset(
        c"readyState",
        Some(ns_media_get_readyState),
        Some(ns_element_noop_set),
    ),
    getset(
        c"networkState",
        Some(ns_media_get_networkState),
        Some(ns_element_noop_set),
    ),
    getset(
        c"seekable",
        Some(ns_media_get_seekable_ranges),
        Some(ns_element_noop_set),
    ),
    getset(
        c"buffered",
        Some(ns_media_get_buffered_ranges),
        Some(ns_element_noop_set),
    ),
    getset(
        c"played",
        Some(ns_media_get_played_ranges),
        Some(ns_element_noop_set),
    ),
    getset(
        c"textTracks",
        Some(ns_media_get_textTracks),
        Some(ns_element_noop_set),
    ),
    getset(
        c"videoTracks",
        Some(ns_element_get_empty_array_prop),
        Some(ns_element_noop_set),
    ),
    getset(
        c"audioTracks",
        Some(ns_element_get_empty_array_prop),
        Some(ns_element_noop_set),
    ),
    getset(
        c"valueAsNumber",
        Some(ns_element_get_value_as_number),
        Some(ns_element_set_value_as_number),
    ),
    getset(
        c"valueAsDate",
        Some(ns_element_get_value_as_date),
        Some(ns_element_set_value_as_date),
    ),
    getset(
        c"position",
        Some(ns_element_get_progress_position),
        Some(ns_element_noop_set),
    ),
    getset(
        c"encoding",
        Some(ns_element_get_form_enctype),
        Some(ns_element_set_form_enctype),
    ),
    getset(
        c"isContentEditable",
        Some(ns_element_get_isContentEditable),
        Some(ns_element_noop_set),
    ),
    getset(
        c"translate",
        Some(ns_element_get_translate),
        Some(ns_element_set_translate),
    ),
    getset(
        c"offsetParent",
        Some(ns_element_get_offsetParent),
        Some(ns_element_noop_set),
    ),
    getset(
        c"videoWidth",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
    getset(
        c"videoHeight",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
    getset(
        c"srcObject",
        Some(ns_media_get_srcObject),
        Some(ns_media_set_srcObject),
    ),
    getset(
        c"clientInformation",
        Some(ns_element_get_null),
        Some(ns_element_noop_set),
    ),
    cfunc(c"decode", 0, ns_returns_resolved_undefined),
    cfunc(c"toBlob", 1, ns_element_toBlob),
    cfunc(c"attachInternals", 0, ns_element_attachInternals),
    getset(
        c"_internals",
        Some(ns_element_get_internals),
        Some(ns_element_noop_set),
    ),
    getset(
        c"index",
        Some(ns_element_get_option_index),
        Some(ns_element_noop_set),
    ),
    getset(
        c"rows",
        Some(ns_element_table_rows),
        Some(ns_element_set_rows),
    ),
    getset(
        c"caption",
        Some(ns_element_table_caption),
        Some(ns_element_noop_set),
    ),
    getset(
        c"tHead",
        Some(ns_element_table_thead),
        Some(ns_element_noop_set),
    ),
    getset(
        c"tFoot",
        Some(ns_element_table_tfoot),
        Some(ns_element_noop_set),
    ),
    getset(
        c"tBodies",
        Some(ns_element_table_tbodies),
        Some(ns_element_noop_set),
    ),
    getset(
        c"cells",
        Some(ns_element_tr_cells),
        Some(ns_element_noop_set),
    ),
    getset(
        c"rowIndex",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
    getset(
        c"sectionRowIndex",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
    getset(
        c"cellIndex",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
    getset_magic(
        c"colSpan",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        6,
    ),
    getset_magic(
        c"rowSpan",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        7,
    ),
    getset(
        c"returnValue",
        Some(ns_dialog_get_returnValue),
        Some(ns_dialog_set_returnValue),
    ),
    cfunc(c"createCaption", 0, ns_table_createCaption),
    cfunc(c"createTHead", 0, ns_table_createTHead),
    cfunc(c"createTFoot", 0, ns_table_createTFoot),
    cfunc(c"createTBody", 0, ns_table_createTBody),
    cfunc(c"deleteCaption", 0, ns_table_deleteCaption),
    cfunc(c"deleteTHead", 0, ns_table_deleteTHead),
    cfunc(c"deleteTFoot", 0, ns_table_deleteTFoot),
    cfunc(c"insertRow", 1, ns_table_insertRow),
    cfunc(c"deleteRow", 1, ns_table_deleteRow),
    cfunc(c"insertCell", 1, ns_tr_insertCell),
    cfunc(c"deleteCell", 1, ns_tr_deleteCell),
    cfunc(c"add", 2, ns_select_add),
    cfunc(c"appendChild", 1, ns_element_appendChild),
    cfunc(c"removeChild", 1, ns_element_removeChild),
    cfunc(c"insertBefore", 2, ns_element_insertBefore),
    cfunc(c"moveBefore", 2, ns_element_moveBefore),
    cfunc(c"replaceChild", 2, ns_element_replaceChild),
    cfunc(c"insertAdjacentHTML", 2, ns_element_insertAdjacentHTML),
    cfunc(c"getHTML", 0, ns_element_getHTML),
    cfunc(c"setHTMLUnsafe", 1, ns_element_setHTMLUnsafe),
    cfunc(
        c"insertAdjacentElement",
        2,
        ns_element_insertAdjacentElement,
    ),
    cfunc(c"insertAdjacentText", 2, ns_element_insertAdjacentText),
    cfunc(c"replaceChildren", 0, ns_element_replaceChildren),
    cfunc(c"getAttributeNames", 0, ns_element_getAttributeNames),
    cfunc(c"getAttributeNode", 1, ns_element_getAttributeNode),
    cfunc(c"getAttributeNodeNS", 2, ns_element_getAttributeNodeNS),
    cfunc(c"removeAttributeNode", 1, ns_element_removeAttributeNode),
    cfunc(c"setAttributeNode", 1, ns_element_setAttributeNode),
    cfunc(c"setAttributeNodeNS", 1, ns_element_setAttributeNode),
    cfunc(c"hasAttributes", 0, ns_element_hasAttributes),
    cfunc(c"remove", 0, ns_element_remove_self),
    cfunc(c"cloneNode", 1, ns_element_cloneNode),
    cfunc(c"normalize", 0, ns_element_normalize),
    cfunc(c"append", 0, ns_element_append),
    cfunc(c"prepend", 0, ns_element_prepend),
    cfunc(c"before", 0, ns_element_before),
    cfunc(c"after", 0, ns_element_after),
    cfunc(c"replaceWith", 0, ns_element_replaceWith),
    cfunc(c"addEventListener", 2, ns_element_addEventListener),
    cfunc(c"removeEventListener", 2, ns_element_removeEventListener),
    cfunc(c"getElementsByTagName", 1, ns_element_getElementsByTagName),
    cfunc(
        c"getElementsByTagNameNS",
        2,
        ns_element_getElementsByTagNameNS,
    ),
    cfunc(c"getElementById", 1, ns_element_getElementById),
    cfunc(
        c"getElementsByClassName",
        1,
        ns_element_getElementsByClassName,
    ),
    cfunc(c"querySelector", 1, ns_element_querySelector),
    cfunc(c"querySelectorAll", 1, ns_element_querySelectorAll),
    cfunc(c"matches", 1, ns_element_matches),
    cfunc(c"webkitMatchesSelector", 1, ns_element_matches),
    cfunc(c"closest", 1, ns_element_closest),
    cfunc(c"contains", 1, ns_element_contains),
    cfunc(c"hasChildNodes", 0, ns_element_hasChildNodes),
    cfunc(
        c"getBoundingClientRect",
        0,
        ns_element_getBoundingClientRect,
    ),
    cfunc(c"getBBox", 0, ns_element_getBBox),
    cfunc(c"getCTM", 0, ns_element_getCTM),
    cfunc(c"getScreenCTM", 0, ns_element_getScreenCTM),
    cfunc(c"getTotalLength", 0, ns_element_getTotalLength),
    cfunc(c"getPointAtLength", 1, ns_element_getPointAtLength),
    cfunc(c"createSVGPoint", 0, ns_element_createSVGPoint),
    cfunc(c"createSVGRect", 0, ns_element_createSVGRect),
    cfunc(c"createSVGMatrix", 0, ns_element_createSVGMatrix),
    cfunc(c"createSVGTransform", 0, ns_element_createSVGTransform),
    getset(
        c"ownerSVGElement",
        Some(ns_element_get_ownerSVGElement),
        Some(ns_element_noop_set),
    ),
    cfunc(c"focus", 0, ns_element_focus),
    cfunc(c"blur", 0, ns_element_blur),
    cfunc(c"click", 0, ns_element_click),
    cfunc(c"submit", 0, ns_element_form_submit),
    cfunc(c"requestSubmit", 0, ns_element_form_requestSubmit),
    cfunc(c"reset", 0, ns_element_form_reset),
    cfunc(c"checkValidity", 0, ns_element_check_validity),
    cfunc(c"reportValidity", 0, ns_element_check_validity),
    cfunc(c"setCustomValidity", 1, ns_element_setCustomValidity),
    cfunc(c"cancelAsync", 1, ns_services_clear_timer),
    cfunc(c"announce", 1, ns_event_noop),
    cfunc(c"scrollIntoView", 0, ns_element_scrollIntoView),
    cfunc(c"checkVisibility", 0, ns_element_checkVisibility),
    cfunc(c"setPointerCapture", 1, ns_element_setPointerCapture),
    cfunc(
        c"releasePointerCapture",
        1,
        ns_element_releasePointerCapture,
    ),
    cfunc(c"hasPointerCapture", 1, ns_element_hasPointerCapture),
    cfunc(c"show", 0, ns_element_show),
    cfunc(c"showModal", 0, ns_element_showModal),
    cfunc(c"close", 0, ns_element_close),
    cfunc(c"requestClose", 0, ns_element_requestClose),
    cfunc(c"dispatchEvent", 1, ns_element_dispatchEvent),
    cfunc(c"assignedNodes", 0, ns_element_assignedNodes),
    cfunc(c"assignedElements", 0, ns_element_assignedElements),
    cfunc(c"getContext", 1, ns_element_getContext),
    cfunc(c"toDataURL", 0, ns_element_toDataURL),
    getset(
        c"nodeType",
        Some(ns_element_get_nodeType),
        Some(ns_element_noop_set),
    ),
    getset(
        c"nodeValue",
        Some(ns_element_get_nodeValue),
        Some(ns_element_set_nodeValue),
    ),
    getset(
        c"wholeText",
        Some(ns_element_get_wholeText),
        Some(ns_element_noop_set),
    ),
    cfunc(c"replaceWholeText", 1, ns_text_replaceWholeText),
    getset(
        c"nodeName",
        Some(ns_element_get_nodeName),
        Some(ns_element_noop_set),
    ),
    getset(
        c"dataset",
        Some(ns_element_get_dataset),
        Some(ns_element_noop_set),
    ),
    getset(
        c"offsetTop",
        Some(ns_element_get_offsetTop),
        Some(ns_element_noop_set),
    ),
    getset(
        c"offsetLeft",
        Some(ns_element_get_offsetLeft),
        Some(ns_element_noop_set),
    ),
    getset(
        c"offsetWidth",
        Some(ns_element_get_offsetWidth),
        Some(ns_element_noop_set),
    ),
    getset(
        c"offsetHeight",
        Some(ns_element_get_offsetHeight),
        Some(ns_element_noop_set),
    ),
    getset(
        c"clientTop",
        Some(ns_element_get_clientTop),
        Some(ns_element_noop_set),
    ),
    getset(
        c"clientLeft",
        Some(ns_element_get_clientLeft),
        Some(ns_element_noop_set),
    ),
    getset(
        c"clientWidth",
        Some(ns_element_get_clientWidth),
        Some(ns_element_noop_set),
    ),
    getset(
        c"clientHeight",
        Some(ns_element_get_clientHeight),
        Some(ns_element_noop_set),
    ),
    getset(
        c"scrollTop",
        Some(ns_element_get_scrollTop),
        Some(ns_element_set_scrollTop),
    ),
    getset(
        c"scrollLeft",
        Some(ns_element_get_scrollLeft),
        Some(ns_element_set_scrollLeft),
    ),
    getset(
        c"scrollWidth",
        Some(ns_element_get_scrollWidth),
        Some(ns_element_noop_set),
    ),
    getset(
        c"scrollHeight",
        Some(ns_element_get_scrollHeight),
        Some(ns_element_noop_set),
    ),
    getset(
        c"attributes",
        Some(ns_element_get_attributes),
        Some(ns_element_noop_set),
    ),
    getset(
        c"naturalWidth",
        Some(ns_element_img_natural_width),
        Some(ns_element_noop_set),
    ),
    getset(
        c"naturalHeight",
        Some(ns_element_img_natural_height),
        Some(ns_element_noop_set),
    ),
    getset(
        c"complete",
        Some(ns_element_img_complete),
        Some(ns_element_noop_set),
    ),
    getset(
        c"currentSrc",
        Some(ns_element_img_current_src),
        Some(ns_element_noop_set),
    ),
    getset(
        c"content",
        Some(ns_element_template_content),
        Some(ns_element_set_content),
    ),
    getset(
        c"hidden",
        Some(ns_element_get_hidden),
        Some(ns_element_set_hidden),
    ),
    getset_magic(
        c"title",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        0,
    ),
    getset_magic(
        c"name",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        1,
    ),
    getset_magic(
        c"alt",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        2,
    ),
    getset_magic(
        c"src",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        3,
    ),
    getset_magic(
        c"href",
        Some(ns_element_anchor_part_get),
        Some(ns_element_anchor_href_set),
        0,
    ),
    getset_magic(
        c"protocol",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        1,
    ),
    getset_magic(
        c"host",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        2,
    ),
    getset_magic(
        c"hostname",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        3,
    ),
    getset_magic(
        c"port",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        4,
    ),
    getset_magic(
        c"pathname",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        5,
    ),
    getset_magic(
        c"search",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        6,
    ),
    getset_magic(
        c"hash",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        7,
    ),
    getset_magic(
        c"origin",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        8,
    ),
    getset_magic(
        c"username",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        9,
    ),
    getset_magic(
        c"password",
        Some(ns_element_anchor_part_get),
        Some(ns_element_url_part_set),
        10,
    ),
    getset(
        c"type",
        Some(ns_element_get_type),
        Some(ns_element_set_type),
    ),
    getset_magic(
        c"placeholder",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        6,
    ),
    getset_magic(
        c"lang",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        7,
    ),
    getset(c"dir", Some(ns_element_get_dir), Some(ns_element_set_dir)),
    getset_magic(
        c"action",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        9,
    ),
    getset_magic(
        c"method",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        2,
    ),
    getset_magic(
        c"enctype",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        11,
    ),
    getset_magic(
        c"target",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        12,
    ),
    getset_magic(
        c"rel",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        13,
    ),
    getset_magic(
        c"accept",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        14,
    ),
    getset_magic(
        c"acceptCharset",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        15,
    ),
    getset(
        c"autocomplete",
        Some(ns_element_get_autocomplete),
        Some(ns_element_set_autocomplete),
    ),
    getset(
        c"list",
        Some(ns_element_get_list_ref),
        Some(ns_element_list_set),
    ),
    getset_magic(
        c"min",
        Some(ns_element_range_number_getter),
        Some(ns_element_range_number_setter),
        0,
    ),
    getset_magic(
        c"max",
        Some(ns_element_range_number_getter),
        Some(ns_element_range_number_setter),
        1,
    ),
    getset_magic(
        c"low",
        Some(ns_element_range_number_getter),
        Some(ns_element_range_number_setter),
        2,
    ),
    getset_magic(
        c"high",
        Some(ns_element_range_number_getter),
        Some(ns_element_range_number_setter),
        3,
    ),
    getset_magic(
        c"optimum",
        Some(ns_element_range_number_getter),
        Some(ns_element_range_number_setter),
        4,
    ),
    getset_magic(
        c"step",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        20,
    ),
    getset_magic(
        c"pattern",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        21,
    ),
    getset(
        c"spellcheck",
        Some(ns_element_get_spellcheck),
        Some(ns_element_set_spellcheck),
    ),
    getset_magic(
        c"crossOrigin",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        3,
    ),
    getset_magic(
        c"referrerPolicy",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        4,
    ),
    getset_magic(
        c"decoding",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        1,
    ),
    getset_magic(
        c"loading",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        0,
    ),
    getset_magic(
        c"fetchPriority",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        27,
    ),
    getset(
        c"sizes",
        Some(ns_element_get_sizes_list),
        Some(ns_element_attr_setter_sizes),
    ),
    getset(
        c"sandbox",
        Some(ns_element_get_sandbox_list),
        Some(ns_element_attr_setter_sandbox),
    ),
    getset_magic(
        c"srcset",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        29,
    ),
    getset_magic(
        c"useMap",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        30,
    ),
    getset_magic(
        c"inputMode",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        31,
    ),
    getset_magic(
        c"size",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        2,
    ),
    getset_magic(
        c"cols",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        3,
    ),
    getset_magic(
        c"allowFullscreen",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        0,
    ),
    getset_magic(
        c"declare",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        1,
    ),
    getset_magic(
        c"defaultMuted",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        2,
    ),
    getset_magic(
        c"default",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        3,
    ),
    getset_magic(
        c"noHref",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        4,
    ),
    getset_magic(
        c"noShade",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        5,
    ),
    getset_magic(
        c"compact",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        6,
    ),
    getset_magic(
        c"noWrap",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        7,
    ),
    getset_magic(
        c"trueSpeed",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        8,
    ),
    getset_magic(
        c"noResize",
        Some(ns_element_bool_attr_getter),
        Some(ns_element_bool_attr_setter),
        9,
    ),
    getset_magic(
        c"face",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        130,
    ),
    getset_magic(
        c"text",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        131,
    ),
    getset_magic(
        c"hspace",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        11,
    ),
    getset_magic(
        c"vspace",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        12,
    ),
    getset_magic(
        c"scrollAmount",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        13,
    ),
    getset_magic(
        c"scrollDelay",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        14,
    ),
    getset_magic(
        c"maxLength",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        0,
    ),
    getset_magic(
        c"minLength",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        1,
    ),
    getset_magic(
        c"span",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        5,
    ),
    getset_magic(
        c"width",
        Some(ns_element_dimension_getter),
        Some(ns_element_dimension_setter),
        8,
    ),
    getset_magic(
        c"height",
        Some(ns_element_dimension_getter),
        Some(ns_element_dimension_setter),
        9,
    ),
    cfunc(c"beginElement", 0, ns_svg_beginElement),
    cfunc(c"setCurrentTime", 1, ns_svg_setCurrentTime),
    getset_magic(
        c"start",
        Some(ns_element_int_attr_getter),
        Some(ns_element_int_attr_setter),
        10,
    ),
    getset_magic(
        c"coords",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        37,
    ),
    getset_magic(
        c"shape",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        38,
    ),
    getset_magic(
        c"formAction",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        39,
    ),
    getset_magic(
        c"formMethod",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        40,
    ),
    getset_magic(
        c"formEnctype",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        41,
    ),
    getset_magic(
        c"formTarget",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        42,
    ),
    getset_magic(
        c"integrity",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        43,
    ),
    getset_magic(
        c"kind",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        44,
    ),
    getset_magic(
        c"hreflang",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        46,
    ),
    getset_magic(
        c"charset",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        47,
    ),
    getset_magic(
        c"ping",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        90,
    ),
    getset_magic(
        c"rev",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        91,
    ),
    getset_magic(
        c"as",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        92,
    ),
    getset_magic(
        c"align",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        93,
    ),
    getset_magic(
        c"vAlign",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        94,
    ),
    getset_magic(
        c"ch",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        95,
    ),
    getset_magic(
        c"chOff",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        96,
    ),
    getset_magic(
        c"bgColor",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        97,
    ),
    getset_magic(
        c"background",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        98,
    ),
    getset_magic(
        c"link",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        99,
    ),
    getset_magic(
        c"vLink",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        100,
    ),
    getset_magic(
        c"aLink",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        101,
    ),
    getset_magic(
        c"color",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        102,
    ),
    getset_magic(
        c"clear",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        103,
    ),
    getset_magic(
        c"summary",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        104,
    ),
    getset_magic(
        c"frame",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        105,
    ),
    getset_magic(
        c"rules",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        106,
    ),
    getset_magic(
        c"border",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        107,
    ),
    getset_magic(
        c"cellPadding",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        108,
    ),
    getset_magic(
        c"cellSpacing",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        109,
    ),
    getset_magic(
        c"axis",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        110,
    ),
    getset_magic(
        c"abbr",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        111,
    ),
    getset_magic(
        c"headers",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        112,
    ),
    getset_magic(
        c"scheme",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        113,
    ),
    getset_magic(
        c"standby",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        114,
    ),
    getset_magic(
        c"codeType",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        115,
    ),
    getset_magic(
        c"codeBase",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        116,
    ),
    getset_magic(
        c"code",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        117,
    ),
    getset_magic(
        c"archive",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        118,
    ),
    getset_magic(
        c"scrolling",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        119,
    ),
    getset_magic(
        c"frameBorder",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        120,
    ),
    getset_magic(
        c"marginWidth",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        121,
    ),
    getset_magic(
        c"marginHeight",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        122,
    ),
    getset_magic(
        c"longDesc",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        123,
    ),
    getset_magic(
        c"lowsrc",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        124,
    ),
    getset_magic(
        c"version",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        125,
    ),
    getset_magic(
        c"event",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        126,
    ),
    getset_magic(
        c"valueType",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        127,
    ),
    getset_magic(
        c"srclang",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        128,
    ),
    getset_magic(
        c"dirName",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        129,
    ),
    getset_magic(
        c"httpEquiv",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        49,
    ),
    getset(
        c"contentEditable",
        Some(ns_element_get_contentEditable),
        Some(ns_element_set_contentEditable),
    ),
    getset_magic(
        c"slot",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        51,
    ),
    getset_magic(
        c"role",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        0,
    ),
    getset_magic(
        c"ariaLabel",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        1,
    ),
    getset_magic(
        c"ariaBrailleLabel",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        2,
    ),
    getset_magic(
        c"ariaBrailleRoleDescription",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        3,
    ),
    getset_magic(
        c"ariaColCount",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        4,
    ),
    getset_magic(
        c"ariaColIndex",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        5,
    ),
    getset_magic(
        c"ariaColIndexText",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        6,
    ),
    getset_magic(
        c"ariaColSpan",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        7,
    ),
    getset_magic(
        c"ariaDescription",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        8,
    ),
    getset_magic(
        c"ariaKeyShortcuts",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        9,
    ),
    getset_magic(
        c"ariaLevel",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        10,
    ),
    getset_magic(
        c"ariaPlaceholder",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        11,
    ),
    getset_magic(
        c"ariaPosInSet",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        12,
    ),
    getset_magic(
        c"ariaRelevant",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        13,
    ),
    getset_magic(
        c"ariaRoleDescription",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        14,
    ),
    getset_magic(
        c"ariaRowCount",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        15,
    ),
    getset_magic(
        c"ariaRowIndex",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        16,
    ),
    getset_magic(
        c"ariaRowIndexText",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        17,
    ),
    getset_magic(
        c"ariaRowSpan",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        18,
    ),
    getset_magic(
        c"ariaSetSize",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        19,
    ),
    getset_magic(
        c"ariaValueMax",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        20,
    ),
    getset_magic(
        c"ariaValueMin",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        21,
    ),
    getset_magic(
        c"ariaValueNow",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        22,
    ),
    getset_magic(
        c"ariaValueText",
        Some(ns_element_aria_string_getter),
        Some(ns_element_aria_string_setter),
        23,
    ),
    getset_magic(
        c"ariaHidden",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        14,
    ),
    getset_magic(
        c"ariaDisabled",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        11,
    ),
    getset_magic(
        c"ariaPressed",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        21,
    ),
    getset_magic(
        c"ariaExpanded",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        12,
    ),
    getset_magic(
        c"ariaControls",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        59,
    ),
    getset_magic(
        c"ariaDescribedBy",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        60,
    ),
    getset_magic(
        c"ariaLabelledBy",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        61,
    ),
    getset_magic(
        c"ariaLive",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        16,
    ),
    getset_magic(
        c"ariaBusy",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        8,
    ),
    getset_magic(
        c"ariaChecked",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        9,
    ),
    getset_magic(
        c"ariaCurrent",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        10,
    ),
    getset_magic(
        c"ariaSelected",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        24,
    ),
    getset_magic(
        c"ariaAtomic",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        6,
    ),
    getset_magic(
        c"ariaAutoComplete",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        7,
    ),
    getset_magic(
        c"ariaHasPopup",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        13,
    ),
    getset_magic(
        c"ariaInvalid",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        15,
    ),
    getset_magic(
        c"ariaModal",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        17,
    ),
    getset_magic(
        c"ariaMultiLine",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        18,
    ),
    getset_magic(
        c"ariaMultiSelectable",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        19,
    ),
    getset_magic(
        c"ariaOrientation",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        20,
    ),
    getset_magic(
        c"ariaReadOnly",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        22,
    ),
    getset_magic(
        c"ariaRequired",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        23,
    ),
    getset_magic(
        c"ariaSort",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        25,
    ),
    getset_magic(
        c"nonce",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        72,
    ),
    getset_magic(
        c"accessKey",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        73,
    ),
    getset_magic(
        c"dateTime",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        74,
    ),
    getset_magic(
        c"srcdoc",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        75,
    ),
    getset_magic(
        c"popoverTargetAction",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        77,
    ),
    getset(
        c"autocapitalize",
        Some(ns_element_get_autocapitalize),
        Some(ns_element_set_autocapitalize),
    ),
    getset_magic(
        c"enterKeyHint",
        Some(ns_element_enum_getter),
        Some(ns_element_enum_setter),
        5,
    ),
    getset_magic(
        c"charSet",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        47,
    ),
    getset_magic(
        c"poster",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        83,
    ),
    getset_magic(
        c"preload",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        84,
    ),
    getset_magic(
        c"wrap",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        85,
    ),
    getset_magic(
        c"scope",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        86,
    ),
    getset_magic(
        c"cite",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        87,
    ),
    getset_magic(
        c"media",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        88,
    ),
    getset_magic(
        c"download",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        89,
    ),
    getset_magic(
        c"open",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        0,
    ),
    getset(
        c"selected",
        Some(ns_element_get_selected),
        Some(ns_element_set_selected),
    ),
    getset_magic(
        c"multiple",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        2,
    ),
    getset_magic(
        c"readOnly",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        3,
    ),
    getset_magic(
        c"autofocus",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        4,
    ),
    getset_magic(
        c"controls",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        5,
    ),
    getset_magic(
        c"loop",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        6,
    ),
    getset_magic(
        c"autoplay",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        8,
    ),
    getset_magic(
        c"defer",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        9,
    ),
    getset_magic(
        c"async",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        10,
    ),
    getset_magic(
        c"noValidate",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        11,
    ),
    getset_magic(
        c"isMap",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        12,
    ),
    getset(
        c"draggable",
        Some(ns_element_get_draggable),
        Some(ns_element_set_draggable),
    ),
    getset_magic(
        c"reversed",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        14,
    ),
    getset_magic(
        c"playsInline",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        15,
    ),
    getset_magic(
        c"inert",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        17,
    ),
    getset_magic(
        c"noModule",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        18,
    ),
    getset_magic(
        c"formNoValidate",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        19,
    ),
    getset_magic(
        c"required",
        Some(ns_element_boolattr_getter),
        Some(ns_element_boolattr_setter),
        20,
    ),
    getset(
        c"htmlFor",
        Some(ns_element_get_htmlFor),
        Some(ns_element_set_htmlFor),
    ),
    getset(
        c"control",
        Some(ns_element_get_label_control),
        Some(ns_element_noop_set),
    ),
    getset(
        c"popoverTargetElement",
        Some(ns_element_get_popoverTargetElement),
        Some(ns_element_set_popoverTargetElement),
    ),
    getset(
        c"tabIndex",
        Some(ns_element_get_tabIndex),
        Some(ns_element_set_tabIndex),
    ),
    getset(
        c"isConnected",
        Some(ns_element_get_isConnected),
        Some(ns_element_noop_set),
    ),
    getset(
        c"baseURI",
        Some(ns_element_get_baseURI),
        Some(ns_element_noop_set),
    ),
    getset(
        c"ownerDocument",
        Some(ns_element_get_ownerDocument),
        Some(ns_element_noop_set),
    ),
    getset(
        c"namespaceURI",
        Some(ns_element_get_namespaceURI),
        Some(ns_element_noop_set),
    ),
    getset(
        c"shadowRoot",
        Some(ns_element_get_shadowRoot),
        Some(ns_element_noop_set),
    ),
    getset(
        c"assignedSlot",
        Some(ns_element_get_assignedSlot),
        Some(ns_element_noop_set),
    ),
    cfunc(c"attachShadow", 1, ns_element_attachShadow),
    getset(
        c"disabled",
        Some(ns_element_get_disabled),
        Some(ns_element_set_disabled),
    ),
    getset(
        c"checked",
        Some(ns_element_get_checked),
        Some(ns_element_set_checked),
    ),
    getset(
        c"label",
        Some(ns_element_get_label_prop),
        Some(ns_element_set_label_prop),
    ),
    getset(
        c"selectedIndex",
        Some(ns_element_get_selectedIndex),
        Some(ns_element_set_selectedIndex),
    ),
    getset(
        c"options",
        Some(ns_element_get_options),
        Some(ns_element_noop_set),
    ),
    getset(
        c"selectedOptions",
        Some(ns_element_get_selectedOptions),
        Some(ns_element_noop_set),
    ),
    getset(
        c"elements",
        Some(ns_element_get_form_elements),
        Some(ns_element_noop_set),
    ),
    getset(
        c"form",
        Some(ns_element_get_form),
        Some(ns_element_noop_set),
    ),
];

pub(crate) static SRC_PROTO: [Prop; 1] = [accessor_magic(
    "src",
    Some(nsr_element_attr_getter),
    Some(nsr_element_attr_setter),
    3,
)];

pub(crate) static IMAGE_PROTO_FUNCS: [FunctionListEntry; 6] = [
    getset_magic(
        c"src",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        3,
    ),
    getset_magic(
        c"srcset",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        29,
    ),
    getset(
        c"currentSrc",
        Some(ns_element_img_current_src),
        Some(ns_element_noop_set),
    ),
    getset(
        c"naturalWidth",
        Some(ns_element_img_natural_width),
        Some(ns_element_noop_set),
    ),
    getset(
        c"naturalHeight",
        Some(ns_element_img_natural_height),
        Some(ns_element_noop_set),
    ),
    getset(
        c"complete",
        Some(ns_element_img_complete),
        Some(ns_element_noop_set),
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

pub(crate) static MEDIA_PROTO_FUNCS: [FunctionListEntry; 21] = [
    cfunc(c"play", 0, ns_media_play),
    cfunc(c"pause", 0, ns_media_pause),
    cfunc(c"load", 0, ns_media_load),
    cfunc(c"canPlayType", 1, ns_media_canPlayType),
    cfunc(c"fastSeek", 1, ns_media_fast_seek),
    getset_magic(
        c"src",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        3,
    ),
    getset(
        c"currentSrc",
        Some(ns_element_img_current_src),
        Some(ns_element_noop_set),
    ),
    getset(
        c"currentTime",
        Some(ns_media_get_current_time),
        Some(ns_media_set_current_time),
    ),
    getset(
        c"duration",
        Some(ns_media_get_duration),
        Some(ns_element_noop_set),
    ),
    getset(
        c"paused",
        Some(ns_media_get_paused),
        Some(ns_element_noop_set),
    ),
    getset(
        c"ended",
        Some(ns_media_get_ended),
        Some(ns_element_noop_set),
    ),
    getset(
        c"seeking",
        Some(ns_media_get_seeking),
        Some(ns_element_noop_set),
    ),
    getset(
        c"volume",
        Some(ns_media_get_volume),
        Some(ns_media_set_volume),
    ),
    getset(c"muted", Some(ns_media_get_muted), Some(ns_media_set_muted)),
    getset(
        c"playbackRate",
        Some(ns_media_get_playbackRate),
        Some(ns_media_set_playbackRate),
    ),
    getset(
        c"defaultPlaybackRate",
        Some(ns_media_get_defaultPlaybackRate),
        Some(ns_media_set_defaultPlaybackRate),
    ),
    getset(
        c"readyState",
        Some(ns_media_get_readyState),
        Some(ns_element_noop_set),
    ),
    getset(
        c"networkState",
        Some(ns_media_get_networkState),
        Some(ns_element_noop_set),
    ),
    getset(
        c"buffered",
        Some(ns_media_get_buffered_ranges),
        Some(ns_element_noop_set),
    ),
    getset(
        c"played",
        Some(ns_media_get_played_ranges),
        Some(ns_element_noop_set),
    ),
    getset(
        c"seekable",
        Some(ns_media_get_seekable_ranges),
        Some(ns_element_noop_set),
    ),
];

pub(crate) static VIDEO_PROTO_FUNCS: [FunctionListEntry; 5] = [
    cfunc(
        c"getVideoPlaybackQuality",
        0,
        ns_media_get_video_playback_quality,
    ),
    cfunc(
        c"requestVideoFrameCallback",
        1,
        ns_media_request_video_frame_callback,
    ),
    cfunc(
        c"cancelVideoFrameCallback",
        1,
        ns_window_cancelAnimationFrame,
    ),
    getset(
        c"videoWidth",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
    getset(
        c"videoHeight",
        Some(ns_element_get_zero_int),
        Some(ns_element_noop_set),
    ),
];

pub(crate) static IFRAME_PROTO_FUNCS: [FunctionListEntry; 2] = [
    getset_magic(
        c"src",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        3,
    ),
    getset_magic(
        c"srcdoc",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        75,
    ),
];

pub(crate) static ANCHOR_PROTO_FUNCS: [FunctionListEntry; 3] = [
    getset_magic(
        c"href",
        Some(ns_element_anchor_part_get),
        Some(ns_element_anchor_href_set),
        0,
    ),
    getset_magic(
        c"ping",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        90,
    ),
    getset_magic(
        c"download",
        Some(ns_element_attr_getter),
        Some(ns_element_attr_setter),
        89,
    ),
];

pub(crate) static HREF_PROTO_FUNCS: [FunctionListEntry; 1] = [getset_magic(
    c"href",
    Some(ns_element_anchor_part_get),
    Some(ns_element_anchor_href_set),
    0,
)];

pub(crate) static NAMEDMAP_PROTO_FUNCS: [FunctionListEntry; 8] = [
    cfunc(c"getNamedItem", 1, ns_namedmap_getNamedItem),
    cfunc(c"getNamedItemNS", 2, ns_namedmap_getNamedItemNS),
    cfunc(c"setNamedItem", 1, ns_namedmap_setNamedItem),
    cfunc(c"setNamedItemNS", 1, ns_namedmap_setNamedItem),
    cfunc(c"removeNamedItem", 1, ns_namedmap_removeNamedItem),
    cfunc(c"removeNamedItemNS", 2, ns_namedmap_removeNamedItemNS),
    cfunc(c"item", 1, ns_namedmap_item),
    getset(
        c"length",
        Some(ns_namedmap_get_length),
        Some(ns_element_noop_set),
    ),
];

pub(crate) static DOM_IMPLEMENTATION_PROTO_FUNCS: [FunctionListEntry; 4] = [
    cfunc(c"hasFeature", 2, ns_event_true),
    cfunc(c"createHTMLDocument", 1, ns_impl_create_html_document),
    cfunc(c"createDocument", 3, ns_impl_create_document),
    cfunc(c"createDocumentType", 3, ns_impl_create_document_type),
];

pub(crate) static DOCUMENT_FUNCS: [FunctionListEntry; 87] = [
    getset_magic(
        c"fgColor",
        Some(ns_document_get_color),
        Some(ns_document_set_color),
        0,
    ),
    getset_magic(
        c"bgColor",
        Some(ns_document_get_color),
        Some(ns_document_set_color),
        1,
    ),
    getset_magic(
        c"linkColor",
        Some(ns_document_get_color),
        Some(ns_document_set_color),
        2,
    ),
    getset_magic(
        c"vlinkColor",
        Some(ns_document_get_color),
        Some(ns_document_set_color),
        3,
    ),
    getset_magic(
        c"alinkColor",
        Some(ns_document_get_color),
        Some(ns_document_set_color),
        4,
    ),
    cfunc(c"getElementById", 1, ns_document_getElementById),
    cfunc(c"createElement", 1, ns_document_createElement),
    cfunc(c"createElementNS", 2, ns_document_createElementNS),
    cfunc(c"createTextNode", 1, ns_document_createTextNode),
    cfunc(c"createComment", 1, ns_document_createComment),
    cfunc(c"createCDATASection", 1, ns_document_createCDATASection),
    cfunc(
        c"createProcessingInstruction",
        2,
        ns_document_createProcessingInstruction,
    ),
    cfunc(c"createAttribute", 1, ns_document_createAttribute),
    cfunc(c"createAttributeNS", 2, ns_document_createAttributeNS),
    cfunc(c"createEvent", 1, ns_document_createEvent),
    cfunc(
        c"createDocumentFragment",
        0,
        ns_document_createDocumentFragment,
    ),
    cfunc(c"getElementsByTagName", 1, ns_document_getElementsByTagName),
    cfunc(
        c"getElementsByTagNameNS",
        2,
        ns_document_getElementsByTagNameNS,
    ),
    cfunc(
        c"getElementsByClassName",
        1,
        ns_document_getElementsByClassName,
    ),
    cfunc(c"querySelector", 1, ns_document_querySelector),
    cfunc(c"querySelectorAll", 1, ns_document_querySelectorAll),
    getset(
        c"documentElement",
        Some(ns_document_get_documentElement),
        Some(ns_element_noop_set),
    ),
    getset(
        c"body",
        Some(ns_document_get_body),
        Some(ns_document_set_body),
    ),
    getset(
        c"head",
        Some(ns_document_get_head),
        Some(ns_element_noop_set),
    ),
    getset(
        c"activeElement",
        Some(ns_document_get_activeElement),
        Some(ns_element_noop_set),
    ),
    getset(
        c"forms",
        Some(ns_document_get_forms),
        Some(ns_element_noop_set),
    ),
    getset(
        c"images",
        Some(ns_document_get_images),
        Some(ns_element_noop_set),
    ),
    getset(
        c"links",
        Some(ns_document_get_links),
        Some(ns_element_noop_set),
    ),
    getset(
        c"scripts",
        Some(ns_document_get_scripts),
        Some(ns_element_noop_set),
    ),
    getset(
        c"embeds",
        Some(ns_document_get_embeds),
        Some(ns_element_noop_set),
    ),
    getset(
        c"plugins",
        Some(ns_document_get_plugins),
        Some(ns_element_noop_set),
    ),
    getset(
        c"designMode",
        Some(ns_document_get_designMode),
        Some(ns_element_noop_set),
    ),
    getset(
        c"lastModified",
        Some(ns_document_get_lastModified),
        Some(ns_element_noop_set),
    ),
    getset(c"all", Some(ns_document_get_all), Some(ns_element_noop_set)),
    getset(
        c"anchors",
        Some(ns_document_get_anchors),
        Some(ns_element_noop_set),
    ),
    getset(
        c"applets",
        Some(ns_document_get_applets),
        Some(ns_element_noop_set),
    ),
    getset(
        c"fonts",
        Some(ns_document_get_fonts),
        Some(ns_element_noop_set),
    ),
    getset(c"implementation", Some(ns_document_implementation), None),
    cfunc(c"write", 1, ns_document_write),
    cfunc(c"writeln", 1, ns_document_writeln),
    cfunc(c"open", 0, ns_document_open),
    cfunc(c"close", 0, ns_document_close),
    cfunc(c"execCommand", 3, ns_services_exec_command),
    cfunc(c"hasFocus", 0, ns_document_has_focus),
    cfunc(c"elementFromPoint", 2, ns_document_element_from_point),
    cfunc(c"elementsFromPoint", 2, ns_document_elements_from_point),
    cfunc(c"createRange", 0, ns_document_create_range),
    cfunc(c"createTreeWalker", 3, ns_document_create_tree_walker),
    cfunc(c"createNodeIterator", 3, ns_document_create_node_iterator),
    cfunc(c"getSelection", 0, ns_window_get_selection),
    cfunc(c"adoptNode", 1, ns_document_adopt_node),
    cfunc(c"importNode", 2, ns_document_import_node),
    cfunc(c"exitFullscreen", 0, ns_document_exit_fullscreen),
    cfunc(c"webkitExitFullscreen", 0, ns_document_exit_fullscreen),
    cfunc(c"webkitCancelFullScreen", 0, ns_document_exit_fullscreen),
    cfunc(c"mozCancelFullScreen", 0, ns_document_exit_fullscreen),
    cfunc(c"msExitFullscreen", 0, ns_document_exit_fullscreen),
    cfunc(c"exitPointerLock", 0, ns_document_exitPointerLock),
    cfunc(
        c"queryCommandSupported",
        1,
        ns_services_query_command_supported,
    ),
    cfunc(c"queryCommandEnabled", 1, ns_services_query_command_enabled),
    cfunc(c"queryCommandState", 1, ns_event_false),
    cfunc(c"queryCommandValue", 1, ns_event_noop),
    getset(
        c"currentScript",
        Some(ns_document_get_currentScript),
        Some(ns_element_noop_set),
    ),
    getset(
        c"rootElement",
        Some(ns_document_get_documentElement),
        Some(ns_element_noop_set),
    ),
    getset(
        c"fullscreenElement",
        Some(ns_document_get_fullscreen_element),
        Some(ns_element_noop_set),
    ),
    getset(
        c"webkitFullscreenElement",
        Some(ns_document_get_fullscreen_element),
        Some(ns_element_noop_set),
    ),
    getset(
        c"mozFullScreenElement",
        Some(ns_document_get_fullscreen_element),
        Some(ns_element_noop_set),
    ),
    getset(
        c"msFullscreenElement",
        Some(ns_document_get_fullscreen_element),
        Some(ns_element_noop_set),
    ),
    getset(
        c"webkitIsFullScreen",
        Some(ns_document_get_is_fullscreen),
        Some(ns_element_noop_set),
    ),
    getset(
        c"mozFullScreen",
        Some(ns_document_get_is_fullscreen),
        Some(ns_element_noop_set),
    ),
    getset(
        c"pointerLockElement",
        Some(ns_document_get_pointerLockElement),
        Some(ns_element_noop_set),
    ),
    getset(
        c"fullscreenEnabled",
        Some(ns_document_get_fullscreen_enabled),
        Some(ns_element_noop_set),
    ),
    getset(
        c"webkitFullscreenEnabled",
        Some(ns_document_get_fullscreen_enabled),
        Some(ns_element_noop_set),
    ),
    getset(
        c"scrollingElement",
        Some(ns_document_get_scrollingElement),
        Some(ns_element_noop_set),
    ),
    cfunc(c"addEventListener", 2, ns_document_addEventListener),
    cfunc(c"removeEventListener", 2, ns_document_removeEventListener),
    cfunc(c"dispatchEvent", 1, ns_document_dispatchEvent),
    cfunc(c"getElementsByName", 1, ns_document_getElementsByName),
    cfunc(c"getItems", 1, ns_document_getItems),
    getset(
        c"title",
        Some(ns_document_get_title),
        Some(ns_document_set_title),
    ),
    getset(
        c"cookie",
        Some(ns_document_get_cookie),
        Some(ns_document_set_cookie),
    ),
    getset(
        c"referrer",
        Some(ns_document_get_referrer),
        Some(ns_element_noop_set),
    ),
    getset(
        c"readyState",
        Some(ns_document_get_readyState),
        Some(ns_element_noop_set),
    ),
    getset(
        c"hidden",
        Some(ns_document_get_hidden),
        Some(ns_element_noop_set),
    ),
    getset(
        c"visibilityState",
        Some(ns_document_get_visibilityState),
        Some(ns_element_noop_set),
    ),
    getset(
        c"compatMode",
        Some(ns_document_get_compatMode),
        Some(ns_element_noop_set),
    ),
    getset(c"dir", Some(ns_document_get_dir), Some(ns_document_set_dir)),
];

pub(crate) static DOCUMENT_PROTO_ACCESSORS: [FunctionListEntry; 9] = [
    getset(
        c"documentElement",
        Some(ns_document_get_documentElement),
        Some(ns_element_noop_set),
    ),
    getset(
        c"body",
        Some(ns_document_get_body),
        Some(ns_document_set_body),
    ),
    getset(
        c"head",
        Some(ns_document_get_head),
        Some(ns_element_noop_set),
    ),
    getset(
        c"currentScript",
        Some(ns_document_get_currentScript),
        Some(ns_element_noop_set),
    ),
    getset(
        c"defaultView",
        Some(ns_document_get_defaultView),
        Some(ns_element_noop_set),
    ),
    getset(c"implementation", Some(ns_document_implementation), None),
    getset(
        c"cookie",
        Some(ns_document_get_cookie),
        Some(ns_document_set_cookie),
    ),
    getset(
        c"readyState",
        Some(ns_document_get_readyState),
        Some(ns_element_noop_set),
    ),
    getset(
        c"xmlVersion",
        Some(ns_document_get_xmlVersion),
        Some(ns_element_noop_set),
    ),
];

pub(crate) static DOCUMENT_PROTO_METHODS: [FunctionListEntry; 5] = [
    cfunc(c"adoptNode", 1, ns_document_adopt_node),
    cfunc(
        c"createDocumentFragment",
        0,
        ns_document_createDocumentFragment,
    ),
    cfunc(c"createElement", 1, ns_document_createElement),
    cfunc(c"createElementNS", 2, ns_document_createElementNS),
    cfunc(c"createTreeWalker", 3, ns_document_create_tree_walker),
];

pub(crate) static TRACK_ACCESSORS: [FunctionListEntry; 1] = [getset_magic(
    c"default",
    Some(ns_element_bool_attr_getter),
    Some(ns_element_bool_attr_setter),
    3,
)];
