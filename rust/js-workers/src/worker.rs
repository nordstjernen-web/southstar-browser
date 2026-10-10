//! Southstar — dedicated workers: the host shared between a Worker and its thread, the messages they exchange, ports bridged between the two runtimes, and the worker thread itself.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use southstar_js_engine::{ObjectKind, Scope, Value};

use crate::ffi::{self, Ctx, Js, MainContext};
use crate::{JsResult, PLAIN, arg, flag, get, get_index, length, ports, push, set, set_str};

const SCRIPT_BYTES_MAX: usize = 32 * 1024 * 1024;
const MESSAGE_BYTES_MAX: usize = 16 * 1024 * 1024;
const THREAD_STACK: usize = 8 * 1024 * 1024;
const BRIDGE_REGISTRY: &str = "__ns_port_bridge";

#[derive(Default)]
pub(crate) struct Inner {
    pub worker_js: Js,
    pub main_loop: usize,
    thread: Option<JoinHandle<()>>,
    joined: bool,
}

pub(crate) struct Host {
    pub url: String,
    pub base_url: CString,
    pub name: String,
    pub origin: String,
    pub inline_script: Option<String>,
    pub is_module: bool,
    pub is_service_worker: bool,
    pub scope: Option<String>,
    pub closing: AtomicBool,
    pub sw_active: AtomicBool,
    pub terminated: AtomicBool,
    pub owner_alive: AtomicBool,
    pub context: MainContext,
    inner: Mutex<Inner>,
}

pub(crate) struct HostSpec {
    pub url: String,
    pub base_url: String,
    pub name: String,
    pub origin: String,
    pub inline_script: Option<String>,
    pub is_module: bool,
    pub is_service_worker: bool,
    pub scope: Option<String>,
}

impl Host {
    pub(crate) fn new(spec: HostSpec) -> Arc<Host> {
        Arc::new(Host {
            url: spec.url,
            base_url: CString::new(spec.base_url).unwrap_or_default(),
            name: spec.name,
            origin: spec.origin,
            inline_script: spec.inline_script,
            is_module: spec.is_module,
            is_service_worker: spec.is_service_worker,
            scope: spec.scope,
            closing: AtomicBool::new(false),
            sw_active: AtomicBool::new(false),
            terminated: AtomicBool::new(false),
            owner_alive: AtomicBool::new(true),
            context: MainContext::new(),
            inner: Mutex::new(Inner::default()),
        })
    }

    pub(crate) fn base_url(&self) -> &str {
        self.base_url.to_str().unwrap_or("")
    }

    pub(crate) fn closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }

    pub(crate) fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn worker_js(&self) -> Js {
        self.inner().worker_js
    }
}

#[derive(Clone)]
pub(crate) struct WorkerRef(pub Option<Arc<Host>>);

pub(crate) struct Owned {
    pub host: Arc<Host>,
    pub js: Js,
    pub ctx: Ctx,
    pub object: Value,
}

