//! Southstar — var() and if() substitution: an element's chain of variable maps, the registered @property rules consulted for initial values and style() queries, and the variable names an element's map exposes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::OnceCell;
use core::ffi::CStr;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::rc::Rc;

use southstar_css_prop_syntax::{self as prop_syntax, SyntaxDef};
use southstar_dom::Node;

use super::custom_props::QueryContext;
use super::declarations::lookup_key;
use super::registry::RegisteredProps;
use crate::vars::{self, Lookup, Registration};

pub(super) type VarTable = HashMap<CString, CString>;

pub(super) struct VarMap {
    own: VarTable,
    computed: OnceCell<VarTable>,
    parent: Option<Rc<VarMap>>,
    names: OnceCell<Vec<CString>>,
}

pub(super) fn new_map(own: VarTable, parent: Option<Rc<VarMap>>) -> Rc<VarMap> {
    Rc::new(VarMap {
        own,
        computed: OnceCell::new(),
        parent,
        names: OnceCell::new(),
    })
}

impl VarMap {
    pub(super) fn table(&self) -> &VarTable {
        self.computed.get().unwrap_or(&self.own)
    }

    pub(super) fn computed(&self) -> bool {
        self.computed.get().is_some()
    }

    pub(super) fn set_computed(&self, table: VarTable) {
        let _ = self.computed.set(table);
    }
}

impl Drop for VarMap {
    fn drop(&mut self) {
        let mut parent = self.parent.take();
        while let Some(map) = parent {
            parent = Rc::try_unwrap(map)
                .ok()
                .and_then(|mut map| map.parent.take());
        }
    }
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

struct Chain<'a> {
    map: Option<&'a VarMap>,
    registered: Option<&'a RegisteredProps>,
    parent: Option<&'a VarMap>,
    node: Option<Node<'a>>,
    query: Option<&'a QueryContext<'a>>,
}

pub(super) fn chain_lookup_text<'a>(mut map: Option<&'a VarMap>, name: &[u8]) -> Option<&'a CStr> {
    let key = c_string(name);
    while let Some(m) = map {
        if let Some(value) = m.table().get(&key) {
            return Some(value);
        }
        map = m.parent.as_deref();
    }
    None
}

pub(super) fn chain_lookup<'a>(map: Option<&'a VarMap>, name: &[u8]) -> Option<&'a [u8]> {
    chain_lookup_text(map, name).map(CStr::to_bytes)
}

impl Lookup for Chain<'_> {
    fn value(&self, name: &[u8]) -> Option<Cow<'_, [u8]>> {
        chain_lookup(self.map, name).map(Cow::Borrowed)
    }

    fn registered_initial(&self, name: &[u8]) -> Option<Option<&[u8]>> {
        self.registered?.get(lookup_key(name))?.registered_initial()
    }

    fn inherited(&self, name: &[u8]) -> Option<Cow<'_, [u8]>> {
        chain_lookup(self.parent, name).map(Cow::Borrowed)
    }

    fn registration(&self, name: &[u8]) -> Option<Registration<'_>> {
        let rule = self.registered?.get(lookup_key(name))?;
        Some(Registration {
            syntax: rule.typed_syntax(),
            inherits: rule.inherits(),
            initial: rule.initial(),
        })
    }

    fn compute(&self, syntax: &SyntaxDef, value: &[u8]) -> Option<Vec<u8>> {
        match self.query {
            Some(query) => query.compute(syntax, value),
            None => prop_syntax::compute(syntax, value, None),
        }
    }

    fn node(&self) -> Option<Node<'_>> {
        self.node
    }
}

pub(super) struct Scope<'a> {
    pub(super) parent: Option<&'a VarMap>,
    pub(super) node: Node<'a>,
    pub(super) query: &'a QueryContext<'a>,
}

pub(super) fn substitute_in(
    value: &[u8],
    map: Option<&VarMap>,
    registered: Option<&RegisteredProps>,
) -> Option<Vec<u8>> {
    let chain = Chain {
        map,
        registered,
        parent: None,
        node: None,
        query: None,
    };
    let lookup: Option<&dyn Lookup> = Some(&chain);
    vars::substitute(value, lookup.filter(|_| map.is_some()), 0)
}

pub(super) fn substitute_scoped(
    value: &[u8],
    map: Option<&VarMap>,
    registered: &RegisteredProps,
    scope: &Scope<'_>,
) -> Option<Vec<u8>> {
    let chain = Chain {
        map,
        registered: Some(registered),
        parent: scope.parent,
        node: Some(scope.node),
        query: Some(scope.query),
    };
    vars::substitute(value, Some(&chain), 0)
}

pub(super) fn chain_names(map: &VarMap) -> Vec<&CStr> {
    map.names
        .get_or_init(|| names_of(map))
        .iter()
        .map(CString::as_c_str)
        .collect()
}

fn names_of(map: &VarMap) -> Vec<CString> {
    let mut names = Vec::new();
    let mut seen: HashSet<&[u8]> = HashSet::new();
    let mut current = Some(map);
    while let Some(m) = current {
        for (name, value) in m.table() {
            if !seen.insert(name.to_bytes()) {
                continue;
            }
            if !value.to_bytes().eq_ignore_ascii_case(b"initial") {
                names.push(name.clone());
            }
        }
        current = m.parent.as_deref();
    }
    names.sort();
    names
}
