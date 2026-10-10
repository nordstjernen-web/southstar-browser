//! Southstar — the WebSocket and EventSource objects over the rust/websocket and rust/eventsource transports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use std::rc::Rc;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js, Realm, Transport};
use crate::headers;
use crate::{JsResult, c_bytes, is_nullish, prop, set, set_str};

const WS_CONNECTING: i32 = 0;
const WS_OPEN: i32 = 1;
const WS_CLOSING: i32 = 2;
const WS_CLOSED: i32 = 3;

const CONSTANT: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    WebSocket,
    EventSource,
}

pub(crate) struct Socket {
    pub js: Js,
    pub realm: Realm,
    pub kind: Kind,
    pub transport: RefCell<Transport>,
    pub origin: Vec<u8>,
    pub pin: RefCell<Option<Value>>,
}

impl Socket {
    pub fn wrapper(&self) -> Option<Value> {
        self.pin.borrow().clone()
    }

    pub fn unpin(&self) {
        let pin = self.pin.borrow_mut().take();
        drop(pin);
    }

    pub fn shut_down(&self) {
        self.transport.borrow_mut().free();
        self.unpin();
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.transport.get_mut().free();
    }
}

#[derive(Clone)]
struct Handle(Rc<Socket>);

fn socket_of(this: &Value) -> Option<Rc<Socket>> {
    this.with_host(|handle: &Handle| handle.0.clone())
}

fn log_exception(scope: &mut Scope<'_>, error: &Value, kind: &str) {
    let message = scope.to_bytes(error).ok();
    let Some(message) = message else {
        return;
    };
    let stack = if error.is_object() {
        prop(scope, error, "stack")
    } else {
        Value::undefined()
    };
    let stack = if is_nullish(&stack) {
        None
    } else {
        scope.to_bytes(&stack).ok()
    };
    let mut line = format!("JS error in WebSocket {kind}: ").into_bytes();
    line.extend_from_slice(&crate::until_nul(message));
    if let Some(stack) = stack {
        line.push(b'\n');
        line.extend_from_slice(&crate::until_nul(stack));
    }
    Js::of(scope).log_line(&line);
}

fn dispatch(scope: &mut Scope<'_>, target: &Value, on_name: &str, event: Value) {
    let kind = on_name.strip_prefix("on").unwrap_or(on_name);
    let entered = ffi::enter_handler_realm(scope, target, kind);
    let handler = prop(scope, target, on_name);
    if scope.is_function(&handler)
        && let Err(error) = scope.call(&handler, target, core::slice::from_ref(&event))
    {
        log_exception(scope, &error, on_name);
    }
    if !kind.is_empty() {
        let listeners = prop(scope, target, "_listeners");
        if listeners.is_array() {
            let count = crate::array_length(scope, &listeners);
            for i in 0..count {
                let entry = scope
                    .get_index(&listeners, i)
                    .unwrap_or_else(|_| Value::undefined());
                if !entry.is_object() {
                    continue;
                }
                let entry_type = prop(scope, &entry, "type");
                if c_bytes(scope, &entry_type).as_deref() != Some(kind.as_bytes()) {
                    continue;
                }
                let listener = prop(scope, &entry, "fn");
                if scope.is_function(&listener)
                    && let Err(error) = scope.call(&listener, target, core::slice::from_ref(&event))
                {
                    log_exception(scope, &error, kind);
                }
            }
        }
    }
    ffi::leave_handler_realm(scope, entered);
}

