//! Southstar — the style-engine side of the animation engine: refcounted values, declaration blocks, @keyframes rules, transition and animation lists and computed styles, read through rust/css's own types.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::ptr::NonNull;

pub use southstar_css::Keyframes;
use southstar_css::{
    AnimationEntry, Declaration, Declarations, NsCssValue, OwnedValue, Prop as CssProp, Sheet,
    StyleData, StyleMap, ValueKind,
};
use southstar_dom::{Node, NsNode};
use southstar_layout::Style;
use southstar_style::{StyleRef, UNIT_PERCENT, UNIT_PX};

use crate::timing::Timing;

pub const PROP_COUNT: usize = southstar_style::PROP_COUNT;

#[derive(Clone)]
pub struct Val(OwnedValue);

impl Val {
    pub fn retain(v: Borrowed) -> Option<Val> {
        v.0.map(Val)
    }

    pub fn into_owned(self) -> OwnedValue {
        self.0
    }

    pub fn placeholder() -> Val {
        Val(OwnedValue::placeholder())
    }

    pub fn borrow(&self) -> Borrowed {
        Borrowed(Some(self.0.clone()))
    }

    pub fn font_units_resolved(&self, font_px: f64, root_px: f64) -> Option<Val> {
        southstar_css::value_resolve_font_units(&self.0, font_px, root_px).map(Val)
    }

    pub fn interpolate(
        prop: i32,
        a: &Borrowed,
        b: &Borrowed,
        t: f64,
        keywords: bool,
    ) -> Option<Val> {
        southstar_css::value_interpolate(prop_at(prop), a.get()?, b.get()?, t, keywords).map(Val)
    }
}

#[derive(Clone)]
pub struct Borrowed(Option<OwnedValue>);

impl PartialEq for Borrowed {
    fn eq(&self, other: &Borrowed) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => OwnedValue::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for Borrowed {}

impl From<&OwnedValue> for Borrowed {
    fn from(v: &OwnedValue) -> Borrowed {
        Borrowed(Some(v.clone()))
    }
}

impl Borrowed {
    pub const NULL: Borrowed = Borrowed(None);

    pub fn of(v: Option<&Val>) -> Borrowed {
        v.map_or(Borrowed::NULL, Val::borrow)
    }

    pub fn is_null(&self) -> bool {
        self.0.is_none()
    }

    fn get(&self) -> Option<&NsCssValue> {
        self.0.as_deref()
    }

    pub fn equal(&self, other: &Borrowed) -> bool {
        southstar_css::values_equal(self.get(), other.get())
    }

    pub fn is_placeholder(&self) -> bool {
        self.get()
            .is_some_and(|v| v.value_kind() == ValueKind::Keyword && v.keyword().is_none())
    }

    pub fn keyword(&self) -> Option<&CStr> {
        self.get()?.keyword()
    }

    pub fn px(&self) -> Option<f64> {
        let (v, unit) = self.get()?.length()?;
        (unit == UNIT_PX).then_some(v)
    }

    pub fn opacity(&self) -> Option<f64> {
        let (v, unit) = self.get()?.length()?;
        Some(if unit == UNIT_PERCENT { v / 100.0 } else { v })
    }

    pub fn color(&self) -> Option<[u8; 4]> {
        self.get()?.color_rgba()
    }

    pub fn transform(&self) -> Option<southstar_style::Transform> {
        let transform = self.get()?.transform()?;
        (transform.n_ops != 0).then_some(*transform)
    }
}

fn prop_at(prop: i32) -> Option<CssProp> {
    usize::try_from(prop).ok().and_then(CssProp::from_id)
}

pub fn prop_name(prop: i32) -> Option<&'static CStr> {
    prop_at(prop).map(CssProp::c_name)
}

pub fn prop_id(name: &CStr) -> i32 {
    prop_id_of(name.to_bytes())
}

pub fn prop_id_of(name: &[u8]) -> i32 {
    CssProp::from_name(name).map_or(-1, |prop| prop.id() as i32)
}

pub fn prop_inherits(prop: i32) -> bool {
    prop_at(prop).is_some_and(CssProp::inherits)
}

pub fn reduced_motion() -> bool {
    southstar_css_media::prefers_reduced_motion()
}

