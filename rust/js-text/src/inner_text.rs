//! Southstar — the innerText getter's rendered-text walk over computed display, visibility, white-space and text-transform.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Kind as NodeKind, Node, children, serialize};
use southstar_style::{Display, Kind, PropId, StyleRef, StyleTable};

use crate::{MAX_DEPTH, element_named, ffi, name_in, name_is};

const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";

const BOX_NORMAL: u8 = 0;
const BOX_CONTENTS: u8 = 2;
const OUTER_INLINE: u8 = 0;
const INNER_FLOW: u8 = 0;
const INNER_TABLE: u8 = 2;
const INNER_FLEX: u8 = 3;
const INNER_GRID: u8 = 4;
const INNER_RUBY: u8 = 5;
const INTERNAL_NONE: u8 = 0;
const INTERNAL_TABLE_CELL: u8 = 5;
const INTERNAL_TABLE_COLUMN_GROUP: u8 = 6;
const INTERNAL_TABLE_COLUMN: u8 = 7;
const INTERNAL_TABLE_CAPTION: u8 = 8;

const SKIPPED: &[&str] = &[
    "head", "title", "meta", "link", "base", "noscript", "template", "datalist", "textarea",
    "iframe", "audio", "video", "object", "embed", "frame", "frameset", "svg", "math",
];
const REPLACED: &[&str] = &["img", "canvas", "input"];
const TABLE_CONTAINERS: &[&str] = &["table", "thead", "tbody", "tfoot", "tr", "colgroup", "col"];
const BLOCKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "html",
    "li",
    "main",
    "nav",
    "ol",
    "optgroup",
    "option",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "tfoot",
    "thead",
    "tr",
    "ul",
];
const SVG_UNRENDERED: &[&str] = &[
    "defs",
    "symbol",
    "clipPath",
    "mask",
    "pattern",
    "marker",
    "linearGradient",
    "radialGradient",
    "filter",
    "title",
    "desc",
    "metadata",
    "style",
    "script",
    "foreignObject",
];

fn is_internal(d: Display) -> bool {
    d.box_ == BOX_NORMAL && d.internal != INTERNAL_NONE
}

fn is_table_internal(d: Display) -> bool {
    is_internal(d) && d.internal <= INTERNAL_TABLE_CAPTION
}

fn inner_is(d: Display, inner: u8) -> bool {
    d.box_ == BOX_NORMAL && d.internal == INTERNAL_NONE && d.inner == inner
}

fn is_atomic_inline(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.internal == INTERNAL_NONE
        && d.outer == OUTER_INLINE
        && d.inner != INNER_FLOW
        && d.inner != INNER_RUBY
}

fn display(s: Option<StyleRef<'_>>) -> Display {
    southstar_style::display_of(s)
}

fn keyword(s: Option<StyleRef<'_>>, prop: PropId) -> Option<&[u8]> {
    s?.get(prop)?.keyword_text().map(CStr::to_bytes)
}

fn display_is_keyword(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::Display))
        .is_some_and(|v| v.kind() == Kind::Keyword)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WhiteSpace {
    Normal,
    Pre,
    PreLine,
}

#[derive(Clone, Copy)]
enum Transform {
    Upper,
    Lower,
    Capitalize,
}

#[derive(Default)]
struct Out {
    text: Vec<u8>,
    pending_space: bool,
    pending_breaks: i32,
    have_content: bool,
    have_text: bool,
}

impl Out {
    fn materialize_breaks(&mut self) {
        if !self.have_text {
            self.pending_breaks = 0;
            if !self.have_content {
                self.pending_space = false;
            }
            return;
        }
        if self.pending_breaks > 0 {
            for _ in 0..self.pending_breaks {
                self.text.push(b'\n');
            }
            self.pending_breaks = 0;
            self.pending_space = false;
        }
    }

    fn require_break(&mut self, count: i32) {
        self.pending_breaks = self.pending_breaks.max(count);
        self.pending_space = false;
    }

    fn flush_space(&mut self) {
        if self.pending_space {
            if self.have_content {
                self.text.push(b' ');
            }
            self.pending_space = false;
        }
    }

    fn glyph(&mut self, byte: u8) {
        self.materialize_breaks();
        self.flush_space();
        self.text.push(byte);
        self.have_content = true;
        self.have_text = true;
    }

    fn forced_break(&mut self) {
        self.materialize_breaks();
        self.pending_space = false;
        self.text.push(b'\n');
        self.have_content = false;
        self.have_text = true;
    }

    fn replaced(&mut self) {
        self.materialize_breaks();
        self.flush_space();
        self.have_content = true;
    }

    fn tab(&mut self) {
        self.materialize_breaks();
        self.pending_space = false;
        self.text.push(b'\t');
        self.have_content = true;
        self.have_text = true;
    }

    fn atomic(&mut self, inner: &[u8]) {
        if inner.is_empty() {
            self.replaced();
            return;
        }
        self.materialize_breaks();
        if self.pending_space && self.have_content {
            self.text.push(b' ');
        }
        self.pending_space = false;
        self.text.extend_from_slice(inner);
        self.have_content = true;
        self.have_text = true;
    }

