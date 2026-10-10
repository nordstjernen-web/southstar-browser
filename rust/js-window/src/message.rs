//! Southstar — window.postMessage between windows and frames: target origins, the links between a frame's outward and inner windows, and delivery into the receiving realm.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js, Realm};
use crate::{array_length, existing_page, get, is_nullish, page, set};

fn put(list: &mut Vec<(Value, Value)>, key: &Value, value: &Value) {
    list.retain(|(held, _)| !held.same_object(key));
    list.push((key.clone(), value.clone()));
}

fn lookup(list: &[(Value, Value)], key: &Value) -> Value {
    list.iter()
        .find(|(held, _)| held.same_object(key))
        .map_or_else(Value::undefined, |(_, value)| value.clone())
}

pub(crate) fn links_clear(js: Js) {
    let Some(page) = existing_page(js) else {
        return;
    };
    let links = core::mem::take(&mut *page.links.borrow_mut());
    drop(links);
}

pub(crate) fn link_outward(js: Js, outward: &Value, realm_window: &Value) {
    if js.is_null() || !outward.is_object() || !realm_window.is_object() {
        return;
    }
    let page = page(js);
    let mut links = page.links.borrow_mut();
    put(&mut links.forwards, outward, realm_window);
    put(&mut links.outwards, realm_window, outward);
}

pub(crate) fn forward_of(js: Js, outward: &Value) -> Value {
    if js.is_null() || !outward.is_object() {
        return Value::undefined();
    }
    existing_page(js).map_or_else(Value::undefined, |page| {
        lookup(&page.links.borrow().forwards, outward)
    })
}

fn outward_of(js: Js, realm_window: &Value) -> Value {
    if js.is_null() || !realm_window.is_object() {
        return Value::undefined();
    }
    existing_page(js).map_or_else(Value::undefined, |page| {
        lookup(&page.links.borrow().outwards, realm_window)
    })
}

fn main_realm(js: Js) -> Option<Realm> {
    ffi::main_realm(js).or_else(|| ffi::current_realm(js))
}

fn is_main_window(js: Js, window: &Value) -> bool {
    main_realm(js).is_some_and(|realm| {
        let global = realm.enter(|scope| scope.global());
        global.same_object(window)
    })
}

fn origin_of(scope: &mut Scope<'_>, window: &Value) -> Option<Vec<u8>> {
    let js = ffi::js_of(scope);
    let forwarded = forward_of(js, window);
    let window = if forwarded.is_object() {
        &forwarded
    } else {
        window
    };
    let frame = ffi::frame_node(js, window);
    if let Some(frame) = frame
        && ffi::frame_origin_is_opaque(frame)
    {
        return Some(b"null".to_vec());
    }
    if !js.is_null()
        && let Some(origin) = ffi::document_origin(js)
        && window.is_object()
        && is_main_window(js, window)
    {
        return Some(origin);
    }
    if let Some(url) = frame.and_then(|frame| ffi::frame_url(js, frame)) {
        return Some(ffi::url_origin(&url).unwrap_or_else(|| b"null".to_vec()));
    }
    let location = scope.get(window, "location").ok()?;
    if !location.is_object() {
        return None;
    }
    let origin = scope.get(&location, "origin").ok()?;
    if !origin.is_string() {
        return None;
    }
    scope.to_bytes(&origin).ok().map(crate::c_text)
}

fn adopt_data(scope: &mut Scope<'_>, realm: Realm, event: &Value) {
    let ports = get(scope, event, "ports");
    let data = get(scope, event, "data");
    if realm != Realm::of(scope) && data.is_object() {
        let transfer = if ports.is_array() {
            ports.clone()
        } else {
            Value::undefined()
        };
        if let Ok(adopted) = realm.enter(|realm_scope| {
            ffi::structured_clone_transfer(
                realm_scope,
                &data,
                &transfer,
                &Value::undefined(),
                &Value::undefined(),
            )
        }) {
            set(scope, event, "data", adopted);
        }
    }
    let count = if ports.is_array() {
        array_length(scope, &ports)
    } else {
        0
    };
    let items: Vec<Value> = (0..count)
        .map(|index| {
            scope
                .get_index(&ports, index)
                .unwrap_or_else(|_| Value::undefined())
        })
        .collect();
    let realm_ports = realm.enter(|realm_scope| {
        let array = realm_scope.new_array();
        for (index, item) in items.into_iter().enumerate() {
            let _ = realm_scope.set_index(&array, index as u32, item);
        }
        let _ = realm_scope.freeze(&array);
        array
    });
    set(scope, event, "ports", realm_ports);
}