thread_local! {
    static OWNED: RefCell<Vec<Owned>> = const { RefCell::new(Vec::new()) };
    static IN_ERROR_REPORT: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn adopt(owned: Owned) {
    OWNED.with(|list| list.borrow_mut().push(owned));
}

pub(crate) fn owner_of(host: &Arc<Host>) -> Option<(Js, Ctx, Value)> {
    OWNED.with(|list| {
        list.borrow()
            .iter()
            .find(|o| Arc::ptr_eq(&o.host, host))
            .map(|o| (o.js, o.ctx, o.object.clone()))
    })
}

pub(crate) fn hosts_of(js: Js) -> Vec<Arc<Host>> {
    OWNED.with(|list| {
        list.borrow()
            .iter()
            .filter(|o| o.js == js)
            .map(|o| o.host.clone())
            .collect()
    })
}

pub(crate) fn teardown(js: Js) {
    let hosts = hosts_of(js);
    for host in &hosts {
        host.owner_alive.store(false, Ordering::SeqCst);
        stop(host, true);
    }
    let removed: Vec<Owned> = OWNED.with(|list| {
        let mut list = list.borrow_mut();
        let (gone, kept) = list.drain(..).partition(|o| o.js == js);
        *list = kept;
        gone
    });
    drop(removed);
}

pub(crate) fn pending(js: Js) -> bool {
    hosts_of(js)
        .iter()
        .any(|h| !h.is_service_worker && !h.closing())
}

pub(crate) fn request_quit(host: &Arc<Host>) {
    let target = host.clone();
    host.context.invoke(move || {
        let inner = target.inner();
        if !inner.worker_js.is_null() {
            ffi::halt(inner.worker_js);
        }
        if inner.main_loop != 0 {
            ffi::quit_loop(inner.main_loop);
        }
    });
}

pub(crate) fn stop(host: &Arc<Host>, join: bool) {
    host.closing.store(true, Ordering::SeqCst);
    host.sw_active.store(false, Ordering::SeqCst);
    request_quit(host);
    if !join {
        return;
    }
    let thread = {
        let mut inner = host.inner();
        if inner.joined {
            None
        } else {
            inner.joined = true;
            inner.thread.take()
        }
    };
    if let Some(thread) = thread
        && thread.thread().id() != std::thread::current().id()
    {
        let _ = thread.join();
    }
}

fn drain_context(host: &Host) {
    host.context.drain(1024);
}

pub(crate) enum Body {
    Undefined,
    Bytes(Vec<u8>),
    Error(ErrorReport),
    SwState(&'static str),
}

pub(crate) struct ErrorReport {
    pub message: String,
    pub filename: String,
    pub lineno: i32,
    pub colno: i32,
    pub load_error: bool,
}

pub(crate) struct Message {
    pub host: Arc<Host>,
    pub body: Body,
    pub port_id: u64,
    pub transferred: Vec<u64>,
    pub followups: Vec<Message>,
}

impl Message {
    fn of(host: &Arc<Host>, body: Body) -> Message {
        Message {
            host: host.clone(),
            body,
            port_id: 0,
            transferred: Vec::new(),
            followups: Vec::new(),
        }
    }
}

pub(crate) fn data_clone_error(scope: &mut Scope<'_>, message: &core::ffi::CStr) -> Value {
    ffi::dom_exception(scope, c"DataCloneError", 25, message)
}

fn message_new(
    scope: &mut Scope<'_>,
    host: &Arc<Host>,
    value: &Value,
    ports: &Value,
) -> Result<Message, Value> {
    if value.is_undefined() {
        return Ok(Message::of(host, Body::Undefined));
    }
    let wire = southstar_js_clone::wire::encode_value(scope, value, ports)?;
    let Ok(bytes) = scope.write_object(&wire) else {
        return Err(data_clone_error(scope, c"value could not be cloned."));
    };
    if bytes.len() > MESSAGE_BYTES_MAX {
        return Err(data_clone_error(
            scope,
            c"postMessage: message is too large",
        ));
    }
    Ok(Message::of(host, Body::Bytes(bytes)))
}

fn message_value(scope: &mut Scope<'_>, message: &Message, ports: &Value) -> JsResult {
    match &message.body {
        Body::Bytes(bytes) if !bytes.is_empty() => {
            let wire = scope.read_object(bytes)?;
            southstar_js_clone::wire::decode_value(scope, &wire, ports)
        }
        Body::Bytes(_) => Ok(Value::null()),
        _ => Ok(Value::undefined()),
    }
}

fn alloc_bridge_id() -> u64 {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    u64::from(COUNTER.fetch_add(1, Ordering::SeqCst)) + 1
}

fn bridge_registry(scope: &mut Scope<'_>) -> Value {
    let global = scope.global();
    let registry = get(scope, &global, BRIDGE_REGISTRY);
    if registry.is_object() {
        return registry;
    }
    let registry = scope.new_object();
    set(scope, &global, BRIDGE_REGISTRY, registry.clone());
    registry
}

fn bridge_register(scope: &mut Scope<'_>, id: u64, port: &Value) {
    let registry = bridge_registry(scope);
    set(scope, &registry, &id.to_string(), port.clone());
}

fn bridge_lookup(scope: &mut Scope<'_>, id: u64) -> Value {
    let registry = bridge_registry(scope);
    get(scope, &registry, &id.to_string())
}

fn transfer_port(
    scope: &mut Scope<'_>,
    port: &Value,
    pair: &Value,
    message: &mut Message,
    bridge_worker: &Value,
) {
    let id = alloc_bridge_id();
    set(scope, pair, "_bridge_id", Value::number(id as f64));
    if bridge_worker.is_object() {
        set(scope, pair, "_bridge_worker", bridge_worker.clone());
    }
    bridge_register(scope, id, pair);
    set(scope, pair, "_pair", Value::null());
    let queue = get(scope, port, "_queue");
    if queue.is_array() {
        for i in 0..length(scope, &queue) {
            let mut item = get_index(scope, &queue, i);
            if item.is_object() && flag(scope, &item, "_portMessage") {
                item = get(scope, &item, "data");
            }
            if let Ok(mut followup) = message_new(scope, &message.host, &item, &Value::undefined())
            {
                followup.port_id = id;
                message.followups.push(followup);
            }
        }
    }
    let fresh = scope.new_array();
    set(scope, port, "_queue", fresh);
    set(scope, port, "_closed", Value::boolean(true));
    set(scope, port, "_pair", Value::null());
    message.transferred.push(id);
}

fn bridge_goes_home(scope: &mut Scope<'_>, port: &Value, target_worker: &Value) -> bool {
    if ports::bridge_id(scope, port) == 0 {
        return false;
    }
    if ffi::worker_host_of(ffi::js_of(scope)).is_some() {
        return target_worker.is_undefined();
    }
    let worker = get(scope, port, "_bridge_worker");
    worker.is_object() && target_worker.is_object() && worker.same_object(target_worker)
}

fn return_port(scope: &mut Scope<'_>, port: &Value, message: &mut Message) {
    let id = ports::bridge_id(scope, port);
    let registry = bridge_registry(scope);
    let _ = scope.delete(&registry, &id.to_string());
    set(scope, port, "_bridge_id", Value::undefined());
    set(scope, port, "_bridge_worker", Value::undefined());
    set(scope, port, "_closed", Value::boolean(true));
    set(scope, port, "_shipped", Value::boolean(true));
    message.transferred.push(id);
}

fn transfer_list(scope: &mut Scope<'_>, args: &[Value], arrays_only: bool) -> Option<Value> {
    let options = args.get(1)?;
    if options.is_array() {
        return Some(options.clone());
    }
    if !options.is_object() {
        return None;
    }
    let transfer = get(scope, options, "transfer");
    (transfer.is_array() || !arrays_only).then_some(transfer)
}

fn walk_transfers(
    scope: &mut Scope<'_>,
    args: &[Value],
    mut message: Option<&mut Message>,
    bridge_worker: &Value,
) -> bool {
    let detach = message.is_some();
    let options = arg(args, 1);
    if options.is_undefined() || options.is_null() {
        return false;
    }
    let Some(transfer) = transfer_list(scope, args, true) else {
        return false;
    };
    let mut nontransferable = false;
    let mut seen: Vec<Value> = Vec::new();
    for i in 0..length(scope, &transfer) {
        let item = get_index(scope, &transfer, i);
        let is_buffer = matches!(scope.object_kind(&item), Some(ObjectKind::ArrayBuffer));
        if !detach && item.is_object() {
            let detached = is_buffer && flag(scope, &item, "detached");
            if seen.iter().any(|s| s.same_object(&item)) || detached {
                nontransferable = true;
            }
            seen.push(item.clone());
        }
        if is_buffer {
            if detach {
                let _ = scope.detach_array_buffer(&item);
            }
        } else if ffi::is_message_port(&item) {
            let pair = get(scope, &item, "_pair");
            if pair.is_object() {
                if let Some(message) = message.as_deref_mut() {
                    transfer_port(scope, &item, &pair, message, bridge_worker);
                }
            } else if bridge_goes_home(scope, &item, bridge_worker) {
                if let Some(message) = message.as_deref_mut() {
                    return_port(scope, &item, message);
                }
            } else {
                nontransferable = true;
            }
        } else {
            nontransferable = true;
        }
    }
    nontransferable
}

fn transfer_ports(scope: &mut Scope<'_>, args: &[Value]) -> Value {
    let ports = scope.new_array();
    if !arg(args, 1).is_object() {
        return ports;
    }
    let Some(transfer) = transfer_list(scope, args, false) else {
        return ports;
    };
    if !transfer.is_array() {
        return ports;
    }
    for i in 0..length(scope, &transfer) {
        let item = get_index(scope, &transfer, i);
        if ffi::is_message_port(&item) {
            push(scope, &ports, item);
        }
    }
    ports
}

pub(crate) fn bridge_deliver(scope: &mut Scope<'_>, id: u64, data: &Value) {
    let port = bridge_lookup(scope, id);
    if !port.is_object() || flag(scope, &port, "_closed") {
        return;
    }
    if flag(scope, &port, "_started") {
        ports::deliver(scope, &port, data, None);
        return;
    }
    let mut queue = get(scope, &port, "_queue");
    if !queue.is_array() {
        queue = scope.new_array();
        set(scope, &port, "_queue", queue.clone());
    }
    push(scope, &queue, data.clone());
}

fn bridge_receive(scope: &mut Scope<'_>, id: u64, worker: &Value) -> Value {
    let other = bridge_lookup(scope, id);
    let port = ports::new_port(scope).unwrap_or_else(|_| Value::undefined());
    if other.is_object() {
        set(scope, &port, "_pair", other.clone());
        set(scope, &other, "_pair", port.clone());
        set(scope, &other, "_bridge_id", Value::undefined());
        set(scope, &other, "_bridge_worker", Value::undefined());
    } else {
        set(scope, &port, "_bridge_id", Value::number(id as f64));
        if worker.is_object() {
            set(scope, &port, "_bridge_worker", worker.clone());
        }
    }
    bridge_register(scope, id, &port);
    port
}

fn message_ports(scope: &mut Scope<'_>, message: &Message, worker: &Value) -> Value {
    let ports = scope.new_array();
    for (i, &id) in message.transferred.iter().enumerate() {
        let port = bridge_receive(scope, id, worker);
        crate::set_index(scope, &ports, i as u32, port);
    }
    ports
}

pub(crate) fn worker_event(
    scope: &mut Scope<'_>,
    kind: &str,
    data: &Value,
    origin: &str,
    message: &str,
    filename: &str,
) -> Value {
    let event = ffi::event_new(scope);
    set_str(scope, &event, "type", kind);
    set(scope, &event, "data", data.clone());
    set_str(scope, &event, "origin", origin);
    set_str(scope, &event, "lastEventId", "");
    set(scope, &event, "source", Value::null());
    let ports = scope.new_array();
    set(scope, &event, "ports", ports);
    set_str(scope, &event, "message", message);
    set_str(scope, &event, "filename", filename);
    set(scope, &event, "lineno", Value::int(0));
    set(scope, &event, "colno", Value::int(0));
    set(scope, &event, "error", Value::null());
    set(scope, &event, "defaultPrevented", Value::boolean(false));
    set(scope, &event, "bubbles", Value::boolean(false));
    set(scope, &event, "cancelable", Value::boolean(false));
    set(scope, &event, "composed", Value::boolean(false));
    ffi::bind_c(scope, &event, "preventDefault", 0, ffi::CFn::PreventDefault);
    ffi::bind_c(
        scope,
        &event,
        "stopPropagation",
        0,
        ffi::CFn::StopPropagation,
    );
    ffi::define_cancel_bubble(scope, &event);
    set(scope, &event, "_is_trusted", Value::boolean(true));
    ffi::bind_c(
        scope,
        &event,
        "stopImmediatePropagation",
        0,
        ffi::CFn::StopImmediate,
    );
    ffi::bind_c(scope, &event, "composedPath", 0, ffi::CFn::ComposedPath);
    event
}

fn shape_error_event(scope: &mut Scope<'_>, event: &Value, report: &ErrorReport) {
    let global = scope.global();
    let ctor = get(
        scope,
        &global,
        if report.load_error {
            "Event"
        } else {
            "ErrorEvent"
        },
    );
    let proto = if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    };
    if proto.is_object() {
        let _ = scope.set_prototype(event, &proto);
    }
    if report.load_error {
        for key in [
            "message",
            "filename",
            "lineno",
            "colno",
            "error",
            "data",
            "origin",
            "lastEventId",
            "source",
            "ports",
        ] {
            let _ = scope.delete(event, key);
        }
        return;
    }
    set(scope, event, "cancelable", Value::boolean(true));
    set(scope, event, "lineno", Value::int(report.lineno));
    set(scope, event, "colno", Value::int(report.colno));
    for key in ["data", "origin", "lastEventId", "source", "ports"] {
        let _ = scope.delete(event, key);
    }
}

fn freeze(scope: &mut Scope<'_>, array: &Value) {
    let _ = scope.freeze(array);
}

fn decode(scope: &mut Scope<'_>, message: &Message, ports: &Value) -> (&'static str, Value) {
    match message_value(scope, message, ports) {
        Ok(data) => ("message", data),
        Err(_) => ("messageerror", Value::undefined()),
    }
}

fn deliver_owner(message: Message) {
    let host = message.host.clone();
    let Some((js, ctx, owner)) = owner_of(&host) else {
        return;
    };
    let is_state = matches!(message.body, Body::SwState(_));
    if !host.owner_alive.load(Ordering::SeqCst)
        || (host.terminated.load(Ordering::SeqCst) && !is_state)
    {
        return;
    }
    ffi::with_ctx(ctx, |scope| {
        let budget = ffi::Budget::enter(scope);
        if let Body::SwState(state) = message.body {
            crate::service::apply_state(scope, &host, &owner, state);
            ffi::drain_mutations(js);
            drop(budget);
            return;
        }
        let report = match &message.body {
            Body::Error(report) => Some(report),
            _ => None,
        };
        let ports = if report.is_some() || message.port_id != 0 {
            scope.new_array()
        } else {
            message_ports(scope, &message, &owner)
        };
        let (kind, data) = if report.is_some() {
            ("error", Value::undefined())
        } else {
            decode(scope, &message, &ports)
        };
        if message.port_id != 0 {
            bridge_deliver(scope, message.port_id, &data);
            ffi::drain_mutations(js);
            drop(budget);
            return;
        }
        let origin = if host.is_service_worker {
            host.origin.as_str()
        } else {
            ""
        };
        let (text, filename) = report
            .map(|r| (r.message.as_str(), r.filename.as_str()))
            .unwrap_or(("", ""));
        let event = worker_event(scope, kind, &data, origin, text, filename);
        freeze(scope, &ports);
        set(scope, &event, "ports", ports);
        if let Some(report) = report {
            shape_error_event(scope, &event, report);
        }
        let mut target = owner.clone();
        if host.is_service_worker && report.is_none() {
            let global = scope.global();
            let navigator = get(scope, &global, "navigator");
            let container = get(scope, &navigator, "serviceWorker");
            if container.is_object() {
                set(scope, &event, "source", owner.clone());
                target = container;
            }
        }
        ffi::dispatch_engine_event(scope, &target, &event);
        if let Some(report) = report.filter(|r| !r.load_error)
            && !flag(scope, &event, "defaultPrevented")
        {
            ffi::report_error_event_in(js, ctx, report);
        }
        ffi::drain_mutations(js);
        drop(budget);
    });
}

pub(crate) fn post_owner(message: Message) {
    ffi::invoke_default(move || deliver_owner(message));
}

pub(crate) fn post_owner_error(
    host: &Arc<Host>,
    message: &str,
    filename: &str,
    lineno: i32,
    colno: i32,
    load_error: bool,
) {
    let report = ErrorReport {
        message: message.to_owned(),
        filename: filename.to_owned(),
        lineno,
        colno,
        load_error,
    };
    post_owner(Message::of(host, Body::Error(report)));
}

pub(crate) fn post_owner_state(host: &Arc<Host>, state: &'static str) {
    post_owner(Message::of(host, Body::SwState(state)));
}

fn deliver_worker(message: Message) {
    let host = message.host.clone();
    let js = host.worker_js();
    if js.is_null() || host.closing() {
        return;
    }
    ffi::with_js(js, |scope| {
        let budget = ffi::Budget::enter(scope);
        let ports = if message.port_id != 0 {
            scope.new_array()
        } else {
            message_ports(scope, &message, &Value::undefined())
        };
        let (kind, data) = decode(scope, &message, &ports);
        if message.port_id != 0 {
            bridge_deliver(scope, message.port_id, &data);
            ffi::drain_microtasks(js);
            drop(budget);
            return;
        }
        let global = scope.global();
        let origin = if host.is_service_worker {
            host.origin.as_str()
        } else {
            ""
        };
        let event = worker_event(scope, kind, &data, origin, "", "");
        freeze(scope, &ports);
        set(scope, &event, "ports", ports);
        ffi::dispatch_engine_event(scope, &global, &event);
        ffi::drain_microtasks(js);
        drop(budget);
    });
}

fn post_worker(message: Message) {
    let context = message.host.context.clone();
    context.invoke(move || deliver_worker(message));
}

fn send(mut message: Message, to_owner: bool) {
    let followups = core::mem::take(&mut message.followups);
    let deliver = |message: Message| {
        if to_owner {
            post_owner(message);
        } else {
            post_worker(message);
        }
    };
    deliver(message);
    for followup in followups {
        deliver(followup);
    }
}

pub(crate) fn bridge_send(scope: &mut Scope<'_>, port: &Value, id: u64, data: &Value) -> JsResult {
    let (host, to_owner) = match ffi::worker_host_of(ffi::js_of(scope)) {
        Some(host) => (Some(host), true),
        None => {
            let worker = get(scope, port, "_bridge_worker");
            let host = worker.with_host(|w: &WorkerRef| w.0.clone()).flatten();
            (host, false)
        }
    };
    let Some(host) = host.filter(|h| !h.closing()) else {
        return Ok(Value::undefined());
    };
    let mut message = message_new(scope, &host, data, &Value::undefined())?;
    message.port_id = id;
    send(message, to_owner);
    Ok(Value::undefined())
}

fn post(
    scope: &mut Scope<'_>,
    host: &Arc<Host>,
    args: &[Value],
    bridge_worker: &Value,
    to_owner: bool,
    refusal: &core::ffi::CStr,
) -> JsResult {
    if walk_transfers(scope, args, None, bridge_worker) {
        return Err(data_clone_error(scope, refusal));
    }
    let ports = transfer_ports(scope, args);
    let mut message = message_new(scope, host, &args[0], &ports)?;
    walk_transfers(scope, args, Some(&mut message), bridge_worker);
    send(message, to_owner);
    Ok(Value::undefined())
}

pub(crate) fn post_message(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let host = this.with_host(|w: &WorkerRef| w.0.clone()).flatten();
    let Some(host) = host.filter(|h| !args.is_empty() && !h.closing()) else {
        return Ok(Value::undefined());
    };
    post(
        scope,
        &host,
        args,
        this,
        false,
        c"Worker.postMessage: a value in the transfer list is not transferable",
    )
}

pub(crate) fn terminate(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    if let Some(host) = this.with_host(|w: &WorkerRef| w.0.clone()).flatten() {
        host.terminated.store(true, Ordering::SeqCst);
        stop(&host, false);
    }
    Ok(Value::undefined())
}

pub(crate) fn global_post_message(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> JsResult {
    let host = ffi::worker_host_of(ffi::js_of(scope));
    let Some(host) = host.filter(|h| !args.is_empty() && !h.closing()) else {
        return Ok(Value::undefined());
    };
    post(
        scope,
        &host,
        args,
        &Value::undefined(),
        true,
        c"postMessage: a value in the transfer list is not transferable",
    )
}

pub(crate) fn global_close(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    if let Some(host) = ffi::worker_host_of(ffi::js_of(scope)) {
        stop(&host, false);
    }
    Ok(Value::undefined())
}

pub(crate) fn script_url_allowed(
    base: Option<&str>,
    url: &str,
    allow_cross_origin: bool,
) -> Result<(), String> {
    if url.is_empty() {
        return Err("Worker script URL is empty".into());
    }
    if url.starts_with("data:") || url.starts_with("blob:") || url.starts_with("about:") {
        return Err("Worker scripts require http, https, or file URLs".into());
    }
    if let Some(base) = base {
        if base.starts_with("https://") && url.starts_with("http://") {
            return Err("Worker script blocked as mixed content".into());
        }
        if base.starts_with("file:") {
            return if url.starts_with("file:") {
                Ok(())
            } else {
                Err(String::new())
            };
        }
    }
    let web_or_file = ffi::url_is_http_or_https(url) || url.starts_with("file:");
    if allow_cross_origin {
        return if web_or_file {
            Ok(())
        } else {
            Err("Worker scripts require http, https, or file URLs".into())
        };
    }
    if let Some(base) = base.filter(|b| ffi::url_is_http_or_https(b)) {
        return if ffi::url_same_origin(base, url) {
            Ok(())
        } else {
            Err("Worker script blocked by same-origin policy".into())
        };
    }
    if web_or_file {
        Ok(())
    } else {
        Err(String::new())
    }
}

pub(crate) fn script_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if let Some(nul) = text.find('\0') {
        text.truncate(nul);
    }
    text
}

fn fetch_script(
    host: &Host,
    url: &str,
    allow_cross_origin: bool,
) -> Result<(String, String), Option<String>> {
    let base = Some(host.base_url());
    script_url_allowed(base, url, allow_cross_origin).map_err(Some)?;
    let destination = if allow_cross_origin {
        "X-ND-Fetch-Dest: script"
    } else if host.is_service_worker {
        "X-ND-Fetch-Dest: serviceworker"
    } else {
        "X-ND-Fetch-Dest: worker"
    };
    let response = ffi::net_get(url, host.base_url(), destination);
    match response.body {
        Some(body) if response.status == 200 && body.len() <= SCRIPT_BYTES_MAX => {
            let final_url = response.final_url.unwrap_or_else(|| url.to_owned());
            script_url_allowed(base, &final_url, allow_cross_origin).map_err(Some)?;
            Ok((script_text(&body), final_url))
        }
        _ => Err(Some(response.error.unwrap_or_else(|| {
            if response.received {
                "non-200 status".into()
            } else {
                "fetch failed".into()
            }
        }))),
    }
}

#[derive(Default)]
struct EvalReport {
    message: Option<String>,
    lineno: i32,
    colno: i32,
    parse_error: bool,
    reported: bool,
}

pub(crate) fn exception_position(
    scope: &mut Scope<'_>,
    exception: &Value,
) -> (Option<String>, i32, i32) {
    let stack = if exception.is_object() {
        get(scope, exception, "stack")
    } else {
        Value::undefined()
    };
    let text = if stack.is_string() {
        crate::text(scope, &stack).unwrap_or_default()
    } else {
        String::new()
    };
    for frame in text.split('\n') {
        let frame = frame.trim_end();
        let frame = frame.strip_suffix(')').unwrap_or(frame);
        let Some(c2) = frame.rfind(':') else {
            continue;
        };
        let Some(c1) = frame[..c2].rfind(':') else {
            continue;
        };
        let starts_digit = |at: usize| {
            frame[at..]
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_digit())
        };
        if !starts_digit(c1 + 1) || !starts_digit(c2 + 1) {
            continue;
        }
        let leading = |s: &str| -> i32 {
            let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        };
        let line = leading(&frame[c1 + 1..]);
        let col = leading(&frame[c2 + 1..]);
        let head = &frame[..c1];
        let file = if let Some(open) = head.rfind('(') {
            &head[open + 1..]
        } else if let Some(at) = head.find("at ") {
            &head[at + 3..]
        } else {
            head
        };
        return (Some(file.trim_start_matches(' ').to_owned()), line, col);
    }
    (None, 0, 0)
}