    fn text(&mut self, text: &CStr, ws: WhiteSpace, transform: Option<Transform>) {
        let transformed = transform.map(|t| apply_transform(text, t));
        let bytes = transformed.as_deref().unwrap_or(text.to_bytes());
        for &byte in bytes {
            if byte == 0 {
                break;
            }
            match ws {
                WhiteSpace::Pre if byte == b'\r' => {}
                WhiteSpace::Pre if byte == b'\n' => self.forced_break(),
                WhiteSpace::Pre => self.glyph(byte),
                WhiteSpace::PreLine if byte == b'\n' || byte == b'\r' => self.forced_break(),
                _ if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c) => {
                    self.pending_space = true
                }
                _ => self.glyph(byte),
            }
        }
    }
}

fn apply_transform(text: &CStr, transform: Transform) -> Vec<u8> {
    match transform {
        Transform::Upper => ffi::utf8_upper(text),
        Transform::Lower => ffi::utf8_lower(text),
        Transform::Capitalize => {
            let source = String::from_utf8_lossy(text.to_bytes());
            let mut out = String::with_capacity(source.len());
            let mut at_word_start = true;
            for ch in source.chars() {
                if ffi::unichar_is_space(ch) || ch == '-' || ch == '/' {
                    out.push(ch);
                    at_word_start = true;
                } else if at_word_start {
                    out.push(ffi::unichar_to_title(ch));
                    at_word_start = false;
                } else {
                    out.push(ch);
                }
            }
            out.into_bytes()
        }
    }
}

fn is_all_ws(text: &CStr) -> bool {
    text.to_bytes()
        .iter()
        .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
}

fn is_cell(s: Option<StyleRef<'_>>, node: Node<'_>) -> bool {
    let d = display(s);
    if s.is_some() && is_internal(d) && d.internal == INTERNAL_TABLE_CELL {
        return true;
    }
    if display_is_keyword(s) {
        return false;
    }
    name_in(node, &["td", "th"])
}

fn is_table_container(s: Option<StyleRef<'_>>, node: Node<'_>) -> bool {
    if keyword(s, PropId::Display).is_some() {
        let d = display(s);
        return inner_is(d, INNER_TABLE)
            || (is_table_internal(d)
                && d.internal != INTERNAL_TABLE_CELL
                && d.internal != INTERNAL_TABLE_CAPTION);
    }
    name_in(node, TABLE_CONTAINERS)
}

fn is_block(s: Option<StyleRef<'_>>, node: Node<'_>) -> bool {
    if keyword(s, PropId::Display).is_some() {
        let d = display(s);
        if d.is_none() || d.box_ == BOX_CONTENTS || d.outer == OUTER_INLINE {
            return false;
        }
        return d.internal != INTERNAL_TABLE_CELL
            && d.internal != INTERNAL_TABLE_COLUMN
            && d.internal != INTERNAL_TABLE_COLUMN_GROUP;
    }
    name_in(node, BLOCKS)
}

fn white_space(s: Option<StyleRef<'_>>, node: Node<'_>, inherited: WhiteSpace) -> WhiteSpace {
    match keyword(s, PropId::WhiteSpace) {
        Some(b"pre" | b"pre-wrap" | b"break-spaces") => return WhiteSpace::Pre,
        Some(b"pre-line") => return WhiteSpace::PreLine,
        Some(b"normal" | b"nowrap") => return WhiteSpace::Normal,
        _ => {}
    }
    if name_in(node, &["pre", "listing", "xmp"]) {
        return WhiteSpace::Pre;
    }
    inherited
}

fn visible(s: Option<StyleRef<'_>>, inherited: bool) -> bool {
    match keyword(s, PropId::Visibility) {
        Some(b"hidden" | b"collapse") => false,
        Some(b"visible") => true,
        _ => inherited,
    }
}

fn transform(s: Option<StyleRef<'_>>) -> Option<Transform> {
    match keyword(s, PropId::TextTransform)? {
        b"uppercase" => Some(Transform::Upper),
        b"lowercase" => Some(Transform::Lower),
        b"capitalize" => Some(Transform::Capitalize),
        _ => None,
    }
}

fn blockifies_children(s: Option<StyleRef<'_>>) -> bool {
    let d = display(s);
    inner_is(d, INNER_FLEX) || inner_is(d, INNER_GRID)
}

fn is_shadow_root(node: Node<'_>) -> bool {
    node.is_element() && node.attr(SHADOW_ATTR).is_some()
}

fn hidden_child(node: Node<'_>) -> bool {
    serialize::is_embedded_doc(node) || is_shadow_root(node)
}

fn rendered_child(parent: Node<'_>, child: Node<'_>) -> bool {
    if parent.name().is_none() {
        return true;
    }
    if is_shadow_root(child) {
        return false;
    }
    let child_is = child.is_element() && child.name().is_some();
    if name_is(parent, "select") {
        return child_is && name_in(child, &["option", "optgroup"]);
    }
    if name_is(parent, "optgroup") && parent.parent().is_some_and(|p| element_named(p, "select")) {
        return child_is && name_is(child, "option");
    }
    true
}

