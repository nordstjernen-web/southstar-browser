//! Southstar — IntersectionObserver: thresholds, root margins, the intersection of each target with its root and the tick that reports changes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;

use southstar_dom::Kind;
use southstar_js_engine::{NativeFn, Scope, Value};
use southstar_layout::{BoxKind, BoxRef};

use crate::ffi::{self, Js, Rect};
use crate::{bind, bind_if_not_callable, page, page_or_new, set, set_index};

#[derive(Clone, Copy)]
struct Margin {
    value: f64,
    percentage: bool,
}

impl Margin {
    fn resolve(self, width: f64) -> f64 {
        if self.percentage {
            self.value * width / 100.0
        } else {
            self.value
        }
    }
}

#[derive(Clone)]
struct Target {
    wrapper: Value,
    last_intersecting: bool,
    has_fired: bool,
    last_ratio: f64,
}

#[derive(Default)]
struct State {
    disconnected: bool,
    pin: Option<Value>,
    targets: Vec<Target>,
}

pub(crate) struct Observer {
    js: Js,
    callback: Value,
    root: Value,
    margins: [Margin; 4],
    thresholds: Vec<f64>,
    state: RefCell<State>,
}

#[derive(Clone)]
struct Handle(Rc<Observer>);

impl Drop for Observer {
    fn drop(&mut self) {
        let Some(page) = page(self.js) else {
            return;
        };
        let gone: *const Observer = self;
        if let Ok(mut list) = page.intersection.try_borrow_mut() {
            crate::forget(&mut list, gone);
        }
    }
}

impl Observer {
    pub(crate) fn reset(&self) {
        let targets = {
            let mut state = self.state.borrow_mut();
            state.disconnected = true;
            core::mem::take(&mut state.targets)
        };
        drop(targets);
    }

    pub(crate) fn teardown(&self) {
        self.reset();
    }

    fn set_pin(&self, pin: Option<&Value>) {
        let old = {
            let mut state = self.state.borrow_mut();
            match pin {
                Some(pin) => {
                    if state.pin.is_none() {
                        state.pin = Some(pin.clone());
                    }
                    None
                }
                None => state.pin.take(),
            }
        };
        drop(old);
    }

    fn threshold_index(&self, ratio: f64) -> usize {
        self.thresholds
            .iter()
            .position(|&t| t > ratio)
            .unwrap_or(self.thresholds.len())
    }
}

fn threshold_append(
    scope: &mut Scope<'_>,
    thresholds: &mut Vec<f64>,
    value: &Value,
) -> Result<(), Value> {
    let threshold = scope.to_number(value)?;
    if !threshold.is_finite() {
        return Err(scope.type_error("IntersectionObserver threshold must be finite"));
    }
    if !(0.0..=1.0).contains(&threshold) {
        return Err(scope.range_error("IntersectionObserver threshold is outside [0, 1]"));
    }
    thresholds.push(threshold);
    Ok(())
}

fn parse_thresholds(scope: &mut Scope<'_>, options: &Value) -> Result<Vec<f64>, Value> {
    let mut thresholds = Vec::new();
    let value = if options.is_undefined() || options.is_null() {
        Value::undefined()
    } else {
        scope.get(options, "threshold")?
    };
    if value.is_undefined() {
        thresholds.push(0.0);
    } else if !value.is_object() {
        threshold_append(scope, &mut thresholds, &value)?;
    } else {
        let global = scope.global();
        let symbol = crate::get(scope, &global, "Symbol");
        let iterator_key = crate::get(scope, &symbol, "iterator");
        let iterator = scope
            .get_key(&value, &iterator_key)
            .unwrap_or_else(|_| Value::undefined());
        if !scope.is_function(&iterator) {
            return Err(scope.type_error("IntersectionObserver threshold must be iterable"));
        }
        let array_constructor = crate::get(scope, &global, "Array");
        let from = crate::get(scope, &array_constructor, "from");
        let array = scope.call(&from, &array_constructor, &[value])?;
        let length = crate::array_length(scope, &array);
        for i in 0..length {
            let item = scope
                .get_index(&array, i)
                .unwrap_or_else(|_| Value::undefined());
            threshold_append(scope, &mut thresholds, &item)?;
        }
    }
    if thresholds.is_empty() {
        thresholds.push(0.0);
    }
    thresholds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    Ok(thresholds)
}

