//! Southstar — pointer and keyboard input on a page: selection gestures, hover, wheel and scrollbar scrolling, dropped files, presses and clicks, select controls, access keys and key events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::{Kind, Node, ancestors_and_self, children};
use southstar_glib::GStr;
use southstar_layout::BoxRef;

use crate::ffi::{self, NsBrowser, Pointer};
use crate::{build, callbacks, edit, hit, page};

const HOVER_RELAYOUT_MIN_US: i64 = 60000;
const MAX_DEPTH: c_int = 1024;
const SCROLLBAR_WIDTH: f64 = 8.0;
const SCROLLBAR_MIN_THUMB: f64 = 16.0;
const SCROLLBAR_SLOP: f64 = 3.0;

pub enum Selected {
    Nothing,
    Text(Vec<u8>),
}

fn same(a: Option<Node<'_>>, b: Option<Node<'_>>) -> bool {
    core::ptr::eq(Node::ptr_or_null(a), Node::ptr_or_null(b))
}

fn relayout_now(b: &NsBrowser) {
    page::relayout(b);
    b.dirty.set(false);
}

fn relayout_if_dirty(b: &NsBrowser) -> bool {
    if b.dirty.get() {
        relayout_now(b);
        return true;
    }
    false
}

pub fn select(b: &NsBrowser, kind: c_int, x: c_int, y: c_int) -> Option<Selected> {
    let layout = b.layout()?;
    let field = edit::focused_field(b);
    let (fx, fy) = (f64::from(x), f64::from(y));
    match kind {
        0 => b.selection_anchor_at(layout, fx, fy),
        1 => {
            b.selection_extend_to(layout, fx, fy);
            b.selection_dragged.set(true);
        }
        2 => b.selection_clear(),
        3 => {
            if let Some(field) = field {
                edit::field_select_all(b, field);
                return Some(Selected::Nothing);
            }
            b.selection_select_all(layout);
        }
        4 => return Some(edit::copy_text(b).map_or(Selected::Nothing, Selected::Text)),
        7 => {
            return Some(
                field
                    .and_then(|f| edit::field_cut(b, f))
                    .map_or(Selected::Nothing, Selected::Text),
            );
        }
        5 => {
            b.selection_word_at(layout, fx, fy);
            b.selection_dragged.set(true);
        }
        6 => {
            b.selection_block_at(layout, fx, fy);
            b.selection_dragged.set(true);
        }
        _ => {}
    }
    callbacks::sync_js_selection(b);
    Some(Selected::Nothing)
}

fn viewport_pointer(
    b: &NsBrowser,
    x: c_int,
    y: c_int,
    button: c_int,
    buttons: c_int,
    mods: c_int,
) -> Pointer {
    Pointer {
        client: (
            f64::from(x) - b.cur_scroll_x.get(),
            f64::from(y) - b.cur_scroll_y.get(),
        ),
        page: (f64::from(x), f64::from(y)),
        button,
        buttons,
        mods,
    }
}

fn hover_dispatch(
    b: &NsBrowser,
    target: Option<Node<'_>>,
    x: c_int,
    y: c_int,
    types: (&CStr, &CStr),
    related: Option<Node<'_>>,
) {
    let (Some(js), Some(target)) = (b.js(), target) else {
        return;
    };
    let p = viewport_pointer(b, x, y, 0, 0, 0);
    js.dispatch_mouse(target, types.0, &p, related, false);
    js.dispatch_mouse(target, types.1, &p, related, false);
}

pub fn hover(b: &NsBrowser, x: c_int, y: c_int) -> c_int {
    let node = hit::hit_node(b, x, y);
    page::prune_cached_nodes(b);
    let prev = b.hover_node.get();
    let changed = !same(node, prev);
    b.hover_node.set(node);
    let mut dirty = false;
    if let Some(js) = b.js() {
        if changed {
            hover_dispatch(b, prev, x, y, (c"pointerout", c"mouseout"), node);
            hover_dispatch(b, prev, x, y, (c"pointerleave", c"mouseleave"), node);
            hover_dispatch(b, node, x, y, (c"pointerover", c"mouseover"), prev);
            hover_dispatch(b, node, x, y, (c"pointerenter", c"mouseenter"), prev);
        }
        hover_dispatch(b, node, x, y, (c"pointermove", c"mousemove"), None);
        if js.consume_mutated() {
            dirty = true;
        }
    }
    let hover_restyle = changed && ffi::page_uses_hover() && !b.selection_has_range();
    if dirty {
        relayout_now(b);
        return 1;
    }
    if hover_restyle {
        let now = ffi::monotonic_us();
        let min_gap = (b.relayout_cost_us.get() * 2).max(HOVER_RELAYOUT_MIN_US);
        if now - b.hover_relayout_us.get() >= min_gap {
            b.hover_relayout_us.set(now);
            b.hover_restyle_pending.set(false);
            relayout_now(b);
            return 1;
        }
        b.hover_restyle_pending.set(true);
    }
    0
}

