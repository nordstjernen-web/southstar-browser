//! Southstar — the window's and the workers' navigator: identity, client hints, permissions, clipboard, geolocation, media devices and the device stubs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, CMethod};

const VERSION: &str = match option_env!("NS_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

const ENUMERABLE_CONFIGURABLE: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

const PERMISSION_NAMES: &[&str] = &[
    "accelerometer",
    "ambient-light-sensor",
    "background-sync",
    "bluetooth",
    "camera",
    "clipboard-read",
    "clipboard-write",
    "display-capture",
    "geolocation",
    "gyroscope",
    "idle-detection",
    "local-fonts",
    "magnetometer",
    "microphone",
    "midi",
    "notifications",
    "payment-handler",
    "persistent-storage",
    "push",
    "speaker-selection",
    "storage-access",
    "window-management",
];

const PROTOCOL_SAFELIST: &[&str] = &[
    "bitcoin",
    "cabal",
    "dat",
    "did",
    "doi",
    "dweb",
    "ed2k",
    "eth",
    "ftp",
    "ftps",
    "geo",
    "gopher",
    "hcp",
    "im",
    "ipfs",
    "ipns",
    "irc",
    "ircs",
    "magnet",
    "mailto",
    "matrix",
    "mms",
    "news",
    "nntp",
    "openpgp4fpr",
    "sip",
    "sms",
    "smsto",
    "ssb",
    "ssh",
    "tel",
    "urn",
    "webcal",
    "wtai",
    "xmpp",
];

const CLIPBOARD_WRITE: &str = "(function(c){\
function plain(item){\
if(!item)return Promise.resolve('');\
var types=item.types||[];\
if(Array.prototype.indexOf.call(types,'text/plain')<0)\
return Promise.resolve('');\
return Promise.resolve(item.getType('text/plain')).then(function(v){\
if(v&&typeof v.text==='function')return v.text();\
return v==null?'':String(v);\
});\
}\
c.write=function(items){\
var list=items?Array.prototype.slice.call(items):[];\
return Promise.all(list.map(plain)).then(function(parts){\
return c.writeText(parts.join(''));\
});\
};\
})";

fn user_agent() -> String {
    let config = ffi::user_agent_config();
    config
        .configured
        .unwrap_or_else(|| ffi::user_agent_for_mode(config.compat_mode.as_deref()))
}

fn is_firefox() -> bool {
    let config = ffi::user_agent_config();
    match config.configured {
        Some(ua) => ua.contains("Firefox") && !ua.contains("Chrome"),
        None => config
            .compat_mode
            .is_some_and(|mode| mode.eq_ignore_ascii_case("firefox")),
    }
}

pub(crate) fn chrome_compat() -> bool {
    user_agent().contains("Chrome/")
}

fn hardware_concurrency() -> i32 {
    ffi::processor_count().clamp(1, 32) as i32
}

fn device_memory() -> i32 {
    let Some(bytes) = ffi::physical_memory_bytes() else {
        return 4;
    };
    let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let mut bucket = 1;
    while (bucket as f64) < gib && bucket < 4 {
        bucket *= 2;
    }
    bucket
}

fn method(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    let _ = scope.set(object, name, function);
}

fn c_method(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, which: CMethod) {
    let function = ffi::c_method(scope, name, arity, which);
    let _ = scope.set(object, name, function);
}

fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &str) {
    let value = scope.string(text);
    let _ = scope.set(object, key, value);
}

fn resolved(scope: &mut Scope<'_>, value: Value) -> Result<Value, Value> {
    let (promise, resolve, _reject) = scope.new_promise()?;
    scope.call(&resolve, &Value::undefined(), &[value])?;
    Ok(promise)
}

fn reject_dom(scope: &mut Scope<'_>, name: &str, message: &str) -> Result<Value, Value> {
    let error = scope.new_error();
    let name = scope.string(name);
    let _ = scope.define(&error, "name", name, Attributes::METHOD);
    let message = scope.string(message);
    let _ = scope.define(&error, "message", message, Attributes::METHOD);
    scope.rejected_promise(&error)
}