fn with_socket(socket: &Socket, f: impl FnOnce(&mut Scope<'_>, &Value)) {
    let Some(wrapper) = socket.wrapper() else {
        return;
    };
    socket
        .js
        .with_budget(|| socket.realm.enter(|scope| f(scope, &wrapper)));
}

pub(crate) fn on_open(socket: &Socket) {
    with_socket(socket, |scope, wrapper| {
        set(scope, wrapper, "readyState", Value::int(1));
        if socket.kind == Kind::WebSocket {
            let protocol = socket.transport.borrow().protocol();
            set_str(scope, wrapper, "protocol", &protocol);
        }
        let event = ffi::socket_event(scope, c"open");
        dispatch(scope, wrapper, "onopen", event);
    });
}

fn message_event(scope: &mut Scope<'_>, socket: &Socket, data: Value) -> Value {
    let event = ffi::socket_event(scope, c"message");
    set(scope, &event, "data", data);
    let origin = socket.js.page_url().unwrap_or_default();
    set_str(scope, &event, "origin", &origin);
    set_str(scope, &event, "lastEventId", b"");
    event
}

pub(crate) fn on_text(socket: &Socket, text: &[u8]) {
    with_socket(socket, |scope, wrapper| {
        let data = scope.string_from_bytes(text);
        let event = message_event(scope, socket, data);
        dispatch(scope, wrapper, "onmessage", event);
    });
}

fn array_buffer(scope: &mut Scope<'_>, bytes: &[u8]) -> Value {
    scope
        .new_array_buffer(bytes)
        .unwrap_or_else(|_| Value::undefined())
}

fn binary_data(scope: &mut Scope<'_>, wrapper: &Value, bytes: &[u8]) -> Value {
    let kind = prop(scope, wrapper, "binaryType");
    if c_bytes(scope, &kind).as_deref() == Some(b"arraybuffer") {
        return array_buffer(scope, bytes);
    }
    let ctor = crate::global_ctor(scope, "Blob");
    if !scope.is_constructor(&ctor) {
        return array_buffer(scope, bytes);
    }
    let parts = scope.new_array();
    let buffer = array_buffer(scope, bytes);
    let _ = scope.set_index(&parts, 0, buffer);
    match scope.construct(&ctor, &[parts]) {
        Ok(blob) => blob,
        Err(_) => array_buffer(scope, bytes),
    }
}

pub(crate) fn on_binary(socket: &Socket, bytes: &[u8]) {
    with_socket(socket, |scope, wrapper| {
        let data = binary_data(scope, wrapper, bytes);
        let event = message_event(scope, socket, data);
        dispatch(scope, wrapper, "onmessage", event);
    });
}

pub(crate) fn on_close(socket: &Socket, code: i32, reason: &[u8], clean: bool) {
    with_socket(socket, |scope, wrapper| {
        set(scope, wrapper, "readyState", Value::int(3));
        let event = ffi::socket_event(scope, c"close");
        ffi::adopt_interface(scope, &event, c"CloseEvent");
        set(scope, &event, "code", Value::int(code));
        set_str(scope, &event, "reason", reason);
        set(scope, &event, "wasClean", Value::boolean(clean));
        dispatch(scope, wrapper, "onclose", event);
    });
    socket.unpin();
}

pub(crate) fn on_error(socket: &Socket, message: &[u8]) {
    with_socket(socket, |scope, wrapper| {
        let event = ffi::socket_event(scope, c"error");
        set_str(scope, &event, "message", message);
        dispatch(scope, wrapper, "onerror", event);
    });
}

pub(crate) fn on_event_message(socket: &Socket, kind: &[u8], data: &[u8], last_id: &[u8]) {
    with_socket(socket, |scope, wrapper| {
        let kind = if kind.is_empty() {
            &b"message"[..]
        } else {
            kind
        };
        let kind_c = ffi::cstring(kind);
        let event = ffi::socket_event(scope, &kind_c);
        ffi::adopt_interface(scope, &event, c"MessageEvent");
        set_str(scope, &event, "data", data);
        set_str(scope, &event, "lastEventId", last_id);
        set_str(scope, &event, "origin", &socket.origin);
        let on_name = format!("on{}", String::from_utf8_lossy(kind));
        dispatch(scope, wrapper, &on_name, event);
    });
}

pub(crate) fn on_event_error(socket: &Socket, fatal: bool) {
    with_socket(socket, |scope, wrapper| {
        set(
            scope,
            wrapper,
            "readyState",
            Value::int(if fatal { 2 } else { 0 }),
        );
        let event = ffi::socket_event(scope, c"error");
        dispatch(scope, wrapper, "onerror", event);
    });
    if fatal {
        socket.unpin();
    }
}

fn send(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(socket), Some(data)) = (socket_of(this), args.first()) else {
        return Ok(Value::undefined());
    };
    if socket.transport.borrow().is_none() {
        return Ok(Value::undefined());
    }
    let state = socket.transport.borrow().ws_state();
    if state == WS_CONNECTING {
        return Err(ffi::throw_dom(
            scope,
            c"InvalidStateError",
            11,
            "WebSocket.send: still in CONNECTING state",
        ));
    }
    if state != WS_OPEN {
        return Ok(Value::undefined());
    }
    if let Some(bytes) = scope.array_buffer_bytes(data) {
        socket.transport.borrow().send_binary(&bytes);
        return Ok(Value::undefined());
    }
    if let Some(bytes) = scope.view_data(data) {
        socket.transport.borrow().send_binary(&bytes);
        return Ok(Value::undefined());
    }
    if data.is_object() && !is_nullish(&prop(scope, data, "__ndBlobBytes")) {
        if let Some(bytes) = ffi::blob_bytes(scope, data) {
            socket.transport.borrow().send_binary(&bytes);
        }
        return Ok(Value::undefined());
    }
    if let Ok(text) = scope.to_bytes(data) {
        socket.transport.borrow().send_text(&text);
    }
    Ok(Value::undefined())
}