fn digits(s: &[u8], mut at: usize, hex: bool) -> usize {
    while at < s.len()
        && (if hex {
            s[at].is_ascii_hexdigit()
        } else {
            s[at].is_ascii_digit()
        })
    {
        at += 1;
    }
    at
}

fn exponent(s: &[u8], at: usize, marker: u8) -> usize {
    if at >= s.len() || !s[at].eq_ignore_ascii_case(&marker) {
        return at;
    }
    let mut digits_at = at + 1;
    if digits_at < s.len() && (s[digits_at] == b'+' || s[digits_at] == b'-') {
        digits_at += 1;
    }
    let end = digits(s, digits_at, false);
    if end > digits_at { end } else { at }
}

fn hex_value(s: &[u8], start: usize, end: usize) -> f64 {
    let mut value = 0.0;
    let mut scale = 0i32;
    let mut fraction = false;
    let mut at = start;
    while at < end {
        let c = s[at];
        if c == b'.' {
            fraction = true;
        } else if c.eq_ignore_ascii_case(&b'p') {
            break;
        } else if let Some(d) = (c as char).to_digit(16) {
            value = value * 16.0 + f64::from(d);
            if fraction {
                scale -= 4;
            }
        }
        at += 1;
    }
    if at < end {
        let exp: i32 = core::str::from_utf8(&s[at + 1..end])
            .ok()
            .and_then(|e| e.parse().ok())
            .unwrap_or(0);
        scale = scale.saturating_add(exp);
    }
    value * 2f64.powi(scale)
}

fn strtod(s: &[u8]) -> Option<(f64, usize)> {
    let mut at = 0;
    while at < s.len() && matches!(s[at], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        at += 1;
    }
    let start = at;
    let negative = at < s.len() && s[at] == b'-';
    if at < s.len() && (s[at] == b'+' || s[at] == b'-') {
        at += 1;
    }
    if at + 1 < s.len() && s[at] == b'0' && s[at + 1].eq_ignore_ascii_case(&b'x') {
        let int_end = digits(s, at + 2, true);
        let mut end = int_end;
        let mut any = int_end > at + 2;
        if end < s.len() && s[end] == b'.' {
            let frac_end = digits(s, end + 1, true);
            any |= frac_end > end + 1;
            end = frac_end;
        }
        if any {
            let end = exponent(s, end, b'p');
            let value = hex_value(s, at + 2, end);
            return Some((if negative { -value } else { value }, end));
        }
    }
    let int_end = digits(s, at, false);
    let mut end = int_end;
    let mut any = int_end > at;
    if end < s.len() && s[end] == b'.' {
        let frac_end = digits(s, end + 1, false);
        any |= frac_end > end + 1;
        end = frac_end;
    }
    if !any {
        return None;
    }
    let end = exponent(s, end, b'e');
    let value = core::str::from_utf8(&s[start..end]).ok()?.parse().ok()?;
    Some((value, end))
}

fn parse_margin_token(token: &[u8]) -> Option<Margin> {
    let (value, end) = strtod(token)?;
    if !f64::is_finite(value) {
        return None;
    }
    let unit = &token[end..];
    let mut factor = 1.0;
    let mut percentage = false;
    if unit == b"%" {
        percentage = true;
    } else if unit.eq_ignore_ascii_case(b"px") {
        factor = 1.0;
    } else if unit.eq_ignore_ascii_case(b"in") {
        factor = 96.0;
    } else if unit.eq_ignore_ascii_case(b"cm") {
        factor = 96.0 / 2.54;
    } else if unit.eq_ignore_ascii_case(b"mm") {
        factor = 96.0 / 25.4;
    } else if unit.eq_ignore_ascii_case(b"q") {
        factor = 96.0 / 101.6;
    } else if unit.eq_ignore_ascii_case(b"pt") {
        factor = 96.0 / 72.0;
    } else if unit.eq_ignore_ascii_case(b"pc") {
        factor = 16.0;
    } else if !unit.is_empty() || value != 0.0 {
        return None;
    }
    Some(Margin {
        value: value * factor,
        percentage,
    })
}