fn reject_message(scope: &mut Scope<'_>, message: &str) -> Result<Value, Value> {
    let error = scope.new_error();
    let (name, rest) = match message.split_once(':') {
        Some((name, rest)) => (name, rest),
        None => (message, message),
    };
    let is_dom_name = name.len() > 5
        && name.len() < 64
        && name.ends_with("Error")
        && name.bytes().all(|b| b.is_ascii_alphanumeric());
    if is_dom_name {
        set_str(scope, &error, "name", name);
        set_str(scope, &error, "message", rest.trim_start_matches(' '));
    } else {
        set_str(scope, &error, "message", message);
    }
    scope.rejected_promise(&error)
}

fn noop(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn returns_null(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(Value::null())
}

fn returns_false(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(Value::boolean(false))
}

fn empty_array(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(scope.new_array())
}

fn resolved_undefined(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    resolved(scope, Value::undefined())
}

fn resolved_false(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    resolved(scope, Value::boolean(false))
}

fn resolved_empty_array(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let array = scope.new_array();
    resolved(scope, array)
}

fn rejected_not_supported(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    reject_message(scope, "not supported")
}

fn reject_not_found(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    reject_dom(scope, "NotFoundError", "Requested device not found")
}

fn reject_not_allowed(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    reject_dom(scope, "NotAllowedError", "Read permission denied")
}

fn set_languages(scope: &mut Scope<'_>, navigator: &Value) {
    let languages = ffi::languages();
    set_str(
        scope,
        navigator,
        "language",
        languages.first().map_or("", String::as_str),
    );
    let list = scope.new_array();
    for (index, language) in languages.iter().enumerate() {
        let language = scope.string(language);
        let _ = scope.set_index(&list, index as u32, language);
    }
    let _ = scope.set(navigator, "languages", list);
}

fn client_hint_brands(scope: &mut Scope<'_>, full_version: bool) -> Value {
    let entries = [
        ("Southstar", if full_version { VERSION } else { "1" }),
        ("Not=A?Brand", if full_version { "24.0.0.0" } else { "24" }),
    ];
    let list = scope.new_array();
    for (index, (brand, version)) in entries.iter().enumerate() {
        let entry = scope.new_object();
        set_str(scope, &entry, "brand", brand);
        set_str(scope, &entry, "version", version);
        let _ = scope.set_index(&list, index as u32, entry);
    }
    list
}

fn client_hint_architecture() -> &'static str {
    if cfg!(any(target_arch = "x86_64", target_arch = "x86")) {
        "x86"
    } else if cfg!(any(target_arch = "aarch64", target_arch = "arm")) {
        "arm"
    } else {
        ""
    }
}

fn set_high_entropy_hint(scope: &mut Scope<'_>, object: &Value, key: &str) {
    let value = match key {
        "architecture" => scope.string(client_hint_architecture()),
        "bitness" => scope.string(if cfg!(target_pointer_width = "64") {
            "64"
        } else {
            "32"
        }),
        "formFactors" => {
            let list = scope.new_array();
            let factor = scope.string(if ffi::mobile_mode() {
                "Mobile"
            } else {
                "Desktop"
            });
            let _ = scope.set_index(&list, 0, factor);
            list
        }
        "fullVersionList" => client_hint_brands(scope, true),
        "model" | "platformVersion" => scope.string(""),
        "uaFullVersion" => scope.string(VERSION),
        "wow64" => Value::boolean(false),
        _ => return,
    };
    let _ = scope.set(object, key, value);
}

fn low_entropy_hints(scope: &mut Scope<'_>, object: &Value) {
    let brands = client_hint_brands(scope, false);
    let _ = scope.set(object, "brands", brands);
    let _ = scope.set(object, "mobile", Value::boolean(ffi::mobile_mode()));
    set_str(scope, object, "platform", &ffi::hint_platform());
}

fn high_entropy_values(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let hints = scope.new_object();
    low_entropy_hints(scope, &hints);
    if let Some(requested) = args.first().filter(|list| list.is_array()) {
        let length = scope.get(requested, "length")?;
        let length = scope.to_number(&length).unwrap_or(0.0) as u32;
        for index in 0..length {
            let Ok(item) = scope.get_index(requested, index) else {
                continue;
            };
            if let Ok(key) = scope.to_string(&item) {
                set_high_entropy_hint(scope, &hints, &key);
            }
        }
    }
    resolved(scope, hints)
}

