//! Southstar — the realm record's typed Rust API for binding crates on the neutral engine layer, which may depend on this crate directly.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_js_engine::{BufferView, ContextHandle, HostClass, Scope, TaskFn, Value};

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

pub use crate::ffi::record::AdoptedProtos;
pub use crate::ffi::{Callbacks as PageCallbacks, NavigationTiming, NsJs, real_time_us};
pub use enter::*;
pub use hooks::*;
pub use hooks_i::*;
pub use hooks_state::*;
pub use hooks_tree::*;
pub use nodes::*;
pub use page::*;
pub use tasks::*;

const BODY_BYTES_MAX: usize = 32 * 1024 * 1024;

pub fn bytes_view(scope: &mut Scope<'_>, value: &Value) -> Option<BufferView> {
    glue::bytes_view(scope, value)
}

pub fn bytes_view_copy(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    glue::bytes_view(scope, value).map(|view| view.bytes().to_vec())
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
        let bytes = view.bytes();
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

pub(crate) fn queue_task(js: Js, ctx: ContextHandle, run: TaskRun) {
    let task = glue::message_task(ctx, run);
    if scheduling::queue_message_task(js, task) {
        ffi::exports::schedule_message_tasks(js, 0);
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

pub fn this_is_detached_window(js: Option<Page>, scope: &mut Scope<'_>, this: &Value) -> bool {
    let Some(js) = Js::of(js) else {
        return false;
    };
    if !this.is_object() {
        return false;
    }
    let doc = match scope.get(this, "document") {
        Ok(doc) => glue::unwrap_element(&doc),
        Err(_) => None,
    };
    let Some(doc) = doc else {
        return false;
    };
    if Some(doc) == js.current_doc() || glue::is_detached_document(doc) {
        return false;
    }
    let top = scopes::top_document(js.current_doc());
    top.is_some() && scopes::top_document(Some(doc)) != top
}

pub fn call_observer(
    js: Option<Page>,
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
            .set(southstar_glib::monotonic_us() + budget::eval_budget_us());
    }
    let result = glue::call_observer(js, ctx, cb, this, args, report_type);
    if let Some(js) = js
        && fresh_budget
    {
        js.st().eval_deadline_us.set(saved);
    }
    result
}

pub fn dispatch_document_window_event(js: Option<Page>, kind: &str, event: Value) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let kind = glue::c_line(kind.as_bytes());
    glue::dispatch_window_only_event(js, js.current_doc(), &kind.to_string_lossy(), event);
}

pub fn iframe_document(js: Option<Page>) -> Option<Value> {
    Js::of(js)?.st().iframe_doc.borrow().clone()
}

pub fn array_length(scope: &mut Scope<'_>, arr: &Value) -> u32 {
    record::array_length(scope, arr)
}

pub fn kind_proto(js: Option<Page>, node: Option<Node<'_>>) -> Option<Value> {
    record::kind_proto_value(js, node)
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
    js: Option<Page>,
    frame: ContextHandle,
    frame_global: &Value,
    parent_global: &Value,
) {
    if let Some(js) = js {
        record::frame_realm_finish(js, frame, frame_global, parent_global);
    }
}

pub fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> Option<Node<'static>> {
    record::document_root_for(scope, this)
}

pub fn node_wrapper(scope: &mut Scope<'_>, node: Node<'_>) -> Option<Value> {
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

pub fn navigator_brand(js: Option<Page>) -> Value {
    record::navigator_brand(js)
}

pub fn pristine_promise(js: Option<Page>) -> Value {
    record::pristine_promise(js)
}

pub fn register_element_class(scope: &mut Scope<'_>, proto: Value) {
    wrapper::register_element_class(scope, proto);
}

pub fn element_class() -> &'static HostClass {
    &wrapper::ELEMENT_CLASS
}

pub fn make_element(scope: &mut Scope<'_>, node: Option<Node<'_>>) -> Result<Value, Value> {
    wrapper::make_element(scope, node)
}

pub fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    wrapper::unwrap(value)
}

pub fn new_document_object(js: Option<Page>, scope: &mut Scope<'_>) -> Result<Value, Value> {
    match Js::of(js) {
        Some(page) => wrapper::new_document_object(page, scope),
        None => Ok(Value::undefined()),
    }
}

pub fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    scope.dom_exception_code(name.as_bytes(), code, message.as_bytes())
}

pub fn abort_error(scope: &mut Scope<'_>) -> Value {
    ffi::imports::nsr_make_abort_error(scope)
}