pub fn initial_value(prop: i32) -> Option<Val> {
    let name = prop_name(prop)?;
    let text = southstar_css::initial_value(name.to_bytes())?;
    let mut decl = name.to_bytes().to_vec();
    decl.extend_from_slice(b": ");
    decl.extend_from_slice(text.to_bytes());
    let first = Declarations::parse(&decl).into_first()?;
    (first.prop() == prop).then(|| Val(first.into_value()))
}

#[derive(Clone)]
pub struct Decls(Declarations);

impl Decls {
    pub fn parse(text: Option<&[u8]>) -> Option<Decls> {
        Some(Decls(
            text.map_or_else(Declarations::default, Declarations::parse),
        ))
    }

    pub fn declarations(&self) -> &Declarations {
        &self.0
    }

    pub fn props(&self) -> impl Iterator<Item = i32> + '_ {
        self.0.iter().map(Declaration::prop)
    }

    pub fn value_of(&self, prop: i32) -> Borrowed {
        self.0
            .iter()
            .rfind(|d| d.prop() == prop)
            .map_or(Borrowed::NULL, |d| Borrowed::from(d.value_handle()))
    }

    pub fn drop_important(&mut self) {
        self.0.remove_important();
    }

    pub fn duplicate(&self) -> Decls {
        self.clone()
    }
}

pub fn sheet_keyframes(sheet: &Sheet) -> Vec<(Vec<u8>, Keyframes)> {
    sheet
        .keyframes()
        .map(|kf| (kf.name().to_vec(), kf))
        .collect()
}

#[derive(Clone, Copy)]
pub struct Entry<'a>(&'a AnimationEntry);

pub const TARGET_ALL: c_int = 1;
const TARGET_OPACITY: c_int = 2;
const TARGET_TRANSFORM: c_int = 3;
const TARGET_COLOR: c_int = 4;
const TARGET_BG_COLOR: c_int = 5;
const TARGET_OTHER: c_int = 6;

pub enum Target<'a> {
    All,
    Opacity,
    Transform,
    Color,
    BackgroundColor,
    Other(Option<&'a [u8]>),
    None,
}

impl<'a> Entry<'a> {
    pub fn target(self) -> Target<'a> {
        match self.0.target {
            TARGET_ALL => Target::All,
            TARGET_OPACITY => Target::Opacity,
            TARGET_TRANSFORM => Target::Transform,
            TARGET_COLOR => Target::Color,
            TARGET_BG_COLOR => Target::BackgroundColor,
            TARGET_OTHER => Target::Other(self.name()),
            _ => Target::None,
        }
    }

    pub fn name(self) -> Option<&'a [u8]> {
        self.0.name.as_deref()
    }

    pub fn duration_ms(self) -> f64 {
        self.0.duration_ms
    }

    pub fn delay_ms(self) -> f64 {
        self.0.delay_ms
    }

    pub fn timing(self) -> Timing {
        Timing::of(&self.0.timing)
    }

    pub fn iterations(self) -> f64 {
        self.0.iterations
    }

    pub fn direction(self) -> c_int {
        self.0.direction
    }

    pub fn fill(self) -> c_int {
        self.0.fill
    }

    pub fn paused(self) -> bool {
        self.0.paused
    }

    pub fn allow_discrete(self) -> bool {
        self.0.allow_discrete
    }

    pub fn document_timeline(self) -> bool {
        self.0.document_timeline
    }
}

pub struct AnimList(Vec<AnimationEntry>);

impl AnimList {
    pub fn effective(style: StyleRef<'_>, animations: bool) -> AnimList {
        AnimList(southstar_css::animation_effective(style.data(), animations))
    }

    pub fn entries(&self) -> Vec<Entry<'_>> {
        self.0.iter().map(Entry).collect()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

pub struct RetainedStyle(southstar_css::RetainedStyle);

impl RetainedStyle {
    pub fn retain(style: StyleRef<'_>) -> RetainedStyle {
        // SAFETY: a StyleRef is a live computed style made by rust/css.
        let retained = unsafe { southstar_css::RetainedStyle::retain(style.as_ptr()) };
        RetainedStyle(retained.expect("a live style"))
    }

    pub fn get(&self) -> StyleRef<'_> {
        StyleRef::retained(&self.0)
    }

    pub fn ptr(&self) -> *const Style {
        self.0.as_ptr().cast()
    }
}

pub fn style_value(style: StyleRef<'_>, prop: i32) -> Borrowed {
    Borrowed(
        usize::try_from(prop)
            .ok()
            .and_then(|i| style.data().value_handle(i))
            .cloned(),
    )
}