fn clamp_scroll(v: f64, max: f64) -> f64 {
    let v = if v < 0.0 { 0.0 } else { v };
    if v > max { max } else { v }
}

pub fn scroll_at(b: &NsBrowser, x: c_int, y: c_int, dx: c_int, dy: c_int) -> (c_int, bool) {
    let Some(layout) = b.layout() else {
        return (0, false);
    };
    let Some(scroller) = ffi::hit_scrollable(layout, f64::from(x), f64::from(y)) else {
        return (0, false);
    };
    let (prev_x, prev_y) = (scroller.scroll_x(), scroller.scroll_y());
    let mut consumed = 0;
    if dy != 0 && scroller.scroll_max_y() > 0.0 {
        let ny = clamp_scroll(scroller.scroll_y() + f64::from(dy), scroller.scroll_max_y());
        if ny != scroller.scroll_y() {
            scroller.set_scroll(scroller.scroll_x(), ny);
            consumed = 1;
        }
    }
    if dx != 0 && scroller.scroll_max_x() > 0.0 {
        let nx = clamp_scroll(scroller.scroll_x() + f64::from(dx), scroller.scroll_max_x());
        if nx != scroller.scroll_x() {
            scroller.set_scroll(nx, scroller.scroll_y());
            consumed = 1;
        }
    }
    let mut snapped = false;
    if consumed != 0 {
        let (moved_x, moved_y) = (scroller.scroll_x(), scroller.scroll_y());
        ffi::scroll_snap_from(scroller, prev_x, prev_y);
        snapped = scroller.scroll_x() != moved_x || scroller.scroll_y() != moved_y;
        if let (Some(js), Some(dom)) = (b.js(), ffi::box_dom(scroller)) {
            js.dispatch_scroll(dom);
        }
    }
    (consumed, snapped)
}

struct Scrollbar {
    track_x: f64,
    track_w: f64,
    track_y: f64,
    track_h: f64,
    thumb_y: f64,
    thumb_h: f64,
}

fn scrollbar_geometry(b: BoxRef<'_>) -> Option<Scrollbar> {
    if b.scroll_max_y() <= 0.0 {
        return None;
    }
    let (margin, border, padding) = (b.margin(), b.border(), b.padding());
    let py = b.y() + margin.top + border.top;
    let ph = b.content_height() + padding.top + padding.bottom;
    if ph <= 16.0 {
        return None;
    }
    let px = b.x() + margin.left + border.left;
    let pw = b.content_width() + padding.left + padding.right;
    let th = ph - 2.0;
    let total = ph + b.scroll_max_y();
    let mut thh = th * (ph / total);
    if thh < SCROLLBAR_MIN_THUMB {
        thh = SCROLLBAR_MIN_THUMB;
    }
    if thh > th {
        thh = th;
    }
    let track_y = py + 1.0;
    Some(Scrollbar {
        track_x: px + pw - SCROLLBAR_WIDTH - 1.0,
        track_w: SCROLLBAR_WIDTH,
        track_y,
        track_h: th,
        thumb_h: thh,
        thumb_y: track_y + (th - thh) * (b.scroll_y() / b.scroll_max_y()),
    })
}

fn find_scrollable_by_dom<'a>(root: BoxRef<'a>, node: Node<'_>) -> Option<BoxRef<'a>> {
    if core::ptr::eq(root.dom_ptr(), node.as_ptr().cast()) && root.scrolls() {
        return Some(root);
    }
    southstar_layout::children(root).find_map(|c| find_scrollable_by_dom(c, node))
}