pub(crate) fn report_exception(js: Js, exception: &Value) -> bool {
    let Some(host) = ffi::worker_host_of(js) else {
        return false;
    };
    if IN_ERROR_REPORT.with(Cell::get) {
        return false;
    }
    let Some(handled) = ffi::with_js(js, |scope| {
        IN_ERROR_REPORT.with(|f| f.set(true));
        let message = format!("Uncaught {}", ffi::exception_message(scope, exception));
        let (file, line, col) = exception_position(scope, exception);
        let file = file
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| ffi::current_url(js).unwrap_or_else(|| host.url.clone()));
        let global = scope.global();
        let ctor = get(scope, &global, "ErrorEvent");
        let init = scope.new_object();
        set_str(scope, &init, "message", &message);
        set_str(scope, &init, "filename", &file);
        set(scope, &init, "lineno", Value::int(line));
        set(scope, &init, "colno", Value::int(col));
        set(scope, &init, "error", exception.clone());
        set(scope, &init, "cancelable", Value::boolean(true));
        let kind = scope.string("error");
        let mut handled = false;
        if scope.is_constructor(&ctor)
            && let Ok(event) = scope.construct(&ctor, &[kind, init])
        {
            let _ = scope.define(&event, "isTrusted", Value::boolean(true), PLAIN);
            let dispatch = get(scope, &global, "dispatchEvent");
            if let Ok(result) = scope.call(&dispatch, &global, &[event]) {
                handled = result.is_bool() && !scope.to_bool(&result);
            }
        }
        IN_ERROR_REPORT.with(|f| f.set(false));
        (handled, message, file, line, col)
    }) else {
        return false;
    };
    let (handled, message, file, line, col) = handled;
    if !handled && !host.closing() {
        post_owner_error(&host, &message, &file, line, col, false);
    }
    handled
}

