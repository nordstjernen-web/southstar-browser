//! Southstar — the document's id, class and tag indexes, document order, and lookups by tag, id and fragment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::ffi::{BucketTable, IdTable, Kind, Node, NodeArray, NodeSet};
use crate::{MAX_DEPTH, children, tree};

const SCAN_MAX: usize = 64;
const KEY_STACK: usize = 96;

static BUILDING: AtomicBool = AtomicBool::new(false);

pub fn scope_boundary(node: Node) -> bool {
    (node.kind() == Kind::Document && node.parent().is_some())
        || (node.is_element() && node.attr(c"data-nd-shadow-root").is_some())
}

pub fn next_in_subtree<'a>(node: Node<'a>, root: Node<'a>, descend: bool) -> Option<Node<'a>> {
    if descend {
        if let Some(child) = node.first_child() {
            return Some(child);
        }
    }
    let mut cur = Some(node);
    while let Some(n) = cur.filter(|n| *n != root) {
        if let Some(sibling) = n.next_sibling() {
            return Some(sibling);
        }
        cur = n.parent();
    }
    None
}

fn walk(doc: Node, root: Node, scoped: bool, mut visit: impl FnMut(Node)) {
    let mut cur = Some(root);
    while let Some(n) = cur {
        let descend = if scoped && n != doc && scope_boundary(n) {
            false
        } else {
            visit(n);
            !(scoped && n.element_name() == Some(b"template"))
        };
        cur = next_in_subtree(n, root, descend);
    }
}

fn depth(node: Option<Node>) -> i32 {
    let mut depth = 0;
    let mut cur = node;
    while let Some(n) = cur {
        if depth >= MAX_DEPTH {
            break;
        }
        depth += 1;
        cur = n.parent();
    }
    depth
}

fn climb(mut node: Option<Node>, from: i32, to: i32) -> Option<Node> {
    let mut level = from;
    while level > to {
        let Some(n) = node else { break };
        node = n.parent();
        level -= 1;
    }
    node
}

fn sibling_order(ca: Node, cb: Node) -> c_int {
    if let Some(parent) = ca.parent() {
        if Some(ca) == parent.first_child() || Some(cb) == parent.last_child() {
            return -1;
        }
        if Some(cb) == parent.first_child() || Some(ca) == parent.last_child() {
            return 1;
        }
    }
    let mut forward = ca.next_sibling();
    let mut back = ca.prev_sibling();
    while forward.is_some() || back.is_some() {
        if forward == Some(cb) {
            return -1;
        }
        if back == Some(cb) {
            return 1;
        }
        forward = forward.and_then(Node::next_sibling);
        back = back.and_then(Node::prev_sibling);
    }
    0
}

pub fn document_order_cmp(a: Option<Node>, b: Option<Node>) -> c_int {
    if a == b {
        return 0;
    }
    let (da, db) = (depth(a), depth(b));
    let mut ca = climb(a, da, db);
    let mut cb = climb(b, db, da);
    if ca == cb {
        return if da < db { -1 } else { 1 };
    }
    let mut guard = 0;
    while let (Some(pa), Some(pb)) = (ca, cb) {
        if pa.parent() == pb.parent() || guard >= MAX_DEPTH {
            break;
        }
        guard += 1;
        ca = pa.parent();
        cb = pb.parent();
    }
    match (ca, cb) {
        (Some(ca), Some(cb)) => sibling_order(ca, cb),
        _ => {
            if Node::ptr_or_null(a) < Node::ptr_or_null(b) {
                -1
            } else {
                1
            }
        }
    }
}

pub struct Bucket {
    nodes: NodeArray,
    members: Option<NodeSet>,
    unsorted: bool,
}

impl Bucket {
    fn with_node(node: Node) -> Bucket {
        let nodes = NodeArray::empty();
        nodes.push(node);
        Bucket {
            nodes,
            members: None,
            unsorted: false,
        }
    }

    fn track_members(&mut self) -> &NodeSet {
        let nodes = &self.nodes;
        self.members.get_or_insert_with(|| {
            let set = NodeSet::empty();
            for i in 0..nodes.len() {
                set.add(nodes.get(i));
            }
            set
        })
    }

    fn note_member(&self, node: Node) {
        if let Some(members) = &self.members {
            members.add(node);
        }
    }

