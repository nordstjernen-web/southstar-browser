//! Southstar — FormData: its methods and iterators, construction from a form and submitter, and the multipart/form-data body it serializes to.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{ancestors, children, controls, select};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi;
use crate::submit::is_submit_trigger;
use crate::{
    Element, JsResult, c_bytes, form_owner, index, length, name_is, named, non_empty, text_is,
    text_is_any, type_of, until_nul,
};

const ITERATORS: &str = "(function(P,check){ function* walk(fd,kind){  for(var i=0;i<fd._entries.length;i++){   var e=fd._entries[i];   yield kind===0?[e[0],e[1]]:kind===1?e[0]:e[1];  } } function make(name,kind){  var f=({[name]:function(){check(this);return walk(this,kind);}})[name];  Object.defineProperty(f,'length',{value:0,configurable:true});  Object.defineProperty(P,name,{value:f,writable:true,enumerable:true,configurable:true});  return f; } var entries=make('entries',0); make('keys',1);make('values',2); Object.defineProperty(P,Symbol.iterator,{value:entries,writable:true,configurable:true});})";

const METHODS: [(&str, u32, NativeFn); 7] = [
    ("append", 2, append),
    ("delete", 1, delete),
    ("get", 1, get),
    ("getAll", 1, get_all),
    ("has", 1, has),
    ("set", 2, set),
    ("forEach", 1, for_each),
];

fn brand(scope: &mut Scope<'_>, this: &Value) -> JsResult<()> {
    if ffi::is_form_data(scope, this) {
        Ok(())
    } else {
        Err(scope.type_error("Illegal invocation"))
    }
}

fn too_few(scope: &mut Scope<'_>, method: &str, need: usize, have: usize) -> Value {
    let plural = if need == 1 { "" } else { "s" };
    scope.type_error(&format!(
        "Failed to execute '{method}' on 'FormData': {need} argument{plural} required, but only {have} present."
    ))
}

fn arity(scope: &mut Scope<'_>, method: &str, need: usize, args: &[Value]) -> JsResult<()> {
    if args.len() < need {
        Err(too_few(scope, method, need, args.len()))
    } else {
        Ok(())
    }
}

fn entries(scope: &mut Scope<'_>, this: &Value) -> Value {
    if let Ok(entries) = scope.get(this, "_entries") {
        if entries.is_array() {
            return entries;
        }
    }
    let entries = scope.new_array();
    let _ = scope.set(this, "_entries", entries.clone());
    entries
}

fn global_get(scope: &mut Scope<'_>, name: &str) -> Value {
    let global = scope.global();
    scope
        .get(&global, name)
        .unwrap_or_else(|_| Value::undefined())
}

fn is_instance(scope: &mut Scope<'_>, value: &Value, constructor: &Value) -> bool {
    scope.is_constructor(constructor) && value.is_object() && scope.instance_of(value, constructor)
}

fn file_of(
    scope: &mut Scope<'_>,
    value: &Value,
    filename: Option<&Value>,
    file_ctor: &Value,
    is_file: bool,
) -> JsResult {
    let name = match filename {
        Some(filename) => scope.to_string_value(filename)?,
        None => scope.string("blob"),
    };
    let parts = scope.new_array();
    scope.set_index(&parts, 0, value.clone())?;
    let opts = scope.new_object();
    let ty = scope.get(value, "type")?;
    scope.set(&opts, "type", ty)?;
    if is_file {
        let modified = scope.get(value, "lastModified")?;
        scope.set(&opts, "lastModified", modified)?;
    }
    if scope.is_constructor(file_ctor) {
        scope.construct(file_ctor, &[parts, name, opts])
    } else {
        Ok(value.clone())
    }
}

fn entry_value(scope: &mut Scope<'_>, value: &Value, filename: Option<&Value>) -> JsResult {
    let blob_ctor = global_get(scope, "Blob");
    let file_ctor = global_get(scope, "File");
    let is_blob = is_instance(scope, value, &blob_ctor);
    let is_file = is_blob && is_instance(scope, value, &file_ctor);
    if !is_blob {
        if filename.is_some() {
            return Err(scope.type_error(
                "Failed to execute on 'FormData': parameter 2 is not of type 'Blob'.",
            ));
        }
        return scope.to_string_value(value);
    }
    if is_file && filename.is_none() {
        return Ok(value.clone());
    }
    file_of(scope, value, filename, &file_ctor, is_file)
}