fn eval_script(
    js: Js,
    source: &str,
    url: &str,
    report: Option<&mut EvalReport>,
) -> (bool, Option<String>) {
    let Some(host) = ffi::worker_host_of(js) else {
        return (false, None);
    };
    ffi::with_js(js, |scope| {
        let budget = ffi::Budget::enter(scope);
        let result = ffi::worker_eval(scope, js, source, url, host.is_module);
        let ok = result.is_ok();
        let mut error_line = None;
        if let Err((parse_error, exception)) = result {
            let message = crate::text(scope, &exception).unwrap_or_else(|| "exception".into());
            let stack = get(scope, &exception, "stack");
            let stack = crate::text(scope, &stack);
            if let Some(report) = report {
                if parse_error {
                    report.parse_error = true;
                    report.message = Some(format!("Uncaught {message}"));
                    let (_, line, col) = exception_position(scope, &exception);
                    report.lineno = line;
                    report.colno = col;
                } else {
                    report_exception(js, &exception);
                    report.reported = true;
                }
            }
            if !host.closing() {
                let line = match &stack {
                    Some(stack) => format!("Worker error in {url}: {message}\n{stack}"),
                    None => format!("Worker error in {url}: {message}"),
                };
                ffi::log_line(js, &line);
                error_line = Some(line);
            }
        }
        ffi::drain_microtasks(js);
        drop(budget);
        (ok, error_line)
    })
    .unwrap_or((false, None))
}

