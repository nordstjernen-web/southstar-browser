//! Southstar — deferred freeing of replaced node strings: on a thread that runs page script, a name, text or attribute value that is replaced stays allocated until the host says no script is running, so a borrow taken before script ran still points at memory.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};

use crate::data::{AttrName, Name};

pub(crate) enum Retired {
    Bytes(Vec<u8>),
    Name(Name),
    AttrName(AttrName),
}

impl Retired {
    fn free(self) {
        match self {
            Retired::Bytes(bytes) => drop(bytes),
            Retired::Name(name) => drop(name),
            Retired::AttrName(name) => drop(name),
        }
    }
}

thread_local! {
    static DEFERRED: Cell<bool> = const { Cell::new(false) };
    static RETIRED: RefCell<Vec<Retired>> = const { RefCell::new(Vec::new()) };
}

pub fn defer_on_this_thread() {
    DEFERRED.with(|deferred| deferred.set(true));
}

pub fn free_retired() {
    let retired = RETIRED.try_with(|retired| retired.take());
    retired
        .unwrap_or_default()
        .into_iter()
        .for_each(Retired::free);
}

pub(crate) fn retire(item: Retired) {
    if DEFERRED.try_with(Cell::get) == Ok(true) {
        let _ = RETIRED.try_with(|retired| retired.borrow_mut().push(item));
    }
}

pub(crate) fn retire_bytes(bytes: Vec<u8>) {
    if bytes.capacity() != 0 {
        retire(Retired::Bytes(bytes));
    }
}

pub(crate) fn retire_name(name: Option<Name>) {
    if let Some(name @ Name::Owned(_)) = name {
        retire(Retired::Name(name));
    }
}