    fn forget_member(&self, node: Node) {
        if let Some(members) = &self.members {
            members.remove(node);
        }
    }

    fn add(&mut self, node: Node) {
        if BUILDING.load(Ordering::Relaxed) {
            if self.nodes.last() != Some(node) {
                self.nodes.push(node);
                self.note_member(node);
            }
            return;
        }
        if self.unsorted {
            if self.members.as_ref().is_some_and(|m| m.add(node)) {
                self.nodes.push(node);
            }
            return;
        }
        let len = self.nodes.len();
        if self
            .nodes
            .last()
            .is_none_or(|last| document_order_cmp(Some(node), Some(last)) > 0)
        {
            self.nodes.push(node);
            self.note_member(node);
            return;
        }
        if len > SCAN_MAX {
            if self.track_members().add(node) {
                self.nodes.push(node);
                self.unsorted = true;
            }
            return;
        }
        let (mut lo, mut hi) = (0, len);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match document_order_cmp(Some(node), Some(self.nodes.get(mid))) {
                0 => return,
                c if c < 0 => hi = mid,
                _ => lo = mid + 1,
            }
        }
        self.nodes.insert(lo, node);
        self.note_member(node);
    }

    fn remove(&mut self, node: Node) {
        let len = self.nodes.len();
        if !self.unsorted && self.nodes.last() == Some(node) {
            self.nodes.truncate(len - 1);
            self.forget_member(node);
            return;
        }
        if !self.unsorted && len <= SCAN_MAX {
            if let Some(k) = (0..len).find(|&k| self.nodes.get(k) == node) {
                self.nodes.remove(k);
                self.forget_member(node);
            }
            return;
        }
        if self.track_members().remove(node) {
            self.unsorted = true;
        }
    }

    fn collect_in_order(&mut self, doc: Node) {
        let Some(members) = &self.members else { return };
        let want = members.len();
        let mut cur = Some(doc);
        while let Some(n) = cur {
            if self.nodes.len() >= want {
                break;
            }
            if members.contains(n) {
                self.nodes.push(n);
            }
            if let Some(child) = n.first_child() {
                cur = Some(child);
                continue;
            }
            let mut up = Some(n);
            while let Some(u) = up {
                if u == doc || u.next_sibling().is_some() {
                    break;
                }
                up = u.parent();
            }
            cur = up.filter(|u| *u != doc).and_then(Node::next_sibling);
        }
        if self.nodes.len() == want {
            return;
        }
        let seen = NodeSet::empty();
        for i in 0..self.nodes.len() {
            seen.add(self.nodes.get(i));
        }
        let rest = NodeArray::empty();
        for node in members.nodes().into_iter().filter(|n| !seen.contains(*n)) {
            rest.push(node);
        }
        rest.sort_in_document_order();
        for i in 0..rest.len() {
            self.nodes.push(rest.get(i));
        }
    }

    fn ordered(&mut self, doc: Node) -> &NodeArray {
        if self.unsorted {
            self.nodes.truncate(0);
            let members = self.members.as_ref().map_or(0, NodeSet::len);
            if members <= SCAN_MAX {
                if let Some(set) = &self.members {
                    for node in set.nodes() {
                        self.nodes.push(node);
                    }
                }
                self.nodes.sort_in_document_order();
            } else {
                self.collect_in_order(doc);
            }
            self.unsorted = false;
        }
        &self.nodes
    }
}

fn with_key<R>(token: &[u8], f: impl FnOnce(&CStr) -> R) -> R {
    if token.len() < KEY_STACK {
        let mut stack = [0u8; KEY_STACK];
        stack[..token.len()].copy_from_slice(token);
        f(CStr::from_bytes_until_nul(&stack).unwrap_or_default())
    } else {
        let mut owned = token.to_vec();
        owned.push(0);
        f(CStr::from_bytes_until_nul(&owned).unwrap_or_default())
    }
}

fn id_add(table: IdTable, node: Node) {
    if !node.is_element() {
        return;
    }
    if let Some(id) = node.attr(c"id").filter(|id| !id.is_empty()) {
        if !table.contains(id) {
            table.insert(id, node);
        }
    }
}

fn id_remove(table: IdTable, node: Node) {
    if !node.is_element() {
        return;
    }
    if let Some(id) = node.attr(c"id").filter(|id| !id.is_empty()) {
        if table.lookup(id) == Some(node) {
            table.remove(id);
        }
    }
}

