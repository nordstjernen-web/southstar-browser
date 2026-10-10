//! Southstar — scripted input for the in-process run: clicks, typing and keys in form controls, drags, holds and scrolls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::collections::VecDeque;
use std::ffi::CString;

use southstar_dom::{Node, ancestors_and_self, children};
use southstar_glib::GBoolean;

use crate::ffi::{self, Js, err, fmt_g, out};
use crate::inproc::{Ctx, NavCapture, relayout, settle};
use crate::text::strip;

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn utf8_skip(b: u8) -> usize {
    match b {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        0xf8..=0xfb => 5,
        0xfc..=0xfd => 6,
        _ => 1,
    }
}

fn next_char(s: &[u8], at: usize) -> usize {
    (at + s.get(at).map_or(1, |&b| utf8_skip(b))).min(s.len())
}

fn prev_char(s: &[u8], at: usize) -> usize {
    let mut p = at;
    while p > 0 {
        p -= 1;
        if s[p] & 0xc0 != 0x80 {
            return p;
        }
    }
    0
}

fn edit_replace(fc: &Ctx, lo: usize, hi: usize, ins: Option<&[u8]>) {
    let Some(t) = fc.focused() else {
        return;
    };
    let cur = ffi::editable_value(t);
    let clen = cur.len();
    let lo = lo.min(clen);
    let hi = hi.min(clen).max(lo);
    let mut ins = ins.map_or(Vec::new(), |i| {
        i[..i.iter().position(|&b| b == 0).unwrap_or(i.len())].to_vec()
    });
    if !ins.is_empty() && ffi::is_numeric_input(t) {
        ins = ffi::numeric_filter_insert(&ins);
        if ins.is_empty() {
            return;
        }
    }
    if !ins.is_empty() && ffi::length_limits_apply(t) {
        let maxl = t
            .attr(c"maxlength")
            .filter(|m| !m.is_empty())
            .map(ffi::c_atol);
        if let Some(maxl) = maxl.filter(|&m| m >= 0) {
            let kept = ffi::utf8_strlen(&cur[..lo]) + ffi::utf8_strlen(&cur[hi..]);
            let room = (maxl - kept).max(0);
            if ffi::utf8_strlen(&ins) > room {
                let mut p = 0;
                for _ in 0..room {
                    p = next_char(&ins, p);
                }
                ins.truncate(p);
                if ins.is_empty() {
                    return;
                }
            }
        }
    }
    let value = [&cur[..lo], ins.as_slice(), &cur[hi..]].concat();
    if let Some(js) = fc.js
        && js.dispatch_checked(t, c"beforeinput")
    {
        return;
    }
    ffi::set_editable_value(t, &value);
    fc.caret.set(lo + ins.len());
    fc.anchor.set(fc.caret.get());
    if let Some(js) = fc.js {
        js.dispatch(t, c"input");
        ffi::consume_mutated(Some(js));
    }
}

pub fn submit_form_from(fc: &Ctx, nav: &NavCapture, trigger: Node) {
    if ffi::effectively_disabled(trigger) {
        return;
    }
    let doc = fc.doc().or_else(|| ffi::root(trigger));
    let Some(form) = ffi::form_owner(trigger, doc) else {
        return;
    };
    let root = doc.unwrap_or(form);
    if form.attr(c"novalidate").is_none()
        && trigger.attr(c"formnovalidate").is_none()
        && let Some(bad) = ffi::first_invalid(form, root)
    {
        let name = bad.attr(c"name").filter(|n| !n.is_empty());
        let name = name.map_or(&b"(unnamed)"[..], CStr::to_bytes);
        err(&[b"[headless] form blocked by invalid field ", name, b"\n"].concat());
        return;
    }
    if let Some(js) = fc.js {
        let prevented = js.dispatch_submit(form, trigger);
        ffi::consume_mutated(Some(js));
        if prevented {
            return;
        }
    }
    crate::inproc::form_submit(nav, form, Some(trigger));
}

fn is_control(n: Node) -> bool {
    n.is_element()
        && n.name().is_some_and(|name| {
            matches!(
                name.to_bytes(),
                b"input" | b"select" | b"textarea" | b"button"
            )
        })
}

