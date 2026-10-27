//! Southstar — the layout box, its inline attribute, link and atomic records, and the borrowed box handle every layout crate reads and writes it through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod image_map;
mod image_source;
mod media;
mod store;

pub use image_map::image_map_area;
pub use image_source::{choose_img, img_chosen_density, img_chosen_url};
pub use media::{MediaHandle, MediaRef, NsBoxMedia};
pub use store::{BoxPool, LinkVec};

use core::any::Any;
use core::cell::Cell;
use core::ffi::{CStr, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr;
use std::rc::Rc;

use southstar_dom::{Node, WeakNode};

#[derive(Clone, Copy, Debug, Default)]
pub struct GridTrack {
    pub start: f64,
    pub end: f64,
}

#[repr(C)]
pub struct Style {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoxKind {
    Block,
    Inline,
    Text,
    Image,
    Table,
    TableCaption,
    TableRow,
    TableCell,
    Video,
    Math,
    Svg,
    Other,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Edges {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

pub(crate) fn peek<T: ?Sized>(cell: &Cell<Option<Rc<T>>>) -> Option<Rc<T>> {
    let value = cell.take();
    let copy = value.clone();
    cell.set(value);
    copy
}

fn rc_slice<T: Clone>(items: &[T]) -> Rc<[T]> {
    Rc::from(items)
}

fn update_slice<T: Clone>(cell: &Cell<Option<Rc<[T]>>>, edit: impl FnOnce(&mut [T])) {
    let Some(mut items) = cell.take() else {
        return;
    };
    if Rc::get_mut(&mut items).is_none() {
        items = rc_slice(&items);
    }
    if let Some(unique) = Rc::get_mut(&mut items) {
        edit(unique);
    }
    cell.set(Some(items));
}

pub(crate) fn shared_cstr(s: Option<&CStr>) -> Option<Rc<CStr>> {
    s.map(Rc::from)
}

pub struct Shared<T>(Option<Rc<[T]>>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

impl<T> Default for Shared<T> {
    fn default() -> Self {
        Shared(None)
    }
}

impl<T> Deref for Shared<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        self.0.as_deref().unwrap_or(&[])
    }
}

impl<'s, T> IntoIterator for &'s Shared<T> {
    type Item = &'s T;
    type IntoIter = core::slice::Iter<'s, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone, Default)]
pub struct Text(Option<Rc<CStr>>);

impl Text {
    pub fn is_present(&self) -> bool {
        self.0.is_some()
    }
}

impl Deref for Text {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.0.as_deref().map_or(&[], CStr::to_bytes)
    }
}

#[derive(Clone, Copy)]
pub struct TableColHint {
    style: *const Style,
    span: c_int,
}

impl TableColHint {
    pub fn style(&self) -> *const Style {
        self.style
    }

    pub fn span(&self) -> c_int {
        self.span
    }
}

#[derive(Clone, Copy)]
pub struct InlineAtomic {
    byte_off: usize,
    b: BoxRef<'static>,
    owner_offset_x: f64,
    owner_offset_y: f64,
}

impl InlineAtomic {
    pub fn new(byte_off: usize, b: BoxRef<'static>) -> InlineAtomic {
        InlineAtomic {
            byte_off,
            b,
            owner_offset_x: 0.0,
            owner_offset_y: 0.0,
        }
    }

    pub fn byte_off(&self) -> usize {
        self.byte_off
    }

    pub fn box_ref(&self) -> Option<BoxRef<'static>> {
        Some(self.b)
    }

    pub fn box_handle(&self) -> Option<BoxRef<'static>> {
        Some(self.b)
    }

    pub fn owner_offset(&self) -> (f64, f64) {
        (self.owner_offset_x, self.owner_offset_y)
    }

    pub fn set_byte_off(&mut self, byte_off: usize) {
        self.byte_off = byte_off;
    }
}

#[derive(Clone)]
pub struct LinkRange {
    pub start: usize,
    pub len: usize,
    href: Option<Rc<CStr>>,
    target: Option<Rc<CStr>>,
    dom: Option<WeakNode>,
}

impl LinkRange {
    pub fn new(
        start: usize,
        len: usize,
        href: Option<&CStr>,
        target: Option<&CStr>,
        dom: Option<Node<'_>>,
    ) -> LinkRange {
        LinkRange {
            start,
            len,
            href: shared_cstr(href),
            target: shared_cstr(target),
            dom: dom.map(Node::downgrade),
        }
    }

    pub fn at(&self, start: usize, len: usize) -> LinkRange {
        LinkRange {
            start,
            len,
            ..self.clone()
        }
    }

    pub fn href(&self) -> Option<&CStr> {
        self.href.as_deref()
    }

    pub fn target(&self) -> Option<&CStr> {
        self.target.as_deref()
    }

    pub fn dom(&self) -> Option<Node<'static>> {
        self.dom.and_then(WeakNode::get)
    }

    pub fn dom_ptr(&self) -> *const c_void {
        Node::ptr_or_null(self.dom()).cast()
    }
}

