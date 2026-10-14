//! Southstar — the table API: rows, sections, tBodies and cells, creating and deleting captions and sections, and inserting and deleting rows and cells.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::children;
use southstar_js_engine::{Scope, Value};

use crate::{
    Element, JsResult, descendants_named, ffi, first_descendant_named, named, named_ignore_case,
    wrap_all,
};

fn reflects_rows(node: Option<Element>) -> bool {
    named(node, b"textarea") || named(node, b"frameset")
}

fn section_rows(table: Element, section: &'static [u8]) -> impl Iterator<Item = Element> {
    children(table)
        .filter(move |&c| named(Some(c), section))
        .flat_map(|c| children(c).filter(|&r| named(Some(r), b"tr")))
}

pub fn rows(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let node = ffi::unwrap_node(this);
    if reflects_rows(node) {
        return ffi::int_attr_get(scope, this, c"rows");
    }
    let Some(table) = node else {
        return Ok(scope.new_array());
    };
    let body_rows = children(table).flat_map(|c| {
        let direct = named(Some(c), b"tr").then_some(c);
        let in_body = named(Some(c), b"tbody")
            .then(|| children(c).filter(|&r| named(Some(r), b"tr")))
            .into_iter()
            .flatten();
        direct.into_iter().chain(in_body)
    });
    let all = section_rows(table, b"thead")
        .chain(body_rows)
        .chain(section_rows(table, b"tfoot"));
    wrap_all(scope, all)
}

pub fn set_rows(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    if !reflects_rows(ffi::unwrap_node(this)) {
        return Ok(());
    }
    ffi::int_attr_set(scope, this, value, c"rows").map(drop)
}

fn section(scope: &mut Scope<'_>, this: &Value, tag: &[u8]) -> JsResult {
    let found =
        ffi::unwrap_node(this).and_then(|table| children(table).find(|&c| named(Some(c), tag)));
    match found {
        Some(section) => Ok(ffi::make_node(scope, Some(section))),
        None => Ok(Value::null()),
    }
}

pub fn caption(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    section(scope, this, b"caption")
}

pub fn thead(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    section(scope, this, b"thead")
}

pub fn tfoot(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    section(scope, this, b"tfoot")
}

pub fn tbodies(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    match ffi::unwrap_node(this) {
        Some(table) => wrap_all(scope, children(table).filter(|&c| named(Some(c), b"tbody"))),
        None => Ok(scope.new_array()),
    }
}

pub fn cells(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    match ffi::unwrap_node(this) {
        Some(row) => wrap_all(
            scope,
            children(row).filter(|&c| named(Some(c), b"td") || named(Some(c), b"th")),
        ),
        None => Ok(scope.new_array()),
    }
}

fn new_element(tag: &[u8]) -> Element {
    southstar_dom::node::new_element(tag)
}

fn table_changed(scope: &Scope<'_>) {
    if let Some(js) = ffi::js_of(scope) {
        ffi::mark_mutated(js);
        ffi::qcache_invalidate(js);
    }
}

fn create_section(scope: &mut Scope<'_>, this: &Value, tag: &[u8], before_body: bool) -> JsResult {
    let Some(table) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if let Some(existing) = first_descendant_named(table, tag) {
        return Ok(ffi::make_node(scope, Some(existing)));
    }
    let created = new_element(tag);
    let body = before_body
        .then(|| first_descendant_named(table, b"tbody"))
        .flatten()
        .filter(|body| body.parent() == Some(table));
    match body {
        Some(body) => ffi::insert_before_single(ffi::js_of(scope), table, created, body),
        None => table.append(created),
    }
    table_changed(scope);
    Ok(ffi::make_node(scope, Some(created)))
}

pub fn create_thead(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    create_section(scope, this, b"thead", true)
}

pub fn create_tbody(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    create_section(scope, this, b"tbody", false)
}

pub fn create_tfoot(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    create_section(scope, this, b"tfoot", false)
}

pub fn create_caption(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(table) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if let Some(existing) = first_descendant_named(table, b"caption") {
        return Ok(ffi::make_node(scope, Some(existing)));
    }
    let created = new_element(b"caption");
    match table.first_child() {
        Some(first) => ffi::insert_before_single(ffi::js_of(scope), table, created, first),
        None => table.append(created),
    }
    table_changed(scope);
    Ok(ffi::make_node(scope, Some(created)))
}

