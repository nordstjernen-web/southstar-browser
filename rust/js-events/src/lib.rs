//! Southstar — the event core: Event and its subclasses, EventTarget listeners and the dispatch algorithm.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod attrs;
mod class;
mod ctors;
mod event;
mod exceptions;
pub(crate) mod ffi;
mod install;

use southstar_dom::Node;
use southstar_js_engine::{Attributes, Scope, Value};

pub use ffi::nsr_dom_exception;
pub use ffi::nsr_event_adopt_interface;
pub use ffi::nsr_event_define_cancel_bubble;
pub use ffi::nsr_event_define_source;
pub use ffi::nsr_event_new;
pub use ffi::nsr_install_event_attribute_getters;
pub use ffi::nsr_make_event;

pub use ffi::nsr_event_composed_path;
pub use ffi::nsr_event_prevent_default;
pub use ffi::nsr_event_stop_immediate;
pub use ffi::nsr_event_stop_propagation;
pub use ffi::nsr_window_event_ctor;

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};
pub(crate) const WRITABLE_CONFIGURABLE: Attributes = Attributes::METHOD;
pub(crate) const ENUMERABLE_CONFIGURABLE: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};
pub(crate) const ALL: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn bool_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> (bool, bool) {
    let value = get(scope, object, key);
    let defined = !value.is_undefined();
    (defined && scope.to_bool(&value), defined)
}

pub(crate) fn int_prop(scope: &mut Scope<'_>, object: &Value, key: &str, fallback: i32) -> i32 {
    let value = get(scope, object, key);
    if value.is_undefined() || value.is_null() {
        return fallback;
    }
    scope.to_int32(&value).unwrap_or(0)
}

pub(crate) fn number_prop(scope: &mut Scope<'_>, object: &Value, key: &str, fallback: f64) -> f64 {
    let value = get(scope, object, key);
    if value.is_undefined() || value.is_null() {
        return fallback;
    }
    scope.to_number(&value).unwrap_or(f64::NAN)
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn interface_prototype(scope: &mut Scope<'_>, global: &Value, name: &str) -> Value {
    let ctor = get(scope, global, name);
    if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    }
}

pub fn install_runtime_hooks() {
    southstar_canvas::links::set_js_events_links(southstar_canvas::links::JsEventsLinks {
        dom_exception: crate::ffi::nsr_dom_exception,
    });
    southstar_js_clone::links::set_js_events_links(southstar_js_clone::links::JsEventsLinks {
        dom_exception: crate::ffi::nsr_dom_exception,
    });
    southstar_js_collections::links::set_js_events_links(
        southstar_js_collections::links::JsEventsLinks {
            dom_exception: crate::ffi::nsr_dom_exception,
        },
    );
    southstar_js_cssom::links::set_js_events_links(southstar_js_cssom::links::JsEventsLinks {
        event_new: crate::ffi::nsr_event_new,
    });
    southstar_js_dispatch::links::set_js_events_links(
        southstar_js_dispatch::links::JsEventsLinks {
            event_stop_immediate: crate::ffi::nsr_event_stop_immediate,
            event_stop_propagation: crate::ffi::nsr_event_stop_propagation,
            event_prevent_default: crate::ffi::nsr_event_prevent_default,
            dom_exception: crate::ffi::nsr_dom_exception,
            event_define_source: crate::ffi::nsr_event_define_source,
            event_adopt_interface: crate::ffi::nsr_event_adopt_interface,
            event_mark_default_prevented: crate::ffi::nsr_event_mark_default_prevented,
            event_define_cancel_bubble: crate::ffi::nsr_event_define_cancel_bubble,
            event_new: crate::ffi::nsr_event_new,
        },
    );
    southstar_js_forms::links::set_js_events_links(southstar_js_forms::links::JsEventsLinks {
        event_ctor: crate::ffi::nsr_event_ctor,
        dom_exception: crate::ffi::nsr_dom_exception,
    });
    southstar_js_net::links::set_js_events_links(southstar_js_net::links::JsEventsLinks {
        event_composed_path: crate::ffi::nsr_event_composed_path,
        event_stop_immediate: crate::ffi::nsr_event_stop_immediate,
        event_stop_propagation: crate::ffi::nsr_event_stop_propagation,
        event_prevent_default: crate::ffi::nsr_event_prevent_default,
        event_adopt_interface: crate::ffi::nsr_event_adopt_interface,
        event_define_cancel_bubble: crate::ffi::nsr_event_define_cancel_bubble,
        event_new: crate::ffi::nsr_event_new,
        make_abort_error: crate::ffi::nsr_make_abort_error,
        dom_exception: crate::ffi::nsr_dom_exception,
    });
    southstar_js_runtime::links::set_js_events_links(southstar_js_runtime::links::JsEventsLinks {
        window_event_ctor: crate::ffi::nsr_window_event_ctor,
        make_abort_error: crate::ffi::nsr_make_abort_error,
        dom_exception: crate::ffi::nsr_dom_exception,
        install_event_attribute_getters: crate::ffi::nsr_install_event_attribute_getters,
        events_install_worker: crate::ffi::nsr_events_install_worker,
        events_install_window: crate::ffi::nsr_events_install_window,
        events_install_window_base: crate::ffi::nsr_events_install_window_base,
    });
    southstar_js_services::links::set_js_events_links(
        southstar_js_services::links::JsEventsLinks {
            event_adopt_interface: crate::ffi::nsr_event_adopt_interface,
        },
    );
}