fn ua_data_to_json(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let json = scope.new_object();
    for key in ["brands", "mobile", "platform"] {
        let value = scope.get(this, key).unwrap_or_else(|_| Value::undefined());
        let _ = scope.set(&json, key, value);
    }
    Ok(json)
}

fn user_agent_data(scope: &mut Scope<'_>) -> Value {
    let data = scope.new_object();
    low_entropy_hints(scope, &data);
    method(scope, &data, "getHighEntropyValues", 1, high_entropy_values);
    method(scope, &data, "toJSON", 0, ua_data_to_json);
    data
}

fn set_identity(scope: &mut Scope<'_>, navigator: &Value, ua: &str) {
    set_str(scope, navigator, "userAgent", ua);
    set_str(scope, navigator, "appName", "Netscape");
    set_str(scope, navigator, "appCodeName", "Mozilla");
    set_str(
        scope,
        navigator,
        "appVersion",
        ua.strip_prefix("Mozilla/").unwrap_or(ua),
    );
    set_str(scope, navigator, "platform", &ffi::navigator_platform());
}

fn set_privacy(scope: &mut Scope<'_>, navigator: &Value) {
    let config = ffi::user_agent_config();
    let do_not_track = if config.do_not_track {
        scope.string("1")
    } else {
        Value::null()
    };
    let _ = scope.set(navigator, "doNotTrack", do_not_track);
    let _ = scope.set(
        navigator,
        "globalPrivacyControl",
        Value::boolean(config.global_privacy_control),
    );
}

fn geolocation_error(
    scope: &mut Scope<'_>,
    _: &Value,
    _: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let Some(callback) = data.first() else {
        return Ok(Value::undefined());
    };
    if !scope.is_function(callback) {
        return Ok(Value::undefined());
    }
    let error = scope.new_object();
    let _ = scope.set(&error, "code", Value::int(1));
    set_str(scope, &error, "message", "User denied Geolocation");
    let _ = scope.set(&error, "PERMISSION_DENIED", Value::int(1));
    let _ = scope.set(&error, "POSITION_UNAVAILABLE", Value::int(2));
    let _ = scope.set(&error, "TIMEOUT", Value::int(3));
    let _ = scope.call(callback, &Value::undefined(), &[error]);
    Ok(Value::undefined())
}

fn get_current_position(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    if let Some(error_callback) = args.get(1)
        && scope.is_function(error_callback)
    {
        let job = scope.bound_function(
            "",
            0,
            geolocation_error,
            core::slice::from_ref(error_callback),
        );
        scope.enqueue_call(&job, &[])?;
    }
    Ok(Value::undefined())
}

fn watch_position(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    get_current_position(scope, this, args)?;
    Ok(Value::int(1))
}

fn clipboard_write_text(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let (promise, resolve, _reject) = scope.new_promise()?;
    let js = ffi::js_of(scope);
    let text = match args.first() {
        Some(value) => scope.to_string(value).unwrap_or_default(),
        None => String::new(),
    };
    match ffi::clipboard_write(js, &text) {
        None => reject_message(scope, "NotAllowedError: clipboard write not available"),
        Some(false) => reject_message(scope, "NotAllowedError: clipboard write denied"),
        Some(true) => {
            let _ = scope.call(&resolve, &Value::undefined(), &[]);
            Ok(promise)
        }
    }
}

fn clipboard(scope: &mut Scope<'_>) -> Value {
    let clipboard = scope.new_object();
    method(scope, &clipboard, "writeText", 1, clipboard_write_text);
    method(scope, &clipboard, "readText", 0, reject_not_allowed);
    method(scope, &clipboard, "read", 0, reject_not_allowed);
    if let Ok(install) = scope.eval_native_script(CLIPBOARD_WRITE, "<clipboard-write>") {
        let _ = scope.call(
            &install,
            &Value::undefined(),
            core::slice::from_ref(&clipboard),
        );
    }
    clipboard
}

