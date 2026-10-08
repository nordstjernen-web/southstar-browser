//! Southstar — the calls behind a page's input: hit testing, the selection, editable fields, UTF-8 stepping, select controls and the script engine's pointer, key, drag and editing events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GStr};
use southstar_layout::{BoxRef, NsBox};

use super::{Js, NsBrowser, Videos};

#[repr(C)]
struct NsVideoHead {
    _url: *mut c_char,
    _natural: [c_int; 2],
    _poster_texture: *mut c_void,
    _poster_url: *mut c_char,
    _frame_texture: *mut c_void,
    _flags: [GBoolean; 3],
    player: *mut c_void,
    _dom_node: *const c_void,
    is_camera: GBoolean,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::offset_of!(NsVideoHead, player) == 56
        && core::mem::offset_of!(NsVideoHead, is_camera) == 72
);

unsafe extern "C" {
    fn ns_js_dispatch_mouse_event(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        client_x: f64,
        client_y: f64,
        page_x: f64,
        page_y: f64,
        button: c_int,
        buttons: c_int,
        shift: GBoolean,
        ctrl: GBoolean,
        alt: GBoolean,
        meta: GBoolean,
        related: *const NsNode,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_drag_event(
        js: *mut c_void,
        session: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        client_x: f64,
        client_y: f64,
        page_x: f64,
        page_y: f64,
        button: c_int,
        buttons: c_int,
        shift: GBoolean,
        ctrl: GBoolean,
        alt: GBoolean,
        meta: GBoolean,
        related: *const NsNode,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_drag_session_new(js: *mut c_void) -> *mut c_void;
    fn ns_js_drag_session_add_file(session: *mut c_void, path: *const c_char);
    fn ns_js_drag_session_free(session: *mut c_void);
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
    fn ns_js_dispatch_clipboard_event(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        text: *const c_char,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_event(
        js: *mut c_void,
        target: *const NsNode,
        kind: *const c_char,
        prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_set_focus(js: *mut c_void, el: *const NsNode);
    fn ns_js_focus_from_pointer(js: *mut c_void, target: *const NsNode);
    fn ns_js_note_pointer_input(js: *mut c_void, pointer: GBoolean);
    fn ns_js_note_user_edit(js: *mut c_void, el: *const NsNode, value_before: *const c_char);
    fn ns_js_commit_change(js: *mut c_void, el: *const NsNode);
    fn ns_js_click_activate(js: *mut c_void, node: *const NsNode) -> GBoolean;
    fn ns_js_activate_summary(js: *mut c_void, el: *const NsNode) -> GBoolean;
    fn ns_js_form_reset(js: *mut c_void, form: *mut NsNode);
    fn ns_js_select_toggle_option(js: *mut c_void, option: *mut NsNode) -> GBoolean;
    fn ns_js_select_choose_option(js: *mut c_void, option: *mut NsNode) -> GBoolean;
    fn ns_js_select_step(js: *mut c_void, select: *mut NsNode, dir: c_int) -> GBoolean;
    fn ns_js_select_edge(js: *mut c_void, select: *mut NsNode, last: GBoolean) -> GBoolean;
    fn ns_js_select_typeahead(js: *mut c_void, select: *mut NsNode, key: *const c_char)
    -> GBoolean;
    fn ns_js_keyboard_activates(el: *const NsNode, key: *const c_char) -> GBoolean;
    fn ns_js_keyboard_activate(
        js: *mut c_void,
        el: *const NsNode,
        key: *const c_char,
        keyup: GBoolean,
    ) -> GBoolean;
    fn ns_js_process_close_request(js: *mut c_void) -> GBoolean;
    fn ns_js_sequential_focus_target(js: *mut c_void, backward: GBoolean) -> *const NsNode;
    fn ns_js_activate_element(js: *mut c_void, el: *const NsNode);
    fn ns_js_node_has_click_handler(js: *mut c_void, target: *const NsNode) -> GBoolean;
    fn ns_js_eval_source(js: *mut c_void, src: *const c_char, origin: *const c_char)
    -> *mut c_char;
    fn ns_box_hit_link(root: *const NsBox, x: f64, y: f64) -> *const c_char;
    fn ns_box_hit_node(root: *const NsBox, x: f64, y: f64) -> *const NsNode;
    fn ns_box_hit_form_dom(root: *const NsBox, x: f64, y: f64) -> *const NsNode;
    fn ns_box_hit_test(root: *const NsBox, x: f64, y: f64) -> *const NsBox;
    fn ns_box_hit_scrollable(root: *mut NsBox, x: f64, y: f64) -> *mut NsBox;
    fn ns_box_hit_scrollbar(
        root: *mut NsBox,
        x: f64,
        y: f64,
        lx: *mut f64,
        ly: *mut f64,
    ) -> *mut NsBox;
    fn ns_box_scroll_snap_from(scroller: *mut NsBox, prev_x: f64, prev_y: f64);
    fn ns_box_count_matches(
        root: *const NsBox,
        needle: *const c_char,
        case_sensitive: GBoolean,
    ) -> c_uint;
    fn ns_box_first_match_below(
        root: *const NsBox,
        needle: *const c_char,
        y: f64,
        case_sensitive: GBoolean,
    ) -> *const NsBox;
    fn ns_box_first_match_above(
        root: *const NsBox,
        needle: *const c_char,
        y: f64,
        case_sensitive: GBoolean,
    ) -> *const NsBox;
    fn ns_box_match_ordinal(
        root: *const NsBox,
        needle: *const c_char,
        target: *const NsBox,
        case_sensitive: GBoolean,
    ) -> c_uint;
    fn ns_selection_text_at(root: *const NsBox, x: f64, y: f64) -> GBoolean;
    fn ns_selection_anchor_at(sel: *mut c_void, root: *const NsBox, x: f64, y: f64) -> GBoolean;
    fn ns_selection_extend_to(sel: *mut c_void, root: *const NsBox, x: f64, y: f64) -> GBoolean;
    fn ns_selection_select_word_at(
        sel: *mut c_void,
        root: *const NsBox,
        x: f64,
        y: f64,
    ) -> GBoolean;
    fn ns_selection_select_block_at(
        sel: *mut c_void,
        root: *const NsBox,
        x: f64,
        y: f64,
    ) -> GBoolean;
    fn ns_node_is_contenteditable_host(n: *const NsNode) -> GBoolean;
    fn ns_node_is_editable(n: *const NsNode) -> GBoolean;
    fn ns_node_is_focusable(n: *const NsNode) -> GBoolean;
    fn ns_node_set_editable_value(n: *mut NsNode, value: *const c_char);
    fn ns_option_label_dup(option: *const NsNode) -> *mut c_char;
    fn ns_element_effectively_inert(el: *const NsNode) -> GBoolean;
    fn ns_form_is_reset_trigger(n: *const NsNode) -> GBoolean;
    fn ns_render_page_uses_hover() -> GBoolean;
    fn ns_render_page_uses_active() -> GBoolean;
    fn ns_css_set_active_node(node: *const NsNode) -> *const NsNode;
    fn ns_video_url_is_inline(url: *const c_char) -> GBoolean;
    fn ns_video_cache_toggle(cache: *mut c_void, v: *mut c_void, now_us: i64) -> GBoolean;
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
        let mut prevented: GBoolean = 0;
        unsafe {
            ns_js_dispatch_mouse_event(
                self.raw(),
                target.as_ptr(),
                kind.as_ptr(),
                p.client.0,
                p.client.1,
                p.page.0,
                p.page.1,
                p.button,
                p.buttons,
                flag(p.mods, 1),
                flag(p.mods, 2),
                flag(p.mods, 4),
                flag(p.mods, 8),
                Node::ptr_or_null(related),
                if want_prevented {
                    &mut prevented
                } else {
                    ptr::null_mut()
                },
            )
        };
        prevented != 0
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
        let mut prevented: GBoolean = 0;
        unsafe {
            ns_js_dispatch_clipboard_event(
                self.raw(),
                target.as_ptr(),
                kind.as_ptr(),
                text.as_ptr(),
                &mut prevented,
            )
        };
        prevented != 0
    }

    pub fn dispatch_scroll(self, target: Node<'_>) {
        unsafe {
            ns_js_dispatch_event(
                self.raw(),
                target.as_ptr(),
                c"scroll".as_ptr(),
                ptr::null_mut(),
            )
        };
    }

    pub fn drop_files(self, target: Node<'_>, paths: &[&CStr], x: c_int, y: c_int) -> bool {
        let session = unsafe { ns_js_drag_session_new(self.raw()) };
        if session.is_null() {
            return false;
        }
        for path in paths {
            unsafe { ns_js_drag_session_add_file(session, path.as_ptr()) };
        }
        let dispatch = |kind: &CStr| {
            let mut prevented: GBoolean = 0;
            let (fx, fy) = (f64::from(x), f64::from(y));
            unsafe {
                ns_js_dispatch_drag_event(
                    self.raw(),
                    session,
                    target.as_ptr(),
                    kind.as_ptr(),
                    fx,
                    fy,
                    fx,
                    fy,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    ptr::null(),
                    &mut prevented,
                )
            };
            prevented != 0
        };
        let mut accept = dispatch(c"dragenter");
        accept |= dispatch(c"dragover");
        if accept {
            dispatch(c"drop");
        } else {
            dispatch(c"dragleave");
        }
        unsafe { ns_js_drag_session_free(session) };
        true
    }

    pub fn set_focus(self, el: Node<'_>) {
        unsafe { ns_js_set_focus(self.raw(), el.as_ptr()) };
    }

    pub fn focus_from_pointer(self, target: Option<Node<'_>>) {
        unsafe { ns_js_focus_from_pointer(self.raw(), Node::ptr_or_null(target)) };
    }

    pub fn note_user_edit(self, el: Node<'_>, value_before: Option<&CStr>) {
        unsafe { ns_js_note_user_edit(self.raw(), el.as_ptr(), opt(value_before)) };
    }

    pub fn commit_change(self, el: Node<'_>) {
        unsafe { ns_js_commit_change(self.raw(), el.as_ptr()) };
    }

    pub fn click_activate(self, node: Node<'_>) -> bool {
        unsafe { ns_js_click_activate(self.raw(), node.as_ptr()) != 0 }
    }

    pub fn activate_summary(self, el: Node<'_>) -> bool {
        unsafe { ns_js_activate_summary(self.raw(), el.as_ptr()) != 0 }
    }

    pub fn form_reset(self, form: Node<'_>) {
        unsafe { ns_js_form_reset(self.raw(), form.as_mut_ptr()) };
    }

    pub fn select_toggle_option(self, option: Node<'_>) -> bool {
        unsafe { ns_js_select_toggle_option(self.raw(), option.as_mut_ptr()) != 0 }
    }

    pub fn select_choose_option(self, option: Node<'_>) -> bool {
        unsafe { ns_js_select_choose_option(self.raw(), option.as_mut_ptr()) != 0 }
    }

    pub fn select_step(self, select: Node<'_>, dir: c_int) -> bool {
        unsafe { ns_js_select_step(self.raw(), select.as_mut_ptr(), dir) != 0 }
    }

    pub fn select_edge(self, select: Node<'_>, last: bool) -> bool {
        unsafe { ns_js_select_edge(self.raw(), select.as_mut_ptr(), glib::boolean(last)) != 0 }
    }

    pub fn select_typeahead(self, select: Node<'_>, key: &CStr) -> bool {
        unsafe { ns_js_select_typeahead(self.raw(), select.as_mut_ptr(), key.as_ptr()) != 0 }
    }

    pub fn keyboard_activate(self, el: Node<'_>, key: Option<&CStr>, keyup: bool) -> bool {
        unsafe {
            ns_js_keyboard_activate(self.raw(), el.as_ptr(), opt(key), glib::boolean(keyup)) != 0
        }
    }

    pub fn process_close_request(self) -> bool {
        unsafe { ns_js_process_close_request(self.raw()) != 0 }
    }

    pub fn sequential_focus_target<'a>(self, backward: bool) -> Option<Node<'a>> {
        unsafe {
            Node::from_ptr(ns_js_sequential_focus_target(
                self.raw(),
                glib::boolean(backward),
            ))
        }
    }

    pub fn activate_element(self, el: Node<'_>) {
        unsafe { ns_js_activate_element(self.raw(), el.as_ptr()) };
    }

    pub fn node_has_click_handler(self, node: Node<'_>) -> bool {
        unsafe { ns_js_node_has_click_handler(self.raw(), node.as_ptr()) != 0 }
    }

    pub fn eval_console(self, src: &CStr) -> *mut c_char {
        unsafe { ns_js_eval_source(self.raw(), src.as_ptr(), c"devtools-console".as_ptr()) }
    }
}

pub fn note_pointer_input(js: Option<Js>, pointer: bool) {
    unsafe {
        ns_js_note_pointer_input(js.map_or(ptr::null_mut(), Js::raw), glib::boolean(pointer))
    };
}

pub fn keyboard_activates(el: Node<'_>, key: Option<&CStr>) -> bool {
    unsafe { ns_js_keyboard_activates(el.as_ptr(), opt(key)) != 0 }
}

pub fn hit_link(layout: BoxRef<'_>, x: f64, y: f64) -> Option<&CStr> {
    let href = unsafe { ns_box_hit_link(layout.as_ptr(), x, y) };
    (!href.is_null()).then(|| unsafe { CStr::from_ptr(href) })
}

pub fn hit_node(layout: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(ns_box_hit_node(layout.as_ptr(), x, y)) }
}

pub fn hit_form_dom(layout: BoxRef<'_>, x: f64, y: f64) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(ns_box_hit_form_dom(layout.as_ptr(), x, y)) }
}

pub fn hit_test(layout: BoxRef<'_>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    unsafe { BoxRef::from_ptr(ns_box_hit_test(layout.as_ptr(), x, y)) }
}

pub fn hit_scrollable(layout: BoxRef<'_>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    unsafe { BoxRef::from_ptr(ns_box_hit_scrollable(layout.as_ptr().cast_mut(), x, y)) }
}

pub fn hit_scrollbar(layout: BoxRef<'_>, x: f64, y: f64) -> Option<(BoxRef<'_>, f64, f64)> {
    let (mut lx, mut ly) = (0.0, 0.0);
    let b = unsafe { ns_box_hit_scrollbar(layout.as_ptr().cast_mut(), x, y, &mut lx, &mut ly) };
    unsafe { BoxRef::from_ptr(b) }.map(|b| (b, lx, ly))
}

pub fn scroll_snap_from(scroller: BoxRef<'_>, prev_x: f64, prev_y: f64) {
    unsafe { ns_box_scroll_snap_from(scroller.as_ptr().cast_mut(), prev_x, prev_y) };
}

pub fn count_matches(layout: BoxRef<'_>, needle: &CStr, case_sensitive: bool) -> c_uint {
    unsafe {
        ns_box_count_matches(
            layout.as_ptr(),
            needle.as_ptr(),
            glib::boolean(case_sensitive),
        )
    }
}

pub fn first_match<'a>(
    layout: BoxRef<'a>,
    needle: &CStr,
    y: f64,
    case_sensitive: bool,
    above: bool,
) -> Option<BoxRef<'a>> {
    let cs = glib::boolean(case_sensitive);
    let b = if above {
        unsafe { ns_box_first_match_above(layout.as_ptr(), needle.as_ptr(), y, cs) }
    } else {
        unsafe { ns_box_first_match_below(layout.as_ptr(), needle.as_ptr(), y, cs) }
    };
    unsafe { BoxRef::from_ptr(b) }
}