pub fn scrollbar_press(b: &NsBrowser, x: c_int, y: c_int) -> c_int {
    let Some(layout) = b.layout() else {
        return 0;
    };
    let Some((bar_box, lx, ly)) = ffi::hit_scrollbar(layout, f64::from(x), f64::from(y)) else {
        return 0;
    };
    let Some(g) = scrollbar_geometry(bar_box) else {
        return 0;
    };
    if lx < g.track_x - SCROLLBAR_SLOP
        || lx > g.track_x + g.track_w + SCROLLBAR_SLOP
        || ly < g.track_y
        || ly > g.track_y + g.track_h
    {
        return 0;
    }
    let box_dom = ffi::box_dom(bar_box);
    let grab;
    if ly >= g.thumb_y && ly <= g.thumb_y + g.thumb_h {
        grab = ly - g.thumb_y;
    } else {
        grab = g.thumb_h / 2.0;
        let target = if g.track_h > g.thumb_h {
            (ly - g.track_y - grab) / (g.track_h - g.thumb_h) * bar_box.scroll_max_y()
        } else {
            0.0
        };
        bar_box.set_scroll(
            bar_box.scroll_x(),
            clamp_scroll(target, bar_box.scroll_max_y()),
        );
        if let (Some(dom), Some(js)) = (box_dom, b.js()) {
            js.dispatch_scroll(dom);
            let found = b.layout().and_then(|l| find_scrollable_by_dom(l, dom));
            b.set_sb_box(found);
            b.sb_dragging.set(true);
            b.sb_node.set(box_dom);
            b.sb_grab.set(grab);
            return 1;
        }
    }
    b.sb_dragging.set(true);
    b.set_sb_box(Some(bar_box));
    b.sb_node.set(box_dom);
    b.sb_grab.set(grab);
    1
}

pub fn scrollbar_drag(b: &NsBrowser, y: c_int) -> c_int {
    if !b.sb_dragging.get() {
        return 0;
    }
    let mut bar_box = b.sb_box();
    if let Some(node) = b.sb_node.get() {
        bar_box = b.layout().and_then(|l| find_scrollable_by_dom(l, node));
    }
    let Some(bar_box) = bar_box else {
        b.sb_dragging.set(false);
        b.set_sb_box(None);
        return 0;
    };
    b.set_sb_box(Some(bar_box));
    let Some(g) = scrollbar_geometry(bar_box).filter(|g| g.track_h > g.thumb_h) else {
        return 0;
    };
    let target = (f64::from(y) - g.track_y - b.sb_grab.get()) / (g.track_h - g.thumb_h)
        * bar_box.scroll_max_y();
    let target = clamp_scroll(target, bar_box.scroll_max_y());
    if target == bar_box.scroll_y() {
        return 0;
    }
    bar_box.set_scroll(bar_box.scroll_x(), target);
    if let (Some(dom), Some(js)) = (ffi::box_dom(bar_box), b.js()) {
        js.dispatch_scroll(dom);
    }
    1
}

pub fn scrollbar_release(b: &NsBrowser) {
    b.sb_dragging.set(false);
    b.set_sb_box(None);
    b.sb_node.set(None);
}

pub fn drop_files(b: &NsBrowser, x: c_int, y: c_int, paths: &[&CStr]) -> c_int {
    let (Some(js), Some(layout)) = (b.js(), b.layout()) else {
        return 0;
    };
    let hit = ffi::hit_test(layout, f64::from(x), f64::from(y));
    let target = hit
        .and_then(ffi::box_dom)
        .or_else(|| {
            b.doc()
                .and_then(|doc| ffi::find_first_element(doc, c"body"))
        })
        .or_else(|| b.doc());
    let Some(target) = target else {
        return 0;
    };
    if !js.drop_files(target, paths, x, y) {
        return 0;
    }
    if js.consume_mutated() {
        relayout_now(b);
        return 1;
    }
    0
}

pub fn eval(b: &NsBrowser, src: &CStr) -> *mut core::ffi::c_char {
    let Some(js) = b.js() else {
        return core::ptr::null_mut();
    };
    page::damp_reset(b);
    let res = js.eval_console(src);
    if js.run_animation_frame() {
        b.dirty.set(true);
    }
    if js.consume_mutated() {
        b.dirty.set(true);
    }
    relayout_if_dirty(b);
    res
}

fn note_mutation(b: &NsBrowser) {
    if b.js().is_some_and(ffi::Js::consume_mutated) {
        b.dirty.set(true);
    }
}