fn label_target<'a>(fc: &'a Ctx, label: Node<'a>) -> Option<Node<'a>> {
    let by_id = label
        .attr(c"for")
        .filter(|id| !id.is_empty())
        .and_then(|id| fc.doc().and_then(|doc| ffi::find_by_id(doc, id)));
    if by_id.is_some() {
        return by_id;
    }
    let mut queue: VecDeque<Node> = children(label).collect();
    while let Some(d) = queue.pop_front() {
        if is_control(d) {
            return Some(d);
        }
        queue.extend(children(d));
    }
    None
}

fn element_named(n: Node, name: &[u8]) -> bool {
    n.is_element() && n.name().is_some_and(|nm| nm.to_bytes() == name)
}

fn click(fc: &Ctx, nav: &NavCapture, x: f64, y: f64) {
    let Some(layout) = fc.layout() else {
        return;
    };
    ffi::note_pointer_input(fc.js, true);
    let link = ffi::hit_link(layout, x, y);
    let form_target = ffi::hit_form_dom(layout, x, y);
    let inline_target = ffi::hit_inline_dom(layout, x, y);
    let hit = ffi::hit_test(Some(layout), x, y);
    let dom = match (form_target, inline_target, &link) {
        (Some(d), _, _) | (None, Some(d), _) => Some(d),
        (None, None, Some(link)) => link.dom,
        (None, None, None) => hit.and_then(ffi::box_dom),
    };
    let Some(mut dom) = dom else {
        fc.set_focused(None);
        return;
    };
    if form_target.is_none()
        && let Some(label) = ancestors_and_self(dom).find(|n| ffi::is_named(*n, c"label"))
        && let Some(tgt) = label_target(fc, label).filter(|t| t.as_ptr() != dom.as_ptr())
    {
        dom = tgt;
    }
    let hit_img = hit
        .and_then(ffi::box_dom)
        .filter(|d| ffi::is_named(*d, c"img"));
    let img = match (hit_img, hit) {
        (Some(_), Some(h)) => {
            let (m, b, p) = (h.margin(), h.border(), h.padding());
            (
                h.x() + m.left + b.left + p.left,
                h.y() + m.top + b.top + p.top,
                h.content_width(),
                h.content_height(),
            )
        }
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    let link_href = link
        .as_ref()
        .and_then(|l| l.href)
        .filter(|h| !h.is_empty())
        .map(|h| h.to_bytes().to_vec());
    let shown = dom.name().map_or(&b"(text)"[..], CStr::to_bytes);
    err(&[b"[headless] click hit <", shown, b">\n"].concat());
    let editable = ancestors_and_self(dom).find(|n| ffi::is_editable(*n));
    let mut prevented = false;
    if let Some(js) = fc.js {
        prevented = js.dispatch_checked(dom, c"click");
        ffi::consume_mutated(Some(js));
    }
    if let Some(editable) = editable {
        ffi::flatten_editable(editable);
        if let (Some(js), Some(focused)) = (fc.js, fc.focused())
            && focused.as_ptr() != editable.as_ptr()
        {
            js.dispatch(focused, c"blur");
        }
        fc.set_focused(Some(editable));
        let caret = if ffi::has_editable_value(editable) {
            ffi::editable_value(editable).len()
        } else {
            0
        };
        fc.caret.set(caret);
        fc.anchor.set(caret);
        if let Some(js) = fc.js {
            js.set_focused_node(editable);
            js.dispatch(editable, c"focus");
            js.dispatch(editable, c"focusin");
        }
        return;
    }
    if prevented {
        return;
    }
    if let Some(js) = fc.js
        && js.click_activate(dom)
    {
        ffi::consume_mutated(Some(js));
    }
    if let Some(trigger) = ancestors_and_self(dom).find(|n| ffi::is_submit_trigger(*n)) {
        submit_form_from(fc, nav, trigger);
        return;
    }
    if let (Some(img_node), Some(doc)) = (hit_img, fc.doc())
        && let Some(usemap) = img_node.attr(c"usemap").filter(|u| !u.is_empty())
        && let Some(href) =
            ffi::image_map_resolve(doc, usemap, (x - img.0, y - img.1), (img.2, img.3))
    {
        nav.set_pending(href);
        return;
    }
    if let Some(href) = link_href {
        nav.set_pending(href);
        return;
    }
    if let Some(a) = ancestors_and_self(dom).find(|n| ffi::is_named(*n, c"a"))
        && let Some(href) = a.attr(c"href").filter(|h| !h.is_empty())
    {
        let url = match hit_img.filter(|i| i.attr(c"ismap").is_some()) {
            Some(_) => {
                let ix = ((x - img.0) as c_int).max(0);
                let iy = ((y - img.1) as c_int).max(0);
                [href.to_bytes(), format!("?{ix},{iy}").as_bytes()].concat()
            }
            None => href.to_bytes().to_vec(),
        };
        nav.set_pending(url);
        return;
    }
    for cur in ancestors_and_self(dom) {
        if element_named(cur, b"summary")
            && let Some(details) = cur.parent().filter(|p| ffi::is_named(*p, c"details"))
        {
            let now_open = details.attr(c"open").is_none();
            if now_open {
                ffi::set_attr(details, c"open", c"");
            } else {
                ffi::remove_attr(details, c"open");
            }
            if let Some(js) = fc.js {
                js.details_toggle_open(details, now_open);
                ffi::consume_mutated(Some(js));
            }
            return;
        }
        if element_named(cur, b"input") {
            let kind = cur.attr(c"type").map(CStr::to_bytes);
            let checked: &CStr = match kind {
                Some(k) if k.eq_ignore_ascii_case(b"checkbox") => {
                    if ffi::input_is_checked(cur) {
                        c"0"
                    } else {
                        c"1"
                    }
                }
                Some(k) if k.eq_ignore_ascii_case(b"radio") => c"1",
                _ => continue,
            };
            ffi::set_attr(cur, c"data-nd-checked", checked);
            if let Some(js) = fc.js {
                js.dispatch(cur, c"input");
                js.dispatch(cur, c"change");
                ffi::consume_mutated(Some(js));
            }
            return;
        }
    }
    fc.set_focused(None);
}

fn attr_is(n: Node, name: &CStr, value: &[u8]) -> bool {
    n.attr(name)
        .is_some_and(|v| v.to_bytes().eq_ignore_ascii_case(value))
}

fn drag_source(fc: &Ctx, x: f64, y: f64) -> Option<Node<'_>> {
    let hit = ffi::hit_test(fc.layout(), x, y)?;
    for p in ancestors_and_self(ffi::box_dom(hit)?) {
        let Some(name) = p.name().filter(|_| p.is_element()) else {
            continue;
        };
        if attr_is(p, c"draggable", b"true") {
            return Some(p);
        }
        if attr_is(p, c"draggable", b"false") {
            continue;
        }
        let nonempty = |attr: &CStr| p.attr(attr).is_some_and(|v| !v.is_empty());
        match name.to_bytes() {
            b"a" if nonempty(c"href") => return Some(p),
            b"img" if nonempty(c"src") => return Some(p),
            _ => {}
        }
    }
    None
}

