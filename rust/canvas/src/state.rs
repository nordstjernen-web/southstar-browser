//! Southstar — the drawing state behind each canvas element: its cairo surface and context, sized from the element's attributes, kept per page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ffi::state::{self, CanvasState, Js, Node};

const DEFAULT_WIDTH: i32 = 300;
const DEFAULT_HEIGHT: i32 = 150;
const MAX_DIMENSION: i32 = 8192;

type Table = HashMap<usize, Box<CanvasState>>;

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

pub(crate) fn state_for(js: Js, el: Node) -> Option<*mut CanvasState> {
    if js.is_null() || el.is_null() {
        return None;
    }
    let w = dimension(el, "width", DEFAULT_WIDTH);
    let h = dimension(el, "height", DEFAULT_HEIGHT);
    let existing = lookup(js, el);
    if let Some(st) = existing {
        if state::size(st) != (w, h) {
            state::reset(st, w, h);
        }
        return Some(st);
    }
    let st = state::new_state(w, h);
    let ptr = STATES.with(|states| {
        let mut states = states.borrow_mut();
        let table = states.entry(js.addr()).or_default();
        let boxed = table.entry(el.addr()).or_insert(st);
        &mut **boxed as *mut CanvasState
    });
    Some(ptr)
}

pub(crate) fn lookup(js: Js, el: Node) -> Option<*mut CanvasState> {
    STATES.with(|states| {
        let mut states = states.borrow_mut();
        let boxed = states.get_mut(&js.addr())?.get_mut(&el.addr())?;
        Some(&mut **boxed as *mut CanvasState)
    })
}

pub(crate) fn teardown(js: Js) {
    let table = STATES.with(|states| states.borrow_mut().remove(&js.addr()));
    drop(table);
}