pub fn contextmenu(b: &NsBrowser, x: c_int, y: c_int) -> (c_int, c_int) {
    let Some(js) = b.js() else {
        return (0, 0);
    };
    let Some(node) = hit::hit_node(b, x, y) else {
        return (0, 0);
    };
    let p = viewport_pointer(b, x, y, 2, 0, 0);
    let prevented = js.dispatch_mouse(node, c"contextmenu", &p, None, true);
    note_mutation(b);
    relayout_if_dirty(b);
    let edit_state = if prevented {
        0
    } else {
        edit::context_field(b, x, y)
    };
    note_mutation(b);
    if js.run_animation_frame() {
        b.dirty.set(true);
    }
    note_mutation(b);
    relayout_if_dirty(b);
    (c_int::from(prevented), edit_state)
}

pub fn press(b: &NsBrowser, x: c_int, y: c_int, mods: c_int) -> Option<GStr> {
    ffi::note_pointer_input(b.js(), true);
    page::damp_reset(b);
    b.pending_nav.clear();
    let extending = mods & 1 != 0 && b.selection_has_range();
    if !extending {
        b.selection_clear();
        callbacks::sync_js_selection(b);
    }
    b.selection_dragged.set(false);
    let node = hit::hit_node(b, x, y);
    b.press_node.set(node);
    b.press_x.set(x);
    b.press_y.set(y);
    b.press_mods.set(mods);
    b.press_active.set(node.is_some());
    ffi::set_active_node(node);
    if node.is_some() && ffi::page_uses_active() {
        relayout_now(b);
    }
    if let (Some(js), Some(node)) = (b.js(), node) {
        let p = viewport_pointer(b, x, y, 0, 1, mods);
        js.dispatch_mouse(node, c"pointerdown", &p, None, false);
        js.dispatch_mouse(node, c"mousedown", &p, None, false);
        note_mutation(b);
    }
    let in_datalist =
        node.is_some_and(|n| ancestors_and_self(n).any(|a| ffi::is_named(a, c"datalist")));
    if let Some(js) = b.js().filter(|_| !in_datalist) {
        let focus = node.and_then(|n| ancestors_and_self(n).find(|&a| ffi::is_focusable(a)));
        js.focus_from_pointer(node);
        let value_len = focus
            .and_then(ffi::editable_value)
            .map_or(0, |v| v.to_bytes().len());
        b.caret_byte.set(value_len);
        b.sel_anchor_byte.set(value_len);
        b.datalist_suppressed
            .set(!focus.is_some_and(|f| ffi::is_named(f, c"input") && f.attr(c"list").is_some()));
        note_mutation(b);
    }
    if let Some(js) = b.js() {
        if js.run_animation_frame() {
            b.dirty.set(true);
        }
        note_mutation(b);
    }
    relayout_if_dirty(b);
    b.pending_nav.take()
}

fn is_dropdown_select(n: Node<'_>) -> bool {
    ffi::is_named(n, c"select")
        && n.attr(c"multiple").is_none()
        && n.attr(c"size").is_none_or(|sz| ffi::parse_int(sz) <= 1)
}

fn datalist_click(b: &NsBrowser, node: Node<'_>) -> bool {
    let mut option = None;
    let mut datalist = None;
    for a in ancestors_and_self(node) {
        if option.is_none() && ffi::is_named(a, c"option") {
            option = Some(a);
        }
        if ffi::is_named(a, c"datalist") {
            datalist = Some(a);
            break;
        }
        if ffi::is_named(a, c"select") {
            return false;
        }
    }
    let (Some(option), Some(_), Some(js)) = (option, datalist, b.js()) else {
        return false;
    };
    let Some(input) = js.focused_node().filter(|&i| ffi::is_named(i, c"input")) else {
        return false;
    };
    let value = option
        .attr(c"value")
        .filter(|v| !v.is_empty())
        .and_then(ffi::dup_text)
        .or_else(|| ffi::option_label(option));
    let cur_len = ffi::editable_value(input).map_or(0, |v| v.to_bytes().len());
    edit::input_replace(
        b,
        input,
        0,
        cur_len,
        Some(value.as_deref().unwrap_or(c"")),
        c"insertReplacementText",
    );
    js.commit_change(input);
    js.consume_mutated();
    b.datalist_suppressed.set(true);
    b.dirty.set(true);
    true
}

