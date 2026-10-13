//! Southstar — the C ABI that installs the DOM function tables: the engine classes and their prototypes, per-interface members, the document's own functions and the proto tables the page setup asks for.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_js_engine::quickjs::{self, FunctionListEntry, JSCFunction, JSContext, JSValue};

use super::tables::{self, *};

type Getter = unsafe extern "C" fn(ctx: *mut JSContext, this: JSValue) -> JSValue;
type Setter = unsafe extern "C" fn(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;

const CLASS_ELEMENT: c_int = 0;
const CLASS_ATTR: c_int = 1;
const CLASS_STYLE: c_int = 2;
const CLASS_TOKEN_LIST: c_int = 3;
const CLASS_LIVE: c_int = 4;
const CLASS_DATASET: c_int = 5;
const CLASS_STORAGE: c_int = 6;
const CLASS_WINDOW_NAMED: c_int = 7;

const HOST_PROTO_STYLE: c_int = 0;
const HOST_PROTO_DATASET: c_int = 1;
const HOST_PROTO_TOKEN_LIST: c_int = 2;

const CFUNC_GETTER: c_int = 8;
const CFUNC_SETTER: c_int = 9;
const PROP_CONFIGURABLE: c_int = 1 << 0;
const PROP_WRITABLE: c_int = 1 << 1;
const PROP_ENUMERABLE: c_int = 1 << 2;
const ATOM_NULL: u32 = 0;
const UNDEFINED: JSValue = quickjs::UNDEFINED;

unsafe extern "C" {
    fn JS_NewObject(ctx: *mut JSContext) -> JSValue;
    fn JS_DupValue(ctx: *mut JSContext, v: JSValue) -> JSValue;
    fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);
    fn JS_GetGlobalObject(ctx: *mut JSContext) -> JSValue;
    fn JS_GetPropertyStr(ctx: *mut JSContext, this_obj: JSValue, prop: *const c_char) -> JSValue;
    fn JS_ValueToAtom(ctx: *mut JSContext, val: JSValue) -> u32;
    fn JS_NewAtom(ctx: *mut JSContext, str: *const c_char) -> u32;
    fn JS_FreeAtom(ctx: *mut JSContext, atom: u32);
    fn JS_NewStringLen(ctx: *mut JSContext, str1: *const c_char, len1: usize) -> JSValue;
    fn JS_DefinePropertyValue(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: u32,
        val: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_DefinePropertyValueStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
        val: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_DefinePropertyGetSet(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: u32,
        getter: JSValue,
        setter: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_NewCFunction2(
        ctx: *mut JSContext,
        func: *const core::ffi::c_void,
        name: *const c_char,
        length: c_int,
        cproto: c_int,
        magic: c_int,
    ) -> JSValue;
    fn JS_Eval(
        ctx: *mut JSContext,
        input: *const c_char,
        input_len: usize,
        filename: *const c_char,
        eval_flags: c_int,
    ) -> JSValue;
    fn JS_GetException(ctx: *mut JSContext) -> JSValue;
    fn JS_SetPropertyFunctionList(
        ctx: *mut JSContext,
        obj: JSValue,
        tab: *const FunctionListEntry,
        len: c_int,
    ) -> c_int;

    fn ns_element_register_class(ctx: *mut JSContext, proto: JSValue);
    fn ns_attr_register_class(ctx: *mut JSContext, proto: JSValue);
    fn ns_cssom_install_style_proto(ctx: *mut JSContext, proto: JSValue);
    fn ns_cssom_register_style_class(ctx: *mut JSContext, proto: JSValue);
    fn ns_token_list_register_class(ctx: *mut JSContext, proto: JSValue);
    fn ns_live_register_class(ctx: *mut JSContext);
    fn ns_dataset_register_class(ctx: *mut JSContext, proto: JSValue);
    fn ns_storage_install_proto(ctx: *mut JSContext, proto: JSValue);
    fn ns_storage_register_class(ctx: *mut JSContext, proto: JSValue);
    fn ns_window_named_register_class(ctx: *mut JSContext);
    fn ns_style_class_proto(ctx: *mut JSContext) -> JSValue;
    fn ns_dataset_class_proto(ctx: *mut JSContext) -> JSValue;
    fn ns_token_list_class_proto(ctx: *mut JSContext) -> JSValue;
    fn ns_element_class_id() -> u32;
    fn ns_attr_class_id() -> u32;
    fn ns_js_brand_node_interfaces(ctx: *mut JSContext, element_cid: u32, attr_cid: u32);
    fn ns_define_element_unscopables(ctx: *mut JSContext, proto: JSValue);
    fn ns_install_event_handler_props(ctx: *mut JSContext, target: JSValue);
    fn ns_bind_fn(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
        func: JSCFunction,
        argc: c_int,
    );
    fn ns_element_matches(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_closest(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_querySelector(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_querySelectorAll(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_options_item(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_options_namedItem(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_get_dataset(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_element_get_popover(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_element_set_popover(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;
    fn ns_element_get_assignedSlot(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_dialog_get_closedBy(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_dialog_set_closedBy(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;
    fn ns_button_get_command(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_button_set_command(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;
    fn ns_button_get_commandForElement(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_button_set_commandForElement(
        ctx: *mut JSContext,
        this: JSValue,
        value: JSValue,
    ) -> JSValue;
    fn ns_element_get_text(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_element_set_text(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;
    fn ns_element_get_data(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_element_set_data(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;
    fn ns_element_get_value_prop(ctx: *mut JSContext, this: JSValue) -> JSValue;
    fn ns_element_set_value_prop(ctx: *mut JSContext, this: JSValue, value: JSValue) -> JSValue;
}

unsafe fn set_list(ctx: *mut JSContext, obj: JSValue, list: &'static [FunctionListEntry]) {
    unsafe { JS_SetPropertyFunctionList(ctx, obj, list.as_ptr(), list.len() as c_int) };
}

unsafe fn set_tostring_tag(ctx: *mut JSContext, obj: JSValue, tag: &CStr) {
    unsafe {
        let global = JS_GetGlobalObject(ctx);
        let symbol = JS_GetPropertyStr(ctx, global, c"Symbol".as_ptr());
        let tag_symbol = JS_GetPropertyStr(ctx, symbol, c"toStringTag".as_ptr());
        let atom = JS_ValueToAtom(ctx, tag_symbol);
        if atom != ATOM_NULL {
            let text = tag.to_bytes();
            JS_DefinePropertyValue(
                ctx,
                obj,
                atom,
                JS_NewStringLen(ctx, text.as_ptr().cast(), text.len()),
                PROP_CONFIGURABLE,
            );
            JS_FreeAtom(ctx, atom);
        }
        JS_FreeValue(ctx, tag_symbol);
        JS_FreeValue(ctx, symbol);
        JS_FreeValue(ctx, global);
    }
}

unsafe fn define_getset(
    ctx: *mut JSContext,
    proto: JSValue,
    name: &CStr,
    getter: Getter,
    setter: Option<Setter>,
) {
    unsafe {
        let atom = JS_NewAtom(ctx, name.as_ptr());
        let get = JS_NewCFunction2(
            ctx,
            getter as *const core::ffi::c_void,
            name.as_ptr(),
            0,
            CFUNC_GETTER,
            0,
        );
        let set = match setter {
            Some(setter) => JS_NewCFunction2(
                ctx,
                setter as *const core::ffi::c_void,
                name.as_ptr(),
                1,
                CFUNC_SETTER,
                0,
            ),
            None => UNDEFINED,
        };
        JS_DefinePropertyGetSet(
            ctx,
            proto,
            atom,
            get,
            set,
            PROP_CONFIGURABLE | PROP_ENUMERABLE,
        );
        JS_FreeAtom(ctx, atom);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_register_class(ctx: *mut JSContext, which: c_int) -> JSValue {
    unsafe {
        match which {
            CLASS_ELEMENT => {
                let proto = JS_NewObject(ctx);
                set_list(ctx, proto, &ELEMENT_PROTO_FUNCS);
                ns_define_element_unscopables(ctx, proto);
                ns_element_register_class(ctx, JS_DupValue(ctx, proto));
                proto
            }
            CLASS_ATTR => {
                ns_attr_register_class(ctx, JS_NewObject(ctx));
                UNDEFINED
            }
            CLASS_STYLE => {
                let proto = JS_NewObject(ctx);
                ns_cssom_install_style_proto(ctx, proto);
                set_tostring_tag(ctx, proto, c"CSSStyleDeclaration");
                ns_cssom_register_style_class(ctx, proto);
                UNDEFINED
            }
            CLASS_TOKEN_LIST => {
                let proto = JS_NewObject(ctx);
                set_list(ctx, proto, &TLIST_PROTO_FUNCS);
                ns_token_list_register_class(ctx, JS_DupValue(ctx, proto));
                proto
            }
            CLASS_LIVE => {
                ns_live_register_class(ctx);
                UNDEFINED
            }
            CLASS_DATASET => {
                ns_dataset_register_class(ctx, JS_NewObject(ctx));
                UNDEFINED
            }
            CLASS_STORAGE => {
                let proto = JS_NewObject(ctx);
                ns_storage_install_proto(ctx, proto);
                ns_storage_register_class(ctx, JS_DupValue(ctx, proto));
                proto
            }
            CLASS_WINDOW_NAMED => {
                ns_window_named_register_class(ctx);
                UNDEFINED
            }
            _ => UNDEFINED,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_install_dom_members(
    ctx: *mut JSContext,
    proto: JSValue,
    which: c_int,
) {
    unsafe {
        match which {
            0 => set_list(ctx, proto, &ELEMENT_PROTO_FUNCS),
            1 => {
                ns_element_register_class(ctx, JS_DupValue(ctx, proto));
                ns_bind_fn(ctx, proto, c"matches".as_ptr(), ns_element_matches, 1);
                ns_bind_fn(
                    ctx,
                    proto,
                    c"webkitMatchesSelector".as_ptr(),
                    ns_element_matches,
                    1,
                );
                ns_bind_fn(ctx, proto, c"closest".as_ptr(), ns_element_closest, 1);
            }
            2 => {
                define_getset(ctx, proto, c"dataset", ns_element_get_dataset, None);
                define_getset(
                    ctx,
                    proto,
                    c"popover",
                    ns_element_get_popover,
                    Some(ns_element_set_popover),
                );
            }
            3 => define_getset(ctx, proto, c"dataset", ns_element_get_dataset, None),
            4 => define_getset(
                ctx,
                proto,
                c"assignedSlot",
                ns_element_get_assignedSlot,
                Some(tables::ns_element_noop_set),
            ),
            5 => {
                ns_bind_fn(
                    ctx,
                    proto,
                    c"querySelector".as_ptr(),
                    ns_element_querySelector,
                    1,
                );
                ns_bind_fn(
                    ctx,
                    proto,
                    c"querySelectorAll".as_ptr(),
                    ns_element_querySelectorAll,
                    1,
                );
            }
            6 => set_list(ctx, proto, &TRACK_ACCESSORS),
            7 => define_getset(
                ctx,
                proto,
                c"closedBy",
                ns_dialog_get_closedBy,
                Some(ns_dialog_set_closedBy),
            ),
            8 => {
                define_getset(
                    ctx,
                    proto,
                    c"command",
                    ns_button_get_command,
                    Some(ns_button_set_command),
                );
                define_getset(
                    ctx,
                    proto,
                    c"commandForElement",
                    ns_button_get_commandForElement,
                    Some(ns_button_set_commandForElement),
                );
            }
            9 => define_getset(
                ctx,
                proto,
                c"text",
                ns_element_get_text,
                Some(ns_element_set_text),
            ),
            10 => define_getset(
                ctx,
                proto,
                c"data",
                ns_element_get_data,
                Some(ns_element_set_data),
            ),
            11 => define_getset(
                ctx,
                proto,
                c"value",
                ns_element_get_value_prop,
                Some(ns_element_set_value_prop),
            ),
            12 => {
                let item = JS_NewCFunction2(
                    ctx,
                    ns_options_item as *const core::ffi::c_void,
                    c"item".as_ptr(),
                    1,
                    0,
                    0,
                );
                JS_DefinePropertyValueStr(
                    ctx,
                    proto,
                    c"item".as_ptr(),
                    item,
                    PROP_CONFIGURABLE | PROP_WRITABLE,
                );
                let named = JS_NewCFunction2(
                    ctx,
                    ns_options_namedItem as *const core::ffi::c_void,
                    c"namedItem".as_ptr(),
                    1,
                    0,
                    0,
                );
                JS_DefinePropertyValueStr(
                    ctx,
                    proto,
                    c"namedItem".as_ptr(),
                    named,
                    PROP_CONFIGURABLE | PROP_WRITABLE,
                );
            }
            _ => {}
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_install_funcs(ctx: *mut JSContext, doc: JSValue) {
    unsafe {
        set_list(ctx, doc, &DOCUMENT_FUNCS);
        ns_install_event_handler_props(ctx, doc);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_install_proto_table(
    ctx: *mut JSContext,
    proto: JSValue,
    table: c_int,
) {
    let list: &'static [FunctionListEntry] = match table {
        0 => &DOCUMENT_PROTO_METHODS,
        1 => &DOCUMENT_PROTO_ACCESSORS,
        2 => &NAMEDMAP_PROTO_FUNCS,
        3 => &SRC_PROTO_FUNCS,
        4 => &IMAGE_PROTO_FUNCS,
        5 => &SOURCE_PROTO_FUNCS,
        6 => &MEDIA_PROTO_FUNCS,
        7 => &VIDEO_PROTO_FUNCS,
        8 => &IFRAME_PROTO_FUNCS,
        9 => &ANCHOR_PROTO_FUNCS,
        10 => &HREF_PROTO_FUNCS,
        11 => &DOM_IMPLEMENTATION_PROTO_FUNCS,
        12 => &VALIDITY_PROTO_FUNCS,
        13 => &ELEMENT_PROTO_FUNCS,
        _ => return,
    };
    unsafe { set_list(ctx, proto, list) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_host_class_proto(ctx: *mut JSContext, which: c_int) -> JSValue {
    unsafe {
        match which {
            HOST_PROTO_STYLE => ns_style_class_proto(ctx),
            HOST_PROTO_DATASET => ns_dataset_class_proto(ctx),
            HOST_PROTO_TOKEN_LIST => ns_token_list_class_proto(ctx),
            _ => UNDEFINED,
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_brand_node_classes(ctx: *mut JSContext) {
    unsafe { ns_js_brand_node_interfaces(ctx, ns_element_class_id(), ns_attr_class_id()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_link_interfaces(ctx: *mut JSContext) {
    const EVAL_TYPE_GLOBAL: c_int = 0;
    const EVAL_FLAG_HIDE_SOURCE: c_int = 1 << 8;
    const EXCEPTION_TAG: i64 = 6;
    let mut source = crate::sources::INTERFACE_LINKS.as_bytes().to_vec();
    source.push(0);
    unsafe {
        let result = JS_Eval(
            ctx,
            source.as_ptr().cast(),
            source.len() - 1,
            c"<interface-links>".as_ptr(),
            EVAL_TYPE_GLOBAL | EVAL_FLAG_HIDE_SOURCE,
        );
        if quickjs::raw_tag(result) == EXCEPTION_TAG {
            JS_FreeValue(ctx, JS_GetException(ctx));
        }
        JS_FreeValue(ctx, result);
    }
}

#[unsafe(no_mangle)]
pub fn nsr_js_register_class(
    scope: &mut southstar_js_engine::Scope<'_>,
    which: c_int,
) -> southstar_js_engine::Value {
    let raw = unsafe { ns_js_register_class(quickjs::raw_context(scope), which) };
    unsafe { quickjs::take_value(scope, raw) }
}

#[unsafe(no_mangle)]
pub fn nsr_js_install_proto_table(
    scope: &mut southstar_js_engine::Scope<'_>,
    proto: &southstar_js_engine::Value,
    table: c_int,
) {
    unsafe { ns_js_install_proto_table(quickjs::raw_context(scope), quickjs::raw(proto), table) };
}
