//! Southstar — tests for the box store: allocation, tree links, owned text and arrays, and recycling a freed tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{
    BoxKind, BoxPool, InlineAtomic, InlineAttr, LinkRange, LinkVec, children, inline_kind,
};

#[test]
fn new_box_starts_clean() {
    let b = BoxPool::new_box(0);
    assert_eq!(b.kind(), BoxKind::Block);
    assert_eq!((b.colspan(), b.rowspan()), (1, 1));
    assert!(b.text().is_none() && b.media().is_none() && b.parent().is_none());
    assert!(!b.has_attrs() && b.inline_atomics().is_none());
    BoxPool::free_tree(Some(b));
}

#[test]
fn children_link_and_detach() {
    let parent = BoxPool::new_box(0);
    let a = BoxPool::new_box(0);
    let c = BoxPool::new_box(0);
    for child in [a, c] {
        child.set_parent(Some(parent));
        match parent.last_child() {
            Some(last) => last.set_next_sibling(Some(child)),
            None => parent.set_first_child(Some(child)),
        }
        parent.set_last_child(Some(child));
    }
    assert_eq!(children(parent).count(), 2);
    parent.detach_child(c);
    assert!(parent.last_child().is_some_and(|l| l.same(a)));
    parent.prepend_child(c);
    assert!(parent.first_child().is_some_and(|f| f.same(c)));
    BoxPool::free_tree(Some(parent));
}

#[test]
fn text_survives_rewrite_while_held() {
    let run = BoxPool::new_inline();
    run.set_text_bytes(b"hello world");
    let held = run.text().expect("text was set");
    run.truncate_text(5);
    assert_eq!(held.to_bytes(), b"hello world");
    assert_eq!(&*run.text_bytes(), b"hello");
    BoxPool::free_tree(Some(run));
}

#[test]
fn arrays_round_trip() {
    let run = BoxPool::new_inline();
    let atomic = BoxPool::new_box(0);
    run.put_attrs(Some(&[InlineAttr::new(inline_kind::BOLD, 0, 3)]));
    run.put_atomics(Some(&[InlineAtomic::new(1, atomic)]));
    let mut links = LinkVec::new();
    links.push(LinkRange::new(0, 2, Some(c"/a"), None, None));
    run.put_links(Some(links));
    run.set_atomic_owner_offset(0, 4.0, 5.0);
    assert_eq!(run.attrs().len(), 1);
    assert_eq!(run.links()[0].href(), Some(c"/a"));
    let atomics = run.inline_atomics().expect("atomics were set");
    assert_eq!(atomics[0].owner_offset(), (4.0, 5.0));
    assert!(atomics[0].box_ref().is_some_and(|b| b.same(atomic)));
    BoxPool::free_tree(Some(run));
}

#[test]
fn freed_slots_are_recycled_clean() {
    let b = BoxPool::new_inline();
    b.set_text_bytes(b"x");
    b.ensure_media().set_image_src(Some(c"a.png"));
    let addr = b.as_ptr();
    BoxPool::free_tree(Some(b));
    BoxPool::free_tree(Some(b));
    let again = BoxPool::new_box(0);
    assert_eq!(again.as_ptr(), addr);
    assert!(again.text().is_none() && again.media().is_none());
    let other = BoxPool::new_box(0);
    assert_ne!(other.as_ptr(), addr);
    BoxPool::free_tree(Some(again));
    BoxPool::free_tree(Some(other));
}
