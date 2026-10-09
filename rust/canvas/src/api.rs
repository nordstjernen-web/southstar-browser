//! Southstar — the WebIDL surface of the canvas objects: interfaces, brand checks, attribute validation and the object factories.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::quickjs::{self, JSCFunction};
use southstar_js_engine::{Attributes, BoundFn, NativeFn, Scope, Value};

use crate::ffi::{self, c};
use crate::hidden::{self, KIND_CTX2D, KIND_GRADIENT, KIND_IMAGEDATA, KIND_OFFSCREEN};
use crate::hidden::{KIND_OFFSCREEN_CTX2D, KIND_PATTERN, KIND_TEXTMETRICS};
use crate::{bitmap, context, ctxpath, draw, path2d};

const MAX_UNSIGNED_LONG_LONG: f64 = 18446744073709551615.0;

const MAX_OFFSCREEN_ATTRIBUTE: f64 = 8192.0;

#[derive(Clone, Copy, PartialEq)]
enum AttrType {
    Readonly,
    Finite,
    Positive,
    Nonnegative,
    Alpha,
    Bool,
    Enum(&'static str),
    Style,
    Color,
    Font,
    Filter,
    Length,
    Text,
    Size,
    Handler,
}

struct AttrDef {
    name: &'static str,
    ty: AttrType,
}

const fn attr(name: &'static str, ty: AttrType) -> AttrDef {
    AttrDef { name, ty }
}

#[derive(Clone, Copy)]
enum Callable {
    C(JSCFunction),
    Native(NativeFn),
}

struct Method {
    name: &'static str,
    f: Callable,
    length: u32,
}

const fn method(name: &'static str, f: JSCFunction, length: u32) -> Method {
    Method {
        name,
        f: Callable::C(f),
        length,
    }
}

const fn native(name: &'static str, f: NativeFn, length: u32) -> Method {
    Method {
        name,
        f: Callable::Native(f),
        length,
    }
}

#[derive(Clone, Copy)]
enum Brand {
    Kind(i32),
    Path2D,
    ImageBitmap,
}

struct Table {
    iface: &'static str,
    brand: Brand,
    attrs: &'static [AttrDef],
    methods: &'static [Method],
}

use AttrType::*;

static CTX2D_ATTRS: [AttrDef; 28] = [
    attr("canvas", Readonly),
    attr("direction", Enum("inherit ltr rtl")),
    attr("fillStyle", Style),
    attr("filter", Filter),
    attr("font", Font),
    attr("fontKerning", Enum("auto normal none")),
    attr(
        "fontStretch",
        Enum(
            "ultra-condensed extra-condensed condensed semi-condensed normal \
             semi-expanded expanded extra-expanded ultra-expanded",
        ),
    ),
    attr(
        "fontVariantCaps",
        Enum(
            "normal small-caps all-small-caps petite-caps all-petite-caps unicase \
             titling-caps",
        ),
    ),
    attr("globalAlpha", Alpha),
    attr(
        "globalCompositeOperation",
        Enum(
            "source-over source-in source-out source-atop destination-over \
             destination-in destination-out destination-atop lighter copy xor clear \
             multiply screen overlay darken lighten color-dodge color-burn \
             hard-light soft-light difference exclusion hue saturation color \
             luminosity",
        ),
    ),
    attr("imageSmoothingEnabled", Bool),
    attr("imageSmoothingQuality", Enum("low medium high")),
    attr("lang", Text),
    attr("letterSpacing", Length),
    attr("lineCap", Enum("butt round square")),
    attr("lineDashOffset", Finite),
    attr("lineJoin", Enum("round bevel miter")),
    attr("lineWidth", Positive),
    attr("miterLimit", Positive),
    attr("shadowBlur", Nonnegative),
    attr("shadowColor", Color),
    attr("shadowOffsetX", Finite),
    attr("shadowOffsetY", Finite),
    attr("strokeStyle", Style),
    attr("textAlign", Enum("start end left right center")),
    attr(
        "textBaseline",
        Enum("top hanging middle alphabetic ideographic bottom"),
    ),
    attr(
        "textRendering",
        Enum("auto optimizeSpeed optimizeLegibility geometricPrecision"),
    ),
    attr("wordSpacing", Length),
];

macro_rules! ctx2d_methods {
    ($($extra:expr,)*) => {
        [
            native("arc", ctxpath::arc, 5),
            native("arcTo", ctxpath::arc_to_method, 5),
            native("beginPath", ctxpath::begin_path, 0),
            native("bezierCurveTo", ctxpath::bezier_curve_to, 6),
            native("clearRect", draw::clear_rect, 4),
            method("clip", c::ns_ctx_clip, 0),
            native("closePath", ctxpath::close_path, 0),
            method("createConicGradient", c::ns_ctx_createConicGradient, 3),
            method("createImageData", c::ns_ctx_createImageData, 1),
            method("createLinearGradient", c::ns_ctx_createLinearGradient, 4),
            method("createPattern", c::ns_ctx_createPattern, 2),
            method("createRadialGradient", c::ns_ctx_createRadialGradient, 6),
            $($extra,)*
            method("drawImage", c::ns_ctx_drawImage, 3),
            native("ellipse", ctxpath::ellipse, 7),
            native("fill", draw::fill, 0),
            native("fillRect", draw::fill_rect, 4),
            method("fillText", c::ns_ctx_fillText, 3),
            native("getContextAttributes", context::get_attributes, 0),
            method("getImageData", c::ns_ctx_getImageData, 4),
            method("getLineDash", c::ns_ctx_getLineDash, 0),
            native("getTransform", ctxpath::get_transform, 0),
            native("isContextLost", context::is_context_lost, 0),
            method("isPointInPath", c::ns_ctx_isPointInPath, 2),
            method("isPointInStroke", c::ns_ctx_isPointInStroke, 2),
            native("lineTo", ctxpath::line_to, 2),
            method("measureText", c::ns_ctx_measureText, 1),
            native("moveTo", ctxpath::move_to, 2),
            method("putImageData", c::ns_ctx_putImageData, 3),
            native("quadraticCurveTo", ctxpath::quadratic_curve_to, 4),
            native("rect", ctxpath::rect, 4),
            method("reset", c::ns_ctx_reset, 0),
            native("resetTransform", ctxpath::reset_transform, 0),
            native("restore", draw::restore, 0),
            native("rotate", ctxpath::rotate, 1),
            native("roundRect", ctxpath::round_rect, 4),
            native("save", draw::save, 0),
            native("scale", ctxpath::scale, 2),
            method("setLineDash", c::ns_ctx_setLineDash, 1),
            native("setTransform", ctxpath::set_transform, 0),
            native("stroke", draw::stroke, 0),
            native("strokeRect", draw::stroke_rect, 4),
            method("strokeText", c::ns_ctx_strokeText, 3),
            native("transform", ctxpath::transform, 6),
            native("translate", ctxpath::translate, 2),
        ]
    };
}

static CTX2D_METHODS: [Method; 45] = ctx2d_methods!(native(
    "drawFocusIfNeeded",
    context::draw_focus_if_needed,
    1
),);

static OFFSCREEN_CTX2D_METHODS: [Method; 44] = ctx2d_methods!();

static GRADIENT_METHODS: [Method; 1] = [method("addColorStop", c::ns_ctx_gradient_addColorStop, 2)];

static PATTERN_METHODS: [Method; 1] = [method("setTransform", ffi::ns_pattern_set_transform, 0)];

static IMAGEDATA_ATTRS: [AttrDef; 5] = [
    attr("colorSpace", Readonly),
    attr("data", Readonly),
    attr("height", Readonly),
    attr("pixelFormat", Readonly),
    attr("width", Readonly),
];

static TEXTMETRICS_ATTRS: [AttrDef; 10] = [
    attr("actualBoundingBoxAscent", Readonly),
    attr("actualBoundingBoxDescent", Readonly),
    attr("actualBoundingBoxLeft", Readonly),
    attr("actualBoundingBoxRight", Readonly),
    attr("alphabeticBaseline", Readonly),
    attr("fontBoundingBoxAscent", Readonly),
    attr("fontBoundingBoxDescent", Readonly),
    attr("hangingBaseline", Readonly),
    attr("ideographicBaseline", Readonly),
    attr("width", Readonly),
];

static OFFSCREEN_ATTRS: [AttrDef; 4] = [
    attr("height", Size),
    attr("oncontextlost", Handler),
    attr("oncontextrestored", Handler),
    attr("width", Size),
];

static PATH2D_METHODS: [Method; 11] = [
    native("addPath", path2d::add_path, 1),
    native("arc", path2d::arc, 5),
    native("arcTo", path2d::arc_to_method, 5),
    native("bezierCurveTo", path2d::bezier_curve_to, 6),
    native("closePath", path2d::close_path, 0),
    native("ellipse", path2d::ellipse_method, 7),
    native("lineTo", path2d::line_to, 2),
    native("moveTo", path2d::move_to, 2),
    native("quadraticCurveTo", path2d::quadratic_curve_to, 4),
    native("rect", path2d::rect, 4),
    native("roundRect", path2d::round_rect, 4),
];

static IMAGE_BITMAP_METHODS: [Method; 1] = [native("close", bitmap::close, 0)];

static OFFSCREEN_METHODS: [Method; 3] = [
    native("convertToBlob", context::convert_to_blob, 0),
    native("getContext", context::offscreen_get_context, 1),
    native(
        "transferToImageBitmap",
        ffi::state::transfer_to_image_bitmap,
        0,
    ),
];

const TABLE_CTX2D: usize = 0;
const TABLE_OFFSCREEN_CTX2D: usize = 1;
const TABLE_GRADIENT: usize = 2;
const TABLE_PATTERN: usize = 3;
const TABLE_IMAGEDATA: usize = 4;
const TABLE_TEXTMETRICS: usize = 5;
const TABLE_OFFSCREEN: usize = 6;
const TABLE_PATH2D: usize = 7;
const TABLE_IMAGEBITMAP: usize = 8;

static TABLES: [Table; 9] = [
    Table {
        iface: "CanvasRenderingContext2D",
        brand: Brand::Kind(KIND_CTX2D),
        attrs: &CTX2D_ATTRS,
        methods: &CTX2D_METHODS,
    },
    Table {
        iface: "OffscreenCanvasRenderingContext2D",
        brand: Brand::Kind(KIND_OFFSCREEN_CTX2D),
        attrs: &CTX2D_ATTRS,
        methods: &OFFSCREEN_CTX2D_METHODS,
    },
    Table {
        iface: "CanvasGradient",
        brand: Brand::Kind(KIND_GRADIENT),
        attrs: &[],
        methods: &GRADIENT_METHODS,
    },
    Table {
        iface: "CanvasPattern",
        brand: Brand::Kind(KIND_PATTERN),
        attrs: &[],
        methods: &PATTERN_METHODS,
    },
    Table {
        iface: "ImageData",
        brand: Brand::Kind(KIND_IMAGEDATA),
        attrs: &IMAGEDATA_ATTRS,
        methods: &[],
    },
    Table {
        iface: "TextMetrics",
        brand: Brand::Kind(KIND_TEXTMETRICS),
        attrs: &TEXTMETRICS_ATTRS,
        methods: &[],
    },
    Table {
        iface: "OffscreenCanvas",
        brand: Brand::Kind(KIND_OFFSCREEN),
        attrs: &OFFSCREEN_ATTRS,
        methods: &OFFSCREEN_METHODS,
    },
    Table {
        iface: "Path2D",
        brand: Brand::Path2D,
        attrs: &[],
        methods: &PATH2D_METHODS,
    },
    Table {
        iface: "ImageBitmap",
        brand: Brand::ImageBitmap,
        attrs: &[],
        methods: &IMAGE_BITMAP_METHODS,
    },
];

const ILLEGAL_CTOR_NAMES: [&str; 6] = [
    "CanvasRenderingContext2D",
    "OffscreenCanvasRenderingContext2D",
    "CanvasGradient",
    "CanvasPattern",
    "TextMetrics",
    "ImageBitmap",
];

fn branded(brand: Brand, value: &Value) -> bool {
    match brand {
        Brand::Kind(kind) => hidden::is(value, kind),
        Brand::Path2D => ffi::is_path2d(value),
        Brand::ImageBitmap => bitmap::is(value),
    }
}

fn table_entry(scope: &mut Scope<'_>, data: &[Value]) -> Option<(usize, usize)> {
    let table = scope.to_int32(data.first()?).ok()? as usize;
    let index = scope.to_int32(data.get(1)?).ok()? as usize;
    (table < TABLES.len()).then_some((table, index))
}

fn illegal_invocation(scope: &mut Scope<'_>) -> Value {
    scope.type_error("Illegal invocation")
}

fn api_call(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let Some((table, index)) = table_entry(scope, data) else {
        return Ok(Value::undefined());
    };
    let t = &TABLES[table];
    let m = &t.methods[index];
    if !branded(t.brand, this) {
        return Err(illegal_invocation(scope));
    }
    if (args.len() as u32) < m.length {
        let plural = if m.length == 1 { "" } else { "s" };
        let message = format!(
            "Failed to execute '{}' on '{}': {} argument{} required, but only {} present.",
            m.name,
            t.iface,
            m.length,
            plural,
            args.len()
        );
        return Err(scope.type_error(&message));
    }
    match m.f {
        Callable::C(f) => quickjs::call_c_function(scope, f, this, args),
        Callable::Native(f) => f(scope, this, args),
    }
}

fn sync_canvas(scope: &mut Scope<'_>, this: &Value) {
    if hidden::is_ctx2d(this) {
        ffi::canvas_state_for(scope, hidden::ptr(this));
    }
}

fn attr_get(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let Some((table, index)) = table_entry(scope, data) else {
        return Ok(Value::undefined());
    };
    let t = &TABLES[table];
    if !branded(t.brand, this) {
        return Err(illegal_invocation(scope));
    }
    sync_canvas(scope, this);
    hidden::get(scope, this, t.attrs[index].name)
}

fn enum_has(values: &str, s: &[u8]) -> bool {
    values.split(' ').any(|v| v.as_bytes() == s)
}

fn c_text(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    bytes
}

fn to_text(scope: &mut Scope<'_>, value: &Value) -> Result<Vec<u8>, Value> {
    scope.to_bytes(value).map(c_text)
}

fn set_string(scope: &mut Scope<'_>, owner: &Value, name: &str, text: &[u8]) {
    let value = scope.string_from_bytes(text);
    hidden::set(scope, owner, name, value);
}

fn color_for(scope: &mut Scope<'_>, owner: &Value, css: &[u8]) -> Option<Vec<u8>> {
    if !css.eq_ignore_ascii_case(b"currentcolor") {
        return crate::color::to_string(css);
    }
    let computed = ffi::computed_color(scope, hidden::ptr(owner));
    let color = computed.as_deref().and_then(crate::color::to_string);
    Some(color.unwrap_or_else(|| b"#000000".to_vec()))
}

fn strip_ascii(s: &[u8]) -> &[u8] {
    let s = crate::color::skip_spaces(s);
    let end = s
        .iter()
        .rposition(|&c| !crate::color::is_space(c))
        .map_or(0, |i| i + 1);
    &s[..end]
}

fn string_value(scope: &mut Scope<'_>, owner: &Value, ty: AttrType, s: &[u8]) -> Option<Vec<u8>> {
    match ty {
        Color => color_for(scope, owner, s),
        Font => ffi::font_string(s),
        Filter => crate::validate::filter_valid(s).then(|| strip_ascii(s).to_vec()),
        Length => crate::validate::length_valid(s).then(|| s.to_ascii_lowercase()),
        Enum(values) => enum_has(values, s).then(|| s.to_vec()),
        _ => Some(s.to_vec()),
    }
}

fn assign_number(
    scope: &mut Scope<'_>,
    owner: &Value,
    a: &AttrDef,
    v: &Value,
) -> Result<Value, Value> {
    let d = scope.to_number(v)?;
    let rejected = !d.is_finite()
        || (a.ty == Positive && d <= 0.0)
        || (a.ty == Nonnegative && d < 0.0)
        || (a.ty == Alpha && !(0.0..=1.0).contains(&d));
    if !rejected {
        hidden::set(scope, owner, a.name, Value::number(d));
    }
    Ok(Value::undefined())
}

fn assign_style(
    scope: &mut Scope<'_>,
    owner: &Value,
    a: &AttrDef,
    v: &Value,
) -> Result<Value, Value> {
    if hidden::is(v, KIND_GRADIENT) || hidden::is(v, KIND_PATTERN) {
        hidden::set(scope, owner, a.name, v.clone());
        return Ok(Value::undefined());
    }
    let s = to_text(scope, v)?;
    if let Some(color) = color_for(scope, owner, &s) {
        set_string(scope, owner, a.name, &color);
    }
    Ok(Value::undefined())
}

fn assign_size(
    scope: &mut Scope<'_>,
    owner: &Value,
    a: &AttrDef,
    v: &Value,
) -> Result<Value, Value> {
    let d = scope.to_number(v)?;
    if !d.is_finite() || !(0.0..=MAX_UNSIGNED_LONG_LONG).contains(&d) {
        let message = format!(
            "Failed to set the '{}' property on 'OffscreenCanvas': Value is outside the \
             'unsigned long long' value range.",
            a.name
        );
        return Err(scope.type_error(&message));
    }
    hidden::set(scope, owner, a.name, Value::number(d.trunc()));
    offscreen_sync_size(scope, owner);
    Ok(Value::undefined())
}

fn attr_assign(
    scope: &mut Scope<'_>,
    owner: &Value,
    a: &AttrDef,
    v: &Value,
) -> Result<Value, Value> {
    match a.ty {
        Bool => {
            let b = scope.to_bool(v);
            hidden::set(scope, owner, a.name, Value::boolean(b));
            Ok(Value::undefined())
        }
        Finite | Positive | Nonnegative | Alpha => assign_number(scope, owner, a, v),
        Style => assign_style(scope, owner, a, v),
        Size => assign_size(scope, owner, a, v),
        Handler => {
            let handler = if scope.is_function(v) {
                v.clone()
            } else {
                Value::null()
            };
            hidden::set(scope, owner, a.name, handler);
            Ok(Value::undefined())
        }
        _ => {
            let s = to_text(scope, v)?;
            if let Some(value) = string_value(scope, owner, a.ty, &s) {
                set_string(scope, owner, a.name, &value);
            }
            Ok(Value::undefined())
        }
    }
}

fn attr_set(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let Some((table, index)) = table_entry(scope, data) else {
        return Ok(Value::undefined());
    };
    let t = &TABLES[table];
    let a = &t.attrs[index];
    if !branded(t.brand, this) {
        return Err(illegal_invocation(scope));
    }
    sync_canvas(scope, this);
    let undefined = Value::undefined();
    attr_assign(scope, this, a, args.first().unwrap_or(&undefined))
}

fn bound(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    f: BoundFn,
    table: usize,
    index: usize,
) -> Value {
    let data = [Value::int(table as i32), Value::int(index as i32)];
    scope.bound_function(name, arity, f, &data)
}

fn define_members(scope: &mut Scope<'_>, proto: &Value, table: usize) {
    let t = &TABLES[table];
    for (i, m) in t.methods.iter().enumerate() {
        let function = bound(scope, m.name, m.length, api_call, table, i);
        let _ = scope.set(proto, m.name, function);
    }
    for (i, a) in t.attrs.iter().enumerate() {
        let getter = bound(scope, &format!("get {}", a.name), 0, attr_get, table, i);
        let setter = (a.ty != Readonly)
            .then(|| bound(scope, &format!("set {}", a.name), 1, attr_set, table, i));
        let accessor = Attributes {
            writable: false,
            enumerable: true,
            configurable: true,
        };
        let _ = scope.define_accessor(proto, a.name, Some(&getter), setter.as_ref(), accessor);
    }
}

pub(crate) fn interface(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    ctor: Value,
    parent: Option<&str>,
) -> Value {
    let parent_ctor = match parent {
        Some(parent) => scope
            .get(global, parent)
            .unwrap_or_else(|_| Value::undefined()),
        None => Value::undefined(),
    };
    let parent_proto = if parent_ctor.is_object() {
        scope
            .get(&parent_ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    };
    let proto = if parent_proto.is_object() {
        scope.new_object_with_proto(&parent_proto)
    } else {
        scope.new_object()
    };
    let _ = scope.set_constructor(&ctor, &proto);
    let _ = scope.define_to_string_tag(&proto, name);
    if parent_ctor.is_object() {
        let _ = scope.set_prototype(&ctor, &parent_ctor);
    }
    let global_binding = Attributes {
        writable: true,
        enumerable: false,
        configurable: true,
    };
    let _ = scope.define(global, name, ctor, global_binding);
    proto
}

fn illegal_constructor(scope: &mut Scope<'_>, this: &Value, index: usize) -> Result<Value, Value> {
    if this.is_undefined() {
        return Err(scope.type_error("Illegal constructor"));
    }
    let message = format!(
        "Failed to construct '{}': Illegal constructor",
        ILLEGAL_CTOR_NAMES[index]
    );
    Err(scope.type_error(&message))
}

macro_rules! illegal_ctors {
    ($($name:ident = $index:expr),*) => {
        $(
            fn $name(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
                illegal_constructor(scope, this, $index)
            }
        )*
        const ILLEGAL_CTORS: [NativeFn; 6] = [$($name),*];
    };
}

illegal_ctors!(
    illegal_ctx2d = 0,
    illegal_offscreen_ctx2d = 1,
    illegal_gradient = 2,
    illegal_pattern = 3,
    illegal_textmetrics = 4,
    illegal_image_bitmap = 5
);

fn install_illegal(scope: &mut Scope<'_>, global: &Value, index: usize, table: usize) {
    let name = ILLEGAL_CTOR_NAMES[index];
    let ctor = scope.constructor_or_function(name, 0, ILLEGAL_CTORS[index]);
    let proto = interface(scope, global, name, ctor, None);
    define_members(scope, &proto, table);
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value, window: bool) {
    let create = scope.function("createImageBitmap", 1, bitmap::create);
    let _ = scope.set(global, "createImageBitmap", create);
    if window {
        install_illegal(scope, global, 0, TABLE_CTX2D);
    }
    install_illegal(scope, global, 1, TABLE_OFFSCREEN_CTX2D);
    let ctor = scope.constructor_or_function("OffscreenCanvas", 2, offscreen_construct);
    let proto = interface(scope, global, "OffscreenCanvas", ctor, Some("EventTarget"));
    define_members(scope, &proto, TABLE_OFFSCREEN);
    install_illegal(scope, global, 2, TABLE_GRADIENT);
    install_illegal(scope, global, 3, TABLE_PATTERN);
    install_illegal(scope, global, 4, TABLE_TEXTMETRICS);
    install_illegal(scope, global, 5, TABLE_IMAGEBITMAP);
    bitmap::define_members(scope);
    let ctor = scope.constructor_or_function("ImageData", 2, imagedata_construct);
    let proto = interface(scope, global, "ImageData", ctor, None);
    define_members(scope, &proto, TABLE_IMAGEDATA);
    let ctor = scope.constructor_or_function("Path2D", 0, path2d::construct);
    let proto = interface(scope, global, "Path2D", ctor, None);
    define_members(scope, &proto, TABLE_PATH2D);
}

pub(crate) fn api_proto(realm: &mut Scope<'_>, iface: &str) -> Value {
    let global = realm.global();
    let ctor = realm
        .get(&global, iface)
        .unwrap_or_else(|_| Value::undefined());
    if !ctor.is_object() {
        return Value::undefined();
    }
    realm
        .get(&ctor, "prototype")
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn api_proto_of_ctor(scope: &mut Scope<'_>, new_target: &Value, iface: &str) -> Value {
    if new_target.is_object() {
        let proto = scope
            .get(new_target, "prototype")
            .unwrap_or_else(|_| Value::undefined());
        if proto.is_object() {
            return proto;
        }
    }
    api_proto(scope, iface)
}

pub(crate) fn new_required(scope: &mut Scope<'_>, iface: &str) -> Value {
    let message = format!(
        "Failed to construct '{iface}': Please use the 'new' operator, this DOM object \
         constructor cannot be called as a function."
    );
    scope.type_error(&message)
}

pub(crate) fn throw_dom(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    let global = scope.global();
    let ctor = scope
        .get(&global, "DOMException")
        .unwrap_or_else(|_| Value::undefined());
    let args = [scope.string(message), scope.string(name)];
    match scope.construct(&ctor, &args) {
        Ok(exception) if exception.is_object() => exception,
        _ => {
            let error = scope.new_error();
            let name_value = scope.string(name);
            let _ = scope.set(&error, "name", name_value);
            let message_value = scope.string(message);
            let _ = scope.set(&error, "message", message_value);
            error
        }
    }
}

const CTX2D_STRINGS: [(&str, &str); 19] = [
    ("fillStyle", "#000000"),
    ("strokeStyle", "#000000"),
    ("font", "10px sans-serif"),
    ("textBaseline", "alphabetic"),
    ("globalCompositeOperation", "source-over"),
    ("imageSmoothingQuality", "low"),
    ("shadowColor", "rgba(0, 0, 0, 0)"),
    ("textAlign", "start"),
    ("direction", "ltr"),
    ("filter", "none"),
    ("letterSpacing", "0px"),
    ("wordSpacing", "0px"),
    ("fontKerning", "auto"),
    ("fontStretch", "normal"),
    ("fontVariantCaps", "normal"),
    ("textRendering", "auto"),
    ("lang", "inherit"),
    ("lineCap", "butt"),
    ("lineJoin", "miter"),
];

const CTX2D_NUMBERS: [(&str, f64); 7] = [
    ("lineWidth", 1.0),
    ("miterLimit", 10.0),
    ("globalAlpha", 1.0),
    ("shadowBlur", 0.0),
    ("shadowOffsetX", 0.0),
    ("shadowOffsetY", 0.0),
    ("lineDashOffset", 0.0),
];

pub(crate) fn ctx2d_init_state(scope: &mut Scope<'_>, obj: &Value) {
    for (name, value) in CTX2D_STRINGS {
        let value = scope.string(value);
        hidden::set(scope, obj, name, value);
    }
    for (name, value) in CTX2D_NUMBERS {
        hidden::set(scope, obj, name, Value::number(value));
    }
    hidden::set(scope, obj, "imageSmoothingEnabled", Value::boolean(true));
    let dashes = scope.new_array();
    hidden::set(scope, obj, "_dashes", dashes);
    let stack = scope.new_array();
    hidden::set(scope, obj, "_stateStack", stack);
}

pub(crate) fn ctx2d_finish(
    scope: &mut Scope<'_>,
    obj: &Value,
    el: usize,
    canvas: &Value,
    attrs: Value,
) {
    hidden::set_ptr(obj, el);
    hidden::set(scope, obj, "canvas", canvas.clone());
    hidden::set(scope, obj, "_attrs", attrs);
    ctx2d_init_state(scope, obj);
}

pub(crate) fn gradient_finish(scope: &mut Scope<'_>, obj: &Value, kind: &[u8]) {
    set_string(scope, obj, "_type", kind);
    let stops = scope.new_array();
    hidden::set(scope, obj, "_stops", stops);
}

pub(crate) fn pattern_finish(
    scope: &mut Scope<'_>,
    obj: &Value,
    source: &Value,
    repetition: &[u8],
) {
    set_string(scope, obj, "_type", b"pattern");
    hidden::set(scope, obj, "_node", source.clone());
    set_string(scope, obj, "_rep", repetition);
}

const TEXTMETRICS_FIELDS: [&str; 10] = [
    "width",
    "actualBoundingBoxLeft",
    "actualBoundingBoxRight",
    "actualBoundingBoxAscent",
    "actualBoundingBoxDescent",
    "fontBoundingBoxAscent",
    "fontBoundingBoxDescent",
    "hangingBaseline",
    "alphabeticBaseline",
    "ideographicBaseline",
];

pub(crate) fn textmetrics_finish(scope: &mut Scope<'_>, obj: &Value, values: &[f64; 10]) {
    for (name, v) in TEXTMETRICS_FIELDS.iter().zip(values) {
        hidden::set(scope, obj, name, Value::number(*v));
    }
}

pub(crate) fn imagedata_finish(
    scope: &mut Scope<'_>,
    obj: &Value,
    size: (i32, i32),
    data: Value,
    color_space: &[u8],
) {
    hidden::set(scope, obj, "width", Value::int(size.0));
    hidden::set(scope, obj, "height", Value::int(size.1));
    hidden::set(scope, obj, "data", data);
    set_string(scope, obj, "colorSpace", color_space);
    set_string(scope, obj, "pixelFormat", b"rgba-unorm8");
}

pub(crate) fn imagedata_wrap(
    scope: &mut Scope<'_>,
    proto: &Value,
    size: (i32, i32),
    data: Value,
    color_space: &[u8],
) -> Value {
    let own_proto;
    let proto = if proto.is_object() {
        proto
    } else {
        own_proto = api_proto(scope, "ImageData");
        &own_proto
    };
    let obj = hidden::new(scope, KIND_IMAGEDATA, proto);
    imagedata_finish(scope, &obj, size, data, color_space);
    obj
}

pub(crate) fn clamped_array(
    realm: &mut Scope<'_>,
    rgba: Option<&[u8]>,
    n: usize,
) -> Result<Value, Value> {
    let buffer = match rgba {
        Some(bytes) => quickjs::array_buffer_copy(realm, bytes)?,
        None => {
            let mut zeros = Vec::new();
            if zeros.try_reserve_exact(n).is_err() {
                return Err(realm.range_error("ImageData allocation failed"));
            }
            zeros.resize(n, 0);
            quickjs::array_buffer_copy(realm, &zeros)?
        }
    };
    let global = realm.global();
    let ctor = realm
        .get(&global, "Uint8ClampedArray")
        .unwrap_or_else(|_| Value::undefined());
    realm.construct(&ctor, &[buffer])
}

const IMAGEDATA_MAX: u32 = 32767;

const IMAGEDATA_RANGE: &str =
    "Failed to construct 'ImageData': The requested image size exceeds the supported range.";

fn color_space(scope: &mut Scope<'_>, settings: Option<&Value>) -> Result<&'static [u8], Value> {
    let Some(settings) = settings.filter(|s| s.is_object()) else {
        return Ok(b"srgb");
    };
    let cs = scope.get(settings, "colorSpace")?;
    if cs.is_undefined() {
        return Ok(b"srgb");
    }
    let s = to_text(scope, &cs)?;
    match s.as_slice() {
        b"srgb" => Ok(b"srgb"),
        b"display-p3" => Ok(b"display-p3"),
        _ => {
            let message = format!(
                "Failed to construct 'ImageData': The provided value '{}' is not a valid enum \
                 value of type PredefinedColorSpace.",
                String::from_utf8_lossy(&s)
            );
            Err(scope.type_error(&message))
        }
    }
}

fn to_u32(scope: &mut Scope<'_>, value: &Value) -> Result<u32, Value> {
    scope.to_int32(value).map(|n| n as u32)
}

fn imagedata_from_array(
    scope: &mut Scope<'_>,
    proto: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let data = &args[0];
    let blen = scope.with_typed_array(data, |t| t.bytes.len()).unwrap_or(0);
    if blen % 4 != 0 {
        return Err(throw_dom(
            scope,
            "InvalidStateError",
            "The input data length is not a multiple of 4.",
        ));
    }
    let sw = to_u32(scope, &args[1])?;
    if sw == 0 {
        return Err(throw_dom(
            scope,
            "IndexSizeError",
            "The source width is zero.",
        ));
    }
    let pixels = (blen / 4) as u64;
    if pixels % u64::from(sw) != 0 {
        return Err(throw_dom(
            scope,
            "InvalidStateError",
            "The input data byte length is not a multiple of (4 * width).",
        ));
    }
    let rows = pixels / u64::from(sw);
    if let Some(height) = args.get(2).filter(|h| !h.is_undefined()) {
        let sh = to_u32(scope, height)?;
        if u64::from(sh) != rows {
            return Err(throw_dom(
                scope,
                "IndexSizeError",
                "The input data byte length is not equal to (4 * width * height).",
            ));
        }
    }
    let space = color_space(scope, args.get(3))?;
    if rows > u64::from(IMAGEDATA_MAX) || sw > IMAGEDATA_MAX {
        return Err(scope.range_error(IMAGEDATA_RANGE));
    }
    Ok(imagedata_wrap(
        scope,
        proto,
        (sw as i32, rows as i32),
        data.clone(),
        space,
    ))
}

fn imagedata_from_size(
    scope: &mut Scope<'_>,
    proto: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let sw = to_u32(scope, &args[0])?;
    let sh = to_u32(scope, &args[1])?;
    let space = color_space(scope, args.get(2))?;
    if sw == 0 || sh == 0 {
        let message = if sw == 0 {
            "The source width is zero or not a number."
        } else {
            "The source height is zero or not a number."
        };
        return Err(throw_dom(scope, "IndexSizeError", message));
    }
    if sw > IMAGEDATA_MAX || sh > IMAGEDATA_MAX {
        return Err(scope.range_error(IMAGEDATA_RANGE));
    }
    let data = clamped_array(scope, None, sw as usize * sh as usize * 4)?;
    Ok(imagedata_wrap(
        scope,
        proto,
        (sw as i32, sh as i32),
        data,
        space,
    ))
}

pub(crate) fn imagedata_construct(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if new_target.is_undefined() {
        return Err(new_required(scope, "ImageData"));
    }
    if args.len() < 2 {
        let message = format!(
            "Failed to construct 'ImageData': 2 arguments required, but only {} present.",
            args.len()
        );
        return Err(scope.type_error(&message));
    }
    let proto = api_proto_of_ctor(scope, new_target, "ImageData");
    if args[0].is_object() && quickjs::typed_array_type(&args[0]) == quickjs::TYPED_ARRAY_UINT8C {
        imagedata_from_array(scope, &proto, args)
    } else {
        imagedata_from_size(scope, &proto, args)
    }
}

pub(crate) fn imagedata_new(
    scope: &mut Scope<'_>,
    realm: &mut Scope<'_>,
    size: (i32, i32),
    rgba: Option<&[u8]>,
) -> Result<Value, Value> {
    let (w, h) = size;
    if w <= 0 || h <= 0 {
        return Ok(Value::null());
    }
    if w > IMAGEDATA_MAX as i32 || h > IMAGEDATA_MAX as i32 {
        return Err(scope.range_error("ImageData too large"));
    }
    let data = clamped_array(realm, rgba, w as usize * h as usize * 4)?;
    let proto = api_proto(realm, "ImageData");
    let obj = hidden::new(realm, KIND_IMAGEDATA, &proto);
    imagedata_finish(scope, &obj, size, data, b"srgb");
    Ok(obj)
}

pub(crate) fn offscreen_node(obj: &Value) -> usize {
    if hidden::is(obj, KIND_OFFSCREEN) {
        hidden::ptr(obj)
    } else {
        0
    }
}

pub(crate) fn offscreen_sync_size(scope: &mut Scope<'_>, obj: &Value) {
    let el = offscreen_node(obj);
    if el == 0 {
        return;
    }
    for name in ["width", "height"] {
        let d = hidden::get(scope, obj, name)
            .ok()
            .and_then(|v| scope.to_number(&v).ok())
            .unwrap_or(0.0);
        let size = if d > MAX_OFFSCREEN_ATTRIBUTE {
            MAX_OFFSCREEN_ATTRIBUTE as i32
        } else {
            d as i32
        };
        let text = size.to_string();
        if ffi::element_attr(el, name).as_deref() != Some(text.as_bytes()) {
            ffi::set_element_attr(el, name, &text);
        }
    }
}

pub(crate) fn offscreen_construct(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if new_target.is_undefined() {
        return Err(new_required(scope, "OffscreenCanvas"));
    }
    if args.len() < 2 {
        let message = format!(
            "Failed to construct 'OffscreenCanvas': 2 arguments required, but only {} present.",
            args.len()
        );
        return Err(scope.type_error(&message));
    }
    let mut dims = [0.0; 2];
    for (dim, arg) in dims.iter_mut().zip(args) {
        let d = scope.to_number(arg)?;
        if !d.is_finite() || !(0.0..=MAX_UNSIGNED_LONG_LONG).contains(&d) {
            return Err(scope.type_error(
                "Failed to construct 'OffscreenCanvas': Value is outside the 'unsigned long \
                 long' value range.",
            ));
        }
        *dim = d.trunc();
    }
    if quickjs::context_opaque(scope).is_null() {
        return Err(scope.type_error("OffscreenCanvas is not available here"));
    }
    let proto = api_proto_of_ctor(scope, new_target, "OffscreenCanvas");
    let obj = hidden::new(scope, KIND_OFFSCREEN, &proto);
    let el = ffi::new_offscreen_canvas_node(scope);
    hidden::set_ptr(&obj, el);
    hidden::set(scope, &obj, "width", Value::number(dims[0]));
    hidden::set(scope, &obj, "height", Value::number(dims[1]));
    hidden::set(scope, &obj, "oncontextlost", Value::null());
    hidden::set(scope, &obj, "oncontextrestored", Value::null());
    offscreen_sync_size(scope, &obj);
    Ok(obj)
}

fn imagedata_clone(scope: &mut Scope<'_>, v: &Value) -> Result<Value, Value> {
    let data = hidden::get(scope, v, "data")?;
    let Some(bytes) = scope.with_typed_array(&data, |t| t.bytes.to_vec()) else {
        return Err(scope.type_error("The ImageData's pixel buffer is detached"));
    };
    let copy = clamped_array(scope, Some(&bytes), bytes.len())?;
    let w = hidden::get(scope, v, "width")?;
    let h = hidden::get(scope, v, "height")?;
    let space = hidden::get(scope, v, "colorSpace")?;
    let w = scope.to_int32(&w).unwrap_or(0);
    let h = scope.to_int32(&h).unwrap_or(0);
    let space = to_text(scope, &space).unwrap_or_else(|_| b"srgb".to_vec());
    Ok(imagedata_wrap(
        scope,
        &Value::undefined(),
        (w, h),
        copy,
        &space,
    ))
}

pub(crate) fn clone_object(scope: &mut Scope<'_>, v: &Value) -> Result<Value, Value> {
    if hidden::is(v, KIND_IMAGEDATA) {
        return imagedata_clone(scope, v);
    }
    if bitmap::is(v) {
        return bitmap::clone(scope, v);
    }
    if hidden::kind_of(v).is_some() || ffi::is_path2d(v) {
        return Err(throw_dom(
            scope,
            "DataCloneError",
            "The object could not be cloned.",
        ));
    }
    Ok(Value::undefined())
}

fn matrix_member(
    scope: &mut Scope<'_>,
    init: &Value,
    alias: &str,
    field: &str,
) -> Result<Option<f64>, Value> {
    let alias_value = scope.get(init, alias)?;
    let field_value = scope.get(init, field)?;
    let a = if alias_value.is_undefined() {
        None
    } else {
        Some(scope.to_number(&alias_value)?)
    };
    let f = if field_value.is_undefined() {
        None
    } else {
        Some(scope.to_number(&field_value)?)
    };
    let conflicting = match (a, f) {
        (Some(a), Some(f)) => !(a == f || (a.is_nan() && f.is_nan())),
        _ => false,
    };
    if conflicting {
        let message = format!("The '{alias}' and '{field}' members must be equal.");
        return Err(scope.type_error(&message));
    }
    Ok(a.or(f))
}

pub(crate) fn pattern_set_transform(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    const ALIASES: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
    const FIELDS: [&str; 6] = ["m11", "m12", "m21", "m22", "m41", "m42"];
    let mut m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    if let Some(init) = args.first().filter(|v| v.is_object()) {
        for i in 0..6 {
            if let Some(v) = matrix_member(scope, init, ALIASES[i], FIELDS[i])? {
                m[i] = v;
            }
        }
    }
    let array = scope.new_array();
    for (i, v) in m.iter().enumerate() {
        let _ = scope.set_index(&array, i as u32, Value::number(*v));
    }
    hidden::set(scope, this, "_matrix", array);
    Ok(Value::undefined())
}
