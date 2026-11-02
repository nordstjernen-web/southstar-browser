//! Southstar — SharedWorker: one worker thread per (constructor origin, script URL, name) that every same-origin page and frame in the renderer connects to through its own MessagePort.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::sync::Arc;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::worker::{self, Host, HostSpec, Owned, WorkerRef};
use crate::{JsResult, arg, ffi, get, ports, set};

const READONLY: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

struct Entry {
    origin: String,
    host: Arc<Host>,
}

thread_local! {
    static SHARED: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

fn find(origin: &str, url: &str, name: &str) -> Option<Arc<Host>> {
    SHARED.with(|list| {
        list.borrow()
            .iter()
            .find(|e| {
                e.origin == origin && e.host.url == url && e.host.name == name && !e.host.closing()
            })
            .map(|e| e.host.clone())
    })
}

fn remember(origin: String, host: &Arc<Host>) {
    SHARED.with(|list| {
        let mut list = list.borrow_mut();
        list.retain(|e| !e.host.closing());
        list.push(Entry {
            origin,
            host: host.clone(),
        });
    });
}

pub(crate) fn forget(host: &Arc<Host>) {
    SHARED.with(|list| list.borrow_mut().retain(|e| !Arc::ptr_eq(&e.host, host)));
}

pub(crate) fn fire_error(host: &Arc<Host>) {
    for (js, ctx, object) in worker::owners_of(host) {
        ffi::with_ctx(ctx, |scope| {
            let budget = ffi::Budget::enter(scope);
            ffi::fire_event(scope, &object, "error");
            ffi::drain_mutations(js);
            drop(budget);
        });
    }
}

fn options_dictionary(scope: &mut Scope<'_>, options: &Value) -> Result<Value, Value> {
    if options.is_undefined() || options.is_null() || options.is_object() {
        return Ok(options.clone());
    }
    let name = scope.to_string(options)?;
    let dictionary = scope.new_object();
    crate::set_str(scope, &dictionary, "name", &name);
    Ok(dictionary)
}

fn new_object(
    scope: &mut Scope<'_>,
    proto: Option<&Value>,
    host: Option<Arc<Host>>,
    port: &Value,
) -> Value {
    let object = scope.new_host_object(proto, WorkerRef(host));
    let listeners = scope.new_array();
    set(scope, &object, "_listeners", listeners);
    set(scope, &object, "onerror", Value::null());
    let _ = scope.define(&object, "port", port.clone(), READONLY);
    worker::bind_event_target(scope, &object);
    object
}

pub(crate) fn construct(scope: &mut Scope<'_>, new_target: &Value, args: &[Value]) -> JsResult {
    let Some(raw) = args.first() else {
        return Err(scope.type_error(
            "Failed to construct 'SharedWorker': 1 argument required, but only 0 present.",
        ));
    };
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Err(scope.type_error("SharedWorker is unavailable"));
    }
    let options = options_dictionary(scope, &arg(args, 1))?;
    let resolved = worker::resolve_script(scope, js, "SharedWorker", raw, &options)?;
    let proto = worker::prototype_of(scope, new_target);
    let outside = ports::new_port(scope)?;
    let name = resolved.name.clone().unwrap_or_default();
    let constructor_origin = ffi::url_origin_from(&resolved.base_url()).unwrap_or_default();
    let existing = if resolved.fails {
        None
    } else {
        find(&constructor_origin, &resolved.url, &name)
    };
    let mismatched = existing
        .as_ref()
        .is_some_and(|host| host.is_module != resolved.is_module);
    if resolved.fails || mismatched {
        let object = new_object(scope, proto.as_ref(), None, &outside);
        ffi::queue_fail_job(scope, &object);
        return Ok(object);
    }
    let fresh = existing.is_none();
    let host = existing.unwrap_or_else(|| {
        Host::new(HostSpec {
            url: resolved.url.clone(),
            base_url: resolved.base_url(),
            origin: resolved.origin(),
            name,
            inline_script: resolved.inline_script,
            is_module: resolved.is_module,
            is_service_worker: false,
            is_shared: true,
            scope: None,
        })
    });
    let object = new_object(scope, proto.as_ref(), Some(host.clone()), &outside);
    worker::adopt(Owned {
        host: host.clone(),
        js,
        ctx: ffi::ctx_of(scope),
        object: object.clone(),
    });
    let inside = ports::new_port(scope)?;
    worker::connect(scope, &host, &object, &inside, &outside);
    if fresh {
        remember(constructor_origin, &host);
        worker::spawn(&host, "nd-js-shared-worker");
    }
    Ok(object)
}

pub(crate) fn install_constructor(scope: &mut Scope<'_>, global: &Value) {
    let ctor = ffi::make_shared_worker_ctor(scope);
    let proto = get(scope, &ctor, "prototype");
    ffi::bind_event_target_listeners(scope, &proto);
    ffi::bind_c(scope, &proto, "dispatchEvent", 1, ffi::CFn::DispatchEvent);
    set(scope, global, "SharedWorker", ctor);
}
