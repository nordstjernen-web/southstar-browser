//! Southstar — Request and Response: their constructors, body extraction and the body mixin methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::headers;
use crate::{JsResult, c_bytes, is_nullish, prop, set, set_str};

const BODY_CONSUMER_SOURCE: &str = "(function(){\
 function encodeStr(s){\
  return (typeof TextEncoder === 'function')\
   ? new TextEncoder().encode(s)\
   : (function(){ var a = new Uint8Array(s.length);\
      for (var i = 0; i < s.length; i++) a[i] = s.charCodeAt(i) & 0xff;\
      return a; })();\
 }\
 function isStream(v){ return v && typeof v.getReader === 'function'; }\
 function dropRaw(r){try{if(Object.prototype.hasOwnProperty.call(r,'body'))delete r.body;}catch(e){}}\
 function normalize(r){\
  if (r._bodyBuffer instanceof ArrayBuffer || r._bodyStream) {\
   if (!('_bodyNull' in r))\
    try { Object.defineProperty(r,'_bodyNull',{value:false,configurable:true}); } catch(e){}\
   dropRaw(r);\
   return;\
  }\
  var raw=r.body;\
  if (isStream(raw)) {\
   try { Object.defineProperty(r,'_bodyStream',{value:raw,configurable:true}); } catch(e){ r._bodyStream = raw; }\
   try { Object.defineProperty(r,'_bodyNull',{value:false,configurable:true}); } catch(e){}\
   dropRaw(r);\
   return;\
  }\
  var isNull = (raw === null || raw === undefined);\
  var s = (typeof raw === 'string') ? raw : (raw == null ? '' : String(raw));\
  var u8 = encodeStr(s);\
  var ab = new ArrayBuffer(u8.length); new Uint8Array(ab).set(u8);\
  try { Object.defineProperty(r,'_bodyBuffer',{value:ab,configurable:true}); } catch(e){ r._bodyBuffer = ab; }\
  try { Object.defineProperty(r,'_bodyNull',{value:isNull,configurable:true}); } catch(e){}\
  dropRaw(r);\
 }\
 function bytes(r){\
  return (r._bodyBuffer instanceof ArrayBuffer)\
   ? new Uint8Array(r._bodyBuffer) : new Uint8Array(0);\
 }\
 function readAll(r){\
  if (r._bodyBuffer instanceof ArrayBuffer) return Promise.resolve(new Uint8Array(r._bodyBuffer));\
  if (!r._bodyStream) return Promise.resolve(new Uint8Array(0));\
  var rd = r._bodyStream.getReader(), chunks = [], total = 0;\
  return (function pump(){ return rd.read().then(function(x){\
   if (x.done) { var out = new Uint8Array(total), o = 0;\
    chunks.forEach(function(c){ out.set(c, o); o += c.length; }); return out; }\
   var c = (x.value instanceof Uint8Array) ? x.value : new Uint8Array(x.value);\
   chunks.push(c); total += c.length; return pump(); }); })();\
 }\
 function decode(u8){\
  return (typeof TextDecoder === 'function')\
   ? new TextDecoder('utf-8').decode(u8)\
   : String.fromCharCode.apply(null, Array.from(u8));\
 }\
 function consume(r){\
  if (r.bodyUsed) return Promise.reject(new TypeError('Already read'));\
  r.bodyUsed = true; return readAll(r);\
 }\
 var streamCache = new WeakMap();\
 function bodyGetter(){\
  var r=this;if(r._bodyNull)return null;if(r._bodyStream)return r._bodyStream;\
  if(streamCache.has(r))return streamCache.get(r);\
  if(typeof ReadableStream!=='function'){streamCache.set(r,null);return null;}\
  var u=bytes(r);var stream=new ReadableStream({start:function(c){\
   if(u&&u.length)c.enqueue(u);c.close();}});streamCache.set(r,stream);return stream;\
 }\
 function text(){var r=this;return consume(r).then(function(u){return decode(u);});}\
 function json(){var r=this;return consume(r).then(function(u){return JSON.parse(decode(u));});}\
 function arrayBuffer(){var r=this;return consume(r).then(function(u){\
  var ab=new ArrayBuffer(u.length);new Uint8Array(ab).set(u);return ab;});}\
 function bodyBytes(){return consume(this);}\
 function blob(){var r=this;var ct=r.headers&&r.headers.get&&\
  r.headers.get('content-type')||'';return consume(r).then(function(u){\
   return new Blob([u],{type:ct});});}\
 function formData(){var r=this;return consume(r).then(function(u){\
  var fd=new FormData();var s=decode(u);if(s)new URLSearchParams(s).forEach(\
   function(v,k){fd.append(k,v);});return fd;});}\
 function clone(){var r=this;var c=Object.create(Object.getPrototypeOf(r));\
  Object.getOwnPropertyNames(r).forEach(function(k){if(k!=='body'&&k!=='bodyUsed')\
   try{Object.defineProperty(c,k,Object.getOwnPropertyDescriptor(r,k));}catch(e){}});\
  if(r._bodyBuffer instanceof ArrayBuffer)\
   try{Object.defineProperty(c,'_bodyBuffer',{value:r._bodyBuffer,configurable:true});}catch(e){}\
  if(r._bodyStream)\
   try{Object.defineProperty(c,'_bodyStream',{value:r._bodyStream,configurable:true});}catch(e){}\
  c.bodyUsed=false;return attach(c);\
 }\
 function attach(r){\
  normalize(r);\
  var p=Object.getPrototypeOf(r);\
  if(!p||!Object.getOwnPropertyDescriptor(p,'body'))\
   try{Object.defineProperty(r,'body',{configurable:true,enumerable:true,\
    get:bodyGetter});}catch(e){}\
  if(!p||typeof p.text!=='function'){r.text=text;r.json=json;\
   r.arrayBuffer=arrayBuffer;r.bytes=bodyBytes;r.blob=blob;\
   r.formData=formData;r.clone=clone;}\
  return r;\
 }\
 return attach;\
})()";