pub fn id_build(doc: Node) {
    let table = match doc.id_table() {
        Some(table) => {
            table.clear();
            table
        }
        None => {
            let table = IdTable::empty();
            doc.set_id_table(table);
            table
        }
    };
    walk(doc, doc, true, |n| id_add(table, n));
}

pub fn id_register(doc: Node, id: &CStr, node: Node) {
    if let Some(table) = doc.id_table().filter(|_| !id.is_empty()) {
        if !table.contains(id) {
            table.insert(id, node);
        }
    }
}

pub fn id_unregister(doc: Node, id: &CStr, node: Option<Node>) {
    if let Some(table) = doc.id_table().filter(|_| !id.is_empty()) {
        if table.lookup(id) == node {
            table.remove(id);
        }
    }
}

pub fn id_subtree_added(doc: Node, root: Node) {
    if let Some(table) = doc.id_table() {
        walk(doc, root, true, |n| id_add(table, n));
    }
}

pub fn id_subtree_removed(doc: Node, root: Node) {
    if let Some(table) = doc.id_table() {
        walk(doc, root, false, |n| id_remove(table, n));
    }
}

fn class_space(c: &u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn class_tokens(class_attr: &CStr) -> impl Iterator<Item = &[u8]> {
    class_attr
        .to_bytes()
        .split(class_space)
        .filter(|token| !token.is_empty())
}

fn bucket_add(table: BucketTable, key: &CStr, node: Node) {
    if table.with_bucket(key, |bucket| bucket.add(node)).is_none() {
        table.insert(key, Bucket::with_node(node));
    }
}

fn bucket_remove(table: BucketTable, key: &CStr, node: Node) {
    table.with_bucket(key, |bucket| bucket.remove(node));
}

fn new_or_cleared(existing: Option<BucketTable>, install: impl FnOnce(BucketTable)) -> BucketTable {
    match existing {
        Some(table) => {
            table.clear();
            table
        }
        None => {
            let table = BucketTable::empty();
            install(table);
            table
        }
    }
}

pub fn class_register(doc: Node, class_attr: &CStr, node: Node) {
    let Some(table) = doc.class_table() else {
        return;
    };
    for token in class_tokens(class_attr) {
        with_key(token, |key| bucket_add(table, key, node));
    }
}

pub fn class_unregister(doc: Node, class_attr: &CStr, node: Node) {
    let Some(table) = doc.class_table() else {
        return;
    };
    for token in class_tokens(class_attr) {
        with_key(token, |key| bucket_remove(table, key, node));
    }
}

fn class_add_node(doc: Node, node: Node) {
    if !node.is_element() {
        return;
    }
    if let Some(class_attr) = node.attr(c"class").filter(|c| !c.is_empty()) {
        class_register(doc, class_attr, node);
    }
}

fn class_remove_node(doc: Node, node: Node) {
    if !node.is_element() {
        return;
    }
    if let Some(class_attr) = node.attr(c"class").filter(|c| !c.is_empty()) {
        class_unregister(doc, class_attr, node);
    }
}

pub fn class_build(doc: Node) {
    new_or_cleared(doc.class_table(), |table| doc.set_class_table(table));
    BUILDING.store(true, Ordering::Relaxed);
    walk(doc, doc, true, |n| class_add_node(doc, n));
    BUILDING.store(false, Ordering::Relaxed);
}

pub fn class_subtree_added(doc: Node, root: Node) {
    if doc.class_table().is_some() {
        walk(doc, root, true, |n| class_add_node(doc, n));
    }
}

pub fn class_subtree_removed(doc: Node, root: Node) {
    if doc.class_table().is_some() {
        walk(doc, root, false, |n| class_remove_node(doc, n));
    }
}

pub fn class_lookup<R>(doc: Node, class: &CStr, f: impl FnOnce(&NodeArray) -> R) -> Option<R> {
    let table = doc.class_table().filter(|_| !class.is_empty())?;
    table.with_bucket(class, |bucket| f(bucket.ordered(doc)))
}

fn with_tag_key<R>(tag: &CStr, f: impl FnOnce(&CStr) -> R) -> R {
    let bytes = tag.to_bytes();
    if bytes.iter().any(u8::is_ascii_uppercase) {
        with_key(&bytes.to_ascii_lowercase(), f)
    } else {
        f(tag)
    }
}

fn tag_add_node(table: BucketTable, node: Node) {
    if let Some(tag) = node.name().filter(|t| node.is_element() && !t.is_empty()) {
        with_tag_key(tag, |key| bucket_add(table, key, node));
    }
}

fn tag_remove_node(table: BucketTable, node: Node) {
    if let Some(tag) = node.name().filter(|t| node.is_element() && !t.is_empty()) {
        with_tag_key(tag, |key| bucket_remove(table, key, node));
    }
}

pub fn tag_build(doc: Node) {
    let table = new_or_cleared(doc.tag_table(), |table| doc.set_tag_table(table));
    BUILDING.store(true, Ordering::Relaxed);
    walk(doc, doc, true, |n| tag_add_node(table, n));
    BUILDING.store(false, Ordering::Relaxed);
}

pub fn tag_subtree_added(doc: Node, root: Node) {
    if let Some(table) = doc.tag_table() {
        walk(doc, root, true, |n| tag_add_node(table, n));
    }
}

pub fn tag_subtree_removed(doc: Node, root: Node) {
    if let Some(table) = doc.tag_table() {
        walk(doc, root, false, |n| tag_remove_node(table, n));
    }
}

pub fn tag_lookup<R>(doc: Node, tag: &CStr, f: impl FnOnce(&NodeArray) -> R) -> Option<R> {
    let table = doc.tag_table().filter(|_| !tag.is_empty())?;
    with_tag_key(tag, |key| {
        table.with_bucket(key, |bucket| f(bucket.ordered(doc)))
    })
}

fn first_element_walk<'a>(root: Node<'a>, tag: &[u8], depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if root.element_name() == Some(tag) {
        return Some(root);
    }
    children(root)
        .filter(|c| !scope_boundary(*c))
        .find_map(|c| first_element_walk(c, tag, depth + 1))
}