fn format_g(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    let scientific = format!("{value:.5e}");
    let (mantissa, exp) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let trim = |text: &str| -> String {
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text.to_owned()
        }
    };
    if !(-4..6).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", trim(mantissa), sign, exp.unsigned_abs())
    } else {
        trim(&format!("{:.*}", (5 - exp) as usize, value))
    }
}

fn parse_root_margin(text: &[u8]) -> Option<([Margin; 4], String)> {
    let parts: Vec<&[u8]> = text
        .split(|c| matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0c))
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() > 4 {
        return None;
    }
    let raw: Vec<&[u8]> = if parts.is_empty() {
        vec![b"0px"]
    } else {
        parts
    };
    let n = raw.len();
    let tokens = [
        raw[0],
        if n > 1 { raw[1] } else { raw[0] },
        if n > 2 { raw[2] } else { raw[0] },
        if n > 3 {
            raw[3]
        } else if n > 1 {
            raw[1]
        } else {
            raw[0]
        },
    ];
    let mut margins = [Margin {
        value: 0.0,
        percentage: false,
    }; 4];
    for (margin, token) in margins.iter_mut().zip(tokens) {
        *margin = parse_margin_token(token)?;
    }
    let serialized = margins
        .iter()
        .map(|m| {
            let unit = if m.percentage { "%" } else { "px" };
            format!("{}{unit}", format_g(m.value))
        })
        .collect::<Vec<_>>()
        .join(" ");
    Some((margins, serialized))
}

fn overflow_clips(b: BoxRef<'_>) -> bool {
    if b.style().is_null() {
        return false;
    }
    if !matches!(
        b.kind(),
        BoxKind::Block | BoxKind::TableCaption | BoxKind::TableCell
    ) {
        return false;
    }
    let overflow = ffi::style_keyword(b, c"overflow");
    let overflow_x = ffi::style_keyword(b, c"overflow-x").or(overflow);
    let overflow_y = ffi::style_keyword(b, c"overflow-y").or(overflow);
    [c"hidden", c"clip", c"auto", c"scroll"]
        .iter()
        .any(|&clipped| overflow_x == Some(clipped) || overflow_y == Some(clipped))
}

fn intersect(clip: Rect, rect: &mut Rect) -> bool {
    let right = rect.x + rect.w;
    let bottom = rect.y + rect.h;
    let clip_right = clip.x + clip.w;
    let clip_bottom = clip.y + clip.h;
    let out_x = if rect.x > clip.x { rect.x } else { clip.x };
    let out_y = if rect.y > clip.y { rect.y } else { clip.y };
    let out_right = if right < clip_right {
        right
    } else {
        clip_right
    };
    let out_bottom = if bottom < clip_bottom {
        bottom
    } else {
        clip_bottom
    };
    let intersects = out_right >= out_x && out_bottom >= out_y;
    rect.x = out_x;
    rect.y = out_y;
    rect.w = if out_right > out_x {
        out_right - out_x
    } else {
        0.0
    };
    rect.h = if out_bottom > out_y {
        out_bottom - out_y
    } else {
        0.0
    };
    intersects
}

fn scroll_prop(scope: &mut Scope<'_>, key: &str) -> f64 {
    let global = scope.global();
    let value = crate::get(scope, &global, key);
    if value.is_number() {
        scope.to_number(&value).unwrap_or(0.0)
    } else {
        0.0
    }
}

fn root_rect(scope: &mut Scope<'_>, observer: &Observer, layout_root: Option<BoxRef<'_>>) -> Rect {
    if !observer.root.is_null() && !observer.root.is_undefined() {
        let root_node = ffi::unwrap_element(&observer.root);
        if let (Some(node), Some(layout_root)) = (root_node, layout_root) {
            if node.kind() == Kind::Element {
                if let Some(b) = ffi::find_by_dom(layout_root, node.as_ptr() as usize) {
                    return if overflow_clips(b) {
                        ffi::visual_padding_box(b)
                    } else {
                        ffi::visual_border_box(b)
                    };
                }
            }
        }
    }
    let mut width = 1000.0;
    let mut height = 800.0;
    if let Some(w) = crate::global_number(scope, "innerWidth").filter(|&w| w > 0.0) {
        width = w;
    }
    if let Some(h) = crate::global_number(scope, "innerHeight").filter(|&h| h > 0.0) {
        height = h;
    }
    Rect {
        x: scroll_prop(scope, "scrollX"),
        y: scroll_prop(scope, "scrollY"),
        w: width,
        h: height,
    }
}

