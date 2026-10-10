//! Southstar — rendered text and character data: innerText and outerText, the CharacterData and Text methods, wholeText and normalize.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod cdata;
mod ffi;
mod inner_text;
mod rendered;

use southstar_dom::Node;
use southstar_js_engine::Value;

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const MAX_DEPTH: i32 = 512;
pub(crate) const FLAG_CDATA: u32 = 1 << 10;

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn name_is(node: Node<'_>, tag: &str) -> bool {
    node.name()
        .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(tag.as_bytes()))
}

pub(crate) fn name_in(node: Node<'_>, set: &[&str]) -> bool {
    set.iter().any(|tag| name_is(node, tag))
}

pub(crate) fn element_named(node: Node<'_>, tag: &str) -> bool {
    node.element_name() == Some(tag.as_bytes())
}

pub(crate) fn unsupported(node: Option<Node<'_>>) -> bool {
    let Some(node) = node.filter(|n| n.is_element()) else {
        return true;
    };
    node.flags() & (southstar_dom::FLAG_SVG_NS | southstar_dom::FLAG_FOREIGN_NS) != 0
        || element_named(node, "svg")
        || element_named(node, "math")
}
