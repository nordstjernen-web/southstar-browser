//! Southstar — the parser hold: a parser-blocking script of the initial parse sees only the part of the document parsed before it, the rest put back node by node as the parse reaches it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::cell::Cell;
use std::collections::HashSet;

use southstar_dom::{Kind, MAX_DEPTH, Node};

use crate::ffi::{self, Js, hidden_child, is_named};
use crate::{Schedule, Task, fetch, page};

#[derive(Clone, Copy)]
struct Held {
    parent: Option<usize>,
    node: Option<usize>,
}

pub(crate) struct Hold {
    held: Vec<Held>,
    next: usize,
    members: HashSet<usize>,
}

thread_local! {
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
}

fn node_at<'a>(addr: usize) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(addr as *const southstar_dom::NsNode) }
}

pub(crate) fn released(count: usize) {
    ACTIVE.with(|active| active.set(active.get().saturating_sub(count)));
}

pub(crate) fn holding(js: Js) -> bool {
    ACTIVE.with(Cell::get) > 0
        && page::peek_page(js, |page| !page.holds.is_empty()).unwrap_or(false)
}

pub(crate) fn forget(js: Js, n: usize) {
    if ACTIVE.with(Cell::get) == 0 {
        return;
    }
    page::with_existing(js, |page| {
        for hold in page.holds.iter_mut() {
            if !hold.members.remove(&n) {
                continue;
            }
            let next = hold.next;
            for entry in hold.held.iter_mut().skip(next) {
                if entry.parent == Some(n) {
                    entry.parent = None;
                }
                if entry.node == Some(n) {
                    entry.node = None;
                }
            }
        }
    });
}

fn collect(hold: &mut Hold, parent: Node, n: Node, depth: i32) {
    if hidden_child(n) {
        return;
    }
    let (parent, addr) = (parent.as_ptr() as usize, n.as_ptr() as usize);
    hold.held.push(Held {
        parent: Some(parent),
        node: Some(addr),
    });
    hold.members.insert(parent);
    hold.members.insert(addr);
    if depth >= MAX_DEPTH || is_named(n, c"template") {
        return;
    }
    for c in southstar_dom::children(n) {
        collect(hold, n, c, depth + 1);
    }
}

fn hold_after(js: Js, script: Node) -> usize {
    let mut hold = Hold {
        held: Vec::new(),
        next: 0,
        members: HashSet::new(),
    };
    let mut tops = Vec::new();
    let mut n = script;
    while let Some(parent) = n.parent() {
        if n.kind() == Kind::Document {
            break;
        }
        let mut sib = n.next_sibling();
        while let Some(s) = sib {
            if !hidden_child(s) {
                tops.push(s);
                collect(&mut hold, parent, s, 0);
            }
            sib = s.next_sibling();
        }
        n = parent;
    }
    let entries = hold.held.clone();
    let index = page::with_page(js, |page| {
        page.holds.push(hold);
        page.holds.len() - 1
    });
    ACTIVE.with(|active| active.set(active.get() + 1));
    for top in tops {
        js.index_child_removed(top.parent(), top);
        ffi::node_remove(top);
    }
    for entry in entries {
        let parent = entry.parent.and_then(node_at);
        let child = entry.node.and_then(node_at);
        ffi::arm_invalidate(parent);
        ffi::arm_invalidate(child);
        if let Some(child) = child
            && child.parent().is_some()
        {
            ffi::node_remove(child);
        }
    }
    index
}

fn put_back(js: Js, index: usize, upto: usize) {
    loop {
        let step = page::with_existing(js, |page| {
            let hold = page.holds.get_mut(index)?;
            if hold.next >= upto || hold.next >= hold.held.len() {
                return None;
            }
            let entry = hold.held[hold.next];
            hold.next += 1;
            Some(entry)
        })
        .flatten();
        let Some(entry) = step else {
            break;
        };
        let (Some(parent), Some(child)) =
            (entry.parent.and_then(node_at), entry.node.and_then(node_at))
        else {
            continue;
        };
        if child.parent().is_some() {
            continue;
        }
        let prev = parent.last_child();
        ffi::append_child(parent, child);
        js.record_child_added(parent, child, prev);
        js.mark_mutated();
    }
}

fn reach(js: Js, index: usize, script: Node) {
    let script = Some(script.as_ptr() as usize);
    let upto = page::with_existing(js, |page| {
        let hold = page.holds.get(index)?;
        let len = hold.held.len();
        let mut i = hold.next;
        while i < len && hold.held[i].node != script {
            i += 1;
        }
        if i == len {
            return None;
        }
        i += 1;
        while i < len && hold.held[i].parent == script {
            i += 1;
        }
        Some(i)
    })
    .flatten();
    if let Some(upto) = upto {
        put_back(js, index, upto);
    }
}

fn release(js: Js, index: usize) {
    put_back(js, index, usize::MAX);
    let dropped = page::with_existing(js, |page| {
        let before = page.holds.len();
        page.holds.truncate(index);
        before - page.holds.len()
    })
    .unwrap_or(0);
    released(dropped);
}

pub(crate) fn run_parser_blocking_scripts(js: Js, tasks: &[Task], origin: Option<&CStr>) {
    let mut hold = None;
    for task in tasks {
        if task.schedule != Schedule::Blocking {
            continue;
        }
        match hold {
            None => hold = Some(hold_after(js, task.node)),
            Some(index) => reach(js, index, task.node),
        }
        fetch::run_script_element(js, task.node, origin);
    }
    if let Some(index) = hold {
        release(js, index);
    }
    js.upgrade_all();
}