struct Geometry {
    target: Rect,
    root: Rect,
    intersection: Rect,
    ratio: f64,
    intersecting: bool,
}

const EMPTY: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 0.0,
    h: 0.0,
};

fn compute(scope: &mut Scope<'_>, observer: &Observer, target: usize) -> (bool, Geometry) {
    let js = ffi::js_of(scope);
    let layout_root = ffi::layout_root(js);
    let mut root = root_rect(scope, observer, layout_root);
    let width = root.w;
    let [top, right, bottom, left] = observer.margins.map(|m| m.resolve(width));
    let r_left = root.x - left;
    let r_top = root.y - top;
    let r_right = root.x + root.w + right;
    let r_bottom = root.y + root.h + bottom;
    root = Rect {
        x: r_left,
        y: r_top,
        w: r_right - r_left,
        h: r_bottom - r_top,
    };
    let mut geometry = Geometry {
        target: EMPTY,
        root,
        intersection: EMPTY,
        ratio: 0.0,
        intersecting: false,
    };
    let Some(layout_root) = layout_root.filter(|_| target != 0) else {
        return (false, geometry);
    };
    let Some(target_box) = ffi::find_by_dom(layout_root, target) else {
        return (false, geometry);
    };
    geometry.target = ffi::visual_border_box(target_box);
    let mut clipped = geometry.target;
    let mut intersects = intersect(root, &mut clipped);
    let root_node = ffi::unwrap_element(&observer.root);
    let root_ptr = root_node.map_or(0, |n| n.as_ptr() as usize);
    let mut found_root = root_node.is_none_or(|n| n.kind() == Kind::Document);
    let mut parent = target_box.parent();
    while let Some(p) = parent {
        if p.dom_ptr() as usize == root_ptr {
            found_root = true;
            break;
        }
        if overflow_clips(p) && !intersect(ffi::visual_padding_box(p), &mut clipped) {
            intersects = false;
        }
        parent = p.parent();
    }
    if !found_root {
        intersects = false;
    }
    if !intersects {
        clipped.w = 0.0;
        clipped.h = 0.0;
    }
    geometry.intersection = clipped;
    geometry.intersecting = intersects;
    let target_area = geometry.target.w * geometry.target.h;
    if target_area > 0.0 {
        geometry.ratio = clipped.w * clipped.h / target_area;
    }
    (true, geometry)
}

fn shifted(rect: Rect, dx: f64, dy: f64) -> Rect {
    Rect {
        x: rect.x - dx,
        y: rect.y - dy,
        ..rect
    }
}

fn entry(scope: &mut Scope<'_>, target: &Value, geometry: &Geometry) -> Value {
    let object = scope.new_object();
    set(scope, &object, "target", target.clone());
    set(
        scope,
        &object,
        "isIntersecting",
        Value::boolean(geometry.intersecting),
    );
    set(
        scope,
        &object,
        "isVisible",
        Value::boolean(geometry.intersecting),
    );
    set(
        scope,
        &object,
        "intersectionRatio",
        Value::number(geometry.ratio),
    );
    let time = ffi::realm_now_ms(scope);
    set(scope, &object, "time", Value::number(time));
    for (key, rect) in [
        ("boundingClientRect", geometry.target),
        ("intersectionRect", geometry.intersection),
        ("rootBounds", geometry.root),
    ] {
        let rect = ffi::dom_rect(scope, rect);
        set(scope, &object, key, rect);
    }
    object
}