fn close(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(socket) = socket_of(this) else {
        return Ok(Value::undefined());
    };
    if socket.transport.borrow().is_none() {
        return Ok(Value::undefined());
    }
    let state = socket.transport.borrow().ws_state();
    if state == WS_CLOSED || state == WS_CLOSING {
        return Ok(Value::undefined());
    }
    let mut code = 1000;
    if let Some(given) = args.first().filter(|c| !c.is_undefined()) {
        if let Ok(given) = scope.to_int32(given) {
            code = given;
        }
        if code != 1000 && !(3000..=4999).contains(&code) {
            return Err(ffi::throw_dom(
                scope,
                c"InvalidAccessError",
                15,
                "WebSocket.close: code must be 1000 or in 3000..4999",
            ));
        }
    }
    let reason = args
        .get(1)
        .filter(|r| r.is_string())
        .and_then(|r| c_bytes(scope, r));
    if reason.as_ref().is_some_and(|r| r.len() > 123) {
        return Err(ffi::throw_dom(
            scope,
            c"SyntaxError",
            12,
            "WebSocket.close: reason must be <= 123 UTF-8 bytes",
        ));
    }
    set(scope, this, "readyState", Value::int(2));
    socket.transport.borrow().close(code, reason.as_deref());
    Ok(Value::undefined())
}

fn protocols(scope: &mut Scope<'_>, value: &Value) -> JsResult<Vec<Vec<u8>>> {
    let mut list = Vec::new();
    let invalid = |scope: &mut Scope<'_>| {
        scope.type_error("WebSocket subprotocol must be a token (RFC 6455)")
    };
    if value.is_string() {
        if let Some(protocol) = c_bytes(scope, value) {
            if !headers::is_token(&protocol) {
                return Err(invalid(scope));
            }
            list.push(protocol);
        }
    } else if value.is_array() {
        let count = crate::array_length(scope, value);
        for i in 0..count {
            let entry = scope
                .get_index(value, i)
                .unwrap_or_else(|_| Value::undefined());
            if let Some(protocol) = c_bytes(scope, &entry) {
                if !headers::is_token(&protocol) {
                    return Err(invalid(scope));
                }
                list.push(protocol);
            }
        }
    }
    Ok(list)
}

fn replace_prefix(url: &[u8], prefix: &[u8], with: &[u8]) -> Option<Vec<u8>> {
    crate::ascii_starts_with(url, prefix).then(|| {
        let mut out = with.to_vec();
        out.extend_from_slice(&url[prefix.len()..]);
        out
    })
}

