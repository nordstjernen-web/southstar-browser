//! Southstar — pre-insertion and pre-replacement validity, and the document checks of a batch of ParentNode arguments.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Kind, children};
use southstar_js_engine::{Scope, Value};

use crate::{
    Element, JsResult, ffi, hierarchy_error, is_document, is_fragment, not_found_error, within,
};

const ONE_ELEMENT: &str = "document may have only one element child";
const DOCTYPE_FIRST: &str = "doctype must precede the document element";

fn has_child_kind(parent: Element, kind: Kind, except: Option<Element>) -> bool {
    children(parent).any(|c| c.kind() == kind && Some(c) != except)
}

fn doctype_follows(child: Element) -> bool {
    core::iter::successors(child.next_sibling(), |c| c.next_sibling())
        .any(|c| c.kind() == Kind::Doctype)
}

fn element_precedes(child: Element) -> bool {
    core::iter::successors(child.prev_sibling(), |c| c.prev_sibling())
        .any(|c| c.kind() == Kind::Element)
}

fn check_parent(scope: &mut Scope<'_>, parent: Element) -> JsResult<()> {
    let parent_doc = is_document(parent);
    if !(parent_doc || is_fragment(parent) || parent.kind() == Kind::Element) {
        return Err(hierarchy_error(scope, "parent node cannot have children"));
    }
    Ok(())
}

fn check_node_kind(scope: &mut Scope<'_>, parent: Element, node: Element) -> JsResult<()> {
    let parent_doc = is_document(parent);
    if is_document(node) {
        return Err(hierarchy_error(scope, "a Document cannot be inserted"));
    }
    if (node.kind() == Kind::Text && parent_doc) || (node.kind() == Kind::Doctype && !parent_doc) {
        return Err(hierarchy_error(scope, "invalid node type for this parent"));
    }
    Ok(())
}

fn fragment_elements(scope: &mut Scope<'_>, node: Element) -> JsResult<usize> {
    let mut elements = 0;
    let mut has_text = false;
    for c in children(node) {
        match c.kind() {
            Kind::Element => elements += 1,
            Kind::Text => has_text = true,
            _ => {}
        }
    }
    if has_text || elements > 1 {
        return Err(hierarchy_error(
            scope,
            "invalid fragment contents for a document",
        ));
    }
    Ok(elements)
}

pub(crate) fn pre_insert(
    scope: &mut Scope<'_>,
    parent: Element,
    node: Element,
    child: Option<Element>,
    child_is_null: bool,
) -> JsResult<()> {
    check_parent(scope, parent)?;
    if parent == node || (node.first_child().is_some() && within(parent, node)) {
        return Err(hierarchy_error(
            scope,
            "the new child is an inclusive ancestor of the parent",
        ));
    }
    if !child_is_null && child.is_none_or(|c| c.parent() != Some(parent)) {
        return Err(not_found_error(
            scope,
            "the reference child is not a child of this node",
        ));
    }
    check_node_kind(scope, parent, node)?;
    if !is_document(parent) {
        return Ok(());
    }
    let reference = if child_is_null { None } else { child };
    let element_blocked = |parent: Element| {
        has_child_kind(parent, Kind::Element, None)
            || reference.is_some_and(|c| c.kind() == Kind::Doctype || doctype_follows(c))
    };
    if is_fragment(node) {
        if fragment_elements(scope, node)? == 1 && element_blocked(parent) {
            return Err(hierarchy_error(scope, ONE_ELEMENT));
        }
    } else if node.kind() == Kind::Element {
        if element_blocked(parent) {
            return Err(hierarchy_error(scope, ONE_ELEMENT));
        }
    } else if node.kind() == Kind::Doctype
        && (has_child_kind(parent, Kind::Doctype, None)
            || reference.is_some_and(element_precedes)
            || (child_is_null && has_child_kind(parent, Kind::Element, None)))
    {
        return Err(hierarchy_error(scope, DOCTYPE_FIRST));
    }
    Ok(())
}

pub(crate) fn pre_replace(
    scope: &mut Scope<'_>,
    parent: Element,
    node: Element,
    child: Element,
) -> JsResult<()> {
    check_parent(scope, parent)?;
    if within(parent, node) {
        return Err(hierarchy_error(
            scope,
            "the new child is an inclusive ancestor of the parent",
        ));
    }
    if child.parent() != Some(parent) {
        return Err(not_found_error(
            scope,
            "the child to replace is not a child of this node",
        ));
    }
    check_node_kind(scope, parent, node)?;
    if !is_document(parent) {
        return Ok(());
    }
    let element_blocked =
        has_child_kind(parent, Kind::Element, Some(child)) || doctype_follows(child);
    if is_fragment(node) {
        if fragment_elements(scope, node)? == 1 && element_blocked {
            return Err(hierarchy_error(scope, ONE_ELEMENT));
        }
    } else if node.kind() == Kind::Element {
        if element_blocked {
            return Err(hierarchy_error(scope, ONE_ELEMENT));
        }
    } else if node.kind() == Kind::Doctype
        && (has_child_kind(parent, Kind::Doctype, Some(child)) || element_precedes(child))
    {
        return Err(hierarchy_error(scope, DOCTYPE_FIRST));
    }
    Ok(())
}

pub(crate) fn document_batch(
    scope: &mut Scope<'_>,
    parent: Element,
    args: &[Value],
) -> JsResult<()> {
    if !is_document(parent) {
        return Ok(());
    }
    let mut new_elements = 0;
    let mut has_text = false;
    for arg in args {
        let Some(n) = ffi::unwrap_node(arg) else {
            has_text = true;
            continue;
        };
        if n.kind() == Kind::Element {
            new_elements += 1;
        } else if n.kind() == Kind::Text {
            has_text = true;
        } else if is_fragment(n) {
            for c in children(n) {
                match c.kind() {
                    Kind::Element => new_elements += 1,
                    Kind::Text => has_text = true,
                    _ => {}
                }
            }
        }
    }
    if has_text {
        return Err(hierarchy_error(
            scope,
            "Nodes of type 'Text' may not be inserted inside a Document.",
        ));
    }
    let existing = children(parent)
        .filter(|c| c.kind() == Kind::Element)
        .count();
    if existing + new_elements > 1 {
        return Err(hierarchy_error(
            scope,
            "A Document may contain at most one element child.",
        ));
    }
    Ok(())
}

pub(crate) fn document_arguments(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<()> {
    let mut elements = 0;
    let mut doctypes = 0;
    let mut seen_element = false;
    let mut count = |scope: &mut Scope<'_>, kind: Kind| -> JsResult<()> {
        match kind {
            Kind::Text => Err(hierarchy_error(
                scope,
                "Document may not have Text nodes as direct children",
            )),
            Kind::Doctype => {
                doctypes += 1;
                if seen_element || doctypes > 1 {
                    return Err(hierarchy_error(scope, DOCTYPE_FIRST));
                }
                Ok(())
            }
            Kind::Element => {
                seen_element = true;
                elements += 1;
                if elements > 1 {
                    return Err(hierarchy_error(scope, ONE_ELEMENT));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    };
    for arg in args {
        let Some(child) = ffi::unwrap_node(arg).filter(|c| c.kind() != Kind::Text) else {
            return Err(hierarchy_error(
                scope,
                "Document may not have Text nodes as direct children",
            ));
        };
        if crate::is_detached_document(child) {
            for c in children(child) {
                count(scope, c.kind())?;
            }
        } else {
            count(scope, child.kind())?;
        }
    }
    Ok(())
}