fn evaluate(scope: &mut Scope<'_>, observer: &Observer, target: &mut Target) -> Option<Value> {
    let node = ffi::node_address(&target.wrapper);
    let (has_box, mut geometry) = compute(scope, observer, node);
    let intersecting = has_box && geometry.intersecting;
    geometry.intersecting = intersecting;
    if geometry.target.w <= 0.0 || geometry.target.h <= 0.0 {
        geometry.ratio = if intersecting { 1.0 } else { 0.0 };
    }
    let ratio = geometry.ratio;
    let changed = !target.has_fired
        || intersecting != target.last_intersecting
        || observer.threshold_index(target.last_ratio) != observer.threshold_index(ratio);
    let value = if changed {
        let dx = scroll_prop(scope, "scrollX");
        let dy = scroll_prop(scope, "scrollY");
        let viewport = Geometry {
            target: shifted(geometry.target, dx, dy),
            root: shifted(geometry.root, dx, dy),
            intersection: shifted(geometry.intersection, dx, dy),
            ratio,
            intersecting,
        };
        Some(entry(scope, &target.wrapper, &viewport))
    } else {
        None
    };
    target.last_intersecting = intersecting;
    target.last_ratio = ratio;
    target.has_fired = true;
    value
}

fn call_back(scope: &mut Scope<'_>, observer: &Observer, wrapper: &Value, entries: Value) {
    if !scope.is_function(&observer.callback) {
        return;
    }
    let js = ffi::js_of(scope);
    let args = [entries, wrapper.clone()];
    if let Err(exception) =
        ffi::call_observer(scope, &observer.callback, wrapper, &args, None, true)
    {
        crate::report_error(js, scope, "IntersectionObserver", &exception);
    }
}

fn deliver(scope: &mut Scope<'_>, observer: &Observer) {
    let wrapper = {
        let state = observer.state.borrow();
        if state.disconnected {
            return;
        }
        state.pin.clone()
    };
    let mut entries: Option<Value> = None;
    let mut count = 0u32;
    let mut i = 0;
    loop {
        let Some(mut target) = observer.state.borrow().targets.get(i).cloned() else {
            break;
        };
        let changed = evaluate(scope, observer, &mut target);
        {
            let mut state = observer.state.borrow_mut();
            if let Some(slot) = state.targets.get_mut(i) {
                if slot.wrapper.same_object(&target.wrapper) {
                    slot.last_intersecting = target.last_intersecting;
                    slot.last_ratio = target.last_ratio;
                    slot.has_fired = target.has_fired;
                }
            }
        }
        drop(target);
        if let Some(entry) = changed {
            let array = entries.get_or_insert_with(|| scope.new_array());
            let array = array.clone();
            set_index(scope, &array, count, entry);
            count += 1;
        }
        i += 1;
    }
    if let (Some(entries), true) = (entries, count > 0) {
        let wrapper = wrapper.unwrap_or_else(Value::undefined);
        call_back(scope, observer, &wrapper, entries);
    }
}

pub(crate) fn tick(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    if !ffi::has_main_context(js) || page.ticking.get() {
        return;
    }
    page.ticking.set(true);
    ffi::with_main_context(js, |scope| {
        let mut index = 0;
        while let Some(observer) = crate::upgrade_at(&page.intersection, index) {
            index += 1;
            if let Some(observer) = observer {
                deliver(scope, &observer);
            }
        }
    });
    page.ticking.set(false);
}

fn observer_of(scope: &mut Scope<'_>, this: &Value) -> Option<Rc<Observer>> {
    scope.host_data::<Handle>(this).map(|handle| handle.0)
}

fn element_node(target: Option<&Value>) -> Option<usize> {
    let node = ffi::unwrap_element(target?)?;
    (node.kind() == Kind::Element).then_some(node.as_ptr() as usize)
}

const NOT_AN_ELEMENT: &str = "IntersectionObserver target must be an Element";
const INCOMPATIBLE: &str = "incompatible IntersectionObserver receiver";

pub(crate) fn observe(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.is_empty() {
        return Err(scope.type_error(NOT_AN_ELEMENT));
    }
    let observer = observer_of(scope, this).filter(|o| !o.state.borrow().disconnected);
    let Some(observer) = observer else {
        return Err(scope.type_error(INCOMPATIBLE));
    };
    let Some(node) = element_node(args.first()) else {
        return Err(scope.type_error(NOT_AN_ELEMENT));
    };
    {
        let mut state = observer.state.borrow_mut();
        if state
            .targets
            .iter()
            .any(|t| ffi::node_address(&t.wrapper) == node)
        {
            return Ok(Value::undefined());
        }
        state.targets.push(Target {
            wrapper: args[0].clone(),
            last_intersecting: false,
            has_fired: false,
            last_ratio: 0.0,
        });
    }
    observer.set_pin(Some(this));
    ffi::schedule_tick(ffi::js_of(scope));
    Ok(Value::undefined())
}