pub fn match_ordinal(
    layout: BoxRef<'_>,
    needle: &CStr,
    target: BoxRef<'_>,
    case_sensitive: bool,
) -> c_uint {
    unsafe {
        ns_box_match_ordinal(
            layout.as_ptr(),
            needle.as_ptr(),
            target.as_ptr(),
            glib::boolean(case_sensitive),
        )
    }
}

pub fn selection_text_at(layout: BoxRef<'_>, x: f64, y: f64) -> bool {
    unsafe { ns_selection_text_at(layout.as_ptr(), x, y) != 0 }
}

pub fn is_contenteditable_host(n: Node<'_>) -> bool {
    unsafe { ns_node_is_contenteditable_host(n.as_ptr()) != 0 }
}

pub fn is_editable(n: Option<Node<'_>>) -> bool {
    unsafe { ns_node_is_editable(Node::ptr_or_null(n)) != 0 }
}

pub fn is_focusable(n: Node<'_>) -> bool {
    unsafe { ns_node_is_focusable(n.as_ptr()) != 0 }
}

pub fn set_editable_value(n: Node<'_>, value: &CStr) {
    unsafe { ns_node_set_editable_value(n.as_mut_ptr(), value.as_ptr()) };
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
    !unsafe { ns_css_set_active_node(Node::ptr_or_null(node)) }.is_null()
}

