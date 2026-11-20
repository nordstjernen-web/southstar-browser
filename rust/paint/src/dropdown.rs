//! Southstar — open drop-down lists recorded while a page paints and drawn again over the finished page, as a native popup sits above every positioned box.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_void};

use southstar_cairo::{ContextRef, Matrix};
use southstar_layout::BoxRef;

struct Dropdown {
    b: BoxRef<'static>,
    ctm: Matrix,
    rect: (f64, f64, f64, f64),
}

struct Recorder {
    target: *mut c_void,
    lists: Vec<Dropdown>,
}

thread_local! {
    static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) };
    static REPLAYING: Cell<bool> = const { Cell::new(false) };
}

pub fn begin(cr: ContextRef) {
    RECORDER.with(|r| {
        *r.borrow_mut() = Some(Recorder {
            target: cr.as_raw(),
            lists: Vec::new(),
        });
    });
}

pub fn record(cr: ContextRef, b: BoxRef<'_>, rect: (f64, f64, f64, f64)) {
    if REPLAYING.with(Cell::get) {
        return;
    }
    RECORDER.with(|r| {
        if let Some(rec) = r.borrow_mut().as_mut()
            && rec.target == cr.as_raw()
        {
            rec.lists.push(Dropdown {
                b: b.into_static(),
                ctm: cr.matrix(),
                rect,
            });
        }
    });
}

pub fn finish(cr: ContextRef, highlight: Option<&CStr>) {
    let Some(rec) = RECORDER.with(|r| r.borrow_mut().take()) else {
        return;
    };
    REPLAYING.with(|p| p.set(true));
    for d in &rec.lists {
        let (x, y, w, h) = d.rect;
        cr.save();
        cr.set_matrix(&d.ctm);
        cr.rectangle(x, y, w, h);
        cr.clip();
        crate::inline::paint_inline(cr, d.b, highlight);
        cr.restore();
    }
    REPLAYING.with(|p| p.set(false));
}