pub mod inline_kind {
    use core::ffi::c_uint;

    pub const BOLD: c_uint = 0;
    pub const ITALIC: c_uint = 1;
    pub const MONOSPACE: c_uint = 2;
    pub const UNDERLINE: c_uint = 3;
    pub const OVERLINE: c_uint = 4;
    pub const STRIKETHROUGH: c_uint = 5;
    pub const INPUT_FIELD: c_uint = 6;
    pub const INPUT_FIELD_FOCUSED: c_uint = 7;
    pub const BUTTON: c_uint = 8;
    pub const CHECKBOX: c_uint = 9;
    pub const CHECKBOX_CHECKED: c_uint = 10;
    pub const RADIO: c_uint = 11;
    pub const RADIO_CHECKED: c_uint = 12;
    pub const PROGRESS: c_uint = 13;
    pub const METER: c_uint = 14;
    pub const FONT_SIZE: c_uint = 15;
    pub const FONT_WEIGHT: c_uint = 16;
    pub const FONT_STRETCH: c_uint = 17;
    pub const FONT_FEATURES: c_uint = 18;
    pub const FONT_VARIATIONS: c_uint = 19;
    pub const COLOR: c_uint = 20;
    pub const FONT_FAMILY: c_uint = 21;
    pub const BG_COLOR: c_uint = 22;
    pub const SUPERSCRIPT: c_uint = 23;
    pub const SUBSCRIPT: c_uint = 24;
    pub const SMALL_CAPS: c_uint = 25;
    pub const CARET: c_uint = 26;
    pub const SELECTION: c_uint = 27;
    pub const ELEMENT: c_uint = 28;
    pub const SPACER: c_uint = 29;
    pub const SPELLCHECK: c_uint = 30;
    pub const NO_WRAP: c_uint = 31;
}

#[derive(Clone)]
pub struct InlineAttr {
    pub kind: c_uint,
    pub start: usize,
    pub len: usize,
    pub font_size_px: f64,
    pub font_weight: c_int,
    pub font_stretch: c_int,
    pub font_kerning: c_int,
    font_ligatures: Option<Rc<CStr>>,
    font_features: Option<Rc<CStr>>,
    font_variations: Option<Rc<CStr>>,
    pub box_w: f64,
    pub box_h: f64,
    pub native_chrome: bool,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    family: Option<Rc<CStr>>,
    dom: Option<WeakNode>,
    style: *const Style,
    bg_image_src: Option<Rc<CStr>>,
    bg_image: Option<MediaHandle>,
}

impl InlineAttr {
    pub fn new(kind: c_uint, start: usize, len: usize) -> InlineAttr {
        InlineAttr {
            kind,
            start,
            len,
            font_size_px: 0.0,
            font_weight: 0,
            font_stretch: 0,
            font_kerning: 0,
            font_ligatures: None,
            font_features: None,
            font_variations: None,
            box_w: 0.0,
            box_h: 0.0,
            native_chrome: false,
            r: 0,
            g: 0,
            b: 0,
            a: 0,
            family: None,
            dom: None,
            style: ptr::null(),
            bg_image_src: None,
            bg_image: None,
        }
    }

    pub fn with_font_settings(
        mut self,
        ligatures: Option<&CStr>,
        features: Option<&CStr>,
        variations: Option<&CStr>,
    ) -> InlineAttr {
        self.font_ligatures = shared_cstr(ligatures);
        self.font_features = shared_cstr(features);
        self.font_variations = shared_cstr(variations);
        self
    }

