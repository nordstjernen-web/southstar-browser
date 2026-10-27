//! Southstar — the drawing state behind each canvas element: its cairo surface and context, sized from the element's attributes, kept per page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use southstar_dom::WeakNode;

use crate::ffi::state::{self, CanvasState, Js, Node};

const DEFAULT_WIDTH: i32 = 300;
const DEFAULT_HEIGHT: i32 = 150;
const MAX_DIMENSION: i32 = 8192;

type Table = HashMap<WeakNode, Rc<CanvasState>>;

thread_local! {
    static STATES: RefCell<HashMap<usize, Table>> = RefCell::new(HashMap::new());
}

pub(crate) fn dimension(el: Node, name: &str, default: i32) -> i32 {
    let Some(value) = state::element_attr(el, name).filter(|v| !v.is_empty()) else {
        return default;
    };
    let n = state::parse_int(&value, default, 0, MAX_DIMENSION);
    if n < 1 { default } else { n }
}

pub(crate) fn state_for(js: Js, el: Node) -> Option<Rc<CanvasState>> {
    if js.is_null() {
        return None;
    }
    let key = el.key()?;
    let w = dimension(el, "width", DEFAULT_WIDTH);
    let h = dimension(el, "height", DEFAULT_HEIGHT);
    if let Some(st) = lookup(js, el) {
        if st.size() != (w, h) {
            st.reset(w, h);
        }
        return Some(st);
    }
    let st = Rc::new(CanvasState::new(w, h));
    let st = STATES.with(|states| {
        let mut states = states.borrow_mut();
        let table = states.entry(js.addr()).or_default();
        Rc::clone(table.entry(key).or_insert(st))
    });
    Some(st)
}

pub(crate) fn lookup(js: Js, el: Node) -> Option<Rc<CanvasState>> {
    let key = el.key()?;
    STATES.with(|states| {
        let states = states.borrow();
        states.get(&js.addr())?.get(&key).cloned()
    })
}

pub(crate) fn teardown(js: Js) {
    let table = STATES.with(|states| states.borrow_mut().remove(&js.addr()));
    drop(table);
}
