//! Southstar — the state one box-tree build carries: focus and caret, the image cache and base URL, the pending absolutely positioned boxes and their placeholders, and the nesting the builders track.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ptr;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use southstar_dom::{Node, NsNode, WeakNode};
use southstar_layout::{BoxPool, BoxRef, Style};

pub struct AbsEntry {
    pub dom: Option<WeakNode>,
    pub pseudo: *const Style,
    pub fixed: bool,
}

#[derive(Clone, Copy)]
pub struct AbsStatic {
    pub run: BoxRef<'static>,
    pub rel_x: f64,
    pub rel_y: f64,
}

pub struct State {
    pub focused_input: *const NsNode,
    pub focused_is_contenteditable: bool,
    pub caret_byte: usize,
    pub anchor_byte: usize,
    pub image_cache: Option<southstar_image::ImageCache>,
    pub base_url: Option<CString>,
    pub abs_pending: Option<Vec<AbsEntry>>,
    pub abs_force_build: bool,
    pub abs_seen: Option<HashSet<usize>>,
    pub abs_placeholders: Option<HashMap<usize, *const NsNode>>,
    pub abs_static: Option<HashMap<usize, AbsStatic>>,
    pub form_control_inline: *const NsNode,
    pub inline_skip: *const NsNode,
    pub pseudo_blocks_host: *const NsNode,
    pub pseudo_block_before: bool,
    pub pseudo_block_after: bool,
    pub inline_collect_depth: i32,
    pub build_block_depth: i32,
    pub contents_depth: i32,
    pub blockified_legend: *const NsNode,
    pub block_media_cache: Option<HashMap<usize, bool>>,
}

impl State {
    const fn new() -> State {
        State {
            focused_input: ptr::null(),
            focused_is_contenteditable: false,
            caret_byte: 0,
            anchor_byte: 0,
            image_cache: None,
            base_url: None,
            abs_pending: None,
            abs_force_build: false,
            abs_seen: None,
            abs_placeholders: None,
            abs_static: None,
            form_control_inline: ptr::null(),
            inline_skip: ptr::null(),
            pseudo_blocks_host: ptr::null(),
            pseudo_block_before: false,
            pseudo_block_after: false,
            inline_collect_depth: 0,
            build_block_depth: 0,
            contents_depth: 0,
            blockified_legend: ptr::null(),
            block_media_cache: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = const { RefCell::new(State::new()) };
}

static OPEN_SELECT: AtomicPtr<NsNode> = AtomicPtr::new(ptr::null_mut());
static DATALIST_OPEN: AtomicBool = AtomicBool::new(false);

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with_borrow_mut(f)
}

pub fn set_open_select(select: *const NsNode) {
    OPEN_SELECT.store(select.cast_mut(), Ordering::Relaxed);
}

pub fn open_select() -> *const NsNode {
    OPEN_SELECT.load(Ordering::Relaxed)
}

pub fn set_datalist_open(open: bool) {
    DATALIST_OPEN.store(open, Ordering::Relaxed);
}

pub fn datalist_open() -> bool {
    DATALIST_OPEN.load(Ordering::Relaxed)
}

pub fn queue_absolute(dom: Node<'_>, fixed: bool) {
    with(|st| {
        if st.abs_pending.is_none() {
            return;
        }
        if !st
            .abs_seen
            .as_mut()
            .is_none_or(|seen| seen.insert(dom.as_ptr() as usize))
        {
            return;
        }
        if let Some(pending) = st.abs_pending.as_mut() {
            pending.push(AbsEntry {
                dom: Some(dom.downgrade()),
                pseudo: ptr::null(),
                fixed,
            });
        }
    });
}

#[derive(Clone, Copy)]
pub struct CollectState {
    pub focused_input: *const NsNode,
    pub open_select: *const NsNode,
    pub inline_skip: *const NsNode,
    pub pseudo_blocks_host: *const NsNode,
    pub form_control_inline: *const NsNode,
    pub caret_byte: usize,
    pub anchor_byte: usize,
    pub inline_collect_depth: i32,
    pub focused_is_contenteditable: bool,
    pub datalist_open: bool,
    pub pseudo_block_before: bool,
    pub pseudo_block_after: bool,
}

pub fn collect_state() -> CollectState {
    let (open_select, datalist_open) = (open_select(), datalist_open());
    with(|st| CollectState {
        focused_input: st.focused_input,
        open_select,
        inline_skip: st.inline_skip,
        pseudo_blocks_host: st.pseudo_blocks_host,
        form_control_inline: st.form_control_inline,
        caret_byte: st.caret_byte,
        anchor_byte: st.anchor_byte,
        inline_collect_depth: st.inline_collect_depth,
        focused_is_contenteditable: st.focused_is_contenteditable,
        datalist_open,
        pseudo_block_before: st.pseudo_block_before,
        pseudo_block_after: st.pseudo_block_after,
    })
}

pub fn collect_absolute(dom: Node<'_>, fixed: bool, placeholder: bool) -> Option<BoxRef<'static>> {
    queue_absolute(dom, fixed);
    if !placeholder {
        return None;
    }
    abs_placeholder(dom.as_ptr())
}

pub fn abs_placeholder(key: *const NsNode) -> Option<BoxRef<'static>> {
    if with(|st| st.abs_placeholders.is_none()) {
        return None;
    }
    let ph = BoxPool::new_box(0);
    with(|st| {
        if let Some(set) = st.abs_placeholders.as_mut() {
            set.insert(ph.as_ptr() as usize, key);
        }
    });
    Some(ph)
}