fn make_pair(scope: &mut Scope<'_>, args: &[Value]) -> JsResult {
    let name = scope.to_string_value(&args[0])?;
    let value = entry_value(scope, &args[1], args.get(2))?;
    let pair = scope.new_array();
    scope.set_index(&pair, 0, name)?;
    scope.set_index(&pair, 1, value)?;
    Ok(pair)
}

fn push(scope: &mut Scope<'_>, fd: &Value, pair: Value) {
    let list = entries(scope, fd);
    let end = length(scope, &list);
    let _ = scope.set_index(&list, end, pair);
}

fn append(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    brand(scope, this)?;
    arity(scope, "append", 2, args)?;
    let pair = make_pair(scope, args)?;
    push(scope, this, pair);
    Ok(Value::undefined())
}

fn pair_named(scope: &mut Scope<'_>, pair: &Value, name: &[u8]) -> bool {
    let key = index(scope, pair, 0);
    c_bytes(scope, &key).is_some_and(|key| key == name)
}

fn key_arg(scope: &mut Scope<'_>, key: &Value) -> JsResult<Vec<u8>> {
    scope.to_bytes(key).map(until_nul)
}

fn set(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    brand(scope, this)?;
    arity(scope, "set", 2, args)?;
    let fresh = make_pair(scope, args)?;
    let key = index(scope, &fresh, 0);
    let key = c_bytes(scope, &key);
    let list = entries(scope, this);
    let len = length(scope, &list);
    let kept = scope.new_array();
    let mut out = 0u32;
    let mut placed = false;
    for i in 0..len {
        let pair = index(scope, &list, i);
        let replaced = match &key {
            Some(key) => pair_named(scope, &pair, key),
            None => false,
        };
        if !replaced {
            let _ = scope.set_index(&kept, out, pair);
            out += 1;
        } else if !placed {
            let _ = scope.set_index(&kept, out, fresh.clone());
            out += 1;
            placed = true;
        }
    }
    if !placed {
        let _ = scope.set_index(&kept, out, fresh);
    }
    let _ = scope.set(this, "_entries", kept);
    Ok(Value::undefined())
}

fn get_all(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    brand(scope, this)?;
    arity(scope, "getAll", 1, args)?;
    let key = key_arg(scope, &args[0])?;
    let out = scope.new_array();
    let list = entries(scope, this);
    let len = length(scope, &list);
    let mut found = 0u32;
    for i in 0..len {
        let pair = index(scope, &list, i);
        if pair_named(scope, &pair, &key) {
            let value = index(scope, &pair, 1);
            let _ = scope.set_index(&out, found, value);
            found += 1;
        }
    }
    Ok(out)
}

fn lookup(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    method: &str,
) -> JsResult<Option<Value>> {
    brand(scope, this)?;
    arity(scope, method, 1, args)?;
    let key = key_arg(scope, &args[0])?;
    let list = entries(scope, this);
    let len = length(scope, &list);
    for i in 0..len {
        let pair = index(scope, &list, i);
        if pair_named(scope, &pair, &key) {
            return Ok(Some(index(scope, &pair, 1)));
        }
    }
    Ok(None)
}

fn get(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    Ok(lookup(scope, this, args, "get")?.unwrap_or_else(Value::null))
}

fn has(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    Ok(Value::boolean(lookup(scope, this, args, "has")?.is_some()))
}

fn delete(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    brand(scope, this)?;
    arity(scope, "delete", 1, args)?;
    let name = key_arg(scope, &args[0])?;
    let list = entries(scope, this);
    let len = length(scope, &list);
    let kept = scope.new_array();
    let mut out = 0u32;
    for i in 0..len {
        let pair = index(scope, &list, i);
        if !pair_named(scope, &pair, &name) {
            let _ = scope.set_index(&kept, out, pair);
            out += 1;
        }
    }
    let _ = scope.set(this, "_entries", kept);
    Ok(Value::undefined())
}

fn for_each(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    brand(scope, this)?;
    arity(scope, "forEach", 1, args)?;
    if !scope.is_function(&args[0]) {
        return Err(scope.type_error(
            "Failed to execute 'forEach' on 'FormData': parameter 1 is not of type 'Function'.",
        ));
    }
    let this_arg = args.get(1).cloned().unwrap_or_else(Value::undefined);
    let mut i = 0u32;
    loop {
        let list = entries(scope, this);
        if i >= length(scope, &list) {
            break;
        }
        let pair = index(scope, &list, i);
        let key = index(scope, &pair, 0);
        let value = index(scope, &pair, 1);
        scope.call(&args[0], &this_arg, &[value, key, this.clone()])?;
        i += 1;
    }
    Ok(Value::undefined())
}