const RESPONSE_STATICS_SOURCE: &str = "(function(){if(typeof Response!=='function')return;\
 function error(){var r=new Response(null);\
  Object.defineProperties(r,{type:{value:'error',configurable:true},\
   status:{value:0,configurable:true},ok:{value:false,configurable:true},\
   statusText:{value:'',configurable:true}});return r;}\
 function json(data,init){var body=JSON.stringify(data);init=init||{};\
  var headers=new Headers(init.headers);if(!headers.has('content-type'))\
   headers.set('content-type','application/json');\
  var next={status:init.status,statusText:init.statusText,headers:headers};\
  return new Response(body,next);}\
 function redirect(url,status){status=status===undefined?302:Number(status);\
  if([301,302,303,307,308].indexOf(status)<0)throw new RangeError('Invalid redirect status');\
  return new Response(null,{status:status,headers:{location:String(url)}});}\
 Object.defineProperties(Response,{\
  error:{value:error,writable:true,configurable:true},\
  json:{value:json,writable:true,configurable:true},\
  redirect:{value:redirect,writable:true,configurable:true}});\
})()";

const BODY_METHODS: [&str; 7] = [
    "arrayBuffer",
    "blob",
    "clone",
    "formData",
    "json",
    "text",
    "bytes",
];

const READ_ONLY: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

const RAW_BODY: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn attach_consumers(scope: &mut Scope<'_>, object: &Value) {
    let js = Js::of(scope);
    if js.is_null() {
        return;
    }
    let cached = crate::with_page(js, |page| page.body_helper.clone()).flatten();
    let helper = match cached {
        Some(helper) => helper,
        None => match scope.eval_native_script(BODY_CONSUMER_SOURCE, "<body-consumer>") {
            Ok(helper) => {
                crate::with_page(js, |page| page.body_helper = Some(helper.clone()));
                helper
            }
            Err(_) => return,
        },
    };
    crate::call_ignoring(
        scope,
        &helper,
        &Value::undefined(),
        core::slice::from_ref(object),
    );
}

fn set_raw(scope: &mut Scope<'_>, object: &Value, value: Value) {
    let _ = scope.define(object, "body", value, RAW_BODY);
}

fn extract_buffer(scope: &mut Scope<'_>, body: &Value) -> Option<Value> {
    let bytes = scope
        .view_data(body)
        .or_else(|| scope.array_buffer_bytes(body))?;
    scope.new_array_buffer(&bytes).ok()
}