fn drag_target(fc: &Ctx, x: f64, y: f64) -> Option<Node<'_>> {
    let hit = ffi::hit_test(fc.layout(), x, y).and_then(ffi::box_dom);
    if hit.is_some() {
        return hit;
    }
    let doc = fc.doc()?;
    ffi::first_element(doc, c"body").or(Some(doc))
}

fn seed_drag_data(fc: &Ctx, session: &ffi::DragSession, source: Node) {
    let raw = if ffi::is_named(source, c"a") {
        source.attr(c"href")
    } else if ffi::is_named(source, c"img") {
        source.attr(c"src")
    } else {
        None
    };
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return;
    };
    let abs = match &fc.base {
        Some(base) => ffi::url_resolve(base, raw),
        None => Some(raw.to_bytes().to_vec()),
    };
    let Some(abs) = abs.map(|a| cstring(&a)) else {
        return;
    };
    session.set_data(c"text/plain", &abs);
    session.set_data(c"text/uri-list", &abs);
}

fn drag(fc: &Ctx, (x0, y0): (f64, f64), (x1, y1): (f64, f64)) {
    let (Some(js), false) = (fc.js, fc.layout.get().is_null()) else {
        return;
    };
    let Some(source) = drag_source(fc, x0, y0) else {
        return;
    };
    let Some(session) = js.drag_session() else {
        return;
    };
    seed_drag_data(fc, &session, source);
    let shown = source.name().map_or(&b"(text)"[..], CStr::to_bytes);
    err(&[b"[headless] drag hit <", shown, b">\n"].concat());
    let send =
        |target: Node, kind: &CStr, at: (f64, f64), buttons: c_int, related: Option<Node>| {
            let prevented = session.dispatch(js, target, kind, at, buttons, related);
            ffi::consume_mutated(Some(js));
            prevented
        };
    if send(source, c"dragstart", (x0, y0), 1, None) {
        return;
    }
    let target = drag_target(fc, x1, y1);
    if let Some(target) = target {
        let mut can_drop = send(target, c"dragenter", (x1, y1), 1, Some(source));
        if send(target, c"dragover", (x1, y1), 1, None) {
            can_drop = true;
        }
        if can_drop {
            send(target, c"drop", (x1, y1), 0, None);
        } else {
            send(target, c"dragleave", (x1, y1), 1, None);
        }
    }
    send(source, c"dragend", (x1, y1), 0, target);
}

