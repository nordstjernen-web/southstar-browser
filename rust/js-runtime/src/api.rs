//! Southstar — the realm record's typed Rust API for binding crates on the neutral engine layer, which may depend on this crate directly.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_js_engine::{ContextHandle, HostClass, Scope, TaskFn, Value};

use crate::budget;
use crate::ffi::state::TaskRun;
use crate::ffi::{self, Js, glue, record, scopes, wrapper};
use crate::scheduling;

mod enter;
mod hooks;
mod hooks_i;
mod hooks_state;
mod hooks_tree;
mod nodes;
mod page;
mod tasks;

pub use crate::ffi::glue::BytesView;
pub use crate::ffi::record::AdoptedProtos;
pub use crate::ffi::{Callbacks as PageCallbacks, NavigationTiming, NsJs};
pub use enter::*;
pub use hooks::*;
pub use hooks_i::*;
pub use hooks_state::*;
pub use hooks_tree::*;
pub use nodes::*;
pub use page::*;
pub use tasks::*;

const BODY_BYTES_MAX: usize = 32 * 1024 * 1024;

pub fn bytes_view(scope: &mut Scope<'_>, value: &Value) -> Option<BytesView> {
    glue::bytes_view(scope, value)
}

pub fn body_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    if value.is_undefined() || value.is_null() {
        return None;
    }
    if value.is_string() {
        return scope
            .to_bytes(value)
            .ok()
            .filter(|b| b.len() <= BODY_BYTES_MAX);
    }
    if let Some(view) = glue::bytes_view(scope, value) {
        let bytes = glue::bytes_of(&view);
        return (bytes.len() <= BODY_BYTES_MAX).then(|| bytes.to_vec());
    }
    if !value.is_object() {
        return None;
    }
    match scope.get(value, "__ndBlobBytes") {
        Ok(bytes) if !bytes.is_undefined() && !bytes.is_null() => {
            Some(glue::blob_bytes(scope, value))
        }
        _ => None,
    }
}

pub fn is_url_search_params(scope: &mut Scope<'_>, value: &Value) -> bool {
    if !value.is_object() || scope.is_function(value) {
        return false;
    }
    let pairs = scope
        .get(value, "__ndPairs")
        .unwrap_or_else(|_| Value::undefined());
    if !pairs.is_array() {
        return false;
    }
    let append = scope
        .get(value, "append")
        .unwrap_or_else(|_| Value::undefined());
    let get_all = scope
        .get(value, "getAll")
        .unwrap_or_else(|_| Value::undefined());
    scope.is_function(&append) && scope.is_function(&get_all)
}

pub fn usp_serialize(scope: &mut Scope<'_>, usp: &Value) -> Vec<u8> {
    let to_string = scope
        .get(usp, "toString")
        .unwrap_or_else(|_| Value::undefined());
    let text = if scope.is_function(&to_string) {
        scope.call(&to_string, usp, &[])
    } else {
        Ok(scope.string(""))
    };
    let bytes = text
        .map(|text| scope.to_bytes(&text).unwrap_or_default())
        .unwrap_or_default();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..end].to_vec()
}

pub fn reject_with_message(scope: &mut Scope<'_>, reject: &Value, message: &[u8]) {
    let name_len = message
        .iter()
        .position(|&b| b == b':')
        .unwrap_or(message.len());
    let colon = name_len < message.len();
    let is_dom_name = name_len > 5
        && name_len < 64
        && message[name_len - 5..name_len] == *b"Error"
        && message[..name_len].iter().all(u8::is_ascii_alphanumeric);
    let error = scope.new_error();
    let lossy = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    if is_dom_name {
        let name = scope.string(&lossy(&message[..name_len]));
        let _ = scope.set(&error, "name", name);
        let mut rest = if colon {
            &message[name_len + 1..]
        } else {
            message
        };
        while let Some((b' ', tail)) = rest.split_first() {
            rest = tail;
        }
        let text = scope.string(&lossy(rest));
        let _ = scope.set(&error, "message", text);
    } else {
        let text = scope.string(&lossy(message));
        let _ = scope.set(&error, "message", text);
    }
    let _ = scope.call(reject, &Value::undefined(), &[error]);
}

pub(crate) fn queue_task(js: Js, ctx: ContextHandle, run: TaskRun) {
    let task = glue::message_task(ctx, run);
    if scheduling::queue_message_task(js, task) {
        let source = glue::attach_timeout(
            Some(js),
            0,
            ffi::exports::run_message_tasks,
            js.ptr().cast(),
        );
        js.st().message_task_source.set(source);
    }
}

pub fn queue_message_task(scope: &mut Scope<'_>, f: TaskFn, args: &[Value]) -> Result<(), Value> {
    let ctx = scope.handle();
    let Some(js) = Js::of_scope(scope).filter(|_| args.len() <= 3) else {
        return scope.enqueue_native(f, args);
    };
    let args = args.to_vec();
    queue_task(js, ctx, Box::new(move |scope| f(scope, &args)));
    Ok(())
}

