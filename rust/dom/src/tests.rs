//! Southstar — unit tests for the node store, the tree links, owned strings and attributes, and the document indexes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::attrs;
use crate::index;
use crate::node::{self, KIND_ELEMENT};
use crate::store;
use crate::{Kind, Node, children};

fn names(parent: Node) -> Vec<Vec<u8>> {
    children(parent)
        .map(|c| c.name().map_or(Vec::new(), |n| n.to_bytes().to_vec()))
        .collect()
}

#[test]
fn element_owns_its_name_and_attributes() {
    let el = node::new_element(b"div");
    attrs::set_attribute(el, c"id", Some(b"main"));
    attrs::set_attribute(el, c"Class", Some(b"a b"));
    assert_eq!(el.kind(), Kind::Element);
    assert_eq!(el.kind_raw(), KIND_ELEMENT);
    assert_eq!(el.element_name(), Some(&b"div"[..]));
    assert_eq!(el.attr(c"id"), Some(c"main"));
    assert_eq!(el.attr(c"class"), Some(c"a b"));
    attrs::set_attribute(el, c"id", Some(b"other"));
    assert_eq!(el.attr(c"id"), Some(c"other"));
    assert_eq!(el.attrs().count(), 2);
    attrs::remove_attribute(el, c"ID");
    assert_eq!(el.attr(c"id"), None);
    assert_eq!(el.attrs().count(), 1);
    el.free_tree();
}

#[test]
fn attribute_values_keep_interior_nul_bytes() {
    let el = node::new_element(b"p");
    attrs::set_attribute(el, c"title", Some(b"a\0b"));
    let attr = el.attrs().next().expect("attribute");
    assert_eq!(attr.value_bytes(), Some(&b"a\0b"[..]));
    assert_eq!(attr.value(), Some(c"a"));
    el.free_tree();
}

