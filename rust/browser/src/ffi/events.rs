//! Southstar — the calls behind a page's input: hit testing, the selection, editable fields, UTF-8 stepping, select controls and the script engine's pointer, key, drag and editing events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long, c_uint};
use core::ptr;

use southstar_dom::Node;
use southstar_layout::BoxRef;
use std::ffi::CString;

use super::{Js, NsBrowser};

fn modifiers(mods: c_int) -> southstar_js_input::Modifiers {
    southstar_js_input::Modifiers {
        shift: mods & 1 != 0,
        ctrl: mods & 2 != 0,
        alt: mods & 4 != 0,
        meta: mods & 8 != 0,
    }
}

pub struct Pointer {
    pub client: (f64, f64),
    pub page: (f64, f64),
    pub button: c_int,
    pub buttons: c_int,
    pub mods: c_int,
}

impl Js {
    pub fn dispatch_mouse(
        self,
        target: Node<'_>,
        kind: &CStr,
        p: &Pointer,
        related: Option<Node<'_>>,
        want_prevented: bool,
    ) -> bool {
        let input = southstar_js_input::Pointer {
            client: p.client,
            page: p.page,
            button: p.button,
            buttons: p.buttons,
            modifiers: modifiers(p.mods),
        };
        let (_, prevented) =
            southstar_js_input::dispatch_mouse_event(self.handle(), target, kind, &input, related);
        want_prevented && prevented
    }

    pub fn dispatch_key(
        self,
        target: Node<'_>,
        kind: &CStr,
        key: &CStr,
        code: &CStr,
        codes: (c_int, c_int),
        mods: c_int,
    ) -> bool {
        let key = key.to_string_lossy();
        let code = code.to_string_lossy();
        let press = southstar_js_dispatch::KeyPress {
            key: Some(&key),
            code: Some(&code),
            key_code: codes.0,
            char_code: codes.1,
            modifiers: southstar_js_dispatch::Modifiers {
                shift: mods & 1 != 0,
                ctrl: mods & 2 != 0,
                alt: mods & 4 != 0,
                meta: mods & 8 != 0,
            },
        };
        let (_, prevented) = southstar_js_dispatch::dispatch_key_event(
            self.page(),
            target,
            &kind.to_string_lossy(),
            &press,
        );
        prevented
    }

    pub fn dispatch_input(
        self,
        target: Node<'_>,
        kind: &CStr,
        input_type: &CStr,
        data: Option<&CStr>,
        want_prevented: bool,
    ) -> bool {
        let data = data.map(CStr::to_string_lossy);
        let (_, prevented) = southstar_js_dispatch::dispatch_input_event(
            self.page(),
            target,
            &kind.to_string_lossy(),
            Some(&input_type.to_string_lossy()),
            data.as_deref(),
        );
        want_prevented && prevented
    }

    pub fn dispatch_clipboard(self, target: Node<'_>, kind: &CStr, text: &CStr) -> bool {
        let (_, prevented) =
            southstar_js_input::dispatch_clipboard_event(self.handle(), target, kind, Some(text));
        prevented
    }

    pub fn dispatch_scroll(self, target: Node<'_>) {
        self.dispatch_event(target, c"scroll");
    }

    pub fn drop_files(self, target: Node<'_>, paths: &[&CStr], x: c_int, y: c_int) -> bool {
        let Some(session) = southstar_js_input::DragData::new(self.handle()) else {
            return false;
        };
        for path in paths {
            session.add_file(path);
        }
        let (fx, fy) = (f64::from(x), f64::from(y));
        let input = southstar_js_input::Pointer {
            client: (fx, fy),
            page: (fx, fy),
            button: 0,
            buttons: 0,
            modifiers: southstar_js_input::Modifiers::default(),
        };
        let dispatch = |kind: &CStr| {
            let (_, prevented) = southstar_js_input::dispatch_drag_event(
                self.handle(),
                Some(&session),
                target,
                kind,
                &input,
                None,
            );
            prevented
        };
        let mut accept = dispatch(c"dragenter");
        accept |= dispatch(c"dragover");
        if accept {
            dispatch(c"drop");
        } else {
            dispatch(c"dragleave");
        }
        drop(session);
        true
    }

    pub fn set_focus(self, el: Node<'_>) {
        southstar_js_focus::set_focus(self.handle(), Some(el));
    }

    pub fn focus_from_pointer(self, target: Option<Node<'_>>) {
        southstar_js_focus::focus_from_pointer(self.handle(), target);
    }

    pub fn note_user_edit(self, el: Node<'_>, value_before: Option<&CStr>) {
        southstar_js_mutation::note_user_edit(self.handle(), el, value_before.unwrap_or_default());
    }

    pub fn commit_change(self, el: Node<'_>) {
        southstar_js_mutation::commit_change(self.handle(), el);
    }

    pub fn click_activate(self, node: Node<'_>) -> bool {
        southstar_js_forms::click_activate(self.handle(), node)
    }