fn delete_section(scope: &mut Scope<'_>, this: &Value, tag: &[u8]) -> JsResult {
    let existing = ffi::unwrap_node(this).and_then(|table| first_descendant_named(table, tag));
    if let Some(existing) = existing {
        existing.detach();
        ffi::orphan(ffi::js_of(scope), existing);
        table_changed(scope);
    }
    Ok(Value::undefined())
}

pub fn delete_thead(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    delete_section(scope, this, b"thead")
}

pub fn delete_tfoot(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    delete_section(scope, this, b"tfoot")
}

pub fn delete_caption(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    delete_section(scope, this, b"caption")
}

fn insertion_index(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<i32> {
    match args.first() {
        Some(index) => scope.to_int32(index),
        None => Ok(-1),
    }
}

fn record_insert(scope: &Scope<'_>, node: Element) {
    if let Some(js) = ffi::js_of(scope) {
        ffi::record_child_change(
            js,
            node.parent(),
            Some(node),
            None,
            node.prev_sibling(),
            node.next_sibling(),
        );
        ffi::mark_mutated(js);
    }
}

fn remove_recorded(scope: &Scope<'_>, node: Element) {
    let js = ffi::js_of(scope);
    let parent = node.parent();
    let previous = node.prev_sibling();
    let next = node.next_sibling();
    node.detach();
    ffi::orphan(js, node);
    if let Some(js) = js {
        ffi::record_child_change(js, parent, None, Some(node), previous, next);
        ffi::mark_mutated(js);
    }
}

fn remove_at(scope: &Scope<'_>, items: &[Element], index: i32) {
    let index = if index < 0 {
        items.len() as i32 - 1
    } else {
        index
    };
    if let Some(&item) = usize::try_from(index).ok().and_then(|i| items.get(i)) {
        remove_recorded(scope, item);
    }
}

fn row_parent(table: Element, rows: &[Element]) -> Element {
    if let Some(parent) = rows.last().and_then(|row| row.parent()) {
        return parent;
    }
    if rows.is_empty() && named(Some(table), b"table") {
        if let Some(body) = children(table).filter(|&c| named(Some(c), b"tbody")).last() {
            return body;
        }
        let body = new_element(b"tbody");
        table.append(body);
        return body;
    }
    table
}

pub fn insert_row(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let index = insertion_index(scope, args)?;
    let Some(table) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    let rows = descendants_named(table, b"tr");
    let row = new_element(b"tr");
    let reference = usize::try_from(index)
        .ok()
        .and_then(|i| rows.get(i).copied());
    match reference {
        Some(reference) => match reference.parent() {
            Some(parent) => ffi::insert_before_single(ffi::js_of(scope), parent, row, reference),
            None => table.append(row),
        },
        None => row_parent(table, &rows).append(row),
    }
    record_insert(scope, row);
    Ok(ffi::make_node(scope, Some(row)))
}

pub fn delete_row(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(table), Some(index)) = (ffi::unwrap_node(this), args.first()) else {
        return Ok(Value::undefined());
    };
    let index = scope.to_int32(index)?;
    let rows = descendants_named(table, b"tr");
    remove_at(scope, &rows, index);
    Ok(Value::undefined())
}

fn row_cells(row: Element) -> Vec<Element> {
    children(row)
        .filter(|&c| named_ignore_case(c, b"td") || named_ignore_case(c, b"th"))
        .collect()
}

pub fn insert_cell(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let index = insertion_index(scope, args)?;
    let Some(row) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    let cells = row_cells(row);
    let cell = new_element(b"td");
    match usize::try_from(index)
        .ok()
        .and_then(|i| cells.get(i).copied())
    {
        Some(reference) => ffi::insert_before_single(ffi::js_of(scope), row, cell, reference),
        None => row.append(cell),
    }
    record_insert(scope, cell);
    Ok(ffi::make_node(scope, Some(cell)))
}

pub fn delete_cell(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(row), Some(index)) = (ffi::unwrap_node(this), args.first()) else {
        return Ok(Value::undefined());
    };
    let index = scope.to_int32(index)?;
    let cells = row_cells(row);
    remove_at(scope, &cells, index);
    Ok(Value::undefined())
}