fn install_body(scope: &mut Scope<'_>, object: &Value, body: &Value, null_when_empty: bool) {
    if is_nullish(body) {
        let raw = if null_when_empty {
            Value::null()
        } else {
            scope.string("")
        };
        set_raw(scope, object, raw);
        return;
    }
    if body.is_string() {
        if let Ok(text) = scope.to_bytes(body)
            && let Ok(buffer) = scope.new_array_buffer(&text)
        {
            set(scope, object, "_bodyBuffer", buffer);
        }
        set_raw(scope, object, body.clone());
        set_str(scope, object, "_bodyCT", b"text/plain;charset=UTF-8");
        return;
    }
    if body.is_object() {
        if let Some(serialized) = ffi::serialize_form_body(scope, body) {
            if let Ok(buffer) = scope.new_array_buffer(&serialized.body) {
                set(scope, object, "_bodyBuffer", buffer);
            }
            if let Some(content_type) = serialized.content_type {
                set_str(scope, object, "_bodyCT", &content_type);
            }
            let empty = scope.string("");
            set_raw(scope, object, empty);
            return;
        }
        if let Some(buffer) = extract_buffer(scope, body) {
            set(scope, object, "_bodyBuffer", buffer);
            let raw = if null_when_empty {
                body.clone()
            } else {
                scope.string("")
            };
            set_raw(scope, object, raw);
            return;
        }
        if !is_nullish(&prop(scope, body, "__ndBlobBytes"))
            && let Some(bytes) = ffi::blob_bytes(scope, body)
        {
            if let Ok(buffer) = scope.new_array_buffer(&bytes) {
                set(scope, object, "_bodyBuffer", buffer);
            }
            let kind = prop(scope, body, "type");
            if kind.is_string()
                && let Some(kind) = c_bytes(scope, &kind).filter(|k| !k.is_empty())
            {
                set_str(scope, object, "_bodyCT", &kind);
            }
            let empty = scope.string("");
            set_raw(scope, object, empty);
            return;
        }
        let get_reader = prop(scope, body, "getReader");
        if scope.is_function(&get_reader) {
            set_raw(scope, object, body.clone());
            return;
        }
    }
    let text = scope
        .to_string_value(body)
        .unwrap_or_else(|_| scope.string(""));
    set_raw(scope, object, text);
}

fn apply_inferred_content_type(scope: &mut Scope<'_>, object: &Value) {
    let inferred = prop(scope, object, "_bodyCT");
    if !inferred.is_string() {
        return;
    }
    set(scope, object, "_bodyCT", Value::undefined());
    let headers_object = prop(scope, object, "headers");
    if !headers_object.is_object() {
        return;
    }
    let has = prop(scope, &headers_object, "has");
    let mut already = false;
    if scope.is_function(&has) {
        let key = scope.string("content-type");
        if let Ok(result) = scope.call(&has, &headers_object, &[key]) {
            already = scope.to_bool(&result);
        }
    }
    if !already {
        let key = scope.string("content-type");
        crate::call_method(scope, &headers_object, "set", &[key, inferred]);
    }
}

fn set_instance_proto(scope: &mut Scope<'_>, object: &Value, name: &str) {
    let proto = crate::proto_of(scope, name);
    if proto.is_object() {
        let _ = scope.set_prototype(object, &proto);
    }
}

fn status_text_valid(text: &[u8]) -> bool {
    !text
        .iter()
        .any(|&c| c >= 0xC4 || (c != 0x09 && c < 0x20) || c == 0x7F)
}

pub(crate) fn response_ctor(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let object = scope.new_object();
    set_instance_proto(scope, &object, "Response");
    let body = args.first().cloned().unwrap_or_else(Value::null);
    let mut status = 200;
    let mut status_text: Option<Vec<u8>> = None;
    let mut headers_init = Value::undefined();
    if let Some(init) = args.get(1).filter(|i| i.is_object()) {
        let given = prop(scope, init, "status");
        let has_status = !given.is_undefined();
        if has_status {
            status = scope.to_int32(&given).unwrap_or(0);
        }
        if has_status && !(200..=599).contains(&status) {
            return Err(scope.range_error(&format!(
                "Response: status {status} is outside the range [200, 599]"
            )));
        }
        if matches!(status, 204 | 205 | 304) && !is_nullish(&args[0]) {
            return Err(scope.type_error("Response: a null body status cannot have a body"));
        }
        let given = prop(scope, init, "statusText");
        if !is_nullish(&given) {
            status_text = c_bytes(scope, &given);
        }
        if status_text
            .as_deref()
            .is_some_and(|t| !status_text_valid(t))
        {
            return Err(scope.type_error("Response: invalid statusText"));
        }
        headers_init = prop(scope, init, "headers");
    }
    install_body(scope, &object, &body, true);
    set(scope, &object, "bodyUsed", Value::boolean(false));
    set(scope, &object, "status", Value::int(status));
    set_str(
        scope,
        &object,
        "statusText",
        status_text.as_deref().unwrap_or_default(),
    );
    set(
        scope,
        &object,
        "ok",
        Value::boolean((200..300).contains(&status)),
    );
    set_str(scope, &object, "type", b"default");
    set_str(scope, &object, "url", b"");
    set(scope, &object, "redirected", Value::boolean(false));
    let headers_object = crate::fetch::make_headers(scope, &headers_init);
    set(scope, &object, "headers", headers_object);
    apply_inferred_content_type(scope, &object);
    attach_consumers(scope, &object);
    Ok(object)
}

fn define_ro(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.define(object, key, value, READ_ONLY);
}

fn define_ro_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &[u8]) {
    let value = scope.string_from_bytes(text);
    define_ro(scope, object, key, value);
}