#[test]
fn links_stay_consistent_through_inserts_and_detach() {
    let parent = node::new_element(b"ul");
    let a = node::new_element(b"a");
    let b = node::new_element(b"b");
    let c = node::new_element(b"c");
    parent.append(b);
    b.insert_before(a);
    b.insert_after(c);
    assert_eq!(names(parent), [b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    assert_eq!(parent.first_child(), Some(a));
    assert_eq!(parent.last_child(), Some(c));
    assert_eq!(c.prev_sibling(), Some(b));
    b.detach();
    assert_eq!(b.parent(), None);
    assert_eq!(a.next_sibling(), Some(c));
    assert_eq!(c.prev_sibling(), Some(a));
    parent.append(a);
    assert_eq!(names(parent), [b"c".to_vec(), b"a".to_vec()]);
    b.free_tree();
    parent.free_tree();
}

#[test]
fn freeing_a_subtree_invalidates_weak_handles() {
    let before = store::live_nodes();
    let root = node::new_element(b"div");
    let child = node::new_text(Kind::Text, b"hello");
    root.append(child);
    let weak_root = root.downgrade();
    let weak_child = child.downgrade();
    assert_eq!(store::live_nodes(), before + 2);
    assert_eq!(weak_child.get(), Some(child));
    let child_ptr = child.as_ptr();
    root.free_tree();
    assert_eq!(store::live_nodes(), before);
    assert!(weak_root.get().is_none());
    assert!(weak_child.get().is_none());
    // SAFETY: the pointer names a slot of this thread's store, which is never deallocated.
    assert!(unsafe { Node::from_ptr(child_ptr) }.is_none());
    let reused = node::new_element(b"span");
    assert!(weak_root.get().is_none() && weak_child.get().is_none());
    reused.free_tree();
}

#[test]
fn freeing_a_child_unlinks_it_from_its_parent() {
    let parent = node::new_element(b"div");
    let a = node::new_element(b"a");
    let b = node::new_element(b"b");
    parent.append(a);
    parent.append(b);
    a.free_tree();
    assert_eq!(parent.first_child(), Some(b));
    assert_eq!(b.prev_sibling(), None);
    parent.free_tree();
}

#[test]
fn text_grows_in_place() {
    let text = node::new_text(Kind::Text, b"abc");
    text.append_text(b"def");
    assert_eq!(text.text_with_len(), Some(&b"abcdef"[..]));
    assert_eq!(text.text(), Some(c"abcdef"));
    assert_eq!(text.text_len(), 6);
    text.free_tree();
}

#[test]
fn class_tokens_follow_the_class_attribute() {
    let el = node::new_element(b"div");
    attrs::set_attribute(el, c"class", Some(b" x  y "));
    assert!(attrs::has_class(el, b"x"));
    assert!(attrs::has_class(el, b"y"));
    assert!(!attrs::has_class(el, b"z"));
    attrs::set_attribute(el, c"class", Some(b"z"));
    assert!(attrs::has_class(el, b"z"));
    assert!(!attrs::has_class(el, b"x"));
    el.free_tree();
}

#[test]
fn indexes_answer_lookups_in_document_order() {
    let doc = Node::alloc(Kind::Document);
    let first = node::new_element(b"p");
    let second = node::new_element(b"P");
    attrs::set_attribute(first, c"class", Some(b"k"));
    attrs::set_attribute(second, c"class", Some(b"k"));
    attrs::set_attribute(second, c"id", Some(b"two"));
    doc.append(first);
    doc.append(second);
    index::id_build(doc);
    index::class_build(doc);
    index::tag_build(doc);
    assert!(index::has_id_index(doc) && index::has_class_index(doc) && index::has_tag_index(doc));
    assert_eq!(index::find_by_id(doc, c"two"), Some(second));
    let classes = index::class_lookup(doc, c"k", |list| list.iter().collect::<Vec<_>>());
    assert_eq!(classes, Some(vec![first, second]));
    let tags = index::tag_lookup(doc, c"p", |list| list.len());
    assert_eq!(tags, Some(2));
    second.detach();
    index::id_subtree_removed(doc, second);
    index::class_subtree_removed(doc, second);
    assert_eq!(index::find_by_id(doc, c"two"), None);
    let classes = index::class_lookup(doc, c"k", |list| list.len());
    assert_eq!(classes, Some(1));
    second.free_tree();
    doc.free_tree();
}

#[test]
fn deep_clone_copies_names_text_and_attributes() {
    let el = node::new_element(b"div");
    attrs::set_attribute(el, c"data-x", Some(b"1"));
    el.append(node::new_text(Kind::Text, b"hi"));
    let copy = node::clone(el, true, 0).expect("clone").into_root();
    assert_ne!(copy, el);
    assert_eq!(copy.element_name(), Some(&b"div"[..]));
    assert_eq!(copy.attr(c"data-x"), Some(c"1"));
    let text = copy.first_child().expect("text child");
    assert_eq!(text.text_with_len(), Some(&b"hi"[..]));
    el.free_tree();
    assert_eq!(text.text_with_len(), Some(&b"hi"[..]));
    copy.free_tree();
}

#[test]
fn template_contents_belong_to_the_template() {
    let template = node::new_element(b"template");
    let content = node::template_content(template);
    content.append(node::new_element(b"b"));
    assert_eq!(node::template_content(template), content);
    let weak = content.downgrade();
    template.free_tree();
    assert!(weak.get().is_none());
}

#[test]
fn other_threads_get_their_own_slots() {
    let main = node::new_element(b"main");
    let freed_elsewhere = std::thread::spawn(|| {
        let before = store::live_nodes();
        let el = node::new_element(b"svg");
        el.append(node::new_element(b"g"));
        let weak = el.downgrade();
        el.free_tree();
        before == store::live_nodes() && weak.get().is_none()
    })
    .join()
    .expect("thread");
    assert!(freed_elsewhere);
    assert_eq!(main.element_name(), Some(&b"main"[..]));
    main.free_tree();
}

#[test]
fn common_names_are_sorted_for_binary_search() {
    let names = crate::data::COMMON_NAMES;
    assert!(names.windows(2).all(|w| w[0].to_bytes() < w[1].to_bytes()));
    let el = node::new_element(b"viewBox");
    assert_eq!(el.name(), Some(c"viewBox"));
    el.set_name(b"custom-element");
    assert_eq!(el.name(), Some(c"custom-element"));
    el.free_tree();
}

#[test]
fn text_moves_from_inline_to_heap_storage() {
    let text = node::new_text(Kind::Text, b"short");
    text.append_text(b" and");
    assert_eq!(text.text_with_len(), Some(&b"short and"[..]));
    text.append_text(b" now quite a bit longer than inline");
    assert_eq!(
        text.text_with_len(),
        Some(&b"short and now quite a bit longer than inline"[..])
    );
    assert_eq!(
        text.text(),
        Some(c"short and now quite a bit longer than inline")
    );
    text.set_text(b"");
    assert_eq!(text.text_with_len(), Some(&b""[..]));
    text.free_tree();
}

#[test]
fn attributes_return_to_the_store() {
    let before = store::live_attrs();
    let el = node::new_element(b"input");
    attrs::set_attribute(el, c"type", Some(b"text"));
    attrs::set_attribute(el, c"value", Some(b"a long value that does not fit inline"));
    assert_eq!(store::live_attrs(), before + 2);
    attrs::remove_attribute(el, c"type");
    assert_eq!(store::live_attrs(), before + 1);
    let kept = el.attrs().next().expect("value attribute");
    assert_eq!(kept.name(), Some(c"value"));
    el.free_tree();
    assert_eq!(store::live_attrs(), before);
}

#[test]
fn slots_stay_small() {
    assert!(size_of::<crate::data::NodeSlot>() <= 216);
    assert!(size_of::<crate::data::AttrSlot>() <= 72);
}

#[test]
fn namespaced_attributes_keep_their_parts() {
    let el = node::new_element(b"svg");
    let qname = attrs::NsName {
        namespace_uri: Some(c"http://www.w3.org/1999/xlink"),
        prefix: Some(c"xlink"),
        local_name: c"href",
        name: Some(c"xlink:href"),
    };
    attrs::set_ns(el, qname, Some(c"#a"));
    let attr = el.attrs().next().expect("attribute");
    assert_eq!(attr.name(), Some(c"xlink:href"));
    assert_eq!(attr.prefix(), Some(c"xlink"));
    assert_eq!(attr.local_name(), Some(c"href"));
    assert_eq!(attr.namespace_uri(), Some(c"http://www.w3.org/1999/xlink"));
    let found = attrs::find_ns(el, Some(c"http://www.w3.org/1999/xlink"), c"href");
    assert_eq!(found.and_then(|a| a.value()), Some(c"#a"));
    el.free_tree();
}

#[test]
fn a_tree_frees_its_subtree_unless_it_was_given_a_parent() {
    let before = store::live_nodes();
    {
        let tree = crate::Tree::new(Kind::Document);
        tree.root().append(node::new_element(b"p"));
        assert_eq!(store::live_nodes(), before + 2);
    }
    assert_eq!(store::live_nodes(), before);
    let host = node::new_element(b"div");
    {
        let tree = crate::Tree::adopt(node::new_element(b"span"));
        host.append(tree.root());
    }
    assert_eq!(
        host.first_child().and_then(|c| c.element_name()),
        Some(&b"span"[..])
    );
    let given_away = crate::Tree::adopt(node::new_element(b"b")).into_root();
    assert_eq!(store::live_nodes(), before + 3);
    drop(crate::Tree::adopt(given_away));
    host.free_tree();
    assert_eq!(store::live_nodes(), before);
}

#[test]
fn taking_template_contents_hands_over_ownership() {
    let template = node::new_element(b"template");
    node::template_content(template).append(node::new_element(b"i"));
    let content = template.take_tpl_content().expect("contents");
    let weak = content.root().downgrade();
    assert!(template.tpl_content().is_none());
    drop(content);
    assert!(weak.get().is_none());
    template.free_tree();
}
