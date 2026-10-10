//! Southstar — matchMedia: MediaQueryList objects and the change events they get when a viewport or preference change flips a query.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{page, page_or_new};

fn change_listener(
    scope: &mut Scope<'_>,
    this: &Value,
    listener: &Value,
    method: &str,
) -> Result<Value, Value> {
    let function = scope.get(this, method)?;
    let change = scope.string("change");
    scope.call(&function, this, &[change, listener.clone()])
}

fn add_listener(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    match args.first() {
        Some(listener) if scope.is_function(listener) => {
            change_listener(scope, this, listener, "addEventListener")
        }
        _ => Ok(Value::undefined()),
    }
}

fn remove_listener(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    match args.first() {
        Some(listener) => change_listener(scope, this, listener, "removeEventListener"),
        None => Ok(Value::undefined()),
    }
}

pub(crate) fn match_media(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let query = match args.first() {
        Some(value) => scope.to_string(value).ok(),
        None => None,
    };
    let list = scope.new_object();
    let matches = ffi::media_query_matches(query.as_deref());
    let _ = scope.set(&list, "matches", Value::boolean(matches));
    let media = scope.string(&ffi::media_list_serialize(query.as_deref()));
    let _ = scope.set(&list, "media", media);
    let listeners = scope.new_array();
    let _ = scope.set(&list, "_listeners", listeners);
    for (name, f) in [
        ("addListener", add_listener as southstar_js_engine::NativeFn),
        ("removeListener", remove_listener),
    ] {
        let function = scope.function(name, 1, f);
        let _ = scope.set(&list, name, function);
    }
    ffi::bind_event_target(scope, &list);
    let js = ffi::js_of(scope);
    if !js.is_null() {
        page_or_new(js).media_lists.borrow_mut().push(list.clone());
    }
    Ok(list)
}

fn reevaluate_list(scope: &mut Scope<'_>, list: &Value) -> Result<(), Value> {
    let media = scope.get(list, "media")?;
    let media = scope.to_string(&media).ok();
    let now = ffi::media_query_matches(media.as_deref());
    let old = scope.get(list, "matches")?;
    if scope.to_bool(&old) == now {
        return Ok(());
    }
    scope.set(list, "matches", Value::boolean(now))?;
    let event = ffi::make_event(scope, list, "change");
    ffi::adopt_interface(scope, &event, "MediaQueryListEvent");
    scope.set(&event, "matches", Value::boolean(now))?;
    let media = scope.get(list, "media")?;
    scope.set(&event, "media", media)?;
    ffi::dispatch_with_event(scope, list, "change", &event);
    Ok(())
}

pub(crate) fn reevaluate(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    ffi::with_main_context(js, |scope| {
        let mut index = 0;
        loop {
            let Some(list) = page.media_lists.borrow().get(index).cloned() else {
                break;
            };
            let _ = reevaluate_list(scope, &list);
            index += 1;
        }
    });
}