fn dropdown_click(b: &NsBrowser, node: Node<'_>) -> bool {
    if datalist_click(b, node) {
        return true;
    }
    let mut option = None;
    let mut select = None;
    for a in ancestors_and_self(node) {
        if option.is_none() && ffi::is_named(a, c"option") {
            option = Some(a);
        }
        if ffi::is_named(a, c"select") {
            select = Some(a);
            break;
        }
    }
    if let (Some(sel), Some(opt)) = (select, option) {
        if !is_dropdown_select(sel) && sel.attr(c"disabled").is_none() {
            let toggle = sel.attr(c"multiple").is_some() && b.press_mods.get() & 2 != 0;
            if let Some(js) = b.js() {
                if toggle {
                    js.select_toggle_option(opt);
                } else {
                    js.select_choose_option(opt);
                }
                js.consume_mutated();
            }
            b.dirty.set(true);
            return true;
        }
    }
    if let (Some(open), Some(opt)) = (b.open_select.get(), option) {
        if same(select, Some(open)) {
            if let Some(js) = b.js() {
                if js.select_choose_option(opt) {
                    js.consume_mutated();
                    b.open_select.set(None);
                }
            }
            b.dirty.set(true);
            return true;
        }
    }
    if let Some(sel) = select.filter(|&s| is_dropdown_select(s) && s.attr(c"disabled").is_none()) {
        b.open_select.set(if same(b.open_select.get(), Some(sel)) {
            None
        } else {
            Some(sel)
        });
        b.dirty.set(true);
        return true;
    }
    if b.open_select.get().is_some() {
        b.open_select.set(None);
        b.dirty.set(true);
    }
    false
}

fn follow_link(b: &NsBrowser, node: Option<Node<'_>>, x: c_int, y: c_int) {
    let mut href = None;
    let mut download = None;
    let mut cur = node;
    while let Some(a) = cur {
        if href.is_some() {
            break;
        }
        if hit::is_hyperlink(a) {
            if let Some(h) = a.attr(c"href").filter(|h| !h.is_empty()) {
                href = Some(h);
                download = a.attr(c"download");
            }
        }
        cur = a.parent();
    }
    if href.is_none() {
        href = b
            .layout()
            .and_then(|l| ffi::hit_link(l, f64::from(x), f64::from(y)));
    }
    let Some(href) = href.filter(|h| !h.is_empty()) else {
        return;
    };
    match download {
        Some(filename) => callbacks::js_download(b, href, Some(filename)),
        None => b.pending_nav.adopt(callbacks::resolve_navigation(b, href)),
    }
}

fn activate(b: &NsBrowser, node: Node<'_>, x: c_int, y: c_int) {
    let js = b.js();
    if js.is_some_and(|js| js.click_activate(node)) {
        b.dirty.set(true);
    }
    if js.is_some_and(|js| js.activate_summary(node)) {
        note_mutation(b);
    } else if !b.pending_nav.is_set() && ffi::form_is_submit_trigger(node) {
        build::submit_form(b, Some(node));
    } else if let (Some(js), Some(doc), true) = (js, b.doc(), ffi::form_is_reset_trigger(node)) {
        if let Some(form) = ffi::form_owner(node, doc) {
            js.form_reset(form);
            note_mutation(b);
        }
    } else if !b.pending_nav.is_set() {
        follow_link(b, Some(node), x, y);
    }
}

pub fn release_click(b: &NsBrowser) -> (Option<GStr>, bool) {
    page::damp_reset(b);
    b.pending_nav.clear();
    page::prune_cached_nodes(b);
    let node = if b.press_active.get() {
        b.press_node.get()
    } else {
        None
    };
    let (x, y, mods) = (b.press_x.get(), b.press_y.get(), b.press_mods.get());
    b.press_node.set(None);
    b.press_active.set(false);
    let drag_selected = b.selection_dragged.get() && b.selection_has_range();
    b.selection_dragged.set(false);
    let mut prevented = false;
    if let (Some(js), Some(node)) = (b.js(), node) {
        let p = viewport_pointer(b, x, y, 0, 0, mods);
        js.dispatch_mouse(node, c"pointerup", &p, None, false);
        js.dispatch_mouse(node, c"mouseup", &p, None, false);
        if !drag_selected {
            prevented = js.dispatch_mouse(node, c"click", &p, None, true);
        }
        note_mutation(b);
    }
    if drag_selected {
        prevented = true;
    }
    if !prevented {
        match node {
            Some(node) => {
                if !dropdown_click(b, node) {
                    activate(b, node, x, y);
                }
            }
            None => {
                if !b.pending_nav.is_set() {
                    follow_link(b, None, x, y);
                }
            }
        }
    }
    if ffi::set_active_node(None) && ffi::page_uses_active() {
        b.dirty.set(true);
    }
    if let Some(js) = b.js() {
        if js.run_animation_frame() {
            b.dirty.set(true);
        }
        note_mutation(b);
    }
    let changed = relayout_if_dirty(b);
    (b.pending_nav.take(), changed)
}