fn check(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let target = args.first().cloned().unwrap_or_else(Value::undefined);
    brand(scope, &target)?;
    Ok(Value::undefined())
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let ctor = scope
        .get(global, "FormData")
        .unwrap_or_else(|_| Value::undefined());
    let proto = if ctor.is_object() {
        scope
            .get(&ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    };
    if !proto.is_object() {
        return;
    }
    for (name, arity, f) in METHODS {
        let function = scope.function(name, arity, f);
        let _ = scope.set(&proto, name, function);
    }
    if let Ok(helper) = scope.eval_native_script(ITERATORS, "<formdata>") {
        let check = scope.function("check", 1, check);
        let _ = scope.call(&helper, &Value::undefined(), &[proto, check]);
    }
}

fn append_pair(scope: &mut Scope<'_>, fd: &Value, name: &[u8], value: &[u8]) {
    let pair = scope.new_array();
    let name = scope.string_from_bytes(name);
    let value = scope.string_from_bytes(value);
    let _ = scope.set_index(&pair, 0, name);
    let _ = scope.set_index(&pair, 1, value);
    push(scope, fd, pair);
}

fn option_disabled(option: Element) -> bool {
    if controls::effectively_disabled(option) {
        return true;
    }
    for p in ancestors(option) {
        if named(p, "select") {
            return false;
        }
        if named(p, "optgroup") && p.attr(c"disabled").is_some() {
            return true;
        }
    }
    false
}

fn option_selected(option: Element) -> bool {
    named(option, "option") && option.attr(c"selected").is_some() && !option_disabled(option)
}

fn append_option(scope: &mut Scope<'_>, fd: &Value, name: &[u8], option: Element) {
    let value = until_nul(select::option_value(option));
    append_pair(scope, fd, name, &value);
}

fn append_select(scope: &mut Scope<'_>, fd: &Value, select_el: Element, name: &[u8]) {
    if select_el.attr(c"multiple").is_none() {
        if let Some(option) = select::chosen_option(select_el).filter(|o| !option_disabled(*o)) {
            append_option(scope, fd, name, option);
        }
        return;
    }
    for child in children(select_el) {
        if named(child, "optgroup") {
            if controls::effectively_disabled(child) || child.attr(c"disabled").is_some() {
                continue;
            }
            for option in children(child).filter(|o| option_selected(*o)) {
                append_option(scope, fd, name, option);
            }
        } else if option_selected(child) {
            append_option(scope, fd, name, child);
        }
    }
}

fn append_submitter(scope: &mut Scope<'_>, fd: &Value, el: Element, name: Option<&[u8]>) {
    let is_image = name_is(el, "input") && text_is(type_of(el), "image");
    if is_image {
        match name {
            Some(name) => {
                let x = [name, b".x"].concat();
                let y = [name, b".y"].concat();
                append_pair(scope, fd, &x, b"0");
                append_pair(scope, fd, &y, b"0");
            }
            None => {
                append_pair(scope, fd, b"x", b"0");
                append_pair(scope, fd, b"y", b"0");
            }
        }
    } else if let Some(name) = name {
        let value = el.attr(c"value").map_or(&b""[..], |v| v.to_bytes());
        append_pair(scope, fd, name, value);
    }
}

fn populate(scope: &mut Scope<'_>, fd: &Value, form: Element, submitter: Option<Element>) {
    for el in crate::listed_controls(form, true) {
        if name_is(el, "object") || controls::effectively_disabled(el) {
            continue;
        }
        let name = non_empty(el.attr(c"name")).map(|n| n.to_bytes());
        let ty = type_of(el);
        let is_button =
            name_is(el, "button") || text_is_any(ty, &["submit", "button", "reset", "image"]);
        if is_button {
            if Some(el) == submitter {
                append_submitter(scope, fd, el, name);
            }
            continue;
        }
        let Some(name) = name else { continue };
        if text_is(ty, "file") {
            continue;
        }
        let checkable = text_is_any(ty, &["checkbox", "radio"]);
        if checkable && !controls::is_checked(el) {
            continue;
        }
        if name_is(el, "select") {
            append_select(scope, fd, el, name);
            continue;
        }
        let value = if name_is(el, "textarea") {
            until_nul(controls::textarea_value(Some(el)))
        } else {
            match controls::used_value(el) {
                Some(value) => value.to_bytes().to_vec(),
                None if checkable => b"on".to_vec(),
                None => Vec::new(),
            }
        };
        append_pair(scope, fd, name, &value);
    }
}

pub(crate) fn construct(scope: &mut Scope<'_>, new_target: &Value, args: &[Value]) -> JsResult {
    let fd = ffi::construct_form_data(scope, new_target)?;
    let list = scope.new_array();
    let _ = scope.set(&fd, "_entries", list);
    let Some(form_arg) = args.first().filter(|a| !a.is_undefined()) else {
        return Ok(fd);
    };
    let Some(form) = ffi::element(form_arg).filter(|f| f.name() == Some(c"form")) else {
        return Err(scope.type_error("FormData constructor argument must be a form"));
    };
    let mut submitter = None;
    if let Some(arg) = args.get(1).filter(|a| !a.is_null() && !a.is_undefined()) {
        let Some(el) = ffi::element(arg).filter(|el| is_submit_trigger(*el)) else {
            return Err(scope.type_error("FormData: the submitter must be a submit button"));
        };
        if form_owner(el) != Some(form) {
            return Err(ffi::not_found(
                scope,
                "FormData: the submitter is not owned by this form",
            ));
        }
        submitter = Some(el);
    }
    populate(scope, &fd, form, submitter);
    Ok(fd)
}

fn quote_field(out: &mut Vec<u8>, text: &[u8]) {
    for &c in text {
        match c {
            b'"' => out.extend_from_slice(b"%22"),
            b'\r' => out.extend_from_slice(b"%0D"),
            b'\n' => out.extend_from_slice(b"%0A"),
            _ => out.push(c),
        }
    }
}

fn is_blob(scope: &mut Scope<'_>, value: &Value) -> bool {
    if !value.is_object() {
        return false;
    }
    scope
        .get(value, "__ndBlobBytes")
        .is_ok_and(|bytes| !bytes.is_undefined() && !bytes.is_null())
}

fn string_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    if value.is_string() {
        c_bytes(scope, value)
    } else {
        None
    }
}

