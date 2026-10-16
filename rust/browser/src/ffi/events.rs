//! Southstar — the calls behind a page's input: hit testing, the selection, editable fields, UTF-8 stepping, select controls and the script engine's pointer, key, drag and editing events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GStr};
use southstar_layout::BoxRef;

use super::{Js, NsBrowser};

unsafe extern "C" {
    fn ns_js_dispatch_key_event_full(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        key: *const c_char,
        code: *const c_char,
        key_code: c_int,
        char_code: c_int,
        shift: GBoolean,
        ctrl: GBoolean,
        alt: GBoolean,
        meta: GBoolean,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_input_event(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        input_type: *const c_char,
        data: *const c_char,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_note_user_edit(js: *mut c_void, el: *const NsNode, value_before: *const c_char);
    fn ns_js_commit_change(js: *mut c_void, el: *const NsNode);
    fn ns_js_activate_summary(js: *mut c_void, el: *const NsNode) -> GBoolean;
    fn ns_js_process_close_request(js: *mut c_void) -> GBoolean;
    fn ns_js_node_has_click_handler(js: *mut c_void, target: *const NsNode) -> GBoolean;
    fn ns_option_label_dup(option: *const NsNode) -> *mut c_char;
    fn ns_element_effectively_inert(el: *const NsNode) -> GBoolean;
    fn ns_form_is_reset_trigger(n: *const NsNode) -> GBoolean;
    fn ns_render_page_uses_hover() -> GBoolean;
    fn ns_render_page_uses_active() -> GBoolean;
    fn g_utf8_prev_char(p: *const c_char) -> *const c_char;
    fn g_utf8_strlen(p: *const c_char, max: isize) -> c_long;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_utf8_validate(s: *const c_char, max_len: isize, end: *mut *const c_char) -> GBoolean;
    fn g_utf8_offset_to_pointer(s: *const c_char, offset: c_long) -> *const c_char;
    fn g_unichar_iscntrl(c: u32) -> GBoolean;
    fn g_unichar_isprint(c: u32) -> GBoolean;
    fn strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long;
    fn atoi(s: *const c_char) -> c_int;
}

fn opt(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

fn flag(mods: c_int, bit: c_int) -> GBoolean {
    glib::boolean(mods & bit != 0)
}

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
        // SAFETY: a Js handle names the page's live script engine.
        let (_, prevented) = unsafe {
            southstar_js_input::dispatch_mouse_event(self.raw(), target, kind, &input, related)
        };
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
        let mut prevented: GBoolean = 0;
        unsafe {
            ns_js_dispatch_key_event_full(
                self.raw(),
                target.as_ptr(),
                kind.as_ptr(),
                key.as_ptr(),
                code.as_ptr(),
                codes.0,
                codes.1,
                flag(mods, 1),
                flag(mods, 2),
                flag(mods, 4),
                flag(mods, 8),
                &mut prevented,
            )
        };
        prevented != 0
    }

    pub fn dispatch_input(
        self,
        target: Node<'_>,
        kind: &CStr,
        input_type: &CStr,
        data: Option<&CStr>,
        want_prevented: bool,
    ) -> bool {
        let mut prevented: GBoolean = 0;
        unsafe {
            ns_js_dispatch_input_event(
                self.raw(),
                target.as_ptr(),
                kind.as_ptr(),
                input_type.as_ptr(),
                opt(data),
                if want_prevented {
                    &mut prevented
                } else {
                    ptr::null_mut()
                },
            )
        };
        prevented != 0
    }

    pub fn dispatch_clipboard(self, target: Node<'_>, kind: &CStr, text: &CStr) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        let (_, prevented) = unsafe {
            southstar_js_input::dispatch_clipboard_event(self.raw(), target, kind, Some(text))
        };
        prevented
    }

    pub fn dispatch_scroll(self, target: Node<'_>) {
        self.dispatch_event(target, c"scroll");
    }

    pub fn drop_files(self, target: Node<'_>, paths: &[&CStr], x: c_int, y: c_int) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        let Some(session) = (unsafe { southstar_js_input::DragData::new(self.raw()) }) else {
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
            // SAFETY: a Js handle names the page's live script engine.
            let (_, prevented) = unsafe {
                southstar_js_input::dispatch_drag_event(
                    self.raw(),
                    Some(&session),
                    target,
                    kind,
                    &input,
                    None,
                )
            };
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
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_focus::set_focus(self.raw(), Some(el)) };
    }

    pub fn focus_from_pointer(self, target: Option<Node<'_>>) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_focus::focus_from_pointer(self.raw(), target) };
    }

    pub fn note_user_edit(self, el: Node<'_>, value_before: Option<&CStr>) {
        unsafe { ns_js_note_user_edit(self.raw(), el.as_ptr(), opt(value_before)) };
    }

    pub fn commit_change(self, el: Node<'_>) {
        unsafe { ns_js_commit_change(self.raw(), el.as_ptr()) };
    }

    pub fn click_activate(self, node: Node<'_>) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_forms::click_activate(self.raw(), node) }
    }

    pub fn activate_summary(self, el: Node<'_>) -> bool {
        unsafe { ns_js_activate_summary(self.raw(), el.as_ptr()) != 0 }
    }

    pub fn form_reset(self, form: Node<'_>) {
        southstar_js_runtime::api::form_reset(self.page(), form);
    }

    pub fn select_toggle_option(self, option: Node<'_>) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_forms::select_toggle_option(self.raw(), option) }
    }

    pub fn select_choose_option(self, option: Node<'_>) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_forms::select_choose_option(self.raw(), option) }
    }

    pub fn select_step(self, select: Node<'_>, dir: c_int) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_forms::select_step(self.raw(), select, dir) }
    }

    pub fn select_edge(self, select: Node<'_>, last: bool) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_forms::select_edge(self.raw(), select, last) }
    }

    pub fn select_typeahead(self, select: Node<'_>, key: &CStr) -> bool {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_forms::select_typeahead(self.raw(), select, key.to_bytes()) }
    }

    pub fn keyboard_activate(self, el: Node<'_>, key: Option<&CStr>, keyup: bool) -> bool {
        key.is_some_and(|key| {
            // SAFETY: a Js handle names the page's live script engine.
            unsafe { southstar_js_focus::keyboard_activate(self.raw(), el, key.to_bytes(), keyup) }
        })
    }

    pub fn process_close_request(self) -> bool {
        unsafe { ns_js_process_close_request(self.raw()) != 0 }
    }

    pub fn sequential_focus_target<'a>(self, backward: bool) -> Option<Node<'a>> {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_focus::sequential_focus_target(self.raw(), backward) }
    }

    pub fn activate_element(self, el: Node<'_>) {
        // SAFETY: a Js handle names the page's live script engine.
        unsafe { southstar_js_focus::activate_element(self.raw(), el) };
    }

    pub fn node_has_click_handler(self, node: Node<'_>) -> bool {
        unsafe { ns_js_node_has_click_handler(self.raw(), node.as_ptr()) != 0 }
    }

    pub fn eval_console(self, src: &CStr) -> Option<GStr> {
        self.eval(src, c"devtools-console")
    }
}