fn message_realm(scope: &mut Scope<'_>, target: &Value) -> Realm {
    let js = ffi::js_of(scope);
    let forwarded = forward_of(js, target);
    let actual = if forwarded.is_object() {
        &forwarded
    } else {
        target
    };
    let here = Realm::of(scope);
    let Ok(deliver) = scope.get(actual, "__nsDeliverMessage") else {
        return here;
    };
    if scope.is_function(&deliver) {
        return ffi::function_realm(scope, &deliver);
    }
    if !js.is_null()
        && let Some(main) = main_realm(js)
        && is_main_window(js, actual)
    {
        return main;
    }
    here
}

fn event_prototype(realm: Realm) -> Value {
    realm.enter(|scope| {
        let global = scope.global();
        let constructor = get(scope, &global, "MessageEvent");
        if constructor.is_object() {
            get(scope, &constructor, "prototype")
        } else {
            Value::undefined()
        }
    })
}

pub(crate) fn deliver(scope: &mut Scope<'_>, target: &Value, event: &Value) {
    let js = ffi::js_of(scope);
    let realm = message_realm(scope, target);
    let prototype = event_prototype(realm);
    if prototype.is_object() {
        let _ = scope.set_prototype(event, &prototype);
    }
    set(
        scope,
        event,
        "timeStamp",
        Value::number(ffi::realm_now_ms(realm)),
    );
    let forwarded = forward_of(js, target);
    let actual = if forwarded.is_object() {
        forwarded.clone()
    } else {
        target.clone()
    };
    set(scope, event, "target", actual.clone());
    set(scope, event, "currentTarget", actual.clone());
    if forwarded.is_object()
        && let Some(main) = main_realm(js)
    {
        let source = get(scope, event, "source");
        let parent_global = main.enter(|main_scope| main_scope.global());
        if source.same_object(target) {
            set(scope, event, "source", actual.clone());
        } else if source.same_object(&parent_global) {
            let parent_proxy = get(scope, &actual, "__ndParentWindowProxy");
            if parent_proxy.is_object() {
                set(scope, event, "source", parent_proxy);
            }
        }
    }
    let deliver = get(scope, &actual, "__nsDeliverMessage");
    if scope.is_function(&deliver) {
        let budget = ffi::budget_enter(js);
        let deliver_realm = ffi::function_realm(scope, &deliver);
        let url = ffi::realm_url_enter(js, deliver_realm);
        adopt_data(scope, deliver_realm, event);
        let _ = scope.call(&deliver, &actual, core::slice::from_ref(event));
        ffi::realm_url_leave(js, url);
        ffi::budget_leave(js, budget);
    } else if let Some(main) = ffi::current_realm(js).and_then(|_| main_realm(js)) {
        if is_main_window(js, &actual) {
            adopt_data(scope, main, event);
            ffi::dispatch_main_window_event(js, "message", event.clone());
        } else {
            let budget = ffi::budget_enter(js);
            ffi::target_dispatch(scope, &actual, "message", event);
            ffi::budget_leave(js, budget);
        }
    }
}

struct Posted<'a> {
    caller: Realm,
    target: Value,
    source_override: Value,
    args: &'a [Value],
}

fn target_origin(scope: &mut Scope<'_>, args: &[Value]) -> Result<(Vec<u8>, Value), Value> {
    let options_form = args.len() < 3
        && args
            .get(1)
            .is_none_or(|options| options.is_object() || is_nullish(options));
    if !options_form {
        let origin = crate::c_text(scope.to_bytes(&args[1])?);
        let transfer = args.get(2).cloned().unwrap_or_else(Value::undefined);
        return Ok((origin, transfer));
    }
    let Some(options) = args.get(1).filter(|options| options.is_object()) else {
        return Ok((b"/".to_vec(), Value::undefined()));
    };
    let transfer = scope.get(options, "transfer")?;
    let origin = scope.get(options, "targetOrigin")?;
    if origin.is_undefined() {
        return Ok((b"/".to_vec(), transfer));
    }
    Ok((crate::c_text(scope.to_bytes(&origin)?), transfer))
}

fn window_or_global(scope: &mut Scope<'_>, window: &Value, realm: Realm) -> Value {
    if window.is_object() {
        window.clone()
    } else if realm == Realm::of(scope) {
        scope.global()
    } else {
        realm.enter(|realm_scope| realm_scope.global())
    }
}