fn permissions_query(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(descriptor) = args.first().filter(|value| value.is_object()) else {
        return reject_dom(scope, "TypeError", "Permission descriptor required");
    };
    let name = scope.get(descriptor, "name")?;
    let known = scope
        .to_string(&name)
        .is_ok_and(|name| PERMISSION_NAMES.contains(&name.as_str()));
    if !known {
        return reject_dom(scope, "TypeError", "Permission name is not supported");
    }
    let status = scope.new_object();
    set_str(scope, &status, "state", "prompt");
    let _ = scope.set(&status, "onchange", Value::null());
    let listeners = scope.new_array();
    let _ = scope.set(&status, "_listeners", listeners);
    ffi::bind_event_target(scope, &status);
    let global = scope.global();
    let constructor = scope.get(&global, "PermissionStatus")?;
    if constructor.is_object() {
        let prototype = scope.get(&constructor, "prototype")?;
        if prototype.is_object() {
            let _ = scope.set_prototype(&status, &prototype);
        }
    }
    resolved(scope, status)
}

fn protocol_scheme_valid(scheme: &str) -> bool {
    if let Some(rest) = scheme.strip_prefix("web+") {
        return !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_lowercase());
    }
    !scheme.is_empty()
        && PROTOCOL_SAFELIST
            .iter()
            .any(|safe| safe.eq_ignore_ascii_case(scheme))
}

fn register_protocol_handler(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.len() < 2 {
        return Err(scope.type_error(
            "Failed to execute 'registerProtocolHandler' on 'Navigator': 2 arguments required",
        ));
    }
    let scheme = scope.to_string(&args[0]).unwrap_or_default();
    let url = scope.to_string(&args[1]).ok();
    if !protocol_scheme_valid(&scheme) {
        let message = format!(
            "Failed to execute 'registerProtocolHandler' on 'Navigator': The scheme '{scheme}' doesn't belong to the scheme allowlist."
        );
        return Err(scope.dom_exception("SecurityError", &message));
    }
    match url {
        Some(url) if url.contains("%s") => Ok(Value::undefined()),
        url => {
            let message = format!(
                "Failed to execute 'registerProtocolHandler' on 'Navigator': The url provided ('{}') does not contain '%s'.",
                url.unwrap_or_default()
            );
            Err(scope.dom_exception("SyntaxError", &message))
        }
    }
}

fn get_battery(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let battery = scope.new_object();
    let _ = scope.set(&battery, "charging", Value::boolean(true));
    let _ = scope.set(&battery, "chargingTime", Value::number(0.0));
    let _ = scope.set(&battery, "dischargingTime", Value::number(f64::INFINITY));
    let _ = scope.set(&battery, "level", Value::number(1.0));
    for handler in [
        "onchargingchange",
        "onchargingtimechange",
        "ondischargingtimechange",
        "onlevelchange",
    ] {
        let _ = scope.set(&battery, handler, Value::null());
    }
    let listeners = scope.new_array();
    let _ = scope.set(&battery, "_listeners", listeners);
    ffi::bind_event_target(scope, &battery);
    resolved(scope, battery)
}

fn storage_estimate(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let estimate = scope.new_object();
    let _ = scope.set(&estimate, "usage", Value::int64(0));
    let _ = scope.set(&estimate, "quota", Value::int64(2 * 1024 * 1024 * 1024));
    let details = scope.new_object();
    let _ = scope.set(&estimate, "usageDetails", details);
    resolved(scope, estimate)
}

fn activation_is_active(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    let (transient, _) = ffi::user_activation(ffi::js_of(scope));
    Ok(Value::boolean(transient))
}

fn activation_has_been_active(
    scope: &mut Scope<'_>,
    _: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let (_, ever) = ffi::user_activation(ffi::js_of(scope));
    Ok(Value::boolean(ever))
}

