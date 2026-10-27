//! Southstar — inline runs: merging a prefix run into the next, splitting a run at a byte offset and rejoining split runs, appending children, and copying a byte range of a run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxKind, BoxPool, BoxRef, InlineAtomic, LinkVec, Text};

fn text_bytes(b: BoxRef<'_>) -> Text {
    b.text_bytes()
}

fn same(a: Option<BoxRef<'_>>, b: BoxRef<'_>) -> bool {
    a.is_some_and(|a| a.same(b))
}

pub fn merge_prefix<'a>(
    prefix: Option<BoxRef<'a>>,
    suffix: Option<BoxRef<'a>>,
) -> Option<BoxRef<'a>> {
    let Some(prefix) = prefix else {
        return suffix;
    };
    let Some(suffix) = suffix else {
        return Some(prefix);
    };
    let plen = text_bytes(prefix).len();
    let slen = text_bytes(suffix).len();
    if plen > usize::MAX - slen - 1 {
        BoxPool::free_tree(Some(suffix));
        return Some(prefix);
    }
    let mut combined = Vec::with_capacity(plen + slen);
    combined.extend_from_slice(&text_bytes(prefix));
    combined.extend_from_slice(&text_bytes(suffix));
    suffix.set_text_bytes(&combined);

    let mut attrs = suffix.take_attrs();
    if let Some(attrs) = attrs.as_mut() {
        for a in attrs.iter_mut() {
            a.start += plen;
        }
    }
    let mut links = suffix.take_links();
    if let Some(links) = links.as_mut() {
        for l in links.items_mut() {
            l.start += plen;
        }
    }
    if prefix.has_attrs() {
        attrs
            .get_or_insert_default()
            .extend_from_slice(&prefix.attrs());
    }
    suffix.put_attrs(attrs.as_deref());

    let mut atomics = suffix.take_atomics();
    if let Some(atomics) = atomics.as_mut() {
        for ia in atomics.iter_mut() {
            ia.set_byte_off(ia.byte_off() + plen);
        }
    }
    if let Some(prefix_atomics) = prefix.take_atomics() {
        let dst = atomics.get_or_insert_default();
        for ia in prefix_atomics {
            if let Some(ab) = ia.box_handle()
                && same(ab.parent(), prefix)
            {
                ab.set_parent(Some(suffix));
            }
            dst.push(ia);
        }
    }
    suffix.put_atomics(atomics.as_deref());

    if prefix.has_links() {
        let dst = links.get_or_insert_default();
        for l in &prefix.links() {
            dst.push_copy(l, l.start, l.len);
        }
    }
    suffix.put_links(links);
    BoxPool::free_tree(Some(prefix));
    Some(suffix)
}

fn split_attrs(run: BoxRef<'_>, tail: BoxRef<'_>, split: usize) {
    let mut head = Vec::new();
    let mut tail_attrs = tail.take_attrs();
    for a in run.take_attrs().unwrap_or_default() {
        let end = a.start + a.len;
        if a.start < split {
            let mut h = a.clone();
            if end > split {
                h.len = split - a.start;
            }
            head.push(h);
        }
        if end > split || a.start >= split {
            let from = a.start.max(split);
            let mut t = a;
            t.start = from - split;
            t.len = if end > split { end - from } else { 0 };
            tail_attrs.get_or_insert_default().push(t);
        }
    }
    run.put_attrs(Some(&head));
    tail.put_attrs(tail_attrs.as_deref());
}

fn split_links(run: BoxRef<'_>, tail: BoxRef<'_>, split: usize) {
    let Some(links) = run.take_links() else {
        return;
    };
    let mut head = LinkVec::new();
    let mut tail_links = tail.take_links();
    for l in links.items() {
        let end = l.start + l.len;
        if l.start < split {
            head.push_copy(l, l.start, end.min(split) - l.start);
        }
        if end > split {
            let from = l.start.max(split);
            tail_links
                .get_or_insert_default()
                .push_copy(l, from - split, end - from);
        }
    }
    drop(links);
    run.put_links(Some(head));
    tail.put_links(tail_links);
}

fn split_atomics(run: BoxRef<'_>, tail: BoxRef<'_>, split: usize) {
    let Some(atomics) = run.take_atomics() else {
        return;
    };
    let mut head: Vec<InlineAtomic> = Vec::new();
    let mut tail_atomics = tail.take_atomics();
    for mut ia in atomics {
        if ia.byte_off() < split {
            head.push(ia);
            continue;
        }
        let dst = tail_atomics.get_or_insert_default();
        ia.set_byte_off(ia.byte_off() - split);
        if let Some(ab) = ia.box_handle()
            && same(ab.parent(), run)
        {
            ab.set_parent(Some(tail));
        }
        dst.push(ia);
    }
    run.put_atomics(Some(&head));
    tail.put_atomics(tail_atomics.as_deref());
}

