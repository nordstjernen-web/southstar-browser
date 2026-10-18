//! Southstar — building table boxes: rows grouped by header, body and footer, cells with their spans and pseudo content, column hints, captions, and the anonymous tables, rows and cells that wrap stray table parts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::{BoxPool, BoxRef};
use southstar_layout_geom::style_is_absolute_or_fixed;
use southstar_style::{Display, StyleRef, display_of};

use crate::ffi::{
    Styles, append_display_contents_children, append_generated_after, append_node_children,
    build_block, build_inline_run, build_pseudo_block_for, build_pseudo_inline_for,
    collect_box_bg_image, continues_inline_run, dom_ptr_of, is_inline_dom, parse_int,
    queue_absolute_node, register_abs_pseudo, tag_is_non_rendering,
};
use crate::runs::{append_child, merge_prefix};

const BOX_TABLE: u32 = 4;
const BOX_TABLE_CAPTION: u32 = 5;
const BOX_TABLE_ROW: u32 = 6;
const BOX_TABLE_CELL: u32 = 7;

const MAX_DEPTH: i32 = 512;

const DISPLAY_BOX_NORMAL: u8 = 0;
const DISPLAY_BOX_NONE: u8 = 1;
const DISPLAY_BOX_CONTENTS: u8 = 2;
const INNER_TABLE: u8 = 2;
const INTERNAL_NONE: u8 = 0;
const INTERNAL_ROW_GROUP: u8 = 1;
const INTERNAL_HEADER_GROUP: u8 = 2;
const INTERNAL_FOOTER_GROUP: u8 = 3;
const INTERNAL_ROW: u8 = 4;
const INTERNAL_CELL: u8 = 5;
const INTERNAL_CAPTION: u8 = 8;

fn is_internal(d: Display, kind: u8) -> bool {
    d.box_ == DISPLAY_BOX_NORMAL && d.internal != INTERNAL_NONE && d.internal == kind
}

fn display(s: Option<StyleRef<'_>>) -> Display {
    display_of(s)
}

fn style_is_none(s: Option<StyleRef<'_>>) -> bool {
    display(s).box_ == DISPLAY_BOX_NONE
}

fn style_is_contents(s: Option<StyleRef<'_>>) -> bool {
    display(s).box_ == DISPLAY_BOX_CONTENTS
}

fn display_is_table(s: Option<StyleRef<'_>>) -> bool {
    let d = display(s);
    d.box_ == DISPLAY_BOX_NORMAL && d.internal == INTERNAL_NONE && d.inner == INNER_TABLE
}

fn named(n: Node<'_>, name: &[u8]) -> bool {
    n.element_name() == Some(name)
}

fn element_tag(n: Node<'_>) -> Option<&[u8]> {
    if n.is_element() {
        n.name().map(|name| name.to_bytes())
    } else {
        None
    }
}

fn node_table_internal(n: Node<'_>, styles: Styles) -> u8 {
    if !n.is_element() {
        return INTERNAL_NONE;
    }
    let s = if styles.as_ptr().is_null() {
        None
    } else {
        styles.get(n)
    };
    let d = display(s);
    if d.box_ == DISPLAY_BOX_NORMAL && d.internal != INTERNAL_NONE && d.internal <= INTERNAL_CAPTION
    {
        d.internal
    } else {
        INTERNAL_NONE
    }
}

fn node_is_row_group(n: Node<'_>, styles: Styles) -> bool {
    matches!(
        node_table_internal(n, styles),
        INTERNAL_ROW_GROUP | INTERNAL_HEADER_GROUP | INTERNAL_FOOTER_GROUP
    )
}

fn is_table_row(n: Node<'_>, styles: Styles) -> bool {
    if !n.is_element() {
        return false;
    }
    element_tag(n) == Some(b"tr") || is_internal(display(styles.get(n)), INTERNAL_ROW)
}

pub fn is_table_box(n: Option<Node<'_>>, styles: Styles) -> bool {
    let Some(n) = n.filter(|n| n.is_element()) else {
        return false;
    };
    element_tag(n) == Some(b"table") || display_is_table(styles.get(n))
}

fn is_cell_element(n: Node<'_>, styles: Styles) -> bool {
    if named(n, b"td") || named(n, b"th") {
        return true;
    }
    n.is_element() && is_internal(display(styles.get(n)), INTERNAL_CELL)
}

fn is_table_caption(n: Node<'_>, styles: Styles) -> bool {
    if named(n, b"caption") {
        return true;
    }
    n.is_element() && is_internal(display(styles.get(n)), INTERNAL_CAPTION)
}

pub fn node_is_table_internal(n: Option<Node<'_>>, styles: Styles) -> bool {
    let Some(n) = n else {
        return false;
    };
    node_table_internal(n, styles) != INTERNAL_NONE
        || is_cell_element(n, styles)
        || is_table_row(n, styles)
        || is_table_caption(n, styles)
}