pub fn video_url_is_inline(url: &CStr) -> bool {
    unsafe { ns_video_url_is_inline(url.as_ptr()) != 0 }
}

pub struct VideoRef(*mut c_void);

impl VideoRef {
    pub fn of(b: BoxRef<'_>) -> Option<VideoRef> {
        let v = b.media()?.video();
        (!v.is_null()).then_some(VideoRef(v))
    }

    fn head(&self) -> &NsVideoHead {
        unsafe { &*self.0.cast::<NsVideoHead>() }
    }

    pub fn is_camera(&self) -> bool {
        self.head().is_camera != 0
    }

    pub fn has_player(&self) -> bool {
        !self.head().player.is_null()
    }

    pub fn toggle(&self, videos: Option<Videos>, now_us: i64) {
        unsafe {
            ns_video_cache_toggle(videos.map_or(ptr::null_mut(), Videos::raw), self.0, now_us)
        };
    }
}

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
        unsafe { ns_selection_anchor_at(self.selection.get().cast(), layout.as_ptr(), x, y) };
    }

    pub fn selection_extend_to(&self, layout: BoxRef<'_>, x: f64, y: f64) {
        unsafe { ns_selection_extend_to(self.selection.get().cast(), layout.as_ptr(), x, y) };
    }

    pub fn selection_word_at(&self, layout: BoxRef<'_>, x: f64, y: f64) {
        unsafe { ns_selection_select_word_at(self.selection.get().cast(), layout.as_ptr(), x, y) };
    }

    pub fn selection_block_at(&self, layout: BoxRef<'_>, x: f64, y: f64) {
        unsafe { ns_selection_select_block_at(self.selection.get().cast(), layout.as_ptr(), x, y) };
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
