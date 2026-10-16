//! Southstar — inline collection: walking an inline run's DOM into its text, links, inline attributes, atomic boxes and white-space ranges.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::{MAX_DEPTH, Node, NsNode, children, controls};
use southstar_layout::{InlineAttr, NsBox, Style, inline_kind};
use southstar_layout_content::controls::{
    FontKeyword, append_inline_spacer, emit_attr, emit_attr_styled, emit_color, emit_font_family,
    emit_font_features, emit_font_size, emit_font_stretch, emit_font_variations, emit_font_weight,
    font_kerning, font_keyword, keyword_is, length_or,
};
use southstar_layout_content::{pseudo, tags};
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef, display_of};

use crate::atomic::{is_atomic_inline, node_is_frame_fallback};
use crate::ffi::{self, RawLink};
use crate::state::{self, CollectState};

pub const LINE_SEPARATOR: &[u8] = "\u{2028}".as_bytes();
pub const NBSP: &[u8] = "\u{a0}".as_bytes();
pub const OBJECT_REPLACEMENT: &[u8] = "\u{FFFC}".as_bytes();

const SMALL_CAPS_SCALE: f64 = 0.7;

pub const WS_COLLAPSE: i32 = 0;
pub const WS_PRESERVE: i32 = 1;
pub const WS_PRE_LINE: i32 = 2;

const BOX_NORMAL: u8 = 0;
const INTERNAL_NONE: u8 = 0;
const OUTER_INLINE: u8 = 0;
const INNER_FLOW: u8 = 0;
const INNER_RUBY: u8 = 5;

pub struct WsRange {
    pub start: usize,
    pub end: usize,
    pub mode: i32,
}

#[derive(Default)]
struct Span {
    depth: i32,
    start: usize,
}

impl Span {
    fn open(&mut self, active: bool, at: usize) {
        if active {
            if self.depth == 0 {
                self.start = at;
            }
            self.depth += 1;
        }
    }

    fn close(&mut self, active: bool) -> Option<usize> {
        if !active {
            return None;
        }
        self.depth -= 1;
        (self.depth == 0).then_some(self.start)
    }
}

pub struct Collector<'a> {
    pub styles: StyleTable,
    pub state: CollectState,
    active_href: *const c_char,
    active_target: *const c_char,
    active_link_node: *const NsNode,
    pub out: Vec<u8>,
    pub links: Vec<RawLink>,
    pub attrs: Vec<InlineAttr>,
    bold: Span,
    italic: Span,
    mono: Span,
    underline: Span,
    overline: Span,
    strike: Span,
    q_depth: i32,
    pub text_transform: Option<&'a CStr>,
    pub atomics: Vec<(usize, *mut NsBox)>,
    abs_placeholders: bool,
    pub ws_ranges: Vec<WsRange>,
    ws_parent: *const NsNode,
    ws_parent_mode: i32,
}

pub fn style_is_none(s: StyleRef<'_>) -> bool {
    display_of(Some(s)).is_none()
}

pub fn style_is_absolute_or_fixed(s: StyleRef<'_>) -> bool {
    let pos = s.get(PropId::Position);
    keyword_is(pos, c"absolute") || keyword_is(pos, c"fixed")
}

fn is_atomic_inline_display(s: StyleRef<'_>) -> bool {
    let d = display_of(Some(s));
    d.box_ == BOX_NORMAL
        && d.internal == INTERNAL_NONE
        && d.outer == OUTER_INLINE
        && d.inner != INNER_FLOW
        && d.inner != INNER_RUBY
}

pub fn white_space_mode(node: Node<'_>, styles: StyleTable) -> i32 {
    let mut p = Some(node);
    while let Some(n) = p {
        p = n.parent();
        if !n.is_element() {
            continue;
        }
        if let Some(kw) = styles
            .get(n)
            .and_then(|s| s.get(PropId::WhiteSpace))
            .and_then(ValueRef::keyword_text)
        {
            match kw.to_bytes() {
                b"pre-line" => return WS_PRE_LINE,
                b"pre" | b"pre-wrap" | b"break-spaces" => return WS_PRESERVE,
                b"normal" | b"nowrap" => return WS_COLLAPSE,
                _ => {}
            }
        }
        if matches!(n.name().map(CStr::to_bytes), Some(b"pre" | b"textarea")) {
            return WS_PRESERVE;
        }
    }
    WS_COLLAPSE
}