pub(crate) fn unobserve(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    if args.is_empty() {
        return Err(scope.type_error(NOT_AN_ELEMENT));
    }
    let Some(observer) = observer_of(scope, this) else {
        return Err(scope.type_error(INCOMPATIBLE));
    };
    let Some(node) = element_node(args.first()) else {
        return Err(scope.type_error(NOT_AN_ELEMENT));
    };
    let (removed, empty) = {
        let mut state = observer.state.borrow_mut();
        let removed = state
            .targets
            .iter()
            .position(|t| ffi::node_address(&t.wrapper) == node)
            .map(|index| state.targets.remove(index));
        (removed, state.targets.is_empty())
    };
    drop(removed);
    if empty {
        observer.set_pin(None);
    }
    Ok(Value::undefined())
}

pub(crate) fn disconnect(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let Some(observer) = observer_of(scope, this) else {
        return Ok(Value::undefined());
    };
    let targets = core::mem::take(&mut observer.state.borrow_mut().targets);
    drop(targets);
    observer.set_pin(None);
    Ok(Value::undefined())
}

pub(crate) fn take_records(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    Ok(scope.new_array())
}

const METHODS: [(&str, u32, NativeFn); 4] = [
    ("observe", 1, observe),
    ("unobserve", 1, unobserve),
    ("disconnect", 0, disconnect),
    ("takeRecords", 0, take_records),
];

fn root_option(scope: &mut Scope<'_>, options: &Value) -> Result<Value, Value> {
    let root = scope.get(options, "root")?;
    if root.is_undefined() || root.is_null() {
        return Ok(Value::null());
    }
    let valid = ffi::unwrap_element(&root)
        .is_some_and(|node| matches!(node.kind(), Kind::Element | Kind::Document));
    if !valid {
        return Err(scope.type_error("IntersectionObserver root must be an Element or Document"));
    }
    Ok(root)
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let Some(callback) = args.first().filter(|cb| scope.is_function(cb)).cloned() else {
        return Err(scope.type_error("IntersectionObserver callback must be callable"));
    };
    let options = args.get(1).cloned().unwrap_or_else(Value::undefined);
    let thresholds = parse_thresholds(scope, &options)?;
    let mut root = Value::null();
    let mut root_margin = Value::undefined();
    if !options.is_undefined() && !options.is_null() {
        root = root_option(scope, &options)?;
        root_margin = scope.get(&options, "rootMargin")?;
    }
    let margin_source = if root_margin.is_undefined() {
        b"0px".to_vec()
    } else {
        crate::text_of(scope, &root_margin)?
    };
    let Some((margins, root_margin_text)) = parse_root_margin(&margin_source) else {
        return Err(scope.syntax_error("invalid IntersectionObserver rootMargin"));
    };
    let js = ffi::js_of(scope);
    let observer = Rc::new(Observer {
        js,
        callback,
        root: root.clone(),
        margins,
        thresholds,
        state: RefCell::new(State::default()),
    });
    let (object, proto_bound) =
        crate::new_instance(scope, new_target, Handle(observer.clone()), &METHODS);
    if !proto_bound {
        for (name, arity, f) in METHODS {
            bind(scope, &object, name, arity, f);
        }
    }
    set(scope, &object, "root", root);
    let margin_text = scope.string(&root_margin_text);
    set(scope, &object, "rootMargin", margin_text);
    let threshold_values = scope.new_array();
    for (i, &threshold) in observer.thresholds.iter().enumerate() {
        set_index(scope, &threshold_values, i as u32, Value::number(threshold));
    }
    let _ = scope.freeze(&threshold_values);
    set(scope, &object, "thresholds", threshold_values);
    for (name, arity, f) in METHODS {
        bind_if_not_callable(scope, &object, name, arity, f);
    }
    if !js.is_null() {
        page_or_new(js)
            .intersection
            .borrow_mut()
            .push(Rc::downgrade(&observer));
    }
    Ok(object)
}
