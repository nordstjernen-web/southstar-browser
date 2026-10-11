//! Southstar — platform objects whose state the page must not see: their kinds, construction and the attribute accessors on their prototypes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostKind {
    AbortController = 1,
    AbortSignal,
    BroadcastChannel,
    FileReader,
    FormData,
    MessageChannel,
    MessagePort,
    TextEncoder,
    TextDecoder,
    Xhr,
    XhrUpload,
}

impl HostKind {
    const ALL: [HostKind; 11] = [
        HostKind::AbortController,
        HostKind::AbortSignal,
        HostKind::BroadcastChannel,
        HostKind::FileReader,
        HostKind::FormData,
        HostKind::MessageChannel,
        HostKind::MessagePort,
        HostKind::TextEncoder,
        HostKind::TextDecoder,
        HostKind::Xhr,
        HostKind::XhrUpload,
    ];

    pub fn raw(self) -> i32 {
        self as i32
    }

    pub fn from_raw(raw: i32) -> Option<HostKind> {
        HostKind::ALL.into_iter().find(|kind| kind.raw() == raw)
    }

    pub fn interface(self) -> &'static str {
        match self {
            HostKind::AbortController => "AbortController",
            HostKind::AbortSignal => "AbortSignal",
            HostKind::BroadcastChannel => "BroadcastChannel",
            HostKind::FileReader => "FileReader",
            HostKind::FormData => "FormData",
            HostKind::MessageChannel => "MessageChannel",
            HostKind::MessagePort => "MessagePort",
            HostKind::TextEncoder => "TextEncoder",
            HostKind::TextDecoder => "TextDecoder",
            HostKind::Xhr => "XMLHttpRequest",
            HostKind::XhrUpload => "XMLHttpRequestUpload",
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    String,
    Bool,
    Number,
    Null,
    Undefined,
    Handler,
}

struct Attr {
    interface: &'static str,
    kinds: &'static [HostKind],
    name: &'static str,
    kind: Kind,
    writable: bool,
}

const fn attr(
    interface: &'static str,
    kinds: &'static [HostKind],
    name: &'static str,
    kind: Kind,
    writable: bool,
) -> Attr {
    Attr {
        interface,
        kinds,
        name,
        kind,
        writable,
    }
}

const ABORT_CONTROLLER: &[HostKind] = &[HostKind::AbortController];
const ABORT_SIGNAL: &[HostKind] = &[HostKind::AbortSignal];
const BROADCAST_CHANNEL: &[HostKind] = &[HostKind::BroadcastChannel];
const FILE_READER: &[HostKind] = &[HostKind::FileReader];
const MESSAGE_CHANNEL: &[HostKind] = &[HostKind::MessageChannel];
const MESSAGE_PORT: &[HostKind] = &[HostKind::MessagePort];
const TEXT_ENCODER: &[HostKind] = &[HostKind::TextEncoder];
const TEXT_DECODER: &[HostKind] = &[HostKind::TextDecoder];
const XHR: &[HostKind] = &[HostKind::Xhr];
const XHR_EVENT_TARGETS: &[HostKind] = &[HostKind::Xhr, HostKind::XhrUpload];

const XHR_TARGET: &str = "XMLHttpRequestEventTarget";

static ATTRS: [Attr; 39] = [
    attr(
        "AbortController",
        ABORT_CONTROLLER,
        "signal",
        Kind::Null,
        false,
    ),
    attr("AbortSignal", ABORT_SIGNAL, "aborted", Kind::Bool, false),
    attr("AbortSignal", ABORT_SIGNAL, "onabort", Kind::Handler, true),
    attr(
        "AbortSignal",
        ABORT_SIGNAL,
        "reason",
        Kind::Undefined,
        false,
    ),
    attr(
        "BroadcastChannel",
        BROADCAST_CHANNEL,
        "name",
        Kind::String,
        false,
    ),
    attr(
        "BroadcastChannel",
        BROADCAST_CHANNEL,
        "onmessage",
        Kind::Handler,
        true,
    ),
    attr(
        "BroadcastChannel",
        BROADCAST_CHANNEL,
        "onmessageerror",
        Kind::Handler,
        true,
    ),
    attr("FileReader", FILE_READER, "error", Kind::Null, false),
    attr("FileReader", FILE_READER, "onabort", Kind::Handler, true),
    attr("FileReader", FILE_READER, "onerror", Kind::Handler, true),
    attr("FileReader", FILE_READER, "onload", Kind::Handler, true),
    attr("FileReader", FILE_READER, "onloadend", Kind::Handler, true),
    attr(
        "FileReader",
        FILE_READER,
        "onloadstart",
        Kind::Handler,
        true,
    ),
    attr("FileReader", FILE_READER, "onprogress", Kind::Handler, true),
    attr("FileReader", FILE_READER, "readyState", Kind::Number, false),
    attr("FileReader", FILE_READER, "result", Kind::Null, false),
    attr(
        "MessageChannel",
        MESSAGE_CHANNEL,
        "port1",
        Kind::Null,
        false,
    ),
    attr(
        "MessageChannel",
        MESSAGE_CHANNEL,
        "port2",
        Kind::Null,
        false,
    ),
    attr(
        "MessagePort",
        MESSAGE_PORT,
        "onmessageerror",
        Kind::Handler,
        true,
    ),
    attr("TextEncoder", TEXT_ENCODER, "encoding", Kind::String, false),
    attr("TextDecoder", TEXT_DECODER, "encoding", Kind::String, false),
    attr("TextDecoder", TEXT_DECODER, "fatal", Kind::Bool, false),
    attr("TextDecoder", TEXT_DECODER, "ignoreBOM", Kind::Bool, false),
    attr(
        XHR_TARGET,
        XHR_EVENT_TARGETS,
        "onabort",
        Kind::Handler,
        true,
    ),
    attr(
        XHR_TARGET,
        XHR_EVENT_TARGETS,
        "onerror",
        Kind::Handler,
        true,
    ),
    attr(XHR_TARGET, XHR_EVENT_TARGETS, "onload", Kind::Handler, true),
    attr(
        XHR_TARGET,
        XHR_EVENT_TARGETS,
        "onloadend",
        Kind::Handler,
        true,
    ),
    attr(
        XHR_TARGET,
        XHR_EVENT_TARGETS,
        "onloadstart",
        Kind::Handler,
        true,
    ),
    attr(
        XHR_TARGET,
        XHR_EVENT_TARGETS,
        "onprogress",
        Kind::Handler,
        true,
    ),
    attr(
        XHR_TARGET,
        XHR_EVENT_TARGETS,
        "ontimeout",
        Kind::Handler,
        true,
    ),
    attr(
        "XMLHttpRequest",
        XHR,
        "onreadystatechange",
        Kind::Handler,
        true,
    ),
    attr("XMLHttpRequest", XHR, "response", Kind::String, false),
    attr("XMLHttpRequest", XHR, "responseURL", Kind::String, false),
    attr("XMLHttpRequest", XHR, "responseXML", Kind::Null, false),
    attr("XMLHttpRequest", XHR, "status", Kind::Number, false),
    attr("XMLHttpRequest", XHR, "statusText", Kind::String, false),
    attr("XMLHttpRequest", XHR, "timeout", Kind::Number, true),
    attr("XMLHttpRequest", XHR, "upload", Kind::Null, false),
    attr("XMLHttpRequest", XHR, "withCredentials", Kind::Bool, true),
];

const ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub fn kind_of(value: &Value) -> Option<HostKind> {
    ffi::host_kind(value)
}

pub fn is(value: &Value, kind: HostKind) -> bool {
    kind_of(value) == Some(kind)
}

pub fn state(scope: &mut Scope<'_>, value: &Value, kind: HostKind) -> Option<Value> {
    ffi::host_state(scope, value, kind)
}

pub fn new_default(scope: &mut Scope<'_>, kind: HostKind) -> Result<Value, Value> {
    let global = scope.global();
    let prototype = crate::prototype_of(scope, &global, kind.interface());
    ffi::new_host(scope, kind, &prototype)
}

pub fn construct(
    scope: &mut Scope<'_>,
    new_target: &Value,
    kind: HostKind,
) -> Result<Value, Value> {
    if !new_target.is_object() {
        let message = format!(
            "Failed to construct '{}': Please use the 'new' operator, this DOM object \
             constructor cannot be called as a function.",
            kind.interface()
        );
        return Err(scope.type_error(&message));
    }
    let prototype = scope.get(new_target, "prototype")?;
    if prototype.is_object() {
        ffi::new_host(scope, kind, &prototype)
    } else {
        new_default(scope, kind)
    }
}

fn checked(
    scope: &mut Scope<'_>,
    this: &Value,
    data: &[Value],
) -> Result<(&'static Attr, Value), Value> {
    let index = data.first().and_then(|value| scope.to_int32(value).ok());
    let attr = index
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| ATTRS.get(index));
    let found = attr.and_then(|attr| {
        let kind = kind_of(this).filter(|kind| attr.kinds.contains(kind))?;
        ffi::host_state(scope, this, kind).map(|state| (attr, state))
    });
    found.ok_or_else(|| scope.type_error("Illegal invocation"))
}