pub fn interpolates_keywords(style: StyleRef<'_>) -> bool {
    style
        .get(southstar_style::PropId::InterpolateSize)
        .is_some_and(|v| v.is_keyword(c"allow-keywords"))
}

pub fn from_currentcolor(style: StyleRef<'_>, prop: i32) -> bool {
    prop_at(prop).is_some_and(|prop| style.from_currentcolor(prop))
}

pub fn may_animate(style: StyleRef<'_>) -> bool {
    southstar_css::style_may_animate(style.data())
}

pub struct StyleMut(NonNull<StyleData>);

impl StyleMut {
    pub fn of(style: StyleRef<'_>) -> StyleMut {
        StyleMut(NonNull::new(style.as_ptr().cast::<StyleData>().cast_mut()).expect("a live style"))
    }

    fn with<R>(&self, f: impl FnOnce(&mut StyleData) -> R) -> R {
        // SAFETY: a StyleMut is made from a live computed style, which the engine alone writes during a pass, and no reference into it outlives this call.
        f(unsafe { &mut *self.0.as_ptr() })
    }

    pub fn value(&self, prop: i32) -> Borrowed {
        let Ok(index) = usize::try_from(prop) else {
            return Borrowed::NULL;
        };
        Borrowed(self.with(|style| style.value_handle(index).cloned()))
    }

    pub fn replace(&self, prop: i32, value: Val) -> Option<Val> {
        let index = usize::try_from(prop).ok()?;
        self.with(|style| style.replace_value(index, Some(value.0)))
            .map(Val)
    }

    pub fn set_retained(&self, prop: i32, value: &Val) {
        let Ok(index) = usize::try_from(prop) else {
            return;
        };
        self.with(|style| {
            if style
                .value_handle(index)
                .is_some_and(|current| OwnedValue::ptr_eq(current, &value.0))
            {
                return;
            }
            style.replace_value(index, Some(value.0.clone()));
        });
    }
}

pub fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodePtr(*const NsNode);

impl NodePtr {
    pub fn new(node: *const NsNode) -> Option<NodePtr> {
        (!node.is_null()).then_some(NodePtr(node))
    }

    pub fn key(self) -> usize {
        self.0 as usize
    }

    pub fn node(self) -> Option<Node<'static>> {
        // SAFETY: a NodePtr is made from a node of this thread's store, and `from_ptr` returns None once that node is freed.
        unsafe { Node::from_ptr(self.0) }
    }

    pub fn parent(self) -> Option<NodePtr> {
        self.node()?.parent().and_then(|p| NodePtr::new(p.as_ptr()))
    }

    pub fn first_child(self) -> Option<NodePtr> {
        self.node()?
            .first_child()
            .and_then(|c| NodePtr::new(c.as_ptr()))
    }

    pub fn next_in_subtree(self, root: NodePtr, descend: bool) -> Option<NodePtr> {
        southstar_dom::index::next_in_subtree(self.node()?, root.node(), descend)
            .and_then(|n| NodePtr::new(n.as_ptr()))
    }

    pub fn ancestors(self) -> impl Iterator<Item = NodePtr> {
        core::iter::successors(self.parent(), |p| p.parent())
    }

    pub fn style_in<'a>(self, styles: StylesTable<'a>) -> Option<StyleRef<'a>> {
        styles.0.get(self.0).map(StyleRef::from_data)
    }

    pub fn style_before_change(self) -> Option<RetainedStyle> {
        southstar_css::style_before_change(self.node()?).map(RetainedStyle)
    }

    pub fn exclude_from_incremental(self, exclude: bool) {
        southstar_css::incremental_exclude(self.0, exclude);
    }
}

#[derive(Clone, Copy)]
pub struct StylesTable<'a>(&'a StyleMap);

impl<'a> StylesTable<'a> {
    pub fn new(table: &'a StyleMap) -> StylesTable<'a> {
        StylesTable(table)
    }

    pub fn contains(self, node: NodePtr) -> bool {
        self.0.contains(node.0)
    }

    pub fn entries(self) -> Vec<(NodePtr, StyleRef<'a>)> {
        self.0
            .iter()
            .filter_map(|(k, v)| Some((NodePtr::new(k as *const NsNode)?, StyleRef::from_data(v))))
            .collect()
    }
}