pub(crate) fn same(a: *const NsNode, n: Node<'_>) -> bool {
    ptr::eq(a, n.as_ptr())
}

fn sorted(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

impl<'a> Collector<'a> {
    pub fn new(styles: StyleTable, abs_placeholders: bool) -> Collector<'a> {
        Collector {
            styles,
            state: state::collect_state(),
            active_href: ptr::null(),
            active_target: ptr::null(),
            active_link_node: ptr::null(),
            out: Vec::new(),
            links: Vec::new(),
            attrs: Vec::new(),
            bold: Span::default(),
            italic: Span::default(),
            mono: Span::default(),
            underline: Span::default(),
            overline: Span::default(),
            strike: Span::default(),
            q_depth: 0,
            text_transform: None,
            atomics: Vec::new(),
            abs_placeholders,
            ws_ranges: Vec::new(),
            ws_parent: ptr::null(),
            ws_parent_mode: 0,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    pub fn push_repeat(&mut self, bytes: &[u8], n: core::ffi::c_long) {
        for _ in 0..n.max(0) {
            self.out.extend_from_slice(bytes);
        }
    }

    fn note_ws(&mut self, parent: Option<Node<'_>>, start: usize) {
        let Some(parent) = parent else {
            return;
        };
        if self.out.len() <= start {
            return;
        }
        if !same(self.ws_parent, parent) {
            self.ws_parent = parent.as_ptr();
            self.ws_parent_mode = white_space_mode(parent, self.styles);
        }
        self.ws_ranges.push(WsRange {
            start,
            end: self.out.len(),
            mode: self.ws_parent_mode,
        });
    }

    fn note_link(&mut self, start: usize) {
        if self.active_href.is_null() {
            return;
        }
        self.links.push(RawLink {
            start,
            len: self.out.len() - start,
            href: self.active_href,
            target: self.active_target,
            dom: self.active_link_node,
        });
    }

    pub fn caret_attrs(&mut self, caret_pos: usize, anchor_pos: usize) {
        if anchor_pos != caret_pos {
            let (s0, s1) = sorted(anchor_pos, caret_pos);
            emit_attr(&mut self.attrs, inline_kind::SELECTION, s0, s1);
        }
        emit_attr(
            &mut self.attrs,
            inline_kind::CARET,
            caret_pos,
            caret_pos + 1,
        );
    }

    pub fn push_atomic(&mut self, b: *mut NsBox) {
        self.atomics.push((self.out.len(), b));
        self.push(OBJECT_REPLACEMENT);
    }

    pub(crate) fn push_editable(&mut self, text: &[u8]) -> (usize, usize) {
        let cb = self.state.caret_byte.min(text.len());
        let ab = self.state.anchor_byte.min(text.len());
        let (mut caret_pos, mut anchor_pos) = (self.out.len(), self.out.len());
        for (i, &ch) in text.iter().enumerate() {
            if i == cb {
                caret_pos = self.out.len();
            }
            if i == ab {
                anchor_pos = self.out.len();
            }
            match ch {
                b'\n' => self.push(LINE_SEPARATOR),
                b'\r' => {}
                ch => self.out.push(ch),
            }
        }
        if cb == text.len() {
            caret_pos = self.out.len();
        }
        if ab == text.len() {
            anchor_pos = self.out.len();
        }
        (caret_pos, anchor_pos)
    }

    fn editable_text(&mut self, n: Node<'_>, text: &[u8]) {
        let start = self.out.len();
        let focused = self.state.focused_is_contenteditable
            && n.parent()
                .is_some_and(|p| same(self.state.focused_input, p));
        let (caret_pos, anchor_pos) = self.push_editable(text);
        self.note_link(start);
        if focused {
            self.push(NBSP);
            self.caret_attrs(caret_pos, anchor_pos);
        }
    }

    fn text(&mut self, n: Node<'_>) {
        let Some(text) = n.text() else {
            return;
        };
        if n.parent().is_some_and(controls::is_contenteditable_host) {
            self.editable_text(n, text.to_bytes());
            return;
        }
        let start = self.out.len();
        match self
            .text_transform
            .and_then(|tt| pseudo::text_transform(text, tt.to_bytes()))
        {
            Some(x) => self.push(&x),
            None => self.push(text.to_bytes()),
        }
        self.note_ws(n.parent(), start);
        self.note_link(start);
    }

    fn absolute(&mut self, n: Node<'_>, s: StyleRef<'_>) {
        let fixed = keyword_is(s.get(PropId::Position), c"fixed");
        if let Some(ph) =
            state::collect_absolute(n.as_ptr(), fixed, !fixed && self.abs_placeholders)
        {
            self.push_atomic(ph.as_mut_ptr());
        }
    }

    pub fn walk(&mut self, n: Node<'a>, depth: i32) {
        if depth >= MAX_DEPTH || node_is_frame_fallback(n) {
            return;
        }
        if n.is_text() {
            self.text(n);
            return;
        }
        if !n.is_element()
            || tags::node_is_non_rendering(Some(n))
            || same(self.state.inline_skip, n)
        {
            return;
        }
        let s = self.styles.get(n);
        if s.is_some_and(style_is_none) {
            return;
        }
        if let Some(st) = s.filter(|&st| style_is_absolute_or_fixed(st)) {
            self.absolute(n, st);
            return;
        }
        if !same(self.state.form_control_inline, n) && is_atomic_inline(Some(n), self.styles) {
            if let Some(sub) = crate::block::collect_atomic_block(n, self.styles, depth + 1) {
                self.push_atomic(sub.as_mut_ptr());
            }
            return;
        }
        let Some(name) = n.name() else {
            for c in children(n) {
                self.walk(c, depth + 1);
            }
            return;
        };
        match name.to_bytes() {
            b"br" => self.push(LINE_SEPARATOR),
            b"wbr" => self.push("\u{200B}".as_bytes()),
            b"progress" => self.gauge(n, false),
            b"meter" => self.gauge(n, true),
            b"input" => self.input(n, s),
            b"button" => self.button(n),
            b"select" => self.select(n),
            b"option" | b"optgroup" => {}
            b"textarea" => self.textarea(n, s),
            _ => self.element(n, s, name, depth),
        }
    }

    fn pseudo_inline(&mut self, ps: StyleRef<'_>, host: Node<'_>) {
        if !pseudo::generates_box(Some(ps)) || style_is_none(ps) {
            return;
        }
        let content = ps.get(PropId::Content);
        if style_is_absolute_or_fixed(ps) {
            if state::with(|st| st.abs_pending.is_some()) {
                crate::generated::register_abs_pseudo(Some(host), Some(ps));
            } else {
                self.pseudo_text(ps, content, host);
            }
            return;
        }
        if is_atomic_inline_display(ps) {
            let txt = content
                .and_then(ValueRef::keyword_text)
                .and_then(|raw| pseudo::resolve_content(raw.to_bytes(), Some(host)));
            let b = crate::generated::pseudo_block_with_text(ps, txt.as_deref());
            self.push_atomic(b.as_mut_ptr());
            return;
        }
        let side = |p| length_or(ps.get(p), 0.0);
        append_inline_spacer(&mut self.out, &mut self.attrs, side(PropId::MarginLeft));
        append_inline_spacer(&mut self.out, &mut self.attrs, side(PropId::PaddingLeft));
        self.pseudo_text(ps, content, host);
        append_inline_spacer(&mut self.out, &mut self.attrs, side(PropId::PaddingRight));
        append_inline_spacer(&mut self.out, &mut self.attrs, side(PropId::MarginRight));
    }

    pub(crate) fn synthesize_small_caps(&mut self, start: usize, end: usize, font_size: f64) {
        if font_size <= 0.0 {
            return;
        }
        let mut run_start = None;
        for i in start..=end {
            let lower = i < end && self.out[i].is_ascii_lowercase();
            if lower {
                self.out[i] = self.out[i].to_ascii_uppercase();
                run_start.get_or_insert(i);
            } else if let Some(s) = run_start.take() {
                emit_font_size(&mut self.attrs, s, i, font_size * SMALL_CAPS_SCALE);
            }
        }
    }

    fn pseudo_text(&mut self, ps: StyleRef<'_>, content: Option<ValueRef<'_>>, host: Node<'_>) {
        let start = self.out.len();
        pseudo::append_content(&mut self.out, content, Some(host), Some(ps));
        let end = self.out.len();
        if end == start {
            return;
        }
        let attrs = &mut self.attrs;
        if let Some((px, 0)) = ps.get(PropId::FontSize).and_then(ValueRef::length)
            && px > 0.0
        {
            emit_font_size(attrs, start, end, px);
        }
        let mut color = ps.text_fill_color().and_then(ValueRef::color);
        if ps
            .keyword_of(PropId::Visibility)
            .is_some_and(|v| matches!(v.to_bytes(), b"hidden" | b"collapse"))
        {
            let mut c = color.unwrap_or([0, 0, 0, 0]);
            c[3] = 0;
            color = Some(c);
        }
        if let Some(c) = color {
            emit_color(attrs, start, end, c);
        }
        if let Some(f) = ps
            .get(PropId::FontFamily)
            .filter(|v| v.kind() == southstar_style::Kind::Keyword)
            .and_then(ValueRef::keyword_text)
        {
            emit_font_family(attrs, start, end, f.as_ptr());
        }
        let weight = ps
            .get(PropId::FontWeight)
            .map_or(-1, |v| v.font_weight_or(-1));
        if weight > 0 {
            emit_font_weight(attrs, start, end, weight);
        }
        let fst = ps.get(PropId::FontStyle);
        if fst.is_some_and(ValueRef::font_slanted) {
            emit_attr(attrs, inline_kind::ITALIC, start, end);
        }
    }

    fn element(&mut self, n: Node<'a>, s: Option<StyleRef<'a>>, name: &CStr, depth: i32) {
        let prev = (self.active_href, self.active_target, self.active_link_node);
        if name.to_bytes() == b"a"
            && let Some(h) = n.attr(c"href").filter(|h| !h.is_empty())
        {
            self.active_href = h.as_ptr();
            self.active_target = n.attr(c"target").map_or(ptr::null(), CStr::as_ptr);
            self.active_link_node = n.as_ptr();
        }
        self.styled_element(n, s, name, depth);
        (self.active_href, self.active_target, self.active_link_node) = prev;
    }

    fn styled_element(&mut self, n: Node<'a>, s: Option<StyleRef<'a>>, name: &CStr, depth: i32) {
        let get = |p| s.and_then(|s| s.get(p));
        let ml = length_or(get(PropId::MarginLeft), 0.0);
        let mr = length_or(get(PropId::MarginRight), 0.0);
        let pl = inline_box_side_px(
            s,
            PropId::PaddingLeft,
            PropId::BorderLeftWidth,
            PropId::BorderLeftStyle,
        );
        let pr = inline_box_side_px(
            s,
            PropId::PaddingRight,
            PropId::BorderRightWidth,
            PropId::BorderRightStyle,
        );
        append_inline_spacer(&mut self.out, &mut self.attrs, ml);
        let elem_start = self.out.len();
        append_inline_spacer(&mut self.out, &mut self.attrs, pl);
        let tag = Some(name);
        let mut bold = tags::is_bold(tag);
        let mut italic = tags::is_italic(tag);
        let mono = tags::is_monospace(tag);
        let mut uline = tags::is_underline(tag);
        let mut oline = false;
        let mut strike = tags::is_strike(tag);
        let fw = get(PropId::FontWeight);
        let font_weight_self = fw.map_or(-1, |v| v.font_weight_or(-1));
        let font_weight_active = font_weight_self > 0;
        if font_weight_active {
            bold = false;
        } else if let Some(kw) = fw.and_then(ValueRef::keyword_text)
            && matches!(kw.to_bytes(), b"bold" | b"bolder")
        {
            bold = true;
        }
        if let Some(kw) = get(PropId::FontStyle).and_then(ValueRef::keyword_text) {
            italic = kw.to_bytes() == b"italic" || kw.to_bytes().starts_with(b"oblique");
        }
        if let Some(kw) = get(PropId::TextDecoration).and_then(ValueRef::keyword_text) {
            let kw = kw.to_bytes();
            if contains(kw, b"underline") {
                uline = true;
            }
            if contains(kw, b"overline") {
                oline = true;
            }
            if contains(kw, b"line-through") {
                strike = true;
            }
            if contains(kw, b"none") {
                (uline, oline, strike) = (false, false, false);
            }
        }
        if get(PropId::TextDecorationColor)
            .and_then(ValueRef::color)
            .is_some_and(|c| c[3] == 0)
        {
            (uline, oline, strike) = (false, false, false);
        }
        let decor_style: *const Style = if uline || oline || strike {
            s.map_or(ptr::null(), StyleRef::as_ptr)
        } else {
            ptr::null()
        };
        let at = self.out.len();
        self.bold.open(bold, at);
        self.italic.open(italic, at);
        self.mono.open(mono, at);
        self.underline.open(uline, at);
        self.overline.open(oline, at);
        self.strike.open(strike, at);

        let weight_start = self.out.len();
        let fst = get(PropId::FontStretch);
        let font_stretch_self = ffi::font_stretch_rank(fst);
        let font_stretch_active = fst.is_some();
        let stretch_start = self.out.len();
        let kerning = font_kerning(s);
        let ligatures = font_keyword(s, FontKeyword::Ligatures);
        let features = font_keyword(s, FontKeyword::Features);
        let features_active = kerning >= 0 || ligatures.is_some() || features.is_some();
        let features_start = self.out.len();
        let variations = font_keyword(s, FontKeyword::Variations);
        let variations_start = self.out.len();

        let is_q = name.to_bytes() == b"q";
        if is_q {
            let open_q = pseudo::quotes_string(s, self.q_depth, false);
            self.push(&open_q);
            self.q_depth += 1;
        }

        let (mut bidi_override, mut bidi_isolate, mut bidi_plaintext) = (false, false, false);
        if let Some(ub) = s.and_then(|s| s.keyword_of(PropId::UnicodeBidi)) {
            match ub.to_bytes() {
                b"bidi-override" => bidi_override = true,
                b"isolate-override" => (bidi_override, bidi_isolate) = (true, true),
                b"isolate" => bidi_isolate = true,
                b"plaintext" => bidi_plaintext = true,
                _ => {}
            }
        }
        if bidi_override || bidi_isolate || bidi_plaintext {
            let bd_dir = n
                .attr(c"dir")
                .filter(|d| !d.is_empty())
                .or_else(|| s.and_then(|s| s.keyword_of(PropId::Direction)))
                .map(CStr::to_bytes);
            let rtl = bd_dir.is_some_and(|d| d.eq_ignore_ascii_case(b"rtl"));
            let dir_auto = bd_dir.is_some_and(|d| d.eq_ignore_ascii_case(b"auto"))
                || (name.to_bytes() == b"bdi" && n.attr(c"dir").is_none());
            if bidi_isolate || bidi_plaintext {
                let mark = if bidi_plaintext || dir_auto {
                    "\u{2068}"
                } else if rtl {
                    "\u{2067}"
                } else {
                    "\u{2066}"
                };
                self.push(mark.as_bytes());
            }
            if bidi_override {
                self.push(if rtl { "\u{202E}" } else { "\u{202D}" }.as_bytes());
            }
        }

        let sup = name.to_bytes() == b"sup";
        let sub = name.to_bytes() == b"sub";
        let rise_start = self.out.len();
        let small_caps = get(PropId::FontVariant)
            .and_then(ValueRef::keyword_text)
            .is_some_and(|kw| has_small_caps(kw.to_bytes()));
        let sc_start = self.out.len();

        let font_size_self = match get(PropId::FontSize).and_then(ValueRef::length) {
            Some((v, 0)) => v,
            _ => 0.0,
        };
        let fs_start = self.out.len();
        let fs_active = font_size_self > 0.0;

        let color_start = self.out.len();
        let mut color = s
            .and_then(StyleRef::text_fill_color)
            .and_then(ValueRef::color);
        if let Some(vis) = s.and_then(|s| s.keyword_of(PropId::Visibility))
            && matches!(vis.to_bytes(), b"hidden" | b"collapse")
        {
            let mut c = color.unwrap_or([0, 0, 0, 0]);
            c[3] = 0;
            color = Some(c);
        }

        let family_start = self.out.len();
        let family = get(PropId::FontFamily)
            .filter(|v| v.kind() == southstar_style::Kind::Keyword)
            .map(|v| v.keyword_text().map_or(ptr::null(), CStr::as_ptr));

        let prev_text_transform = self.text_transform;
        if let Some(v) =
            get(PropId::TextTransform).filter(|v| v.kind() == southstar_style::Kind::Keyword)
        {
            match v.keyword_text() {
                Some(kw) if kw.to_bytes() == b"none" => self.text_transform = None,
                Some(kw)
                    if matches!(kw.to_bytes(), b"uppercase" | b"lowercase" | b"capitalize") =>
                {
                    self.text_transform = Some(kw)
                }
                _ => {}
            }
        }

        let pseudo_blocks = same(self.state.pseudo_blocks_host, n);
        if let Some(before) = s.and_then(StyleRef::before)
            && before.get(PropId::Content).is_some()
            && !(pseudo_blocks && self.state.pseudo_block_before)
        {
            self.pseudo_inline(before, n);
        }
        for c in children(n) {
            self.walk(c, depth + 1);
        }
        if let Some(after) = s.and_then(StyleRef::after)
            && after.get(PropId::Content).is_some()
            && !(pseudo_blocks && self.state.pseudo_block_after)
        {
            self.pseudo_inline(after, n);
        }

        self.text_transform = prev_text_transform;

        let end = self.out.len();
        let attrs = &mut self.attrs;
        if fs_active && end > fs_start {
            emit_font_size(attrs, fs_start, end, font_size_self);
        }
        if let Some(c) = color.filter(|_| end > color_start) {
            emit_color(attrs, color_start, end, c);
        }
        if let Some(f) = family.filter(|f| !f.is_null() && end > family_start) {
            emit_font_family(attrs, family_start, end, f);
        }
        if font_weight_active && end > weight_start {
            emit_font_weight(attrs, weight_start, end, font_weight_self);
        }
        if font_stretch_active && end > stretch_start {
            emit_font_stretch(attrs, stretch_start, end, font_stretch_self);
        }
        if features_active && end > features_start {
            emit_font_features(
                attrs,
                features_start,
                end,
                kerning,
                ligatures.map_or(ptr::null(), CStr::as_ptr),
                features.map_or(ptr::null(), CStr::as_ptr),
            );
        }
        if let Some(v) = variations.filter(|_| end > variations_start) {
            emit_font_variations(attrs, variations_start, end, v.as_ptr());
        }
        if let Some(start) = self.bold.close(bold) {
            emit_attr(attrs, inline_kind::BOLD, start, end);
        }
        if let Some(start) = self.italic.close(italic) {
            emit_attr(attrs, inline_kind::ITALIC, start, end);
        }
        if let Some(start) = self.mono.close(mono) {
            emit_attr(attrs, inline_kind::MONOSPACE, start, end);
        }
        if let Some(start) = self.underline.close(uline) {
            emit_attr_styled(attrs, inline_kind::UNDERLINE, start, end, decor_style);
        }
        if let Some(start) = self.overline.close(oline) {
            emit_attr_styled(attrs, inline_kind::OVERLINE, start, end, decor_style);
        }
        if let Some(start) = self.strike.close(strike) {
            emit_attr_styled(attrs, inline_kind::STRIKETHROUGH, start, end, decor_style);
        }
        if sup && end > rise_start {
            emit_attr(attrs, inline_kind::SUPERSCRIPT, rise_start, end);
        }
        if sub && end > rise_start {
            emit_attr(attrs, inline_kind::SUBSCRIPT, rise_start, end);
        }
        if small_caps && end > sc_start {
            emit_attr(attrs, inline_kind::SMALL_CAPS, sc_start, end);
            self.synthesize_small_caps(sc_start, end, font_size_self);
        }
        if bidi_override {
            self.push("\u{202C}".as_bytes());
        }
        if bidi_isolate || bidi_plaintext {
            self.push("\u{2069}".as_bytes());
        }
        if is_q {
            self.q_depth -= 1;
            let close_q = pseudo::quotes_string(s, self.q_depth, true);
            self.push(&close_q);
        }
        append_inline_spacer(&mut self.out, &mut self.attrs, pr);
        if self.out.len() > elem_start {
            self.attrs.push(
                InlineAttr::new(
                    inline_kind::ELEMENT,
                    elem_start,
                    self.out.len() - elem_start,
                )
                .with_dom(n.as_ptr().cast())
                .with_style(s.map_or(ptr::null(), StyleRef::as_ptr)),
            );
        }
        append_inline_spacer(&mut self.out, &mut self.attrs, mr);
    }
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn inline_box_side_px(
    s: Option<StyleRef<'_>>,
    padding: PropId,
    border_width: PropId,
    border_style: PropId,
) -> f64 {
    let Some(s) = s else {
        return 0.0;
    };
    let mut side = length_or(s.get(padding), 0.0);
    if let Some(st) = s.get(border_style) {
        let hidden = st
            .keyword_text()
            .is_some_and(|k| matches!(k.to_bytes(), b"none" | b"hidden"));
        if !hidden {
            side += length_or(s.get(border_width), 0.0);
        }
    }
    side
}

pub(crate) fn has_small_caps(variant: &[u8]) -> bool {
    variant
        .split(|&c| c == b' ')
        .any(|w| w == b"small-caps" || w == b"all-small-caps")
}