fn post(scope: &mut Scope<'_>, posted: Posted<'_>) -> Result<Value, Value> {
    let Posted {
        caller,
        target,
        source_override,
        args,
    } = posted;
    if args.is_empty() {
        return Err(scope.type_error(
            "Failed to execute 'postMessage' on 'Window': 1 argument required, but only 0 present.",
        ));
    }
    let (want, transfer) = target_origin(scope, args)?;
    let mut wanted = None;
    if want == b"/" {
        let source = window_or_global(scope, &source_override, caller);
        wanted = origin_of(scope, &source);
    } else if want != b"*" {
        wanted = ffi::url_origin(&want);
        if wanted.is_none() {
            return Err(scope.dom_exception(
                "SyntaxError",
                "Failed to execute 'postMessage' on 'Window': Invalid target origin.",
            ));
        }
    }

    let receiving = message_realm(scope, &target);
    let (old_ports, ports) = ffi::port_transfer_prepare(scope, &transfer, receiving)?;
    let data = ffi::structured_clone_transfer(scope, &args[0], &transfer, &old_ports, &ports)?;
    ffi::port_transfer_commit(scope, &old_ports, &ports);

    if want != b"*" {
        let actual = origin_of(scope, &target);
        let matches = match (&actual, &wanted) {
            (Some(actual), Some(wanted)) => {
                wanted.as_slice() != b"null" && wanted.eq_ignore_ascii_case(actual)
            }
            _ => false,
        };
        if !matches {
            return Ok(Value::undefined());
        }
    }

    let source_global = window_or_global(scope, &source_override, caller);
    let source_origin = origin_of(scope, &source_global);
    let source_outward = outward_of(ffi::js_of(scope), &source_global);
    let mut source = if source_outward.is_object() {
        ffi::cross_origin_window(scope, &source_outward)
    } else {
        source_global.clone()
    };
    if !source.is_object() {
        source = if source_outward.is_object() {
            source_outward
        } else {
            source_global
        };
    }

    let event = ffi::target_make_event(scope, &target, "message");
    set(scope, &event, "data", data);
    let origin = scope.string_from_bytes(source_origin.as_deref().unwrap_or_default());
    set(scope, &event, "origin", origin);
    let empty = scope.string("");
    set(scope, &event, "lastEventId", empty);
    set(scope, &event, "source", source);
    set(scope, &event, "ports", ports);
    ffi::queue_delivery(scope, &target, &event);
    Ok(Value::undefined())
}

fn post_from_caller(
    scope: &mut Scope<'_>,
    target: &Value,
    source_override: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let caller = ffi::caller_realm(scope);
    post(
        scope,
        Posted {
            caller,
            target: target.clone(),
            source_override: source_override.clone(),
            args,
        },
    )
}

fn window_realm(scope: &mut Scope<'_>, window: &Value) -> Realm {
    let js = ffi::js_of(scope);
    let here = Realm::of(scope);
    if js.is_null() {
        return here;
    }
    if let Some(realm) = ffi::frame_node(js, window).and_then(|frame| ffi::frame_realm(js, frame)) {
        return realm;
    }
    ffi::main_realm(js).unwrap_or(here)
}

fn post_message(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let window = &data[0];
    let realm = window_realm(scope, window);
    let target = if this.is_object() { this } else { window }.clone();
    let caller = ffi::caller_realm(scope);
    let posted = Posted {
        caller,
        target,
        source_override: Value::undefined(),
        args,
    };
    if realm == Realm::of(scope) {
        post(scope, posted)
    } else {
        realm.enter(|realm_scope| post(realm_scope, posted))
    }
}

fn post_message_to(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    match args {
        [target, rest @ ..] if !rest.is_empty() && target.is_object() => {
            post_from_caller(scope, target, &Value::undefined(), rest)
        }
        _ => Ok(Value::undefined()),
    }
}

fn post_message_this(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    if !this.is_object() {
        return Ok(Value::undefined());
    }
    post_from_caller(scope, this, &Value::undefined(), args)
}

fn post_message_from(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    match args {
        [target, source, rest @ ..]
            if !rest.is_empty() && target.is_object() && source.is_object() =>
        {
            post_from_caller(scope, target, source, rest)
        }
        _ => Ok(Value::undefined()),
    }
}

const HELPERS: [(&str, u32, NativeFn); 3] = [
    ("__nsPostMessageTo", 4, post_message_to),
    ("__nsPostMessageThis", 3, post_message_this),
    ("__nsPostMessageFrom", 5, post_message_from),
];

pub(crate) fn make_post_message(scope: &mut Scope<'_>, window: &Value) -> Value {
    scope.bound_function("", 2, post_message, core::slice::from_ref(window))
}

pub(crate) fn bind_post_message(scope: &mut Scope<'_>, global: &Value) {
    let post = make_post_message(scope, global);
    set(scope, global, "postMessage", post);
    for (name, arity, helper) in HELPERS {
        let function = scope.function(name, arity, helper);
        let _ = scope.define(global, name, function, Attributes::METHOD);
    }
}