fn attr_get(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let (attr, state) = checked(scope, this, data)?;
    let value = scope.get(&state, attr.name)?;
    if !value.is_undefined() {
        return Ok(value);
    }
    Ok(match attr.kind {
        Kind::String => scope.string(""),
        Kind::Bool => Value::boolean(false),
        Kind::Number => Value::int(0),
        Kind::Undefined => Value::undefined(),
        Kind::Null | Kind::Handler => Value::null(),
    })
}

fn attr_set(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let (attr, state) = checked(scope, this, data)?;
    let input = args.first().cloned().unwrap_or_else(Value::undefined);
    let value = match attr.kind {
        Kind::Handler if input.is_object() => input,
        Kind::Handler => Value::null(),
        Kind::String => scope.to_string_value(&input)?,
        Kind::Bool => Value::boolean(scope.to_bool(&input)),
        Kind::Number => Value::number(scope.to_number(&input)?),
        Kind::Null | Kind::Undefined => input,
    };
    let _ = scope.set(&state, attr.name, value);
    Ok(Value::undefined())
}

pub fn install_attributes(scope: &mut Scope<'_>, global: &Value) {
    let mut interface = "";
    let mut prototype = Value::undefined();
    for (index, attr) in ATTRS.iter().enumerate() {
        if attr.interface != interface {
            interface = attr.interface;
            prototype = crate::prototype_of(scope, global, interface);
        }
        if !prototype.is_object() {
            continue;
        }
        let data = [Value::int(index as i32)];
        let getter = scope.bound_function(&format!("get {}", attr.name), 0, attr_get, &data);
        let setter = attr
            .writable
            .then(|| scope.bound_function(&format!("set {}", attr.name), 1, attr_set, &data));
        let _ = scope.define_accessor(
            &prototype,
            attr.name,
            Some(&getter),
            setter.as_ref(),
            ACCESSOR,
        );
    }
}