fn network_information(scope: &mut Scope<'_>) -> Value {
    let connection = scope.new_object();
    set_str(scope, &connection, "effectiveType", "4g");
    set_str(scope, &connection, "type", "wifi");
    let _ = scope.set(&connection, "downlink", Value::number(10.0));
    let _ = scope.set(&connection, "downlinkMax", Value::number(10.0));
    let _ = scope.set(&connection, "rtt", Value::int(50));
    let _ = scope.set(&connection, "saveData", Value::boolean(false));
    let listeners = scope.new_array();
    let _ = scope.set(&connection, "_listeners", listeners);
    ffi::bind_event_target(scope, &connection);
    connection
}

fn geolocation(scope: &mut Scope<'_>) -> Value {
    let geolocation = scope.new_object();
    method(
        scope,
        &geolocation,
        "getCurrentPosition",
        3,
        get_current_position,
    );
    method(scope, &geolocation, "watchPosition", 3, watch_position);
    method(scope, &geolocation, "clearWatch", 1, noop);
    geolocation
}

fn permissions(scope: &mut Scope<'_>) -> Value {
    let permissions = scope.new_object();
    method(scope, &permissions, "query", 1, permissions_query);
    let _ = scope.define_to_string_tag(&permissions, "Permissions");
    permissions
}

fn media_devices(scope: &mut Scope<'_>) -> Value {
    let devices = scope.new_object();
    method(scope, &devices, "getUserMedia", 1, reject_not_found);
    method(scope, &devices, "getDisplayMedia", 1, reject_not_found);
    method(scope, &devices, "enumerateDevices", 0, resolved_empty_array);
    method(scope, &devices, "getSupportedConstraints", 0, noop);
    let listeners = scope.new_array();
    let _ = scope.set(&devices, "_listeners", listeners);
    ffi::bind_event_target(scope, &devices);
    c_method(scope, &devices, "dispatchEvent", 1, CMethod::DispatchEvent);
    let _ = scope.set(&devices, "ondevicechange", Value::null());
    let _ = scope.define_to_string_tag(&devices, "MediaDevices");
    devices
}

fn empty_plugin_array(scope: &mut Scope<'_>, with_refresh: bool) -> Value {
    let list = scope.new_object();
    let _ = scope.define(&list, "length", Value::int(0), ENUMERABLE_CONFIGURABLE);
    method(scope, &list, "item", 1, returns_null);
    method(scope, &list, "namedItem", 1, returns_null);
    if with_refresh {
        method(scope, &list, "refresh", 0, noop);
    }
    list
}

fn user_activation(scope: &mut Scope<'_>) -> Value {
    let activation = scope.new_object();
    for (name, getter) in [
        ("hasBeenActive", activation_has_been_active as NativeFn),
        ("isActive", activation_is_active as NativeFn),
    ] {
        let getter = scope.function(name, 0, getter);
        let _ = scope.define_accessor(
            &activation,
            name,
            Some(&getter),
            None,
            ENUMERABLE_CONFIGURABLE,
        );
    }
    activation
}

fn storage_manager(scope: &mut Scope<'_>) -> Value {
    let storage = scope.new_object();
    method(scope, &storage, "estimate", 0, storage_estimate);
    method(scope, &storage, "persist", 0, resolved_false);
    method(scope, &storage, "persisted", 0, resolved_false);
    storage
}