    pub fn with_family(mut self, family: Option<&CStr>) -> InlineAttr {
        self.family = shared_cstr(family);
        self
    }

    pub fn with_dom(mut self, dom: Option<Node<'_>>) -> InlineAttr {
        self.dom = dom.map(Node::downgrade);
        self
    }

    pub fn with_style(mut self, style: *const Style) -> InlineAttr {
        self.style = style;
        self
    }

    pub fn with_bg_image(mut self, src: Option<&CStr>, image: Option<MediaHandle>) -> InlineAttr {
        self.bg_image_src = shared_cstr(src);
        self.bg_image = image;
        self
    }

    pub fn font_ligatures(&self) -> Option<&CStr> {
        self.font_ligatures.as_deref()
    }

    pub fn font_features(&self) -> Option<&CStr> {
        self.font_features.as_deref()
    }

    pub fn font_variations(&self) -> Option<&CStr> {
        self.font_variations.as_deref()
    }

    pub fn family(&self) -> Option<&CStr> {
        self.family.as_deref()
    }

    pub fn dom(&self) -> Option<Node<'static>> {
        self.dom.and_then(WeakNode::get)
    }

    pub fn dom_ptr(&self) -> *const c_void {
        Node::ptr_or_null(self.dom()).cast()
    }

    pub fn style(&self) -> *const Style {
        self.style
    }

    pub fn bg_image(&self) -> Option<MediaHandle> {
        self.bg_image
    }

    pub fn bg_image_src(&self) -> Option<&CStr> {
        self.bg_image_src.as_deref()
    }
}

macro_rules! box_fields {
    ($($name:ident: $ty:ty = $init:expr),* $(,)?) => {
        pub struct NsBox {
            $($name: Cell<$ty>,)*
            has_media: Cell<bool>,
            media: NsBoxMedia,
        }

        impl NsBox {
            fn vacant() -> NsBox {
                NsBox {
                    $($name: Cell::new($init),)*
                    has_media: Cell::new(false),
                    media: NsBoxMedia::vacant(),
                }
            }

            fn reset(&self) {
                $(self.$name.set($init);)*
                self.has_media.set(false);
                self.media.reset();
            }
        }
    };
}

type StyleCache = Option<(*const Style, f64, f64)>;
type WidthCache = Option<(*const Style, f64)>;

box_fields! {
    live: bool = false,
    kind: c_uint = 0,
    dom: Option<WeakNode> = None,
    style: *const Style = ptr::null(),
    x: f64 = 0.0,
    y: f64 = 0.0,
    rel_dx: f64 = 0.0,
    rel_dy: f64 = 0.0,
    content_width: f64 = 0.0,
    content_height: f64 = 0.0,
    first_baseline: f64 = 0.0,
    definite_height: f64 = 0.0,
    definite_height_before_flex: f64 = 0.0,
    flex_pass_x: f64 = 0.0,
    flex_pass_y: f64 = 0.0,
    last_layout_width: f64 = 0.0,
    definite_height_read: bool = false,
    measured_content_height: f64 = 0.0,
    cb_height_override: f64 = 0.0,
    flex_main: Option<f64> = None,
    is_rendered_legend: bool = false,
    inline_split_tail: bool = false,
    margin_top_through: f64 = 0.0,
    paint_top: f64 = 0.0,
    paint_bottom: f64 = 0.0,
    margin: Edges = Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
    padding: Edges = Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
    border: Edges = Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
    scroll_x: f64 = 0.0,
    scroll_y: f64 = 0.0,
    scroll_max_x: f64 = 0.0,
    scroll_max_y: f64 = 0.0,
    scrolls: bool = false,
    text: Option<Rc<CStr>> = None,
    inline_layout_cache: StyleCache = None,
    vertical_wm: c_int = 0,
    text_orient: c_int = 0,
    inline_natural_cache: WidthCache = None,
    inline_min_cache: WidthCache = None,
    paint_layout: Option<Rc<dyn Any>> = None,
    links: Option<Rc<[LinkRange]>> = None,
    attrs: Option<Rc<[InlineAttr]>> = None,
    inline_atomics: Option<Rc<[InlineAtomic]>> = None,
    atomic_line_heights: Option<Rc<[f64]>> = None,
    table_col_hints: Option<Rc<[TableColHint]>> = None,
    grid_col_tracks: Option<Rc<[GridTrack]>> = None,
    grid_row_tracks: Option<Rc<[GridTrack]>> = None,
    grid_explicit_cols: c_int = 0,
    grid_explicit_rows: c_int = 0,
    svg_styles: *const southstar_css::StyleMap = ptr::null(),
    colspan: c_int = 1,
    rowspan: c_int = 1,
    columns: c_int = 0,
    parent: Option<&'static NsBox> = None,
    first_child: Option<&'static NsBox> = None,
    last_child: Option<&'static NsBox> = None,
    next_sibling: Option<&'static NsBox> = None,
    pseudo_host: Option<(WeakNode, bool)> = None,
}