fn accesskey_matches(el: Node<'_>, key: &CStr) -> bool {
    let Some(ak) = el.attr(c"accesskey").filter(|a| !a.is_empty()) else {
        return false;
    };
    let key = key.to_bytes();
    if key.is_empty() {
        return false;
    }
    ak.to_bytes()
        .split(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r'))
        .any(|token| token.len() == key.len() && token.eq_ignore_ascii_case(key))
}

fn find_accesskey<'a>(n: Node<'a>, key: &CStr, depth: c_int) -> Option<Node<'a>> {
    if depth > MAX_DEPTH {
        return None;
    }
    let element = n.kind() == Kind::Element;
    if element
        && n.name()
            .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"template"))
    {
        return None;
    }
    if element && accesskey_matches(n, key) && !ffi::effectively_inert(n) {
        return Some(n);
    }
    children(n).find_map(|c| find_accesskey(c, key, depth + 1))
}

fn select_key(b: &NsBrowser, select: Node<'_>, key: Option<&CStr>, mods: c_int) -> bool {
    let (Some(key), Some(js)) = (key.filter(|k| !k.is_empty()), b.js()) else {
        return false;
    };
    let dropdown = is_dropdown_select(select);
    match key.to_bytes() {
        b"ArrowDown" => return js.select_step(select, 1),
        b"ArrowUp" => return js.select_step(select, -1),
        b"Home" => return js.select_edge(select, false),
        b"End" => return js.select_edge(select, true),
        b"Enter" | b" " if dropdown => {
            b.open_select
                .set(if same(b.open_select.get(), Some(select)) {
                    None
                } else {
                    Some(select)
                });
            return true;
        }
        b"Escape" if dropdown && same(b.open_select.get(), Some(select)) => {
            b.open_select.set(None);
            return true;
        }
        _ => {}
    }
    if mods & (2 | 4 | 8) == 0
        && ffi::utf8_validate(key)
        && ffi::utf8_strlen(key) == 1
        && key.to_bytes()[0] != b' '
        && ffi::unichar_isprint(ffi::utf8_get_char(key))
    {
        return js.select_typeahead(select, key);
    }
    false
}

pub struct KeyEvent<'a> {
    pub kind: c_int,
    pub key: Option<&'a CStr>,
    pub code: Option<&'a CStr>,
    pub keycode: c_int,
    pub mods: c_int,
}

fn insert_text(b: &NsBrowser, key: Option<&CStr>) {
    let Some(f) = b.js().and_then(|js| js.focused_node()) else {
        return;
    };
    let (Some(cur), Some(key)) = (
        ffi::editable_value(f),
        key.filter(|k| !k.is_empty() && ffi::utf8_validate(k)),
    ) else {
        return;
    };
    let cur_len = cur.to_bytes().len();
    if b.caret_byte.get() > cur_len {
        b.caret_byte.set(cur_len);
    }
    if b.sel_anchor_byte.get() > cur_len {
        b.sel_anchor_byte.set(cur_len);
    }
    let (caret, anchor) = (b.caret_byte.get(), b.sel_anchor_byte.get());
    edit::input_replace(
        b,
        f,
        anchor.min(caret),
        anchor.max(caret),
        Some(key),
        c"insertText",
    );
    b.datalist_suppressed.set(false);
    b.dirty.set(true);
}