fn serialize_blob(scope: &mut Scope<'_>, body: &mut Vec<u8>, value: &Value, filename: &Value) {
    let filename = if filename.is_undefined() {
        scope
            .get(value, "name")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        filename.clone()
    };
    let filename = string_bytes(scope, &filename);
    body.extend_from_slice(b"; filename=\"");
    quote_field(body, filename.as_deref().unwrap_or(b"blob"));
    body.extend_from_slice(b"\"\r\n");
    let ty = scope
        .get(value, "type")
        .unwrap_or_else(|_| Value::undefined());
    let ty: Vec<u8> = string_bytes(scope, &ty)
        .unwrap_or_default()
        .into_iter()
        .filter(|&c| c >= 0x20 && c != 0x7f)
        .collect();
    body.extend_from_slice(b"Content-Type: ");
    body.extend_from_slice(if ty.is_empty() {
        b"application/octet-stream"
    } else {
        &ty
    });
    body.extend_from_slice(b"\r\n\r\n");
    body.extend_from_slice(&ffi::blob_bytes(scope, value));
    body.extend_from_slice(b"\r\n");
}

pub(crate) fn serialize(scope: &mut Scope<'_>, fd: &Value) -> (Vec<u8>, Vec<u8>) {
    let boundary = ffi::multipart_boundary();
    let content_type = [&b"multipart/form-data; boundary="[..], &boundary].concat();
    let mut body = Vec::new();
    let list = scope
        .get(fd, "_entries")
        .unwrap_or_else(|_| Value::undefined());
    if list.is_array() {
        let len = length(scope, &list);
        for i in 0..len {
            let pair = index(scope, &list, i);
            if !pair.is_array() {
                continue;
            }
            let key = index(scope, &pair, 0);
            let value = index(scope, &pair, 1);
            let filename = index(scope, &pair, 2);
            let key = c_bytes(scope, &key).unwrap_or_default();
            body.extend_from_slice(b"--");
            body.extend_from_slice(&boundary);
            body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
            quote_field(&mut body, &key);
            body.push(b'"');
            if is_blob(scope, &value) {
                serialize_blob(scope, &mut body, &value, &filename);
            } else {
                body.extend_from_slice(b"\r\n\r\n");
                body.extend_from_slice(&scope.to_bytes(&value).unwrap_or_default());
                body.extend_from_slice(b"\r\n");
            }
        }
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(&boundary);
    body.extend_from_slice(b"--\r\n");
    (body, content_type)
}