fn mouse_target(fc: &Ctx, x: f64, y: f64) -> Option<Node<'_>> {
    let hit = ffi::hit_test(fc.layout(), x, y).and_then(ffi::box_dom);
    hit.or_else(|| fc.doc().and_then(|doc| ffi::first_element(doc, c"body")))
}

fn emit_pointer_and_mouse(
    js: Js,
    target: Node,
    kinds: (&CStr, &CStr),
    at: (f64, f64),
    buttons: (c_int, c_int),
) {
    let mut prevented: GBoolean = 0;
    js.dispatch_mouse(target, kinds.0, at, buttons, &mut prevented);
    js.dispatch_mouse(target, kinds.1, at, buttons, &mut prevented);
    ffi::consume_mutated(Some(js));
}

fn mouse_drag(fc: &Ctx, (x0, y0): (f64, f64), (x1, y1): (f64, f64)) {
    let Some(js) = fc.js else {
        return;
    };
    let Some(down) = mouse_target(fc, x0, y0) else {
        return;
    };
    emit_pointer_and_mouse(js, down, (c"pointerdown", c"mousedown"), (x0, y0), (0, 1));
    let steps = 8;
    for i in 1..=steps {
        let x = x0 + (x1 - x0) * i as f64 / steps as f64;
        let y = y0 + (y1 - y0) * i as f64 / steps as f64;
        if let Some(over) = mouse_target(fc, x, y) {
            emit_pointer_and_mouse(js, over, (c"pointermove", c"mousemove"), (x, y), (0, 1));
        }
        settle(30, fc);
    }
    if let Some(up) = mouse_target(fc, x1, y1) {
        emit_pointer_and_mouse(js, up, (c"pointerup", c"mouseup"), (x1, y1), (0, 0));
    }
}

fn key(fc: &Ctx, nav: &NavCapture, name: &[u8]) {
    ffi::note_pointer_input(fc.js, false);
    let Some(t) = fc.focused() else {
        return;
    };
    if name.is_empty() {
        return;
    }
    let mut key_prevented = false;
    if let Some(js) = fc.js {
        let mapped = crate::renderer::KEYS
            .iter()
            .find(|(n, _, _)| name.eq_ignore_ascii_case(n))
            .map(|&(_, key, code)| (key, code));
        if let Some((jskey, code)) = mapped {
            key_prevented = js.dispatch_key(t, c"keydown", jskey, code, true);
            js.dispatch_key(t, c"keyup", jskey, code, false);
            ffi::consume_mutated(Some(js));
        }
    }
    let cur = ffi::editable_value(t);
    let clen = cur.len();
    fc.caret.set(fc.caret.get().min(clen));
    fc.anchor.set(fc.anchor.get().min(clen));
    let (caret, anchor) = (fc.caret.get(), fc.anchor.get());
    let (lo, hi) = (caret.min(anchor), caret.max(anchor));
    let has_sel = lo != hi;
    let multiline = element_named(t, b"textarea") || ffi::is_contenteditable_host(t);
    let is = |k: &[u8]| name.eq_ignore_ascii_case(k);
    if is(b"Tab") {
        let (Some(js), false) = (fc.js, key_prevented) else {
            return;
        };
        if let Some(next) = js.sequential_focus_target() {
            if let Some(focused) = fc.focused().filter(|f| f.as_ptr() != next.as_ptr()) {
                js.dispatch(focused, c"blur");
            }
            fc.set_focused(Some(next));
            js.set_focused_node(next);
            let caret = if ffi::has_editable_value(next) {
                ffi::editable_value(next).len()
            } else {
                0
            };
            fc.caret.set(caret);
            fc.anchor.set(caret);
            ffi::consume_mutated(Some(js));
        }
        return;
    }
    if is(b"Enter") || is(b"Return") {
        if multiline {
            edit_replace(fc, lo, hi, Some(b"\n"));
        } else if !key_prevented && element_named(t, b"input") {
            submit_form_from(fc, nav, t);
        }
        return;
    }
    if is(b"Backspace") {
        if has_sel {
            edit_replace(fc, lo, hi, None);
        } else if caret > 0 {
            edit_replace(fc, prev_char(&cur, caret), caret, None);
        }
        return;
    }
    if is(b"Delete") {
        if has_sel {
            edit_replace(fc, lo, hi, None);
        } else if caret < clen {
            edit_replace(fc, caret, next_char(&cur, caret), None);
        }
        return;
    }
    if is(b"Left") {
        if caret > 0 {
            fc.caret.set(prev_char(&cur, caret));
        }
        fc.anchor.set(fc.caret.get());
        return;
    }
    if is(b"Right") {
        if caret < clen {
            fc.caret.set(next_char(&cur, caret));
        }
        fc.anchor.set(fc.caret.get());
        return;
    }
    if is(b"Home") {
        fc.caret.set(0);
        fc.anchor.set(0);
        return;
    }
    if is(b"End") {
        fc.caret.set(clen);
        fc.anchor.set(clen);
        return;
    }
    if is(b"Up") || is(b"Down") {
        step_number(fc, t, &cur, is(b"Up"));
    }
}

