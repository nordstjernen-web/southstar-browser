//! Southstar — SMIL attribute parsing: clock values, begin and end lists, value lists and the animation element itself.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::{Node, children};

use super::values::{combine, number_span, span_value, zero_like};
use super::{Calc, Element, Kind, MOTION_ATTR, Restart, Spec, SpecKind};

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn skip_ws(text: &[u8]) -> &[u8] {
    let start = text
        .iter()
        .position(|&c| !is_space(c))
        .unwrap_or(text.len());
    &text[start..]
}

fn strip(text: &[u8]) -> &[u8] {
    let text = skip_ws(text);
    let end = text
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(0, |e| e + 1);
    &text[..end]
}

fn text_of(value: &[u8]) -> String {
    String::from_utf8_lossy(strip(value)).into_owned()
}

fn attr_text(n: Node<'_>, name: &CStr) -> Option<String> {
    n.attr(name)
        .map(|v| String::from_utf8_lossy(v.to_bytes()).into_owned())
}

pub fn clock(text: &[u8]) -> Option<f64> {
    let mut s = skip_ws(text);
    let mut sign = 1.0;
    match s.first() {
        Some(b'+') => s = skip_ws(&s[1..]),
        Some(b'-') => {
            sign = -1.0;
            s = skip_ws(&s[1..]);
        }
        _ => {}
    }
    if !s.first().is_some_and(|&c| c.is_ascii_digit() || c == b'.') {
        return None;
    }
    let mut parts = Vec::with_capacity(3);
    loop {
        let len = number_span(s);
        if len == 0 || matches!(s[0], b'+' | b'-') {
            return None;
        }
        parts.push(span_value(&s[..len]));
        s = &s[len..];
        if s.first() != Some(&b':') || parts.len() == 3 {
            break;
        }
        s = &s[1..];
    }
    let value = match parts[..] {
        [h, m, sec] => h * 3600.0 + m * 60.0 + sec,
        [m, sec] => m * 60.0 + sec,
        _ => {
            s = skip_ws(s);
            let (scale, unit) = if s.starts_with(b"ms") {
                (0.001, 2)
            } else if s.starts_with(b"min") {
                (60.0, 3)
            } else if s.starts_with(b"h") {
                (3600.0, 1)
            } else if s.starts_with(b"s") {
                (1.0, 1)
            } else {
                (1.0, 0)
            };
            s = &s[unit..];
            parts[0] * scale
        }
    };
    skip_ws(s).is_empty().then_some(sign * value)
}

fn number_list(text: Option<&CStr>) -> Vec<f64> {
    let Some(text) = text else {
        return Vec::new();
    };
    let bytes = text.to_bytes();
    let mut out = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        let len = number_span(&bytes[p..]);
        if len == 0 {
            p += 1;
            continue;
        }
        out.push(span_value(&bytes[p..p + len]));
        p += len;
    }
    out
}

fn build_values(el: &mut Element, n: Node<'_>) {
    let from = attr_text(n, c"from");
    let to = attr_text(n, c"to");
    let by = attr_text(n, c"by");
    if el.kind == Kind::Set {
        el.values.extend(to.map(|to| text_of(to.as_bytes())));
        return;
    }
    if let Some(values) = n.attr(c"values") {
        el.values = values
            .to_bytes()
            .split(|&c| c == b';')
            .filter(|item| !strip(item).is_empty())
            .map(text_of)
            .collect();
        if !el.values.is_empty() {
            return;
        }
    }
    if let Some(to) = to {
        match from {
            Some(from) => el.values.push(text_of(from.as_bytes())),
            None => el.to_anim = true,
        }
        el.values.push(text_of(to.as_bytes()));
        return;
    }
    let Some(by) = by else {
        return;
    };
    if let Some(from) = from {
        let sum = combine(&from, &by, 1.0, 1.0, el.is_color);
        el.values.push(text_of(from.as_bytes()));
        el.values
            .push(text_of(sum.as_ref().unwrap_or(&by).as_bytes()));
        return;
    }
    if let Some(zero) = zero_like(&by) {
        el.values.push(text_of(zero.as_bytes()));
        el.additive = true;
    }
    el.values.push(text_of(by.as_bytes()));
}