pub fn note_pointer_input(js: Option<Js>, pointer: bool) {
    // SAFETY: the page passes null or its live script engine.
    unsafe { southstar_js_focus::note_pointer_input(js.map_or(ptr::null_mut(), Js::raw), pointer) };
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

pub fn option_label(option: Node<'_>) -> Option<GStr> {
    unsafe { GStr::take(ns_option_label_dup(option.as_ptr())) }
}

pub fn effectively_inert(el: Node<'_>) -> bool {
    unsafe { ns_element_effectively_inert(el.as_ptr()) != 0 }
}

pub fn form_is_reset_trigger(n: Node<'_>) -> bool {
    unsafe { ns_form_is_reset_trigger(n.as_ptr()) != 0 }
}

pub fn page_uses_hover() -> bool {
    unsafe { ns_render_page_uses_hover() != 0 }
}

pub fn page_uses_active() -> bool {
    unsafe { ns_render_page_uses_active() != 0 }
}

pub fn set_active_node(node: Option<Node<'_>>) -> bool {
    southstar_css::set_active_node(node).is_some()
}

pub fn video_url_is_inline(url: &CStr) -> bool {
    southstar_video::url_is_inline_video(url.to_bytes())
}

pub use southstar_video::BoxVideo as VideoRef;

pub fn utf8_prev(s: &CStr, at: usize) -> usize {
    let base = s.as_ptr();
    let prev = unsafe { g_utf8_prev_char(base.add(at)) };
    (prev as usize).wrapping_sub(base as usize)
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

pub fn utf8_strlen(s: &CStr) -> c_long {
    unsafe { g_utf8_strlen(s.as_ptr(), -1) }
}

pub fn utf8_strlen_bytes(s: &CStr, max: usize) -> c_long {
    unsafe { g_utf8_strlen(s.as_ptr(), max as isize) }
}

pub fn utf8_strlen_from(s: &CStr, at: usize) -> c_long {
    unsafe { g_utf8_strlen(s.as_ptr().add(at), -1) }
}

pub fn utf8_get_char(s: &CStr) -> u32 {
    unsafe { g_utf8_get_char(s.as_ptr()) }
}

pub fn utf8_validate(s: &CStr) -> bool {
    unsafe { g_utf8_validate(s.as_ptr(), -1, ptr::null_mut()) != 0 }
}

pub fn utf8_offset(s: &CStr, chars: c_long) -> usize {
    let p = unsafe { g_utf8_offset_to_pointer(s.as_ptr(), chars) };
    (p as usize).wrapping_sub(s.as_ptr() as usize)
}

pub fn unichar_iscntrl(c: u32) -> bool {
    unsafe { g_unichar_iscntrl(c) != 0 }
}

pub fn unichar_isprint(c: u32) -> bool {
    unsafe { g_unichar_isprint(c) != 0 }
}

pub fn parse_long(s: &CStr) -> Option<c_long> {
    let mut end: *mut c_char = ptr::null_mut();
    let v = unsafe { strtol(s.as_ptr(), &mut end, 10) };
    (end.cast_const() != s.as_ptr()).then_some(v)
}

pub fn parse_int(s: &CStr) -> c_int {
    unsafe { atoi(s.as_ptr()) }
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
        unsafe { BoxRef::from_ptr(self.sb_box.get()) }
    }

    pub fn set_sb_box(&self, b: Option<BoxRef<'_>>) {
        self.sb_box
            .set(b.map_or(ptr::null_mut(), |b| b.as_ptr().cast_mut()));
    }

    pub fn search_active_y(&self) -> Option<f64> {
        unsafe { BoxRef::from_ptr(self.search_active.0.get()) }.map(BoxRef::y)
    }
}