fn step_number(fc: &Ctx, t: Node, cur: &[u8], up: bool) {
    let is_number = element_named(t, b"input")
        && t.attr(c"type")
            .is_some_and(|k| k.to_bytes().eq_ignore_ascii_case(b"number"));
    if !is_number {
        return;
    }
    let parse = |s: &CStr| southstar_glib::ascii_strtod(s.to_bytes());
    let mut step = t.attr(c"step").filter(|s| !s.is_empty()).map_or(1.0, parse);
    let positive = step > 0.0;
    if !positive {
        step = 1.0;
    }
    let mut val = if cur.is_empty() {
        0.0
    } else {
        southstar_glib::ascii_strtod(cur)
    };
    val += if up { step } else { -step };
    if let Some(min) = t.attr(c"min").filter(|m| !m.is_empty()).map(parse)
        && val < min
    {
        val = min;
    }
    if let Some(max) = t.attr(c"max").filter(|m| !m.is_empty()).map(parse)
        && val > max
    {
        val = max;
    }
    let buf = fmt_g(val);
    ffi::set_editable_value(t, buf.as_bytes());
    fc.caret.set(buf.len());
    fc.anchor.set(buf.len());
    if let Some(js) = fc.js {
        js.dispatch(t, c"input");
        js.dispatch(t, c"change");
        ffi::consume_mutated(Some(js));
    }
}

fn rightclick(fc: &Ctx, x: f64, y: f64) {
    let layout = fc.layout();
    let form_target = layout.and_then(|l| ffi::hit_form_dom(l, x, y));
    let inline_target = layout.and_then(|l| ffi::hit_inline_dom(l, x, y));
    let hit = layout.and_then(|l| ffi::hit_test(Some(l), x, y));
    let dom = form_target
        .or(inline_target)
        .or_else(|| hit.and_then(ffi::box_dom));
    let (Some(dom), Some(js)) = (dom, fc.js) else {
        return;
    };
    let mut prevented: GBoolean = 0;
    js.dispatch_mouse(dom, c"contextmenu", (x, y), (2, 0), &mut prevented);
    err(format!(
        "[headless] rightclick {},{} prevented={prevented}\n",
        fmt_g(x),
        fmt_g(y)
    )
    .as_bytes());
    relayout(fc);
}

fn hold(fc: &Ctx, x: f64, y: f64, ms: core::ffi::c_long) {
    err(format!("[headless] hold {},{} {ms}ms\n", fmt_g(x), fmt_g(y)).as_bytes());
    let dom = fc
        .layout()
        .and_then(|l| ffi::hit_test(Some(l), x, y))
        .and_then(ffi::box_dom);
    let shown = dom
        .and_then(|d| d.name())
        .map_or(&b"(none)"[..], CStr::to_bytes);
    err(&[b"[headless] hold hit <", shown, b">\n"].concat());
    let Some(dom) = dom else {
        return;
    };
    ffi::css_set_active_node(Some(dom));
    relayout(fc);
    if ms > 0 {
        settle(ms as c_int, fc);
    }
    ffi::css_set_active_node(None);
    relayout(fc);
}

