//! Southstar — selector lists for DOM queries: parsing a querySelector() argument, matching its selectors against elements, the indexable key of a lone selector and the match context a query runs in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Node, WeakNode};

use super::matcher::{
    replace_focus_node, replace_match_scope, restore_focus_node, restore_match_scope,
};
use super::selector::SelectorGroup;
use super::selector_view::SelectorRef;
use crate::{element_state, matcher, selector};

pub struct SelectorList(SelectorGroup);

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
        Some(SelectorList(SelectorGroup::new(&list)))
    }

    pub fn parse_lenient(text: &[u8]) -> Option<SelectorList> {
        let list = selector::parse_list(text);
        let parsed = SelectorList(SelectorGroup::new(&list));
        (!parsed.0.selectors().is_empty()).then_some(parsed)
    }

    pub fn matches_any(&self, el: Node<'_>) -> bool {
        self.0
            .refs()
            .any(|sel| sel.is_some_and(|sel| matcher::matches(sel, el)))
    }

    pub fn single(&self) -> Option<Selector<'_>> {
        let mut refs = self.0.refs();
        match (refs.next(), refs.next()) {
            (Some(only), None) => only.map(Selector),
            _ => None,
        }
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
    prev_scope: Option<WeakNode>,
    prev_focus: Option<WeakNode>,
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
        let prev_scope = replace_match_scope(Some(scope));
        element_state::set_target_fragment(fragment);
        let prev_focus = replace_focus_node(focus);
        MatchContext {
            prev_scope,
            prev_focus,
            batch,
        }
    }
}

impl Drop for MatchContext {
    fn drop(&mut self) {
        restore_focus_node(self.prev_focus);
        restore_match_scope(self.prev_scope);
        element_state::set_target_fragment(None);
        if self.batch {
            matcher::batch_end();
        }
    }
}