fn collect_svg(node: Node<'_>, out: &mut Out, in_text: bool, tt: Option<Transform>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in children(node) {
        match child.kind() {
            NodeKind::Text => {
                if in_text && let Some(text) = child.text() {
                    out.text(text, WhiteSpace::Normal, tt);
                }
            }
            NodeKind::Element if child.name().is_some() && !name_in(child, SVG_UNRENDERED) => {
                let text = in_text || name_is(child, "text");
                collect_svg(child, out, text, tt, depth + 1);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
struct Inherited {
    ws: WhiteSpace,
    visible: bool,
    transform: Option<Transform>,
    block: bool,
}

struct Walker {
    styles: StyleTable,
}

impl Walker {
    fn style<'a>(&self, node: Node<'a>) -> Option<StyleRef<'a>> {
        self.styles.get(node)
    }

    fn has_following_cell(&self, node: Node<'_>) -> bool {
        core::iter::successors(node.next_sibling(), |s| s.next_sibling())
            .filter(|s| s.is_element())
            .any(|s| is_cell(self.style(s), s))
    }

    fn children(&self, node: Node<'_>, out: &mut Out, inherited: Inherited, depth: i32) {
        let details_closed = name_is(node, "details") && node.attr(c"open").is_none();
        for child in children(node) {
            let is_summary = child.is_element() && name_is(child, "summary");
            if details_closed && !is_summary {
                continue;
            }
            if !rendered_child(node, child) {
                continue;
            }
            self.collect(child, out, inherited, depth + 1, false);
            if child.is_element() {
                let cs = self.style(child);
                if is_cell(cs, child)
                    && visible(cs, inherited.visible)
                    && self.has_following_cell(child)
                {
                    out.tab();
                }
            }
            if details_closed {
                break;
            }
        }
    }

    fn collect(&self, node: Node<'_>, out: &mut Out, inherited: Inherited, depth: i32, root: bool) {
        if depth >= MAX_DEPTH || hidden_child(node) {
            return;
        }
        if node.is_text() {
            let Some(text) = node.text().filter(|_| inherited.visible) else {
                return;
            };
            if let Some(parent) = node.parent()
                && is_all_ws(text)
                && is_table_container(self.style(parent), parent)
            {
                return;
            }
            out.text(text, inherited.ws, inherited.transform);
            return;
        }
        if !node.is_element() {
            return;
        }
        if !root && inherited.visible && element_named(node, "svg") {
            let ss = self.style(node);
            if display(ss).is_none() || !visible(ss, inherited.visible) {
                return;
            }
            let mut sub = Out::default();
            collect_svg(node, &mut sub, false, inherited.transform, depth);
            out.atomic(&sub.text);
            return;
        }
        if name_in(node, SKIPPED) {
            return;
        }
        if node.flags() & (southstar_dom::FLAG_SVG_NS | southstar_dom::FLAG_FOREIGN_NS) != 0 {
            return;
        }
        let s = self.style(node);
        if display(s).is_none() {
            return;
        }
        if s.is_none() && name_in(node, &["script", "style"]) {
            return;
        }
        let child = Inherited {
            ws: white_space(s, node, inherited.ws),
            visible: visible(s, inherited.visible),
            transform: transform(s).or(inherited.transform),
            block: blockifies_children(s),
        };
        let mut breaks = 0;
        if name_in(node, REPLACED) {
            if root || !visible(s, inherited.visible) {
                return;
            }
            if is_block(s, node) {
                out.require_break(1);
                out.have_content = true;
                out.require_break(1);
            } else {
                out.replaced();
            }
            return;
        }
        if !root {
            if name_is(node, "br") {
                if child.visible {
                    out.forced_break();
                }
                return;
            }
            let is_p = name_is(node, "p");
            let intrinsic_break = is_p || name_is(node, "select");
            if is_atomic_inline(display(s)) && !intrinsic_break {
                let mut sub = Out::default();
                self.children(node, &mut sub, child, depth);
                out.atomic(&sub.text);
                return;
            }
            let block = inherited.block || is_block(s, node);
            breaks = match (child.visible, is_p, block) {
                (false, _, _) => 0,
                (true, true, _) => 2,
                (true, false, true) => 1,
                (true, false, false) => 0,
            };
            if breaks != 0 {
                out.require_break(breaks);
            }
        }
        self.children(node, out, child, depth);
        if !root && breaks != 0 {
            out.require_break(breaks);
        }
    }
}

pub(crate) fn rendered_text(styles: StyleTable, node: Node<'_>) -> Vec<u8> {
    let walker = Walker { styles };
    let mut out = Out::default();
    let inherited = Inherited {
        ws: WhiteSpace::Normal,
        visible: true,
        transform: None,
        block: false,
    };
    walker.collect(node, &mut out, inherited, 0, true);
    out.text
}

pub(crate) fn display_none(styles: StyleTable, node: Node<'_>) -> bool {
    display(styles.get(node)).is_none()
}

pub(crate) fn text_content(node: Node<'_>) -> Vec<u8> {
    serialize::collect_all_text(Some(node))
}