fn event_name_start(item: &[u8]) -> usize {
    let mut start = 0;
    for (i, &c) in item.iter().enumerate() {
        if c == b'.' && item.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            start = i + 1;
        }
    }
    start
}

fn reference(item: &[u8]) -> Option<Spec> {
    let name = event_name_start(item);
    let end = name
        + item[name..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
    if end == name {
        return None;
    }
    let rest = skip_ws(&item[end..]);
    let offset = if rest.is_empty() { 0.0 } else { clock(rest)? };
    let id = (name > 1).then(|| &item[..name - 1]);
    let event = &item[name..end];
    let kind = match (id, event) {
        (Some(_), b"begin") => SpecKind::SyncBegin,
        (Some(_), b"end") => SpecKind::SyncEnd,
        _ => SpecKind::Event,
    };
    Some(Spec {
        kind,
        offset,
        id: id.and_then(|id| CString::new(id).ok()),
        event: if kind == SpecKind::Event {
            event.to_vec()
        } else {
            Vec::new()
        },
    })
}

fn offset_spec(offset: f64) -> Spec {
    Spec {
        kind: SpecKind::Offset,
        offset,
        id: None,
        event: Vec::new(),
    }
}

fn begins(el: &mut Element, list: Option<&CStr>) {
    let list = list.map_or(&[][..], CStr::to_bytes);
    if skip_ws(list).is_empty() {
        el.begins.push(offset_spec(0.0));
        return;
    }
    for item in list.split(|&c| c == b';').map(strip) {
        if let Some(offset) = clock(item) {
            el.begins.push(offset_spec(offset));
        } else if let Some(spec) = reference(item) {
            el.begins.push(spec);
        }
    }
}

fn ends(el: &mut Element, list: Option<&CStr>) {
    let Some(list) = list else {
        return;
    };
    for item in list.to_bytes().split(|&c| c == b';').map(strip) {
        if let Some(offset) = clock(item) {
            el.ends.push(offset);
        } else if let Some(spec) = reference(item) {
            el.end_specs.push(spec);
        }
    }
}

fn repeat(text: Option<&CStr>, is_clock: bool) -> f64 {
    let Some(text) = text else {
        return -1.0;
    };
    let s = skip_ws(text.to_bytes());
    if s.starts_with(b"indefinite") {
        return f64::INFINITY;
    }
    let value = if is_clock {
        clock(s)
    } else {
        Some(span_value(&s[..number_span(s)]))
    };
    value.filter(|&v| v > 0.0).unwrap_or(-1.0)
}

pub fn href(n: Node<'_>) -> Option<&CStr> {
    n.attr(c"href").or_else(|| n.attr(c"xlink:href"))
}

pub fn href_target<'a>(doc: Node<'a>, n: Node<'_>) -> Option<Node<'a>> {
    let id = href(n)?.to_bytes().strip_prefix(b"#")?;
    southstar_dom::index::find_by_id(doc, &CString::new(id).ok()?)
}

pub fn target_of<'a>(doc: Node<'a>, n: Node<'a>) -> Option<Node<'a>> {
    if href(n).is_some_and(|h| h.to_bytes().starts_with(b"#")) {
        return href_target(doc, n);
    }
    n.parent().filter(|p| p.is_element())
}

pub fn mpath_of(n: Node<'_>) -> Option<Node<'_>> {
    children(n).find(|c| {
        c.element_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(b"mpath"))
    })
}

fn load_motion_path(el: &mut Element, doc: Node<'_>, n: Node<'_>) {
    let referenced = mpath_of(n)
        .and_then(|mpath| href_target(doc, mpath))
        .and_then(|path| path.attr(c"d"));
    if let Some(d) = referenced.or_else(|| n.attr(c"path")) {
        el.path = crate::path_polyline(d);
    }
    let Some(rotate) = n.attr(c"rotate") else {
        return;
    };
    let rotate = skip_ws(rotate.to_bytes());
    if rotate.starts_with(b"auto-reverse") {
        el.rotate_auto = true;
        el.rotate_reverse = true;
    } else if rotate.starts_with(b"auto") {
        el.rotate_auto = true;
    } else {
        el.rotate = span_value(&rotate[..number_span(rotate)]);
    }
}

fn attr_is_color(attr: &[u8]) -> bool {
    const COLORS: [&[u8]; 6] = [
        b"fill",
        b"stroke",
        b"color",
        b"stop-color",
        b"flood-color",
        b"lighting-color",
    ];
    COLORS.contains(&attr)
}

fn attr_is(n: Node<'_>, name: &CStr, value: &[u8]) -> bool {
    n.attr(name)
        .is_some_and(|v| skip_ws(v.to_bytes()).eq_ignore_ascii_case(value))
}

pub fn kind_of(name: &[u8]) -> Option<Kind> {
    const KINDS: [(&[u8], Kind); 4] = [
        (b"animate", Kind::Animate),
        (b"set", Kind::Set),
        (b"animateTransform", Kind::Transform),
        (b"animateMotion", Kind::Motion),
    ];
    KINDS
        .iter()
        .find(|(tag, _)| name.eq_ignore_ascii_case(tag))
        .map(|&(_, kind)| kind)
}

fn target_attr(n: Node<'_>, kind: Kind) -> Option<CString> {
    match kind {
        Kind::Motion => Some(MOTION_ATTR.to_owned()),
        _ => match n.attr(c"attributeName") {
            Some(name) => CString::new(strip(name.to_bytes())).ok(),
            None => (kind == Kind::Transform).then(|| c"transform".to_owned()),
        },
    }
}

pub fn element(doc: Node<'_>, n: Node<'_>, kind: Kind) -> Element {
    let attr = target_attr(n, kind);
    let mut el = Element {
        node: n.downgrade(),
        target: target_of(doc, n).map(Node::downgrade),
        svg: super::outer_svg(n).map(Node::downgrade),
        kind,
        calc: if kind == Kind::Motion {
            Calc::Paced
        } else {
            Calc::Linear
        },
        restart: Restart::Always,
        is_color: attr.as_ref().is_some_and(|a| attr_is_color(a.to_bytes())),
        attr,
        transform_type: String::new(),
        begin_src: n
            .attr(c"begin")
            .map_or_else(Vec::new, |b| b.to_bytes().to_vec()),
        values: Vec::new(),
        key_times: number_list(n.attr(c"keyTimes")),
        key_splines: number_list(n.attr(c"keySplines")),
        key_points: number_list(n.attr(c"keyPoints")),
        begins: Vec::new(),
        ends: Vec::new(),
        end_specs: Vec::new(),
        end_instances: Vec::new(),
        instances: Vec::new(),
        path: Vec::new(),
        dur: n
            .attr(c"dur")
            .and_then(|d| clock(d.to_bytes()))
            .filter(|&d| d > 0.0)
            .unwrap_or(-1.0),
        repeat_count: repeat(n.attr(c"repeatCount"), false),
        repeat_dur: repeat(n.attr(c"repeatDur"), true),
        forced_begin: 0.0,
        forced_end: 0.0,
        horizon: f64::NEG_INFINITY,
        rotate: 0.0,
        reported_begin: 0.0,
        reported_iteration: 0.0,
        reported: false,
        reported_end: false,
        has_forced_end: false,
        to_anim: false,
        additive: attr_is(n, c"additive", b"sum"),
        accumulate: attr_is(n, c"accumulate", b"sum"),
        freeze: attr_is(n, c"fill", b"freeze"),
        rotate_auto: false,
        rotate_reverse: false,
    };
    if kind == Kind::Transform {
        el.transform_type = n
            .attr(c"type")
            .map_or_else(|| String::from("translate"), |t| text_of(t.to_bytes()));
    }
    if attr_is(n, c"calcMode", b"discrete") {
        el.calc = Calc::Discrete;
    } else if attr_is(n, c"calcMode", b"linear") {
        el.calc = Calc::Linear;
    } else if attr_is(n, c"calcMode", b"paced") {
        el.calc = Calc::Paced;
    } else if attr_is(n, c"calcMode", b"spline") {
        el.calc = Calc::Spline;
    }
    if attr_is(n, c"restart", b"whenNotActive") {
        el.restart = Restart::WhenIdle;
    } else if attr_is(n, c"restart", b"never") {
        el.restart = Restart::Never;
    }
    build_values(&mut el, n);
    if kind == Kind::Motion {
        load_motion_path(&mut el, doc, n);
    }
    begins(&mut el, n.attr(c"begin"));
    ends(&mut el, n.attr(c"end"));
    el
}