fn eval_action(fc: &Ctx, src: &CStr, origin: &CStr) {
    if let Some(result) = ffi::eval_source(fc.js, src, origin) {
        out(&[b"act-eval: ", result.to_bytes(), b"\n"].concat());
    }
    ffi::consume_mutated(fc.js);
}

pub fn run_actions(fc: &Ctx, nav: &NavCapture, spec: &[u8]) {
    if spec.is_empty() {
        return;
    }
    relayout(fc);
    for action in spec.split(|&b| b == b';') {
        let a = strip(action);
        if a.is_empty() {
            continue;
        }
        if let Some(rest) = a.strip_prefix(b"click ") {
            if let Some((x, y)) = ffi::scan_point(rest) {
                err(format!("[headless] click {},{}\n", fmt_g(x), fmt_g(y)).as_bytes());
                click(fc, nav, x, y);
            }
        } else if let Some(rest) = a.strip_prefix(b"rightclick ") {
            if let Some((x, y)) = ffi::scan_point(rest) {
                rightclick(fc, x, y);
            }
        } else if let Some(rest) = a.strip_prefix(b"hold ") {
            if let Some((x, y, ms)) = ffi::scan_hold(rest) {
                hold(fc, x, y, ms);
            }
        } else if let Some(rest) = a.strip_prefix(b"mousedrag ") {
            if let Some([x0, y0, x1, y1]) = ffi::scan_drag(rest) {
                err(format!(
                    "[headless] mousedrag {},{} -> {},{}\n",
                    fmt_g(x0),
                    fmt_g(y0),
                    fmt_g(x1),
                    fmt_g(y1)
                )
                .as_bytes());
                mouse_drag(fc, (x0, y0), (x1, y1));
            }
        } else if let Some(rest) = a.strip_prefix(b"drag ") {
            if let Some([x0, y0, x1, y1]) = ffi::scan_drag(rest) {
                err(format!(
                    "[headless] drag {},{} -> {},{}\n",
                    fmt_g(x0),
                    fmt_g(y0),
                    fmt_g(x1),
                    fmt_g(y1)
                )
                .as_bytes());
                drag(fc, (x0, y0), (x1, y1));
            }
        } else if let Some(text) = a.strip_prefix(b"type ") {
            err(&[b"[headless] type \"", text, b"\"\n"].concat());
            let (caret, anchor) = (fc.caret.get(), fc.anchor.get());
            edit_replace(fc, caret.min(anchor), caret.max(anchor), Some(text));
        } else if let Some(rest) = a.strip_prefix(b"key ") {
            err(&[b"[headless] key ", rest, b"\n"].concat());
            key(fc, nav, strip(rest));
        } else if let Some(src) = a.strip_prefix(b"eval ") {
            eval_action(fc, &cstring(src), c"headless-act-eval");
        } else if let Some(rest) = a.strip_prefix(b"evalfile ") {
            let path = strip(rest);
            match ffi::file_contents(path) {
                Some(src) => eval_action(fc, &src, c"headless-act-evalfile"),
                None => err(&[b"[headless] evalfile: cannot read ", path, b"\n"].concat()),
            }
        } else if let Some(rest) = a.strip_prefix(b"scroll ") {
            if let Some((x, y)) = ffi::scan_point(rest).or_else(|| ffi::scan_pair(rest)) {
                err(format!("[headless] scroll {},{}\n", fmt_g(x), fmt_g(y)).as_bytes());
                ffi::note_viewport_scroll(fc.js, x, y);
                ffi::consume_mutated(fc.js);
            }
        } else if let Some(rest) = a.strip_prefix(b"wait ") {
            let ms = ffi::ascii_strtoll(rest).clamp(0, 600_000);
            err(format!("[headless] wait {ms}ms\n").as_bytes());
            settle(ms as c_int, fc);
        } else {
            err(&[b"[headless] unknown action: ", a, b"\n"].concat());
        }
        relayout(fc);
        if nav.has_pending() {
            break;
        }
    }
}
