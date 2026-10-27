//! Southstar — the Rust-ABI entry points of the binding crates the document bindings call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) use crate::links::nsr_document_adopt_node;
pub(crate) use crate::links::nsr_document_createAttribute;
pub(crate) use crate::links::nsr_document_createAttributeNS;
pub(crate) use crate::links::nsr_document_createCDATASection;
pub(crate) use crate::links::nsr_document_createComment;
pub(crate) use crate::links::nsr_document_createDocumentFragment;
pub(crate) use crate::links::nsr_document_createElement;
pub(crate) use crate::links::nsr_document_createElementNS;
pub(crate) use crate::links::nsr_document_createEvent;
pub(crate) use crate::links::nsr_document_createProcessingInstruction;
pub(crate) use crate::links::nsr_document_createTextNode;
pub(crate) use crate::links::nsr_document_implementation;
pub(crate) use crate::links::nsr_document_import_node;
pub(crate) use crate::links::nsr_document_install_funcs;
pub(crate) use southstar_js_collections::nsr_element_getElementsByClassName;
pub(crate) use southstar_js_collections::nsr_element_getElementsByTagName;
pub(crate) use southstar_js_collections::nsr_element_querySelector;
pub(crate) use southstar_js_collections::nsr_element_querySelectorAll;
pub(crate) use southstar_js_collections::nsr_make_live;
pub(crate) use southstar_js_events::nsr_dom_exception;
pub(crate) use southstar_js_focus::nsr_document_has_focus;
pub(crate) use southstar_js_traversal::nsr_document_create_node_iterator;
pub(crate) use southstar_js_traversal::nsr_document_create_range;
pub(crate) use southstar_js_traversal::nsr_document_create_tree_walker;
pub(crate) use southstar_js_traversal::nsr_window_get_selection;
pub(crate) use southstar_js_window::nsr_document_expose_legacy_named;
pub(crate) use southstar_js_window::nsr_window_open_method;