    pub fn activate_summary(self, el: Node<'_>) -> bool {
        southstar_js_top_layer::activate_summary(self.page(), el)
    }

    pub fn form_reset(self, form: Node<'_>) {
        southstar_js_runtime::api::form_reset(self.page(), form);
    }

    pub fn select_toggle_option(self, option: Node<'_>) -> bool {
        southstar_js_forms::select_toggle_option(self.handle(), option)
    }

    pub fn select_choose_option(self, option: Node<'_>) -> bool {
        southstar_js_forms::select_choose_option(self.handle(), option)
    }

    pub fn select_step(self, select: Node<'_>, dir: c_int) -> bool {
        southstar_js_forms::select_step(self.handle(), select, dir)
    }

    pub fn select_edge(self, select: Node<'_>, last: bool) -> bool {
        southstar_js_forms::select_edge(self.handle(), select, last)
    }

    pub fn select_typeahead(self, select: Node<'_>, key: &CStr) -> bool {
        southstar_js_forms::select_typeahead(self.handle(), select, key.to_bytes())
    }

    pub fn keyboard_activate(self, el: Node<'_>, key: Option<&CStr>, keyup: bool) -> bool {
        key.is_some_and(|key| {
            southstar_js_focus::keyboard_activate(self.handle(), el, key.to_bytes(), keyup)
        })
    }

    pub fn process_close_request(self) -> bool {
        southstar_js_top_layer::process_close_request(self.page())
    }

    pub fn sequential_focus_target<'a>(self, backward: bool) -> Option<Node<'a>> {
        southstar_js_focus::sequential_focus_target(self.handle(), backward)
    }

    pub fn activate_element(self, el: Node<'_>) {
        southstar_js_focus::activate_element(self.handle(), el);
    }

    pub fn node_has_click_handler(self, node: Node<'_>) -> bool {
        southstar_js_dispatch::node_has_click_handler(self.page(), node)
    }

    pub fn eval_console(self, src: &CStr) -> Option<CString> {
        self.eval(src, c"devtools-console")
    }
}

pub fn note_pointer_input(js: Option<Js>, pointer: bool) {
    if let Some(js) = js {
        southstar_js_focus::note_pointer_input(js.handle(), pointer);
    }
}

pub fn keyboard_activates(el: Node<'_>, key: Option<&CStr>) -> bool {
    key.is_some_and(|key| southstar_js_focus::keyboard_activates(el, key.to_bytes()))
}

pub fn hit_link(layout: BoxRef<'_>, x: f64, y: f64) -> Option<&CStr> {
    southstar_layout_hit::hit_link(layout, x, y).and_then(|link| link.href)
}

pub use southstar_layout_hit::{hit_form_dom, hit_node, hit_test};

pub fn hit_scrollable(layout: BoxRef<'_>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    southstar_layout_hit::hit_scrollable(layout, x, y).map(|(b, _, _)| b)
}

pub use southstar_layout_hit::hit_scrollable as hit_scrollbar;

pub fn scroll_snap_from(scroller: BoxRef<'_>, prev_x: f64, prev_y: f64) {
    southstar_layout_query::scroll_snap_from(scroller, prev_x, prev_y);
}

pub fn count_matches(layout: BoxRef<'_>, needle: &CStr, case_sensitive: bool) -> c_uint {
    southstar_layout_query::count_matches(layout, needle.to_bytes(), case_sensitive)
}

pub fn first_match<'a>(
    layout: BoxRef<'a>,
    needle: &CStr,
    y: f64,
    case_sensitive: bool,
    above: bool,
) -> Option<BoxRef<'a>> {
    if above {
        southstar_layout_query::first_match_above(layout, needle.to_bytes(), y, case_sensitive)
    } else {
        southstar_layout_query::first_match_below(layout, needle.to_bytes(), y, case_sensitive)
    }
}

pub fn match_ordinal(
    layout: BoxRef<'_>,
    needle: &CStr,
    target: BoxRef<'_>,
    case_sensitive: bool,
) -> c_uint {
    southstar_layout_query::match_ordinal(layout, needle.to_bytes(), target, case_sensitive)
}

pub fn selection_text_at(layout: BoxRef<'_>, x: f64, y: f64) -> bool {
    southstar_selection::text_at(Some(layout), x, y)
}

pub fn is_contenteditable_host(n: Node<'_>) -> bool {
    southstar_dom::controls::is_contenteditable_host(n)
}

pub fn is_editable(n: Option<Node<'_>>) -> bool {
    n.is_some_and(southstar_dom::controls::is_editable)
}

pub use southstar_js_focus::is_focusable;

pub fn set_editable_value(n: Node<'_>, value: &CStr) {
    southstar_dom::controls::set_editable_value(n, value);
}

pub fn option_label(option: Node<'_>) -> Option<CString> {
    super::owned_text(&southstar_dom::select::option_label(option))
}

pub fn effectively_inert(el: Node<'_>) -> bool {
    southstar_dom::tree::effectively_inert(el)
}

pub use southstar_render::{page_uses_active, page_uses_hover};