pub(crate) fn import_scripts(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(host) = ffi::worker_host_of(js) else {
        return Ok(Value::undefined());
    };
    if host.is_module {
        return Err(scope.type_error("importScripts is not available in module workers"));
    }
    for raw in args {
        let raw = scope.to_string(raw)?;
        let base = ffi::current_url(js).unwrap_or_else(|| host.url.clone());
        let Some(url) = ffi::url_resolve(Some(&base), &raw) else {
            return Err(scope.type_error("importScripts: invalid URL"));
        };
        let (body, final_url) = match fetch_script(&host, &url, true) {
            Ok(fetched) => fetched,
            Err(error) => {
                let error = error.filter(|e| !e.is_empty());
                return Err(scope.type_error(&format!(
                    "importScripts: {}",
                    error.as_deref().unwrap_or("load failed")
                )));
            }
        };
        let previous = ffi::current_url(js);
        ffi::set_current_url(js, Some(&final_url));
        let (ok, error) = eval_script(js, &body, &final_url, None);
        ffi::set_current_url(js, previous.as_deref());
        if !ok {
            return Err(scope.type_error(&format!(
                "importScripts: {}",
                error.as_deref().unwrap_or("evaluation failed")
            )));
        }
    }
    Ok(Value::undefined())
}

fn run(host: Arc<Host>) {
    let context = host.context.clone();
    context.push_thread_default();
    let fetched = match &host.inline_script {
        Some(script) => Ok((script.clone(), None)),
        None => fetch_script(&host, &host.url, false).map(|(body, url)| (body, Some(url))),
    };
    let finish = |host: &Arc<Host>| {
        host.closing.store(true, Ordering::SeqCst);
        drain_context(host);
        host.context.pop_thread_default();
    };
    let (body, final_url) = match fetched {
        Ok(fetched) => fetched,
        Err(error) => {
            let error = error.filter(|e| !e.is_empty());
            post_owner_error(
                &host,
                error.as_deref().unwrap_or("Worker load failed"),
                &host.url,
                0,
                0,
                true,
            );
            finish(&host);
            return;
        }
    };
    let js = ffi::worker_js_new(&host);
    if js.is_null() {
        post_owner_error(
            &host,
            "Worker runtime initialization failed",
            &host.url,
            0,
            0,
            true,
        );
        finish(&host);
        return;
    }
    let current = final_url.unwrap_or_else(|| host.url.clone());
    ffi::set_current_url(js, Some(&current));
    host.inner().worker_js = js;
    let mut report = EvalReport::default();
    let (ok, error) = eval_script(js, &body, &current, Some(&mut report));
    if !ok && !report.reported && !host.closing() {
        let message = report
            .message
            .clone()
            .or(error)
            .unwrap_or_else(|| "Worker script failed".into());
        post_owner_error(
            &host,
            &message,
            &current,
            report.lineno,
            report.colno,
            report.parse_error,
        );
        if report.parse_error {
            host.closing.store(true, Ordering::SeqCst);
        }
    }
    let runs = ok || !report.parse_error;
    if ok && host.is_service_worker && !host.closing() {
        crate::service::fire_lifecycle(js, &host);
    }
    if runs && !host.closing() {
        let main_loop = context.new_loop();
        host.inner().main_loop = main_loop;
        if !host.closing() {
            ffi::run_loop(main_loop);
        }
        host.inner().main_loop = 0;
        ffi::unref_loop(main_loop);
    }
    host.inner().worker_js = Js::default();
    ffi::js_free(js);
    finish(&host);
}

