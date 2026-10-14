//! Southstar — the document's id, class and tag indexes, document order, and lookups by tag, id and fragment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::collections::{HashMap, HashSet};

use crate::{Kind, MAX_DEPTH, Node, children, tree};

const SCAN_MAX: usize = 64;

pub fn scope_boundary(node: Node) -> bool {
    (node.kind() == Kind::Document && node.parent().is_some())
        || (node.is_element() && node.attr(c"data-nd-shadow-root").is_some())
}

pub fn next_in_subtree<'a>(
    node: Node<'a>,
    root: Option<Node<'a>>,
    descend: bool,
) -> Option<Node<'a>> {
    if descend && let Some(child) = node.first_child() {
        return Some(child);
    }
    let mut cur = Some(node);
    while let Some(n) = cur.filter(|n| Some(*n) != root) {
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
        cur = next_in_subtree(n, Some(root), descend);
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

pub struct NodeArray {
    nodes: Vec<Node<'static>>,
}

impl NodeArray {
    fn new(nodes: Vec<Node<'static>>) -> Self {
        NodeArray { nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, index: usize) -> Node<'static> {
        self.nodes[index]
    }

    pub fn last(&self) -> Option<Node<'static>> {
        self.nodes.last().copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = Node<'static>> + '_ {
        self.nodes.iter().copied()
    }
}

fn sort_in_document_order(nodes: &mut [Node<'static>]) {
    nodes.sort_by(|a, b| document_order_cmp(Some(*a), Some(*b)).cmp(&0));
}

struct Bucket {
    nodes: Vec<Node<'static>>,
    members: Option<HashSet<Node<'static>>>,
    unsorted: bool,
}

impl Bucket {
    fn with_node(node: Node<'static>) -> Bucket {
        Bucket {
            nodes: vec![node],
            members: None,
            unsorted: false,
        }
    }

    fn track_members(&mut self) -> &mut HashSet<Node<'static>> {
        let nodes = &self.nodes;
        self.members
            .get_or_insert_with(|| nodes.iter().copied().collect())
    }

    fn note_member(&mut self, node: Node<'static>) {
        if let Some(members) = &mut self.members {
            members.insert(node);
        }
    }

    fn forget_member(&mut self, node: Node<'static>) {
        if let Some(members) = &mut self.members {
            members.remove(&node);
        }
    }

    fn add(&mut self, node: Node<'static>, building: bool) {
        if building {
            if self.nodes.last() != Some(&node) {
                self.nodes.push(node);
                self.note_member(node);
            }
            return;
        }
        if self.unsorted {
            if self.members.as_mut().is_some_and(|m| m.insert(node)) {
                self.nodes.push(node);
            }
            return;
        }
        let len = self.nodes.len();
        if self
            .nodes
            .last()
            .is_none_or(|last| document_order_cmp(Some(node), Some(*last)) > 0)
        {
            self.nodes.push(node);
            self.note_member(node);
            return;
        }
        if len > SCAN_MAX {
            if self.track_members().insert(node) {
                self.nodes.push(node);
                self.unsorted = true;
            }
            return;
        }
        let (mut lo, mut hi) = (0, len);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match document_order_cmp(Some(node), Some(self.nodes[mid])) {
                0 => return,
                c if c < 0 => hi = mid,
                _ => lo = mid + 1,
            }
        }
        self.nodes.insert(lo, node);
        self.note_member(node);
    }

    fn remove(&mut self, node: Node<'static>) {
        let len = self.nodes.len();
        if !self.unsorted && self.nodes.last() == Some(&node) {
            self.nodes.truncate(len - 1);
            self.forget_member(node);
            return;
        }
        if !self.unsorted && len <= SCAN_MAX {
            if let Some(k) = self.nodes.iter().position(|&n| n == node) {
                self.nodes.remove(k);
                self.forget_member(node);
            }
            return;
        }
        if self.track_members().remove(&node) {
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
            if let Some(&member) = members.get(&Node::of(n.slot())) {
                self.nodes.push(member);
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
        let seen: HashSet<Node<'static>> = self.nodes.iter().copied().collect();
        let mut rest: Vec<Node<'static>> = members
            .iter()
            .copied()
            .filter(|n| !seen.contains(n))
            .collect();
        sort_in_document_order(&mut rest);
        self.nodes.extend(rest);
    }

    fn ordered(&mut self, doc: Node) -> &[Node<'static>] {
        if self.unsorted {
            self.nodes.clear();
            let members = self.members.as_ref().map_or(0, HashSet::len);
            if members <= SCAN_MAX {
                if let Some(set) = &self.members {
                    self.nodes.extend(set.iter().copied());
                }
                sort_in_document_order(&mut self.nodes);
            } else {
                self.collect_in_order(doc);
            }
            self.unsorted = false;
        }
        &self.nodes
    }
}

#[derive(Default)]
pub(crate) struct Indexes {
    ids: Option<HashMap<Box<[u8]>, Node<'static>>>,
    classes: Option<HashMap<Box<[u8]>, Bucket>>,
    tags: Option<HashMap<Box<[u8]>, Bucket>>,
}

fn with_indexes<R>(doc: Node, f: impl FnOnce(&mut Indexes) -> R) -> R {
    let mut indexes = doc.indexes().borrow_mut();
    f(indexes.get_or_insert_with(Box::default))
}

fn has_index(doc: Node, which: impl FnOnce(&Indexes) -> bool) -> bool {
    doc.indexes().borrow().as_deref().is_some_and(which)
}

pub fn has_id_index(doc: Node) -> bool {
    has_index(doc, |ix| ix.ids.is_some())
}

pub fn has_class_index(doc: Node) -> bool {
    has_index(doc, |ix| ix.classes.is_some())
}

pub fn has_tag_index(doc: Node) -> bool {
    has_index(doc, |ix| ix.tags.is_some())
}

fn static_node(node: Node) -> Node<'static> {
    Node::of(node.slot())
}

fn id_add(ids: &mut HashMap<Box<[u8]>, Node<'static>>, node: Node) {
    if !node.is_element() {
        return;
    }
    if let Some(id) = node.attr(c"id").filter(|id| !id.is_empty()) {
        ids.entry(Box::from(id.to_bytes()))
            .or_insert_with(|| static_node(node));
    }
}

fn id_remove(ids: &mut HashMap<Box<[u8]>, Node<'static>>, node: Node) {
    if !node.is_element() {
        return;
    }
    if let Some(id) = node.attr(c"id").filter(|id| !id.is_empty())
        && ids.get(id.to_bytes()) == Some(&static_node(node))
    {
        ids.remove(id.to_bytes());
    }
}

pub fn id_build(doc: Node) {
    let mut ids = HashMap::new();
    walk(doc, doc, true, |n| id_add(&mut ids, n));
    with_indexes(doc, |ix| ix.ids = Some(ids));
}

pub fn id_register(doc: Node, id: &CStr, node: Node) {
    if id.is_empty() {
        return;
    }
    with_indexes(doc, |ix| {
        if let Some(ids) = &mut ix.ids {
            ids.entry(Box::from(id.to_bytes()))
                .or_insert_with(|| static_node(node));
        }
    });
}

pub fn id_unregister(doc: Node, id: &CStr, node: Option<Node>) {
    if id.is_empty() {
        return;
    }
    with_indexes(doc, |ix| {
        if let Some(ids) = &mut ix.ids
            && ids.get(id.to_bytes()).copied() == node.map(static_node)
        {
            ids.remove(id.to_bytes());
        }
    });
}

fn subtree_nodes(doc: Node, root: Node, scoped: bool) -> Vec<Node<'static>> {
    let mut nodes = Vec::new();
    walk(doc, root, scoped, |n| nodes.push(static_node(n)));
    nodes
}