#[derive(Clone, Copy)]
pub struct BoxRef<'a>(&'static NsBox, PhantomData<&'a NsBox>);

impl<'a> BoxRef<'a> {
    fn of(b: &'static NsBox) -> BoxRef<'a> {
        BoxRef(b, PhantomData)
    }

    fn link(b: Option<&'static NsBox>) -> Option<BoxRef<'a>> {
        b.map(BoxRef::of)
    }

    /// # Safety
    ///
    /// `b` is null or a pointer `BoxRef::as_ptr` returned on this thread.
    pub unsafe fn from_ptr(b: *const NsBox) -> Option<BoxRef<'a>> {
        // SAFETY: box slots are never deallocated, and the caller passes null or one of this thread's slots.
        unsafe { b.as_ref() }.map(BoxRef::of)
    }

    pub fn as_ptr(self) -> *const NsBox {
        ptr::from_ref(self.0)
    }

    pub fn into_static(self) -> BoxRef<'static> {
        BoxRef::of(self.0)
    }

    pub fn as_mut_ptr(self) -> *mut NsBox {
        self.as_ptr().cast_mut()
    }

    fn raw(self) -> &'static NsBox {
        self.0
    }

    pub fn kind(self) -> BoxKind {
        match self.raw().kind.get() {
            0 => BoxKind::Block,
            1 => BoxKind::Inline,
            2 => BoxKind::Text,
            3 => BoxKind::Image,
            4 => BoxKind::Table,
            5 => BoxKind::TableCaption,
            6 => BoxKind::TableRow,
            7 => BoxKind::TableCell,
            8 => BoxKind::Video,
            9 => BoxKind::Math,
            10 => BoxKind::Svg,
            _ => BoxKind::Other,
        }
    }

    pub fn kind_raw(self) -> c_uint {
        self.raw().kind.get()
    }

    pub fn style(self) -> *const Style {
        self.raw().style.get()
    }

    pub fn dom_ptr(self) -> *const c_void {
        Node::ptr_or_null(self.dom()).cast()
    }

    pub fn dom(self) -> Option<Node<'a>> {
        self.raw().dom.get().and_then(WeakNode::get)
    }

    pub fn pseudo_host(self) -> Option<(Node<'a>, bool)> {
        let (host, after) = self.raw().pseudo_host.get()?;
        host.get().map(|node| (node, after))
    }

    pub fn set_pseudo_host(self, node: Node<'_>, after: bool) {
        self.raw().pseudo_host.set(Some((node.downgrade(), after)));
    }

    pub fn media(self) -> Option<MediaRef<'a>> {
        self.raw()
            .has_media
            .get()
            .then(|| MediaRef::of(&self.raw().media))
    }

    pub fn ensure_media(self) -> MediaRef<'a> {
        self.raw().has_media.set(true);
        MediaRef::of(&self.raw().media)
    }

    pub fn inline_atomic_boxes(self) -> Vec<BoxRef<'a>> {
        self.inline_atomics()
            .unwrap_or_default()
            .iter()
            .map(|a| BoxRef::of(a.b.0))
            .collect()
    }

    pub fn x(self) -> f64 {
        self.raw().x.get()
    }

    pub fn y(self) -> f64 {
        self.raw().y.get()
    }

    pub fn content_width(self) -> f64 {
        self.raw().content_width.get()
    }

    pub fn content_height(self) -> f64 {
        self.raw().content_height.get()
    }

    pub fn scroll_x(self) -> f64 {
        self.raw().scroll_x.get()
    }

    pub fn scroll_y(self) -> f64 {
        self.raw().scroll_y.get()
    }

    pub fn scroll_max_x(self) -> f64 {
        self.raw().scroll_max_x.get()
    }

    pub fn scroll_max_y(self) -> f64 {
        self.raw().scroll_max_y.get()
    }

    pub fn scrolls(self) -> bool {
        self.raw().scrolls.get()
    }

    pub fn set_scroll(self, x: f64, y: f64) {
        self.raw().scroll_y.set(y);
        self.raw().scroll_x.set(x);
    }

    pub fn margin(self) -> Edges {
        self.raw().margin.get()
    }

    pub fn padding(self) -> Edges {
        self.raw().padding.get()
    }

    pub fn border(self) -> Edges {
        self.raw().border.get()
    }

    pub fn text(self) -> Option<Rc<CStr>> {
        peek(&self.raw().text)
    }

    pub fn text_bytes(self) -> Text {
        Text(self.text())
    }

    pub fn parent(self) -> Option<BoxRef<'a>> {
        BoxRef::link(self.raw().parent.get())
    }

    pub fn first_child(self) -> Option<BoxRef<'a>> {
        BoxRef::link(self.raw().first_child.get())
    }

    pub fn next_sibling(self) -> Option<BoxRef<'a>> {
        BoxRef::link(self.raw().next_sibling.get())
    }

    pub fn last_child(self) -> Option<BoxRef<'a>> {
        BoxRef::link(self.raw().last_child.get())
    }

    pub fn max_bottom(self, seed: f64) -> f64 {
        (crate::dispatch::get().max_bottom)(self, seed)
    }

    pub fn clips_out_point(self, x: f64, y: f64) -> bool {
        (crate::dispatch::get().clips_out_point)(self, x, y)
    }

    pub fn same(self, other: BoxRef<'_>) -> bool {
        ptr::eq(self.0, other.0)
    }

    pub fn rel_dx(self) -> f64 {
        self.raw().rel_dx.get()
    }

    pub fn rel_dy(self) -> f64 {
        self.raw().rel_dy.get()
    }

    pub fn set_rel_offset(self, dx: f64, dy: f64) {
        self.raw().rel_dx.set(dx);
        self.raw().rel_dy.set(dy);
    }

    pub fn paint_top(self) -> f64 {
        self.raw().paint_top.get()
    }

    pub fn paint_bottom(self) -> f64 {
        self.raw().paint_bottom.get()
    }

    pub fn set_paint_bounds(self, top: f64, bottom: f64) {
        self.raw().paint_top.set(top);
        self.raw().paint_bottom.set(bottom);
    }

    pub fn vertical_wm(self) -> c_int {
        self.raw().vertical_wm.get()
    }

    pub fn text_orient(self) -> c_int {
        self.raw().text_orient.get()
    }

    pub fn columns(self) -> c_int {
        self.raw().columns.get()
    }

    pub fn svg_styles(self) -> *const southstar_css::StyleMap {
        self.raw().svg_styles.get()
    }

    pub fn paint_layout(self) -> Option<Rc<dyn Any>> {
        peek(&self.raw().paint_layout)
    }

    pub fn has_paint_layout(self) -> bool {
        let layout = self.raw().paint_layout.take();
        let has = layout.is_some();
        self.raw().paint_layout.set(layout);
        has
    }

    pub fn set_paint_layout(self, layout: Option<Rc<dyn Any>>) {
        self.raw().paint_layout.set(layout);
    }

    pub fn attrs(self) -> Shared<InlineAttr> {
        Shared(peek(&self.raw().attrs))
    }

    pub fn links(self) -> Shared<LinkRange> {
        Shared(peek(&self.raw().links))
    }

    pub fn has_attrs(self) -> bool {
        let attrs = self.raw().attrs.take();
        let has = attrs.is_some();
        self.raw().attrs.set(attrs);
        has
    }

    pub fn inline_atomics(self) -> Option<Shared<InlineAtomic>> {
        peek(&self.raw().inline_atomics).map(|a| Shared(Some(a)))
    }

    pub fn set_atomic_owner_offset(self, index: usize, x: f64, y: f64) {
        update_slice(&self.raw().inline_atomics, |atomics| {
            if let Some(atomic) = atomics.get_mut(index) {
                atomic.owner_offset_x = x;
                atomic.owner_offset_y = y;
            }
        });
    }

    pub fn colspan(self) -> c_int {
        self.raw().colspan.get()
    }

    pub fn rowspan(self) -> c_int {
        self.raw().rowspan.get()
    }

    pub fn table_col_hints(self) -> Shared<TableColHint> {
        Shared(peek(&self.raw().table_col_hints))
    }

    pub fn set_x(self, x: f64) {
        self.raw().x.set(x);
    }

    pub fn set_y(self, y: f64) {
        self.raw().y.set(y);
    }

    pub fn set_content_width(self, w: f64) {
        self.raw().content_width.set(w);
    }

    pub fn set_content_height(self, h: f64) {
        self.raw().content_height.set(h);
    }

    pub fn set_margin(self, e: Edges) {
        self.raw().margin.set(e);
    }

    pub fn take_flex_main(self) -> Option<f64> {
        self.raw().flex_main.take()
    }

    pub fn margin_top_through(self) -> f64 {
        self.raw().margin_top_through.get()
    }

    pub fn set_margin_top_through(self, v: f64) {
        self.raw().margin_top_through.set(v);
    }

    pub fn set_columns(self, n: c_int) {
        self.raw().columns.set(n);
    }

    pub fn set_scrolls(self, scrolls: bool) {
        self.raw().scrolls.set(scrolls);
    }

    pub fn set_scroll_max(self, x: Option<f64>, y: Option<f64>) {
        if let Some(x) = x {
            self.raw().scroll_max_x.set(x);
        }
        if let Some(y) = y {
            self.raw().scroll_max_y.set(y);
        }
    }

    pub fn begin_layout(self, parent_content_width: f64) {
        self.raw().definite_height_read.set(false);
        self.raw().last_layout_width.set(parent_content_width);
    }

    pub fn translate_grid_tracks(self, dx: f64, dy: f64) {
        let b = self.raw();
        for (tracks, d) in [(&b.grid_col_tracks, dx), (&b.grid_row_tracks, dy)] {
            update_slice(tracks, |tracks| {
                for track in tracks {
                    track.start += d;
                    track.end += d;
                }
            });
        }
    }

    pub fn detach_child(self, child: BoxRef<'_>) {
        let parent = self.raw();
        let child = child.raw();
        let mut prev: Option<&'static NsBox> = None;
        let mut c = parent.first_child.get();
        while let Some(cur) = c
            && !ptr::eq(cur, child)
        {
            prev = Some(cur);
            c = cur.next_sibling.get();
        }
        match prev {
            None => parent.first_child.set(child.next_sibling.get()),
            Some(prev) => prev.next_sibling.set(child.next_sibling.get()),
        }
        if parent
            .last_child
            .get()
            .is_some_and(|last| ptr::eq(last, child))
        {
            parent.last_child.set(prev);
        }
        child.next_sibling.set(None);
    }

    pub fn prepend_child(self, child: BoxRef<'_>) {
        let parent = self.raw();
        let child = child.raw();
        child.next_sibling.set(parent.first_child.get());
        parent.first_child.set(Some(child));
        if parent.last_child.get().is_none() {
            parent.last_child.set(Some(child));
        }
    }

    pub fn set_padding(self, e: Edges) {
        self.raw().padding.set(e);
    }

    pub fn is_rendered_legend(self) -> bool {
        self.raw().is_rendered_legend.get()
    }

    pub fn set_rendered_legend(self, legend: bool) {
        self.raw().is_rendered_legend.set(legend);
    }

    pub fn set_svg_styles(self, styles: *const southstar_css::StyleMap) {
        self.raw().svg_styles.set(styles);
    }

    pub fn read_definite_height(self) -> f64 {
        self.raw().definite_height_read.set(true);
        self.raw().definite_height.get()
    }

    pub fn cb_height_override(self) -> f64 {
        self.raw().cb_height_override.get()
    }

    pub fn set_border(self, e: Edges) {
        self.raw().border.set(e);
    }

    pub fn definite_height(self) -> f64 {
        self.raw().definite_height.get()
    }

    pub fn set_definite_height(self, h: f64) {
        self.raw().definite_height.set(h);
    }

    pub fn definite_height_before_flex(self) -> f64 {
        self.raw().definite_height_before_flex.get()
    }

    pub fn set_definite_height_before_flex(self, h: f64) {
        self.raw().definite_height_before_flex.set(h);
    }

    pub fn definite_height_read(self) -> bool {
        self.raw().definite_height_read.get()
    }

    pub fn flex_pass(self) -> (f64, f64) {
        (self.raw().flex_pass_x.get(), self.raw().flex_pass_y.get())
    }

    pub fn set_flex_pass(self, x: f64, y: f64) {
        self.raw().flex_pass_x.set(x);
        self.raw().flex_pass_y.set(y);
    }

    pub fn last_layout_width(self) -> f64 {
        self.raw().last_layout_width.get()
    }

    pub fn measured_content_height(self) -> f64 {
        self.raw().measured_content_height.get()
    }

    pub fn set_measured_content_height(self, h: f64) {
        self.raw().measured_content_height.set(h);
    }

    pub fn set_flex_main(self, size: f64) {
        self.raw().flex_main.set(Some(size));
    }

    pub fn set_scroll_overflow_y(self, max_y: f64) {
        self.raw().scrolls.set(true);
        self.raw().scroll_max_y.set(max_y);
    }

    pub fn set_cb_height_override(self, h: f64) {
        self.raw().cb_height_override.set(h);
    }

    pub fn grid_tracks(self, columns: bool) -> Option<Shared<GridTrack>> {
        let tracks = if columns {
            &self.raw().grid_col_tracks
        } else {
            &self.raw().grid_row_tracks
        };
        peek(tracks).map(|t| Shared(Some(t)))
    }

    pub fn set_grid_tracks(self, cols: Vec<GridTrack>, rows: Vec<GridTrack>) {
        self.raw().grid_col_tracks.set(Some(Rc::from(cols)));
        self.raw().grid_row_tracks.set(Some(Rc::from(rows)));
    }

    pub fn grid_explicit(self) -> (c_int, c_int) {
        (
            self.raw().grid_explicit_cols.get(),
            self.raw().grid_explicit_rows.get(),
        )
    }

    pub fn set_grid_explicit(self, cols: c_int, rows: c_int) {
        self.raw().grid_explicit_cols.set(cols);
        self.raw().grid_explicit_rows.set(rows);
    }

    pub fn first_baseline(self) -> f64 {
        self.raw().first_baseline.get()
    }

    pub fn set_first_baseline(self, v: f64) {
        self.raw().first_baseline.set(v);
    }

    pub fn set_writing_mode(self, vertical_wm: c_int, text_orient: c_int) {
        self.raw().vertical_wm.set(vertical_wm);
        self.raw().text_orient.set(text_orient);
    }

    pub fn inline_layout_cache(self) -> StyleCache {
        self.raw().inline_layout_cache.get()
    }

    pub fn set_inline_layout_cache(self, cache: StyleCache) {
        self.raw().inline_layout_cache.set(cache);
    }

    pub fn inline_natural_cache(self) -> WidthCache {
        self.raw().inline_natural_cache.get()
    }

    pub fn set_inline_natural_cache(self, style: *const Style, width: f64) {
        self.raw().inline_natural_cache.set(Some((style, width)));
    }

    pub fn inline_min_cache(self) -> WidthCache {
        self.raw().inline_min_cache.get()
    }

    pub fn set_inline_min_cache(self, style: *const Style, width: f64) {
        self.raw().inline_min_cache.set(Some((style, width)));
    }

    pub fn set_atomic_line_heights(self, heights: &[f64]) {
        self.raw().atomic_line_heights.set(Some(Rc::from(heights)));
    }

    pub fn atomic_line_heights(self) -> Option<Shared<f64>> {
        peek(&self.raw().atomic_line_heights).map(|h| Shared(Some(h)))
    }
}