fn siblings_from(start: Option<Node<'_>>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(start, |c| c.next_sibling())
}

fn collect_rows_recurse<'a>(n: Node<'a>, styles: Styles, out: &mut Vec<Node<'a>>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if let Some(tag) = element_tag(n) {
        if is_table_row(n, styles) {
            let rs = styles.get(n);
            if rs.is_some() && style_is_none(rs) {
                return;
            }
            if let Some(rs) = rs.filter(|&rs| style_is_absolute_or_fixed(Some(rs))) {
                queue_absolute_node(n, rs);
                return;
            }
            out.push(n);
            return;
        }
        let s = styles.get(n);
        if tag == b"table" || is_internal(display(s), INTERNAL_CAPTION) || display_is_table(s) {
            return;
        }
    }
    for c in siblings_from(n.first_child()) {
        collect_rows_recurse(c, styles, out, depth + 1);
    }
}

#[derive(PartialEq, Eq)]
enum RowGroup {
    Body,
    Header,
    Footer,
}

fn node_row_group(n: Node<'_>, styles: Styles) -> RowGroup {
    if named(n, b"thead") {
        return RowGroup::Header;
    }
    if named(n, b"tfoot") {
        return RowGroup::Footer;
    }
    let s = if styles.as_ptr().is_null() {
        None
    } else {
        styles.get(n)
    };
    let d = display(s);
    if is_internal(d, INTERNAL_HEADER_GROUP) {
        return RowGroup::Header;
    }
    if is_internal(d, INTERNAL_FOOTER_GROUP) {
        return RowGroup::Footer;
    }
    RowGroup::Body
}

fn collect_rows<'a>(table: Node<'a>, styles: Styles) -> Vec<Node<'a>> {
    let mut header = Vec::new();
    let mut body = Vec::new();
    let mut footer = Vec::new();
    for c in siblings_from(table.first_child()) {
        let dst = match node_row_group(c, styles) {
            RowGroup::Header => &mut header,
            RowGroup::Footer => &mut footer,
            RowGroup::Body => &mut body,
        };
        collect_rows_recurse(c, styles, dst, 0);
    }
    header.extend(body);
    header.extend(footer);
    header
}

fn cell_pseudo_before<'a>(
    cell: BoxRef<'a>,
    n: Node<'_>,
    s: Option<StyleRef<'_>>,
) -> Option<BoxRef<'a>> {
    let before = s?.before()?;
    if let Some(block) = build_pseudo_block_for(before, n) {
        append_child(cell, block);
        return None;
    }
    if style_is_absolute_or_fixed(Some(before)) {
        return None;
    }
    build_pseudo_inline_for(before, n)
}

fn cell_pseudo_after(cell: BoxRef<'_>, n: Node<'_>, s: Option<StyleRef<'_>>) {
    let Some(after) = s.and_then(|s| s.after()) else {
        return;
    };
    if let Some(block) = build_pseudo_block_for(after, n) {
        append_child(cell, block);
        return;
    }
    if style_is_absolute_or_fixed(Some(after)) {
        return;
    }
    if let Some(generated) = build_pseudo_inline_for(after, n) {
        append_generated_after(cell, generated);
    }
}

fn has_text(b: BoxRef<'_>) -> bool {
    b.text().is_some_and(|t| !t.is_empty())
}

fn cell_append_run<'a>(
    cell: BoxRef<'a>,
    run: Option<BoxRef<'a>>,
    pending_before: &mut Option<BoxRef<'a>>,
) {
    let run = match pending_before.take() {
        Some(pending) => merge_prefix(Some(pending), run),
        None => run,
    };
    let Some(run) = run else {
        return;
    };
    if has_text(run) {
        append_child(cell, run);
    } else {
        BoxPool::free_tree(Some(run));
    }
}

fn cell_flush_pending<'a>(cell: BoxRef<'a>, pending_before: &mut Option<BoxRef<'a>>) {
    if let Some(pending) = pending_before.take() {
        append_child(cell, pending);
    }
}