fn keydown_default(b: &NsBrowser, ev: &KeyEvent<'_>, prevented_out: &mut bool) {
    let Some(js) = b.js() else {
        return;
    };
    let mods = ev.mods;
    let plain = mods & (2 | 4 | 8) == 0;
    let key = ev.key;
    let key_is = |name: &[u8]| key.is_some_and(|k| k.to_bytes() == name);
    if key_is(b"Tab") {
        if let Some(next) = js.sequential_focus_target(mods & 1 != 0) {
            js.set_focus(next);
            let len = ffi::editable_value(next).map_or(0, |v| v.to_bytes().len());
            b.caret_byte.set(len);
            b.sel_anchor_byte.set(len);
            b.dirty.set(true);
        }
        return;
    }
    let f = js.focused_node();
    if let Some(f) = f {
        if ffi::is_named(f, c"input")
            && !b.datalist_suppressed.get()
            && key_is(b"Escape")
            && f.attr(c"list").is_some()
        {
            b.datalist_suppressed.set(true);
            b.dirty.set(true);
            *prevented_out = true;
            return;
        }
        if ffi::is_named(f, c"select")
            && f.attr(c"disabled").is_none()
            && select_key(b, f, key, mods)
        {
            b.dirty.set(true);
            *prevented_out = true;
            return;
        }
        if plain && ffi::keyboard_activates(f, key) {
            *prevented_out = true;
            return;
        }
        if plain && js.keyboard_activate(f, key, false) {
            b.dirty.set(true);
            *prevented_out = true;
            return;
        }
        if ffi::editable_value(f).is_some() && edit::edit_key(b, f, key, mods) {
            b.dirty.set(true);
            return;
        }
    }
    if key_is(b"Escape") && js.process_close_request() {
        b.dirty.set(true);
        *prevented_out = true;
    }
}

pub fn key(b: &NsBrowser, ev: &KeyEvent<'_>) -> (Option<GStr>, bool) {
    let mut prevented_out = false;
    let Some(js) = b.js() else {
        return (None, false);
    };
    ffi::note_pointer_input(Some(js), false);
    page::damp_reset(b);
    b.pending_nav.clear();
    let target = js.focused_node().or_else(|| {
        b.doc()
            .and_then(|doc| ffi::find_first_element(doc, c"body"))
    });
    let Some(target) = target else {
        return (None, false);
    };
    let key = ev.key;
    let mods = ev.mods;
    match ev.kind {
        2 => insert_text(b, key),
        4 => {
            if let Some(text) = key.filter(|k| !k.is_empty() && ffi::utf8_validate(k)) {
                edit::paste(b, target, text);
            }
        }
        3 => {
            let char_code = edit::key_char_code(key);
            if !b.keydown_prevented.get() && char_code > 0 && mods & (2 | 4 | 8) == 0 {
                let pressed = js.dispatch_key(
                    target,
                    c"keypress",
                    key.unwrap_or(c""),
                    ev.code.unwrap_or(c""),
                    (ev.keycode, char_code),
                    mods & 1,
                );
                if pressed {
                    prevented_out = true;
                }
                note_mutation(b);
            }
            b.keydown_prevented.set(false);
        }
        kind => {
            let kind_name = if kind == 1 { c"keyup" } else { c"keydown" };
            let prevented = js.dispatch_key(
                target,
                kind_name,
                key.unwrap_or(c""),
                ev.code.unwrap_or(c""),
                (ev.keycode, 0),
                mods,
            );
            if prevented {
                prevented_out = true;
            }
            b.keydown_prevented.set(kind == 0 && prevented);
            note_mutation(b);
            if kind == 0 && !prevented && mods & 4 != 0 && mods & (2 | 8) == 0 {
                if let (Some(k), Some(doc)) = (
                    key.filter(|k| ffi::utf8_validate(k) && ffi::utf8_strlen(k) == 1),
                    b.doc(),
                ) {
                    if let Some(ak) = find_accesskey(doc, k, 0) {
                        js.set_focus(ak);
                        js.activate_element(ak);
                        note_mutation(b);
                        prevented_out = true;
                    }
                }
            }
            if !prevented && kind == 0 {
                keydown_default(b, ev, &mut prevented_out);
            } else if !prevented && kind == 1 {
                if let Some(f) = js.focused_node() {
                    if mods & (2 | 4 | 8) == 0 && js.keyboard_activate(f, key, true) {
                        b.dirty.set(true);
                    }
                }
            }
        }
    }
    if js.run_animation_frame() {
        b.dirty.set(true);
    }
    note_mutation(b);
    relayout_if_dirty(b);
    (b.pending_nav.take(), prevented_out)
}