pub(crate) fn window(scope: &mut Scope<'_>) -> Value {
    let ua = user_agent();
    let firefox = is_firefox();
    let navigator = scope.new_object();
    set_identity(scope, &navigator, &ua);
    set_languages(scope, &navigator);
    let _ = scope.set(&navigator, "onLine", Value::boolean(true));
    set_privacy(scope, &navigator);
    let _ = scope.set(&navigator, "cookieEnabled", Value::boolean(true));
    let _ = scope.set(
        &navigator,
        "hardwareConcurrency",
        Value::int(hardware_concurrency()),
    );
    set_str(
        scope,
        &navigator,
        "vendor",
        if firefox { "" } else { "Google Inc." },
    );
    set_str(scope, &navigator, "product", "Gecko");
    set_str(
        scope,
        &navigator,
        "productSub",
        if firefox { "20100101" } else { "20030107" },
    );
    if firefox {
        set_str(scope, &navigator, "oscpu", &ffi::navigator_platform());
        set_str(scope, &navigator, "buildID", "20181001000000");
    }
    let _ = scope.set(&navigator, "maxTouchPoints", Value::int(0));
    let _ = scope.set(&navigator, "deviceMemory", Value::int(device_memory()));
    let _ = scope.set(&navigator, "pdfViewerEnabled", Value::boolean(true));
    let _ = scope.set(&navigator, "webdriver", Value::boolean(false));

    if ua.contains("Chrome/") {
        let connection = network_information(scope);
        let _ = scope.set(&navigator, "connection", connection);
    }
    let geolocation = geolocation(scope);
    let _ = scope.set(&navigator, "geolocation", geolocation);
    let clipboard = clipboard(scope);
    let _ = scope.set(&navigator, "clipboard", clipboard);
    let permissions = permissions(scope);
    let _ = scope.set(&navigator, "permissions", permissions);
    let devices = media_devices(scope);
    let _ = scope.set(&navigator, "mediaDevices", devices);

    method(scope, &navigator, "share", 1, rejected_not_supported);
    method(scope, &navigator, "canShare", 1, noop);
    method(scope, &navigator, "vibrate", 1, noop);
    c_method(scope, &navigator, "sendBeacon", 2, CMethod::SendBeacon);
    method(
        scope,
        &navigator,
        "registerProtocolHandler",
        2,
        register_protocol_handler,
    );
    method(scope, &navigator, "unregisterProtocolHandler", 2, noop);

    if ffi::has_client_hints(&ua) {
        let data = user_agent_data(scope);
        let _ = scope.set(&navigator, "userAgentData", data);
    }

    let plugins = empty_plugin_array(scope, true);
    let _ = scope.set(&navigator, "plugins", plugins);
    let mime_types = empty_plugin_array(scope, false);
    let _ = scope.set(&navigator, "mimeTypes", mime_types);

    method(scope, &navigator, "javaEnabled", 0, returns_false);
    method(scope, &navigator, "taintEnabled", 0, returns_false);
    method(scope, &navigator, "getAutoplayPolicy", 1, noop);
    method(scope, &navigator, "getBattery", 0, get_battery);
    method(scope, &navigator, "getGamepads", 0, empty_array);
    method(
        scope,
        &navigator,
        "requestMIDIAccess",
        1,
        rejected_not_supported,
    );
    c_method(
        scope,
        &navigator,
        "requestMediaKeySystemAccess",
        2,
        CMethod::EmeRequestAccess,
    );

    let capabilities = scope.new_object();
    c_method(
        scope,
        &capabilities,
        "decodingInfo",
        1,
        CMethod::MediaCapabilitiesInfo,
    );
    c_method(
        scope,
        &capabilities,
        "encodingInfo",
        1,
        CMethod::MediaCapabilitiesInfo,
    );
    let _ = scope.set(&navigator, "mediaCapabilities", capabilities);

    set_str(scope, &navigator, "vendorSub", "");
    let activation = user_activation(scope);
    let _ = scope.set(&navigator, "userActivation", activation);
    let storage = storage_manager(scope);
    let _ = scope.set(&navigator, "storage", storage);

    method(
        scope,
        &navigator,
        "getInstalledRelatedApps",
        0,
        resolved_empty_array,
    );
    method(scope, &navigator, "setAppBadge", 1, resolved_undefined);
    method(scope, &navigator, "clearAppBadge", 0, resolved_undefined);
    navigator
}

pub(crate) fn worker(scope: &mut Scope<'_>) -> Value {
    let ua = user_agent();
    let navigator = scope.new_object();
    set_identity(scope, &navigator, &ua);
    set_str(scope, &navigator, "product", "Gecko");
    let _ = scope.set(&navigator, "deviceMemory", Value::int(device_memory()));
    set_languages(scope, &navigator);
    let _ = scope.set(&navigator, "onLine", Value::boolean(true));
    let _ = scope.set(
        &navigator,
        "hardwareConcurrency",
        Value::int(hardware_concurrency()),
    );
    set_privacy(scope, &navigator);
    if ffi::has_client_hints(&ua) {
        let data = user_agent_data(scope);
        let _ = scope.set(&navigator, "userAgentData", data);
    }
    navigator
}