fn build_cell<'a>(n: Node<'_>, styles: Styles) -> BoxRef<'a> {
    let cell = BoxPool::new_box(BOX_TABLE_CELL);
    cell.set_dom(dom_ptr_of(n));
    let s = styles.get(n);
    cell.set_style(s.map_or(core::ptr::null(), StyleRef::as_ptr));
    collect_box_bg_image(cell, s);
    let colspan = n
        .attr(c"colspan")
        .map_or(1, |v| parse_int(Some(v), 1, 1, 1000));
    let rowspan = n
        .attr(c"rowspan")
        .map_or(1, |v| parse_int(Some(v), 1, 1, 65534));
    cell.set_spans(colspan, rowspan);
    if let Some(s) = s {
        register_abs_pseudo(n, s.before());
        register_abs_pseudo(n, s.after());
    }
    let mut pending_before = cell_pseudo_before(cell, n, s);
    let mut c = n.first_child();
    while let Some(cur) = c {
        if is_inline_dom(cur, styles) {
            let start = cur;
            while let Some(x) = c.filter(|&x| continues_inline_run(x, styles)) {
                c = x.next_sibling();
            }
            cell_append_run(
                cell,
                build_inline_run(start, c, styles),
                &mut pending_before,
            );
        } else {
            cell_flush_pending(cell, &mut pending_before);
            if let Some(child) = build_block(cur, styles) {
                append_child(cell, child);
            }
            c = cur.next_sibling();
        }
    }
    cell_flush_pending(cell, &mut pending_before);
    cell_pseudo_after(cell, n, s);
    cell
}

fn append_table_col_hint(
    table: BoxRef<'_>,
    n: Node<'_>,
    styles: Styles,
    span: i32,
    fallback: Option<StyleRef<'_>>,
) {
    let span = if span < 1 { 1 } else { span };
    let s = if styles.as_ptr().is_null() {
        None
    } else {
        styles.get(n)
    };
    let s = s.or(fallback);
    table.push_table_col_hint(s.map_or(core::ptr::null(), StyleRef::as_ptr), span);
}

fn span_of(n: Node<'_>) -> i32 {
    parse_int(n.attr(c"span"), 1, 1, 1000)
}

fn collect_table_col_hints(table: BoxRef<'_>, n: Node<'_>, styles: Styles) {
    for c in siblings_from(n.first_child()) {
        let Some(tag) = element_tag(c) else {
            continue;
        };
        if tag == b"col" {
            append_table_col_hint(table, c, styles, span_of(c), None);
            continue;
        }
        if tag != b"colgroup" {
            continue;
        }
        let group_style = if styles.as_ptr().is_null() {
            None
        } else {
            styles.get(c)
        };
        let mut saw_col = false;
        for col in siblings_from(c.first_child()) {
            if !named(col, b"col") {
                continue;
            }
            append_table_col_hint(table, col, styles, span_of(col), group_style);
            saw_col = true;
        }
        if !saw_col {
            append_table_col_hint(table, c, styles, span_of(c), None);
        }
    }
}

fn build_table_caption<'a>(n: Node<'_>, styles: Styles) -> BoxRef<'a> {
    let caption = BoxPool::new_box(BOX_TABLE_CAPTION);
    caption.set_dom(dom_ptr_of(n));
    let s = styles.get(n);
    caption.set_style(s.map_or(core::ptr::null(), StyleRef::as_ptr));
    collect_box_bg_image(caption, s);
    if let Some(s) = s {
        register_abs_pseudo(n, s.before());
        register_abs_pseudo(n, s.after());
    }
    if let Some(inline_before) = cell_pseudo_before(caption, n, s) {
        append_child(caption, inline_before);
    }
    append_node_children(caption, n, styles);
    cell_pseudo_after(caption, n, s);
    caption
}

fn is_ascii_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn text_is_ws_only(n: Node<'_>) -> bool {
    n.text()
        .is_none_or(|t| t.to_bytes().iter().all(|&b| is_ascii_space(b)))
}

fn non_rendering_element(n: Node<'_>) -> bool {
    n.is_element() && n.name().is_some_and(tag_is_non_rendering)
}

fn build_anonymous_table_cell<'a>(n: Node<'_>, styles: Styles) -> Option<BoxRef<'a>> {
    let cell = BoxPool::new_box(BOX_TABLE_CELL);
    let mut any = false;
    let mut c = n.first_child();
    while let Some(cur) = c {
        if non_rendering_element(cur)
            || is_table_caption(cur, styles)
            || (cur.is_text() && text_is_ws_only(cur))
        {
            c = cur.next_sibling();
            continue;
        }
        if cur.is_element() {
            let cs = styles.get(cur);
            if cs.is_some() && style_is_none(cs) {
                c = cur.next_sibling();
                continue;
            }
            if style_is_contents(cs) {
                append_display_contents_children(cell, cur, styles);
                if cell.last_child().is_some() {
                    any = true;
                }
                c = cur.next_sibling();
                continue;
            }
        }
        if is_inline_dom(cur, styles) {
            let start = cur;
            c = cur.next_sibling();
            while let Some(x) = c.filter(|&x| !is_table_caption(x, styles)) {
                if non_rendering_element(x) {
                    c = x.next_sibling();
                    continue;
                }
                if x.is_element() && style_is_contents(styles.get(x)) {
                    break;
                }
                if !continues_inline_run(x, styles) {
                    break;
                }
                c = x.next_sibling();
            }
            if let Some(run) = build_inline_run(start, c, styles) {
                if has_text(run) {
                    append_child(cell, run);
                    any = true;
                } else {
                    BoxPool::free_tree(Some(run));
                }
            }
        } else {
            if let Some(child) = build_block(cur, styles) {
                append_child(cell, child);
                any = true;
            }
            c = cur.next_sibling();
        }
    }
    if !any {
        BoxPool::free_tree(Some(cell));
        return None;
    }
    Some(cell)
}