fn page_origin(js: Js) -> Option<Vec<u8>> {
    let page = js.page_url()?;
    (crate::ascii_starts_with(&page, b"http://") || crate::ascii_starts_with(&page, b"https://"))
        .then(|| ffi::url_origin_from(Some(&page)))
        .flatten()
}

fn resolve_target(scope: &mut Scope<'_>, js: Js, url: &Value) -> JsResult<Vec<u8>> {
    let raw = crate::until_nul(scope.to_bytes(url)?);
    let page = js.page_url();
    Ok(page
        .as_deref()
        .and_then(|page| ffi::url_resolve(Some(page), &raw))
        .unwrap_or(raw))
}

fn define_constants(scope: &mut Scope<'_>, object: &Value, names: &[&str]) {
    for (i, name) in names.iter().enumerate() {
        set(scope, object, name, Value::int(i as i32));
    }
}

fn new_wrapper(scope: &mut Scope<'_>, socket: &Rc<Socket>) -> Value {
    let wrapper = scope.new_host_object(Some(&Value::null()), Handle(socket.clone()));
    *socket.pin.borrow_mut() = Some(wrapper.clone());
    wrapper
}

pub(crate) fn websocket_ctor(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(url) = args.first() else {
        return Err(scope.type_error("WebSocket requires a URL"));
    };
    let js = Js::of(scope);
    let mut target = resolve_target(scope, js, url)?;
    if let Some(remapped) = replace_prefix(&target, b"http://", b"ws://")
        .or_else(|| replace_prefix(&target, b"https://", b"wss://"))
    {
        target = remapped;
    }
    if !crate::ascii_starts_with(&target, b"ws://") && !crate::ascii_starts_with(&target, b"wss://")
    {
        return Err(ffi::throw_dom(
            scope,
            c"SyntaxError",
            12,
            "WebSocket URL must use ws: or wss:",
        ));
    }
    if target.contains(&b'#') {
        return Err(ffi::throw_dom(
            scope,
            c"SyntaxError",
            12,
            "WebSocket URL must not contain a fragment",
        ));
    }
    if crate::ascii_starts_with(&target, b"ws://")
        && ffi::url_host_from(&target).is_some_and(|host| ffi::hsts_should_upgrade(&host))
    {
        target = replace_prefix(&target, b"ws://", b"wss://").unwrap_or(target);
    }
    let page = js.page_url();
    if page
        .as_deref()
        .is_some_and(|p| crate::ascii_starts_with(p, b"https://"))
        && crate::ascii_starts_with(&target, b"ws://")
    {
        return Err(scope.type_error("WebSocket: mixed content (ws:// not allowed from https://)"));
    }
    if !js.csp_allows_connect(&target, page.as_deref()) {
        return Err(scope.type_error("WebSocket: blocked by Content-Security-Policy connect-src"));
    }
    let protocol_list = match args.get(1).filter(|p| !p.is_undefined()) {
        Some(value) => Some(protocols(scope, value)?),
        None => None,
    };
    let socket = Rc::new(Socket {
        js,
        realm: Realm::of(scope),
        kind: Kind::WebSocket,
        transport: RefCell::new(Transport::none()),
        origin: Vec::new(),
        pin: RefCell::new(None),
    });
    let wrapper = new_wrapper(scope, &socket);
    set_str(scope, &wrapper, "url", &target);
    set(scope, &wrapper, "readyState", Value::int(0));
    set(scope, &wrapper, "bufferedAmount", Value::int(0));
    set_str(scope, &wrapper, "protocol", b"");
    set_str(scope, &wrapper, "extensions", b"");
    set_str(scope, &wrapper, "binaryType", b"blob");
    define_constants(
        scope,
        &wrapper,
        &["CONNECTING", "OPEN", "CLOSING", "CLOSED"],
    );
    let send_fn = scope.function("send", 1, send);
    set(scope, &wrapper, "send", send_fn);
    let close_fn = scope.function("close", 2, close);
    set(scope, &wrapper, "close", close_fn);
    let listeners = scope.new_array();
    set(scope, &wrapper, "_listeners", listeners);
    ffi::bind_event_target(scope, &wrapper);
    let origin = page_origin(js)
        .filter(|o| !o.is_empty())
        .unwrap_or_else(|| b"null".to_vec());
    let transport = Transport::websocket(&target, &origin, protocol_list.as_deref(), &socket);
    if transport.is_none() {
        socket.pin.borrow_mut().take();
        return Err(scope.type_error("WebSocket: failed to start"));
    }
    *socket.transport.borrow_mut() = transport;
    crate::track_socket(js, Rc::downgrade(&socket));
    Ok(wrapper)
}