pub fn this_is_detached_window(js: *const NsJs, scope: &mut Scope<'_>, this: &Value) -> bool {
    let Some(js) = Js::of(js) else {
        return false;
    };
    if !this.is_object() {
        return false;
    }
    let doc = match scope.get(this, "document") {
        Ok(doc) => glue::unwrap_element(&doc),
        Err(_) => ptr::null(),
    };
    if doc.is_null() || doc == js.current_doc().cast_const() || glue::is_detached_document(doc) {
        return false;
    }
    let top = scopes::top_document(js.current_doc());
    !top.is_null() && scopes::top_document(doc.cast_mut()) != top
}

pub fn call_observer(
    js: *const NsJs,
    ctx: ContextHandle,
    cb: &Value,
    this: &Value,
    args: &[Value],
    report_type: Option<&str>,
    fresh_budget: bool,
) -> Result<Value, Value> {
    let js = Js::of(js);
    let saved = js.map_or(0, |js| js.st().eval_deadline_us.get());
    if let Some(js) = js
        && fresh_budget
    {
        js.st()
            .eval_deadline_us
            .set(ffi::monotonic_us() + budget::eval_budget_us());
    }
    let result = glue::call_observer(js, ctx, cb, this, args, report_type);
    if let Some(js) = js
        && fresh_budget
    {
        js.st().eval_deadline_us.set(saved);
    }
    result
}

pub fn dispatch_document_window_event(js: *const NsJs, kind: &str, event: Value) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let kind = glue::c_line(kind.as_bytes());
    glue::dispatch_window_only_event(js, js.current_doc(), kind.as_ptr(), event);
}

pub fn iframe_document(js: *const NsJs) -> Option<Value> {
    Js::of(js)?.st().iframe_doc.borrow().clone()
}

pub fn array_length(scope: &mut Scope<'_>, arr: &Value) -> u32 {
    record::array_length(scope, arr)
}

pub unsafe fn kind_proto(js: *const NsJs, node: *const NsNode) -> Option<Value> {
    unsafe { record::kind_proto_value(js, node) }
}

pub fn bind_event_target_listeners(scope: &mut Scope<'_>, obj: &Value) {
    record::bind_event_target_listeners(scope, obj);
}

pub fn attr_apply_proto(scope: &mut Scope<'_>, obj: &Value) {
    record::attr_apply_proto(scope, obj);
}

pub fn build_attributes(scope: &mut Scope<'_>, owner: &Value) -> Value {
    record::build_attributes(scope, owner)
}

pub fn frame_realm_finish(
    js: *const NsJs,
    frame: ContextHandle,
    frame_global: &Value,
    parent_global: &Value,
) {
    if !js.is_null() {
        unsafe { record::frame_realm_finish(js.cast_mut(), frame, frame_global, parent_global) };
    }
}

pub fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> *mut NsNode {
    record::document_root_for(scope, this)
}

pub fn node_wrapper(scope: &mut Scope<'_>, node: *const NsNode) -> Option<Value> {
    record::node_wrapper(scope, node)
}

pub fn new_orphan_element(scope: &mut Scope<'_>, name: &[u8]) -> Result<Value, Value> {
    record::new_orphan_element(scope, name)
}

pub fn illegal_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    record::illegal_constructor(scope, this, args)
}

pub fn register_tag_proto(scope: &mut Scope<'_>, tag: &[u8], proto: Value) {
    record::register_tag_proto(scope, tag, proto);
}

pub fn adopt_dom_protos(scope: &mut Scope<'_>, protos: AdoptedProtos) {
    record::adopt_dom_protos(scope, protos);
}

pub fn tag_caller_document(scope: &mut Scope<'_>, node: &Value) {
    record::tag_caller_document(scope, node);
}

pub fn exception_message(scope: &mut Scope<'_>, exception: &Value) -> Vec<u8> {
    record::exception_message(scope, exception)
}

pub fn navigator_brand(js: *const NsJs) -> Value {
    record::navigator_brand(js)
}

pub fn pristine_promise(js: *const NsJs) -> Value {
    record::pristine_promise(js)
}

pub fn register_element_class(scope: &mut Scope<'_>, proto: Value) {
    wrapper::register_element_class(scope, proto);
}

pub fn element_class() -> &'static HostClass {
    &wrapper::ELEMENT_CLASS
}

pub fn make_element(scope: &mut Scope<'_>, node: *const NsNode) -> Result<Value, Value> {
    wrapper::make_element(scope, node)
}

pub fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    wrapper::unwrap(value)
}

pub fn new_document_object(js: *const NsJs, scope: &mut Scope<'_>) -> Result<Value, Value> {
    match Js::of(js) {
        Some(page) => wrapper::new_document_object(page, scope),
        None => Ok(Value::undefined()),
    }
}

pub fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    ffi::imports::nsr_dom_exception(scope, name.as_bytes(), code, message.as_bytes())
}

pub fn abort_error(scope: &mut Scope<'_>) -> Value {
    ffi::imports::nsr_make_abort_error(scope)
}