pub fn find_first_element<'a>(root: Node<'a>, tag: &CStr) -> Option<Node<'a>> {
    if !tag.is_empty() && root.tag_table().is_some() {
        return tag_lookup(root, tag, |list| (list.len() > 0).then(|| list.get(0))).flatten();
    }
    first_element_walk(root, tag.to_bytes(), 0)
}

fn by_id_walk<'a>(root: Node<'a>, id: &CStr, depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if root.is_element() && root.attr(c"id") == Some(id) {
        return Some(root);
    }
    if root.element_name() == Some(b"template") {
        return None;
    }
    children(root)
        .filter(|c| !scope_boundary(*c))
        .find_map(|c| by_id_walk(c, id, depth + 1))
}

fn indexed_hit<'a>(table: IdTable, root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
    let hit = table.lookup(id)?;
    (hit.attr(c"id") == Some(id) && tree::contains(root.as_ptr(), hit)).then_some(hit)
}

pub fn find_by_id<'a>(root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
    if id.is_empty() {
        return None;
    }
    if let Some(table) = root.id_table() {
        if let Some(hit) = indexed_hit(table, root, id) {
            return Some(hit);
        }
        let found = by_id_walk(root, id, 0);
        match found {
            Some(found) => table.replace(id, found),
            None => table.remove(id),
        }
        return found;
    }
    let mut doc = root;
    while let Some(parent) = doc.parent() {
        if doc.kind() == Kind::Document || scope_boundary(doc) {
            break;
        }
        doc = parent;
    }
    if doc != root && doc.kind() == Kind::Document {
        if let Some(table) = doc.id_table() {
            if let Some(hit) = indexed_hit(table, root, id) {
                return Some(hit);
            }
            let found = by_id_walk(root, id, 0);
            if let Some(found) = found {
                table.replace(id, found);
            }
            return found;
        }
    }
    by_id_walk(root, id, 0)
}

fn anchor_walk<'a>(root: Node<'a>, name: &CStr, depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if root.element_name() == Some(b"a") && root.attr(c"name") == Some(name) {
        return Some(root);
    }
    if root.element_name() == Some(b"template") {
        return None;
    }
    children(root).find_map(|c| anchor_walk(c, name, depth + 1))
}

pub fn find_fragment_target<'a>(root: Node<'a>, fragment: &CStr) -> Option<Node<'a>> {
    if fragment.is_empty() {
        return None;
    }
    find_by_id(root, fragment).or_else(|| anchor_walk(root, fragment, 0))
}