fn build_table_row<'a>(tr: Node<'_>, styles: Styles) -> BoxRef<'a> {
    let row = BoxPool::new_box(BOX_TABLE_ROW);
    row.set_dom(dom_ptr_of(tr));
    row.set_style(styles.get(tr).map_or(core::ptr::null(), StyleRef::as_ptr));
    for c in siblings_from(tr.first_child()) {
        if !is_cell_element(c, styles) || style_is_none(styles.get(c)) {
            continue;
        }
        append_child(row, build_cell(c, styles));
    }
    row
}

fn until<'a>(start: Option<Node<'a>>, end: Option<Node<'a>>) -> impl Iterator<Item = Node<'a>> {
    siblings_from(start).take_while(move |c| end.is_none_or(|e| e.as_ptr() != c.as_ptr()))
}

fn build_row_from_cell_run<'a>(
    start: Option<Node<'_>>,
    end: Option<Node<'_>>,
    dom: Option<Node<'_>>,
    styles: Styles,
) -> Option<BoxRef<'a>> {
    let row = BoxPool::new_box(BOX_TABLE_ROW);
    row.set_dom(dom.map_or(core::ptr::null(), dom_ptr_of));
    for c in until(start, end) {
        if !is_cell_element(c, styles) {
            continue;
        }
        let cs = styles.get(c);
        if cs.is_some() && (style_is_none(cs) || style_is_absolute_or_fixed(cs)) {
            continue;
        }
        append_child(row, build_cell(c, styles));
    }
    if row.first_child().is_some() {
        return Some(row);
    }
    BoxPool::free_tree(Some(row));
    None
}

pub fn build_table<'a>(n: Node<'_>, styles: Styles) -> BoxRef<'a> {
    let table = BoxPool::new_box(BOX_TABLE);
    table.set_dom(dom_ptr_of(n));
    table.set_style(styles.get(n).map_or(core::ptr::null(), StyleRef::as_ptr));
    collect_table_col_hints(table, n, styles);
    for c in siblings_from(n.first_child()) {
        if is_table_caption(c, styles) {
            append_child(table, build_table_caption(c, styles));
        }
    }
    let rows = collect_rows(n, styles);
    let has_direct_cells =
        rows.is_empty() && siblings_from(n.first_child()).any(|c| is_cell_element(c, styles));
    if rows.is_empty() && has_direct_cells {
        if let Some(row) = build_row_from_cell_run(n.first_child(), None, Some(n), styles) {
            append_child(table, row);
        }
    } else if rows.is_empty() {
        if let Some(cell) = build_anonymous_table_cell(n, styles) {
            let row = BoxPool::new_box(BOX_TABLE_ROW);
            row.set_dom(dom_ptr_of(n));
            append_child(row, cell);
            append_child(table, row);
        }
    } else {
        for r in rows {
            append_child(table, build_table_row(r, styles));
        }
    }
    table
}

pub fn build_anonymous_table<'a>(
    start: Option<Node<'_>>,
    end: Option<Node<'_>>,
    styles: Styles,
) -> Option<BoxRef<'a>> {
    let table = BoxPool::new_box(BOX_TABLE);
    for c in until(start, end) {
        if is_table_caption(c, styles) {
            append_child(table, build_table_caption(c, styles));
        }
    }
    let mut cells: Option<Node<'_>> = None;
    for c in until(start, end) {
        if is_cell_element(c, styles) {
            if cells.is_none() {
                cells = Some(c);
            }
            continue;
        }
        if cells.is_some() {
            if let Some(row) = build_row_from_cell_run(cells, Some(c), None, styles) {
                append_child(table, row);
            }
            cells = None;
        }
        if is_table_row(c, styles) {
            append_child(table, build_table_row(c, styles));
        } else if node_is_row_group(c, styles) {
            for r in siblings_from(c.first_child()) {
                if is_table_row(r, styles) {
                    append_child(table, build_table_row(r, styles));
                }
            }
        }
    }
    if cells.is_some()
        && let Some(row) = build_row_from_cell_run(cells, end, None, styles)
    {
        append_child(table, row);
    }
    if table.first_child().is_some() {
        return Some(table);
    }
    BoxPool::free_tree(Some(table));
    None
}