pub fn set_active_node(node: Option<Node<'_>>) -> bool {
    southstar_css::set_active_node(node).is_some()
}

pub fn video_url_is_inline(url: &CStr) -> bool {
    southstar_video::url_is_inline_video(url.to_bytes())
}

pub use southstar_video::BoxVideo as VideoRef;

pub fn utf8_prev(s: &CStr, at: usize) -> usize {
    let bytes = s.to_bytes_with_nul();
    let mut i = at;
    while i > 0 {
        i -= 1;
        if bytes.get(i).is_none_or(|&b| b & 0xc0 != 0x80) {
            break;
        }
    }
    i
}

pub fn utf8_next(s: &[u8], at: usize) -> usize {
    let skip = match s.get(at).copied().unwrap_or(0) {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        0xf8..=0xfb => 5,
        0xfc..=0xfd => 6,
        _ => 1,
    };
    at + skip
}

fn byte_at(s: &[u8], at: usize) -> u8 {
    s.get(at).copied().unwrap_or(0)
}

fn strlen_until_nul(s: &[u8], from: usize) -> c_long {
    let mut p = from;
    let mut len = 0;
    while byte_at(s, p) != 0 {
        p = utf8_next(s, p);
        len += 1;
    }
    len
}

pub fn utf8_strlen(s: &CStr) -> c_long {
    strlen_until_nul(s.to_bytes(), 0)
}

pub fn utf8_strlen_bytes(s: &CStr, max: usize) -> c_long {
    let bytes = s.to_bytes();
    if max == 0 || byte_at(bytes, 0) == 0 {
        return 0;
    }
    let mut len = 0;
    let mut p = utf8_next(bytes, 0);
    while p < max && byte_at(bytes, p) != 0 {
        len += 1;
        p = utf8_next(bytes, p);
    }
    if p <= max {
        len += 1;
    }
    len
}

pub fn utf8_strlen_from(s: &CStr, at: usize) -> c_long {
    strlen_until_nul(s.to_bytes(), at)
}

pub fn utf8_get_char(s: &CStr) -> u32 {
    southstar_layout::utf8_get_char(s.to_bytes(), 0)
}

pub fn utf8_validate(s: &CStr) -> bool {
    core::str::from_utf8(s.to_bytes()).is_ok()
}

pub fn utf8_offset(s: &CStr, chars: c_long) -> usize {
    let bytes = s.to_bytes();
    (0..chars.max(0)).fold(0, |p, _| utf8_next(bytes, p))
}

pub fn unichar_iscntrl(c: u32) -> bool {
    southstar_unicode::is_control(c)
}

pub fn unichar_isprint(c: u32) -> bool {
    southstar_unicode::is_print(c)
}

fn strtol(s: &[u8]) -> Option<c_long> {
    let mut p = 0;
    while matches!(byte_at(s, p), b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        p += 1;
    }
    let negative = byte_at(s, p) == b'-';
    if matches!(byte_at(s, p), b'-' | b'+') {
        p += 1;
    }
    let digits = s.get(p..).unwrap_or_default();
    let count = digits.iter().take_while(|b| b.is_ascii_digit()).count();
    if count == 0 {
        return None;
    }
    let mut v: c_long = 0;
    for &d in &digits[..count] {
        let d = c_long::from(d - b'0');
        v = if negative {
            v.saturating_mul(10).saturating_sub(d)
        } else {
            v.saturating_mul(10).saturating_add(d)
        };
    }
    Some(v)
}

pub fn parse_long(s: &CStr) -> Option<c_long> {
    strtol(s.to_bytes())
}

pub fn parse_int(s: &CStr) -> c_int {
    strtol(s.to_bytes()).unwrap_or(0) as c_int
}

impl NsBrowser {
    pub fn selection_anchor_at(&self, layout: BoxRef<'_>, x: f64, y: f64) {
        self.update_selection(|s| s.anchor_at(Some(layout), x, y));
    }

    pub fn selection_extend_to(&self, layout: BoxRef<'_>, x: f64, y: f64) {
        self.update_selection(|s| s.extend_to(Some(layout), x, y));
    }

    pub fn selection_word_at(&self, layout: BoxRef<'_>, x: f64, y: f64) {
        self.update_selection(|s| s.select_word_at(Some(layout), x, y));
    }

    pub fn selection_block_at(&self, layout: BoxRef<'_>, x: f64, y: f64) {
        self.update_selection(|s| s.select_block_at(Some(layout), x, y));
    }

    pub fn sb_box(&self) -> Option<BoxRef<'_>> {
        unsafe { BoxRef::from_ptr(self.sb_box.0.get()) }
    }

    pub fn set_sb_box(&self, b: Option<BoxRef<'_>>) {
        self.sb_box
            .0
            .set(b.map_or(ptr::null_mut(), |b| b.as_ptr().cast_mut()));
    }

    pub fn search_active_y(&self) -> Option<f64> {
        unsafe { BoxRef::from_ptr(self.search_active.0.get()) }.map(BoxRef::y)
    }
}