pub fn run_split(run: BoxRef<'_>, split: usize) -> BoxRef<'static> {
    let tail = BoxPool::new_inline();
    tail.set_dom(run.dom());
    tail.set_style(run.style());
    tail.set_inline_split_tail(true);
    tail.set_text_bytes(&text_bytes(run)[split..]);
    run.truncate_text(split);

    split_attrs(run, tail, split);
    split_links(run, tail, split);
    split_atomics(run, tail, split);

    let parent = run.parent();
    tail.set_parent(parent);
    tail.set_next_sibling(run.next_sibling());
    run.set_next_sibling(Some(tail));
    if let Some(parent) = parent
        && same(parent.last_child(), run)
    {
        parent.set_last_child(Some(tail));
    }
    run.invalidate_inline_caches();
    tail
}

pub fn join_splits(b: BoxRef<'_>) {
    let mut prev: Option<BoxRef<'_>> = None;
    let mut cur = b.first_child();
    while let Some(c) = cur {
        let next = c.next_sibling();
        let Some(n) = next.filter(|n| {
            n.inline_split_tail() && c.kind() == BoxKind::Inline && n.kind() == BoxKind::Inline
        }) else {
            prev = Some(c);
            cur = next;
            continue;
        };
        let head_is_tail = c.inline_split_tail();
        let after = n.next_sibling();
        let joined = merge_prefix(Some(c), Some(n)).expect("both runs present");
        joined.set_inline_split_tail(head_is_tail);
        joined.set_parent(Some(b));
        joined.set_next_sibling(after);
        match prev {
            Some(p) => p.set_next_sibling(Some(joined)),
            None => b.set_first_child(Some(joined)),
        }
        if after.is_none() {
            b.set_last_child(Some(joined));
        }
        joined.invalidate_inline_caches();
        cur = Some(joined);
    }
}

pub fn append_child<'a>(parent: BoxRef<'a>, child: BoxRef<'a>) {
    if let Some(prefix) = parent.last_child()
        && prefix.kind() == BoxKind::Inline
        && child.kind() == BoxKind::Inline
        && prefix.style().is_null()
        && child.style().is_null()
    {
        let mut previous = None;
        let mut c = parent.first_child();
        while let Some(x) = c.filter(|x| !x.same(prefix)) {
            previous = Some(x);
            c = x.next_sibling();
        }
        let merged = merge_prefix(Some(prefix), Some(child)).expect("both runs present");
        merged.set_parent(Some(parent));
        match previous {
            Some(p) => p.set_next_sibling(Some(merged)),
            None => parent.set_first_child(Some(merged)),
        }
        parent.set_last_child(Some(merged));
        return;
    }
    child.set_parent(Some(parent));
    match parent
        .last_child()
        .filter(|_| parent.first_child().is_some())
    {
        Some(last) => last.set_next_sibling(Some(child)),
        None => parent.set_first_child(Some(child)),
    }
    parent.set_last_child(Some(child));
}

fn range_end_clamped(start: usize, len: usize, cap: usize) -> usize {
    let end = if len > usize::MAX - start {
        usize::MAX
    } else {
        start + len
    };
    end.min(cap)
}

pub fn clone_range(src: BoxRef<'_>, start: usize, end: usize) -> Option<BoxRef<'static>> {
    let text = text_bytes(src);
    let text_len = text.len();
    if start >= text_len || end <= start {
        return None;
    }
    let end = end.min(text_len);
    let out = BoxPool::new_inline();
    out.set_dom(src.dom());
    out.set_style(src.style());
    out.set_text_bytes(&text[start..end]);

    let mut attrs = out.take_attrs();
    for a in &src.attrs() {
        let a_end = range_end_clamped(a.start, a.len, text_len);
        let is = a.start.max(start);
        let ie = a_end.min(end);
        if ie <= is {
            continue;
        }
        let mut copy = a.clone();
        copy.start = is - start;
        copy.len = ie - is;
        if let Some(bg) = copy.bg_image_src() {
            out.set_bg_image_if_unset(bg, copy.bg_image());
        }
        attrs.get_or_insert_default().push(copy);
    }
    out.put_attrs(attrs.as_deref());

    if src.has_links() {
        let mut links: Option<LinkVec> = None;
        for r in &src.links() {
            let r_end = range_end_clamped(r.start, r.len, text_len);
            let is = r.start.max(start);
            let ie = r_end.min(end);
            if ie <= is {
                continue;
            }
            links
                .get_or_insert_default()
                .push_copy(r, is - start, ie - is);
        }
        out.put_links(links);
    }
    Some(out)
}