pub(crate) fn spawn(host: &Arc<Host>, name: &str) {
    let thread_host = host.clone();
    let thread = std::thread::Builder::new()
        .name(name.to_owned())
        .stack_size(THREAD_STACK)
        .spawn(move || run(thread_host))
        .ok();
    host.inner().thread = thread;
}

fn option_string(scope: &mut Scope<'_>, options: &Value, name: &str) -> Option<String> {
    if !options.is_object() {
        return None;
    }
    let value = get(scope, options, name);
    if value.is_string() {
        crate::text(scope, &value)
    } else {
        None
    }
}

fn init_event_slots(scope: &mut Scope<'_>, object: &Value) {
    let listeners = scope.new_array();
    set(scope, object, "_listeners", listeners);
    set(scope, object, "onmessage", Value::null());
    set(scope, object, "onmessageerror", Value::null());
    set(scope, object, "onerror", Value::null());
}

pub(crate) fn construct(scope: &mut Scope<'_>, new_target: &Value, args: &[Value]) -> JsResult {
    let Some(raw) = args.first() else {
        return Err(scope.type_error("Worker requires a script URL"));
    };
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Err(scope.type_error("Worker is unavailable"));
    }
    let raw = scope.to_string(raw)?;
    let mut inline_script = None;
    let mut fails = false;
    let mut opaque = false;
    let url = if raw.starts_with("blob:") {
        match ffi::blob_url_lookup(js, &raw) {
            Some(bytes) => inline_script = Some(script_text(&bytes)),
            None => fails = true,
        }
        Some(raw)
    } else {
        let base = ffi::doc_base_url(js).filter(|b| !b.is_empty());
        ffi::url_resolve(base.as_deref(), &raw)
    };
    let Some(url) = url else {
        return Err(ffi::dom_exception(
            scope,
            c"SyntaxError",
            12,
            c"Failed to construct 'Worker': the script URL is invalid.",
        ));
    };
    if url.starts_with("data:") {
        inline_script = ffi::decode_data_url(&url).map(|bytes| script_text(&bytes));
        fails = inline_script.is_none();
        opaque = true;
    }
    let options = arg(args, 1);
    let kind = option_string(scope, &options, "type");
    let is_module = kind
        .as_deref()
        .is_some_and(|k| k.eq_ignore_ascii_case("module"));
    if kind
        .as_deref()
        .is_some_and(|k| !k.is_empty() && !is_module && !k.eq_ignore_ascii_case("classic"))
    {
        return Err(scope.type_error("Worker: unsupported worker type"));
    }
    let name = option_string(scope, &options, "name");
    let current = ffi::current_url(js);
    if inline_script.is_none()
        && !fails
        && let Err(error) = script_url_allowed(current.as_deref(), &url, false)
    {
        if !error.is_empty() {
            ffi::log_line(js, &format!("Worker {url}: {error}"));
        }
        fails = true;
    }
    if !fails && !ffi::csp_allows_worker(js, &url) {
        fails = true;
    }
    let proto = if new_target.is_object() {
        get(scope, new_target, "prototype")
    } else {
        Value::null()
    };
    let proto = proto.is_object().then_some(proto);
    if fails {
        let object = scope.new_host_object(proto.as_ref(), WorkerRef(None));
        init_event_slots(scope, &object);
        ffi::queue_fail_job(scope, &object);
        return Ok(object);
    }
    let base_url = current.clone().unwrap_or_else(|| url.clone());
    let origin = if opaque {
        "null".to_owned()
    } else {
        ffi::url_origin_from(&base_url).unwrap_or_default()
    };
    let host = Host::new(HostSpec {
        url: url.clone(),
        base_url,
        name: name.unwrap_or_default(),
        origin,
        inline_script,
        is_module,
        is_service_worker: false,
        scope: None,
    });
    let object = scope.new_host_object(proto.as_ref(), WorkerRef(Some(host.clone())));
    init_event_slots(scope, &object);
    set_str(scope, &object, "url", &url);
    crate::bind_if_not_callable(scope, &object, "postMessage", 1, post_message);
    crate::bind_if_not_callable(scope, &object, "terminate", 0, terminate);
    ffi::bind_c_if_not_callable(
        scope,
        &object,
        "addEventListener",
        2,
        ffi::CFn::AddEventListener,
    );
    ffi::bind_c_if_not_callable(
        scope,
        &object,
        "removeEventListener",
        2,
        ffi::CFn::RemoveEventListener,
    );
    ffi::bind_c_if_not_callable(scope, &object, "dispatchEvent", 1, ffi::CFn::DispatchEvent);
    adopt(Owned {
        host: host.clone(),
        js,
        ctx: ffi::ctx_of(scope),
        object: object.clone(),
    });
    spawn(&host, "nd-js-worker");
    Ok(object)
}

pub(crate) fn install_constructor(scope: &mut Scope<'_>, global: &Value) {
    let ctor = ffi::make_worker_ctor(scope);
    let proto = get(scope, &ctor, "prototype");
    crate::bind(scope, &proto, "postMessage", 1, post_message);
    crate::bind(scope, &proto, "terminate", 0, terminate);
    ffi::bind_event_target_listeners(scope, &proto);
    ffi::bind_c(scope, &proto, "dispatchEvent", 1, ffi::CFn::DispatchEvent);
    set(scope, global, "Worker", ctor);
}