fn event_source_close(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(socket) = socket_of(this) else {
        return Ok(Value::undefined());
    };
    set(scope, this, "readyState", Value::int(2));
    socket.transport.borrow().close(0, None);
    socket.unpin();
    Ok(Value::undefined())
}

pub(crate) fn event_source_ctor(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(url) = args.first() else {
        return Err(scope.type_error("EventSource requires a URL"));
    };
    let js = Js::of(scope);
    let target = resolve_target(scope, js, url)?;
    if !crate::ascii_starts_with(&target, b"http://")
        && !crate::ascii_starts_with(&target, b"https://")
    {
        return Err(scope.type_error("EventSource URL must use http: or https:"));
    }
    let page = js.page_url();
    if page
        .as_deref()
        .is_some_and(|p| crate::ascii_starts_with(p, b"https://"))
        && crate::ascii_starts_with(&target, b"http://")
    {
        return Err(
            scope.type_error("EventSource: mixed content (http:// not allowed from https://)")
        );
    }
    if !js.csp_allows_connect(&target, page.as_deref()) {
        return Err(scope.type_error("EventSource: blocked by Content-Security-Policy connect-src"));
    }
    let with_credentials = args
        .get(1)
        .filter(|init| init.is_object())
        .is_some_and(|init| crate::bool_prop(scope, init, "withCredentials"));
    let socket = Rc::new(Socket {
        js,
        realm: Realm::of(scope),
        kind: Kind::EventSource,
        transport: RefCell::new(Transport::none()),
        origin: ffi::url_origin_from(Some(&target)).unwrap_or_default(),
        pin: RefCell::new(None),
    });
    let wrapper = new_wrapper(scope, &socket);
    set_str(scope, &wrapper, "url", &target);
    set(scope, &wrapper, "readyState", Value::int(0));
    set(
        scope,
        &wrapper,
        "withCredentials",
        Value::boolean(with_credentials),
    );
    define_constants(scope, &wrapper, &["CONNECTING", "OPEN", "CLOSED"]);
    let close_fn = scope.function("close", 0, event_source_close);
    set(scope, &wrapper, "close", close_fn);
    let listeners = scope.new_array();
    set(scope, &wrapper, "_listeners", listeners);
    ffi::bind_event_target(scope, &wrapper);
    let origin = page_origin(js)
        .filter(|o| !o.is_empty())
        .unwrap_or_else(|| b"null".to_vec());
    let transport = Transport::event_source(&target, &origin, &socket);
    if transport.is_none() {
        socket.pin.borrow_mut().take();
        return Err(scope.type_error("EventSource: failed to start"));
    }
    *socket.transport.borrow_mut() = transport;
    crate::track_socket(js, Rc::downgrade(&socket));
    Ok(wrapper)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    ffi::bind_socket_ctors(scope, global);
    let ctor = prop(scope, global, "WebSocket");
    if !ctor.is_object() {
        return;
    }
    let proto = prop(scope, &ctor, "prototype");
    for (i, name) in ["CONNECTING", "OPEN", "CLOSING", "CLOSED"]
        .into_iter()
        .enumerate()
    {
        let _ = scope.define(&ctor, name, Value::int(i as i32), CONSTANT);
        if proto.is_object() {
            let _ = scope.define(&proto, name, Value::int(i as i32), CONSTANT);
        }
    }
}