pub fn id_subtree_added(doc: Node, root: Node) {
    if !has_id_index(doc) {
        return;
    }
    let nodes = subtree_nodes(doc, root, true);
    with_indexes(doc, |ix| {
        if let Some(ids) = &mut ix.ids {
            nodes.into_iter().for_each(|n| id_add(ids, n));
        }
    });
}

pub fn id_subtree_removed(doc: Node, root: Node) {
    if !has_id_index(doc) {
        return;
    }
    let nodes = subtree_nodes(doc, root, false);
    with_indexes(doc, |ix| {
        if let Some(ids) = &mut ix.ids {
            nodes.into_iter().for_each(|n| id_remove(ids, n));
        }
    });
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

fn bucket_add(table: &mut HashMap<Box<[u8]>, Bucket>, key: &[u8], node: Node, building: bool) {
    let node = static_node(node);
    match table.get_mut(key) {
        Some(bucket) => bucket.add(node, building),
        None => {
            table.insert(Box::from(key), Bucket::with_node(node));
        }
    }
}

fn bucket_remove(table: &mut HashMap<Box<[u8]>, Bucket>, key: &[u8], node: Node) {
    if let Some(bucket) = table.get_mut(key) {
        bucket.remove(static_node(node));
    }
}

fn class_entries(node: Node<'_>) -> Option<(Node<'static>, &CStr)> {
    if !node.is_element() {
        return None;
    }
    let class_attr = node.attr(c"class").filter(|c| !c.is_empty())?;
    Some((static_node(node), class_attr))
}

pub fn class_register(doc: Node, class_attr: &CStr, node: Node) {
    with_indexes(doc, |ix| {
        if let Some(classes) = &mut ix.classes {
            for token in class_tokens(class_attr) {
                bucket_add(classes, token, node, false);
            }
        }
    });
}

pub fn class_unregister(doc: Node, class_attr: &CStr, node: Node) {
    with_indexes(doc, |ix| {
        if let Some(classes) = &mut ix.classes {
            for token in class_tokens(class_attr) {
                bucket_remove(classes, token, node);
            }
        }
    });
}

fn add_classes(classes: &mut HashMap<Box<[u8]>, Bucket>, nodes: &[Node<'static>], building: bool) {
    for (node, class_attr) in nodes.iter().filter_map(|n| class_entries(*n)) {
        for token in class_tokens(class_attr) {
            bucket_add(classes, token, node, building);
        }
    }
}

pub fn class_build(doc: Node) {
    let nodes = subtree_nodes(doc, doc, true);
    let mut classes = HashMap::new();
    add_classes(&mut classes, &nodes, true);
    with_indexes(doc, |ix| ix.classes = Some(classes));
}

pub fn class_subtree_added(doc: Node, root: Node) {
    if !has_class_index(doc) {
        return;
    }
    let nodes = subtree_nodes(doc, root, true);
    with_indexes(doc, |ix| {
        if let Some(classes) = &mut ix.classes {
            add_classes(classes, &nodes, false);
        }
    });
}

pub fn class_subtree_removed(doc: Node, root: Node) {
    if !has_class_index(doc) {
        return;
    }
    let nodes = subtree_nodes(doc, root, false);
    with_indexes(doc, |ix| {
        if let Some(classes) = &mut ix.classes {
            for (node, class_attr) in nodes.iter().filter_map(|n| class_entries(*n)) {
                for token in class_tokens(class_attr) {
                    bucket_remove(classes, token, node);
                }
            }
        }
    });
}

fn ordered_snapshot(
    doc: Node,
    key: &[u8],
    table: impl FnOnce(&mut Indexes) -> Option<&mut HashMap<Box<[u8]>, Bucket>>,
) -> Option<NodeArray> {
    let mut indexes = doc.indexes().borrow_mut();
    let bucket = table(indexes.as_deref_mut()?)?.get_mut(key)?;
    Some(NodeArray::new(bucket.ordered(doc).to_vec()))
}

pub fn class_lookup<R>(doc: Node, class: &CStr, f: impl FnOnce(&NodeArray) -> R) -> Option<R> {
    if class.is_empty() {
        return None;
    }
    let list = ordered_snapshot(doc, class.to_bytes(), |ix| ix.classes.as_mut())?;
    Some(f(&list))
}

fn tag_key(tag: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if tag.iter().any(u8::is_ascii_uppercase) {
        std::borrow::Cow::Owned(tag.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Borrowed(tag)
    }
}

fn tag_entry(node: Node<'_>) -> Option<(Node<'static>, &[u8])> {
    node.name()
        .filter(|t| node.is_element() && !t.is_empty())
        .map(|tag| (static_node(node), tag.to_bytes()))
}

fn add_tags(tags: &mut HashMap<Box<[u8]>, Bucket>, nodes: &[Node<'static>], building: bool) {
    for (node, tag) in nodes.iter().filter_map(|n| tag_entry(*n)) {
        bucket_add(tags, &tag_key(tag), node, building);
    }
}

pub fn tag_build(doc: Node) {
    let nodes = subtree_nodes(doc, doc, true);
    let mut tags = HashMap::new();
    add_tags(&mut tags, &nodes, true);
    with_indexes(doc, |ix| ix.tags = Some(tags));
}

pub fn tag_subtree_added(doc: Node, root: Node) {
    if !has_tag_index(doc) {
        return;
    }
    let nodes = subtree_nodes(doc, root, true);
    with_indexes(doc, |ix| {
        if let Some(tags) = &mut ix.tags {
            add_tags(tags, &nodes, false);
        }
    });
}

pub fn tag_subtree_removed(doc: Node, root: Node) {
    if !has_tag_index(doc) {
        return;
    }
    let nodes = subtree_nodes(doc, root, false);
    with_indexes(doc, |ix| {
        if let Some(tags) = &mut ix.tags {
            for (node, tag) in nodes.iter().filter_map(|n| tag_entry(*n)) {
                bucket_remove(tags, &tag_key(tag), node);
            }
        }
    });
}

pub fn tag_lookup<R>(doc: Node, tag: &CStr, f: impl FnOnce(&NodeArray) -> R) -> Option<R> {
    if tag.is_empty() {
        return None;
    }
    let list = ordered_snapshot(doc, &tag_key(tag.to_bytes()), |ix| ix.tags.as_mut())?;
    Some(f(&list))
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
    if !tag.is_empty() && has_tag_index(root) {
        return tag_lookup(root, tag, |list| (!list.is_empty()).then(|| list.get(0))).flatten();
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

fn indexed_hit<'a>(doc: Node, root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
    let hit = doc
        .indexes()
        .borrow()
        .as_deref()?
        .ids
        .as_ref()?
        .get(id.to_bytes())
        .copied()?;
    (hit.attr(c"id") == Some(id) && tree::contains(root.as_ptr(), hit))
        .then(|| Node::of(hit.slot()))
}

fn index_id(doc: Node, id: &CStr, found: Option<Node>, replace_missing: bool) {
    with_indexes(doc, |ix| {
        let Some(ids) = &mut ix.ids else { return };
        match found {
            Some(found) => {
                ids.insert(Box::from(id.to_bytes()), static_node(found));
            }
            None if replace_missing => {
                ids.remove(id.to_bytes());
            }
            None => {}
        }
    });
}

pub fn find_by_id<'a>(root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
    if id.is_empty() {
        return None;
    }
    if has_id_index(root) {
        if let Some(hit) = indexed_hit(root, root, id) {
            return Some(hit);
        }
        let found = by_id_walk(root, id, 0);
        index_id(root, id, found, true);
        return found;
    }
    let mut doc = root;
    while let Some(parent) = doc.parent() {
        if doc.kind() == Kind::Document || scope_boundary(doc) {
            break;
        }
        doc = parent;
    }
    if doc != root && doc.kind() == Kind::Document && has_id_index(doc) {
        if let Some(hit) = indexed_hit(doc, root, id) {
            return Some(hit);
        }
        let found = by_id_walk(root, id, 0);
        if found.is_some() {
            index_id(doc, id, found, false);
        }
        return found;
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