pub(crate) fn request_ctor(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let object = scope.new_object();
    set_instance_proto(scope, &object, "Request");
    let mut url_raw: Option<Vec<u8>> = None;
    let mut method: Option<Vec<u8>> = None;
    let mut headers_init = Value::undefined();
    let mut body = Value::undefined();
    if let Some(input) = args.first() {
        if input.is_object() {
            let url = prop(scope, input, "url");
            if url.is_string() {
                url_raw = c_bytes(scope, &url);
                let given = prop(scope, input, "method");
                if !given.is_undefined() {
                    method = c_bytes(scope, &given);
                }
                headers_init = prop(scope, input, "headers");
                body = prop(scope, input, "body");
            } else {
                url_raw = c_bytes(scope, input);
            }
        } else {
            url_raw = c_bytes(scope, input);
        }
    }
    if let Some(init) = args.get(1).filter(|i| i.is_object()) {
        let given = prop(scope, init, "method");
        if !given.is_undefined() {
            method = c_bytes(scope, &given);
        }
        let given = prop(scope, init, "headers");
        if !given.is_undefined() {
            headers_init = given;
        }
        let given = prop(scope, init, "body");
        if !given.is_undefined() {
            body = given;
        }
    }
    let page = Js::of(scope).page_url().filter(|p| !p.is_empty());
    let final_url = match (&url_raw, &page) {
        (Some(url), Some(page)) => ffi::url_resolve(Some(page), url).unwrap_or_else(|| url.clone()),
        (Some(url), None) => url.clone(),
        (None, _) => Vec::new(),
    };
    define_ro_str(scope, &object, "url", &final_url);
    let hidden = scope.string_from_bytes(&final_url);
    let _ = scope.define(&object, "__ns_url", hidden, HIDDEN);
    let normalized = method.as_deref().and_then(headers::normalize_method);
    if normalized
        .as_deref()
        .is_some_and(|m| m == b"GET" || m == b"HEAD")
        && !is_nullish(&body)
    {
        return Err(scope.type_error("Request with GET/HEAD method cannot have a body"));
    }
    let method = normalized.unwrap_or_else(|| b"GET".to_vec());
    define_ro_str(scope, &object, "method", &method);
    let hidden = scope.string_from_bytes(&method);
    let _ = scope.define(&object, "__ns_method", hidden, HIDDEN);
    let headers_object = crate::fetch::make_headers(scope, &headers_init);
    define_ro(scope, &object, "headers", headers_object);
    install_body(scope, &object, &body, true);
    apply_inferred_content_type(scope, &object);
    define_ro(scope, &object, "bodyUsed", Value::boolean(false));
    for (key, value) in [
        ("mode", &b"cors"[..]),
        ("credentials", b"same-origin"),
        ("cache", b"default"),
        ("redirect", b"follow"),
        ("referrer", b"about:client"),
        ("referrerPolicy", b""),
        ("integrity", b""),
    ] {
        define_ro_str(scope, &object, key, value);
    }
    define_ro(scope, &object, "keepalive", Value::boolean(false));
    define_ro_str(scope, &object, "destination", b"");
    define_ro(scope, &object, "isReloadNavigation", Value::boolean(false));
    define_ro(scope, &object, "isHistoryNavigation", Value::boolean(false));
    define_ro_str(scope, &object, "duplex", b"half");
    attach_consumers(scope, &object);
    Ok(object)
}

fn request_url(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    scope.get(this, "__ns_url")
}

fn install_interface(scope: &mut Scope<'_>, global: &Value, name: &str) {
    let ctor = prop(scope, global, name);
    let proto = if ctor.is_object() {
        prop(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    };
    if !scope.is_constructor(&ctor) || !proto.is_object() {
        return;
    }
    let is_request = name == "Request";
    if is_request {
        let getter = scope.function("get url", 0, request_url);
        let _ = scope.define_accessor(&proto, "url", Some(&getter), None, Attributes::CONFIGURABLE);
    }
    let sample = if is_request {
        let url = scope.string("about:blank");
        scope.construct(&ctor, &[url])
    } else {
        scope.construct(&ctor, &[])
    };
    let Ok(sample) = sample else {
        return;
    };
    for method in BODY_METHODS {
        let key = scope.string(method);
        if let Ok(Some(desc)) = scope.own_property(&sample, &key) {
            let _ = scope.define(&proto, method, desc.value, Attributes::METHOD);
        }
    }
    let key = scope.string("body");
    if let Ok(Some(desc)) = scope.own_property(&sample, &key) {
        let getter = (!desc.getter.is_undefined()).then_some(&desc.getter);
        let setter = (!desc.setter.is_undefined()).then_some(&desc.setter);
        let _ = scope.define_accessor(
            &proto,
            "body",
            getter,
            setter,
            Attributes {
                writable: false,
                enumerable: true,
                configurable: true,
            },
        );
    }
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    install_interface(scope, global, "Response");
    install_interface(scope, global, "Request");
    let _ = scope.eval_native_script(RESPONSE_STATICS_SOURCE, "<response-static>");
}
