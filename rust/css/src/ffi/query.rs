//! Southstar — selector lists for DOM queries: parsing a querySelector() argument, matching its selectors against elements, the indexable key of a lone selector and the match context a query runs in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr::NonNull;
use core::slice;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GPtrArray};

use super::matcher::{ns_css_set_focus_node, ns_css_set_match_scope};
use super::selector::group_to_c;
use super::selector_view::SelectorRef;
use crate::{element_state, matcher, selector};

pub struct SelectorList(NonNull<GPtrArray>);

#[derive(Clone, Copy)]
pub struct Selector<'a>(SelectorRef<'a>);

#[derive(Clone, Copy)]
pub struct QueryKey<'a> {
    pub id: Option<&'a CStr>,
    pub class: Option<&'a CStr>,
    pub type_name: Option<&'a CStr>,
}

impl SelectorList {
    pub fn parse(text: &[u8]) -> Option<SelectorList> {
        let (list, valid) = selector::parse_list_checked(Some(text));
        if !valid {
            return None;
        }
        NonNull::new(group_to_c(&list)).map(SelectorList)
    }

    fn pointers(&self) -> &[*mut c_void] {
        let array = unsafe { self.0.as_ref() };
        if array.pdata.is_null() || array.len == 0 {
            return &[];
        }
        unsafe { slice::from_raw_parts(array.pdata.cast::<*mut c_void>(), array.len as usize) }
    }

    pub fn matches_any(&self, el: Node<'_>) -> bool {
        self.pointers().iter().any(|&sel| {
            unsafe { SelectorRef::from_ptr(sel) }.is_some_and(|sel| matcher::matches(sel, el))
        })
    }

    pub fn single(&self) -> Option<Selector<'_>> {
        match self.pointers() {
            [only] => unsafe { SelectorRef::from_ptr(*only) }.map(Selector),
            _ => None,
        }
    }
}

impl Drop for SelectorList {
    fn drop(&mut self) {
        unsafe { glib::g_ptr_array_free(self.0.as_ptr(), glib::TRUE) };
    }
}

impl<'a> Selector<'a> {
    pub fn matches(self, el: Node<'_>) -> bool {
        matcher::matches(self.0, el)
    }

    pub fn key(self) -> Option<QueryKey<'a>> {
        if self.0.pseudo_element() != 0 || self.0.len() == 0 {
            return None;
        }
        let key = self.0.compound(self.0.len() - 1)?;
        if key.never_match() {
            return None;
        }
        Some(QueryKey {
            id: key.id_text().filter(|id| !id.is_empty()),
            class: key.first_class_text().filter(|class| !class.is_empty()),
            type_name: key.type_text().filter(|name| name.to_bytes() != b"*"),
        })
    }
}

pub struct MatchContext {
    prev_scope: *const NsNode,
    prev_focus: *const NsNode,
    batch: bool,
}

impl MatchContext {
    pub fn enter(
        scope: Node<'_>,
        focus: Option<Node<'_>>,
        fragment: Option<&[u8]>,
        batch: bool,
    ) -> MatchContext {
        if batch {
            matcher::batch_begin();
        }
        let prev_scope = ns_css_set_match_scope(scope.as_ptr());
        element_state::set_target_fragment(fragment);
        let prev_focus = ns_css_set_focus_node(Node::ptr_or_null(focus));
        MatchContext {
            prev_scope,
            prev_focus,
            batch,
        }
    }
}

impl Drop for MatchContext {
    fn drop(&mut self) {
        ns_css_set_focus_node(self.prev_focus);
        ns_css_set_match_scope(self.prev_scope);
        element_state::set_target_fragment(None);
        if self.batch {
            matcher::batch_end();
        }
    }
}
