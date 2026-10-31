//! Southstar — a computed style: its value per property, display, pseudo-element styles and variable map, each value viewed by kind, and a shared value copied before it is changed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::rc::Rc;

use super::style_alloc::RetainedStyle;
use super::value::{Length, NsCssValue, OwnedValue, Payload, RawCalc, RawSize};
use super::vars::VarMap;
use crate::computed_units;
use crate::display::Display;
use crate::grid::Tracks;
use crate::prop::Prop;
use crate::shadow::ShadowList;
use crate::track_text;
use crate::transform::Transform;
use crate::units::PX;

pub const PROP_COUNT: usize = Prop::ALL.len();

pub struct RawStyle {
    pub(super) values: [Option<OwnedValue>; PROP_COUNT],
    pub(super) display: Display,
    pub(super) specified_inline: u8,
    pub(super) pseudo_styles: [Option<RetainedStyle>; 11],
    pub(super) share_id: u64,
    pub(super) currentcolor_bits: u32,
    pub(super) vars: Option<Rc<VarMap>>,
}

impl RawStyle {
    pub(super) fn new() -> RawStyle {
        RawStyle {
            values: [const { None }; PROP_COUNT],
            display: Display::default(),
            specified_inline: 0,
            pseudo_styles: [const { None }; 11],
            share_id: 0,
            currentcolor_bits: 0,
            vars: None,
        }
    }

    pub fn value_at(&self, prop: usize) -> Option<&NsCssValue> {
        self.values.get(prop)?.as_deref()
    }

    pub fn value_handle(&self, prop: usize) -> Option<&OwnedValue> {
        self.values.get(prop)?.as_ref()
    }

    pub fn replace_value(&mut self, prop: usize, value: Option<OwnedValue>) -> Option<OwnedValue> {
        let slot = self.values.get_mut(prop)?;
        core::mem::replace(slot, value)
    }

    pub fn display_parts(&self) -> [u8; 5] {
        let d = self.display;
        [d.box_, d.outer, d.inner, d.internal, d.list_item]
    }

    pub fn specified_inline(&self) -> bool {
        self.specified_inline != 0
    }

    pub fn pseudo(&self, slot: usize) -> Option<&RawStyle> {
        self.pseudo_styles
            .get(slot)?
            .as_ref()
            .map(RetainedStyle::get)
    }

    pub fn currentcolor_bits(&self) -> u32 {
        self.currentcolor_bits
    }

    pub fn has_vars(&self) -> bool {
        self.vars.is_some()
    }

    pub fn vars_ptr(&self) -> *const core::ffi::c_void {
        self.vars
            .as_ref()
            .map_or(core::ptr::null(), |vars| Rc::as_ptr(vars).cast())
    }

    fn var_map(&self) -> Option<&VarMap> {
        self.vars.as_deref()
    }

    pub fn same_vars(&self, other: &RawStyle) -> bool {
        match (&self.vars, &other.vars) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }

    pub fn var(&self, name: &[u8]) -> Option<&CStr> {
        super::vars::chain_lookup_text(self.var_map(), name)
    }

    pub fn var_names(&self) -> Vec<&CStr> {
        self.var_map()
            .map_or_else(Vec::new, super::vars::chain_names)
    }

    pub fn keyword(&self, prop: Prop) -> Option<&CStr> {
        let keyword = self.value_at(prop.id())?.keyword()?;
        Some(if crate::style_query::is_alignment(prop) {
            crate::style_query::alignment_base(keyword)
        } else {
            keyword
        })
    }

    pub fn resolve_vars(&self, text: &[u8]) -> Option<Vec<u8>> {
        let registered = super::registry::pass_registered();
        super::vars::substitute_in(text, self.var_map(), registered.as_deref())
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Slot<'a> {
    Keyword(Option<&'a CStr>),
    Length(Length),
    Calc(&'a RawCalc),
    Shadow(&'a ShadowList),
    Tracks(&'a Tracks),
    Transform(&'a Transform),
    Size(&'a RawSize),
    Other,
}

pub(crate) enum SlotMut<'a> {
    Length(&'a mut Length),
    Calc(&'a mut RawCalc),
    Shadow(&'a mut ShadowList),
    Tracks(&'a mut Tracks),
    Transform(&'a mut Transform),
    Size(&'a mut RawSize),
    Other,
}

pub(crate) fn slot(v: &NsCssValue) -> Slot<'_> {
    match &v.u {
        Payload::Keyword(keyword) => Slot::Keyword(keyword.as_deref()),
        Payload::Length(length) => Slot::Length(*length),
        Payload::Calc(calc) => Slot::Calc(calc),
        Payload::Shadow(shadow) => Slot::Shadow(shadow),
        Payload::Tracks(tracks) => Slot::Tracks(tracks),
        Payload::Transform(transform) => Slot::Transform(transform),
        Payload::Size(size) => Slot::Size(size),
        _ => Slot::Other,
    }
}

fn slot_mut(v: &mut NsCssValue) -> SlotMut<'_> {
    match &mut v.u {
        Payload::Length(length) => SlotMut::Length(length),
        Payload::Calc(calc) => SlotMut::Calc(calc),
        Payload::Shadow(shadow) => SlotMut::Shadow(shadow),
        Payload::Tracks(tracks) => SlotMut::Tracks(tracks),
        Payload::Transform(transform) => SlotMut::Transform(transform),
        Payload::Size(size) => SlotMut::Size(size),
        _ => SlotMut::Other,
    }
}

pub(crate) struct ComputedStyle<'a>(pub(super) &'a mut RawStyle);

pub(crate) struct StyleView<'a>(pub(crate) &'a RawStyle);

impl ComputedStyle<'_> {
    pub(crate) fn get(&self, prop: usize) -> Option<Slot<'_>> {
        self.0.value_at(prop).map(slot)
    }

    pub(crate) fn make_mut(&mut self, prop: usize) -> Option<SlotMut<'_>> {
        self.0.make_mut(prop).map(slot_mut)
    }

    pub(crate) fn set_length_px(&mut self, prop: usize, px: f64) {
        if matches!(self.get(prop), Some(Slot::Length(_))) {
            if let Some(SlotMut::Length(length)) = self.make_mut(prop) {
                length.v = px;
                length.unit = PX;
            }
            return;
        }
        self.0.values[prop] = Some(OwnedValue::of(Payload::Length(Length { v: px, unit: PX })));
    }
}

impl StyleView<'_> {
    pub(crate) fn get(&self, prop: usize) -> Option<Slot<'_>> {
        self.0.value_at(prop).map(slot)
    }
}

impl RawStyle {
    pub(super) fn make_mut(&mut self, prop: usize) -> Option<&mut NsCssValue> {
        self.values
            .get_mut(prop)?
            .as_mut()
            .map(OwnedValue::make_mut)
    }
}

pub fn tracks_computed_text(
    style: &RawStyle,
    root: Option<&RawStyle>,
    prop: Prop,
) -> Option<Vec<u8>> {
    let value = style.value_at(prop.id())?;
    let tracks = value.tracks()?;
    if tracks.subgrid != 0 {
        return None;
    }
    let specified = value.specified.as_deref()?.to_bytes();
    let root = root.unwrap_or(style);
    Some(track_text::tracks_computed_text(
        specified,
        computed_units::style_font_px(&StyleView(style)),
        computed_units::style_font_px(&StyleView(root)),
    ))
}

pub fn style_scale_font_size(style: &mut RawStyle, factor: f64) {
    let font_size = Prop::FontSize.id();
    if style
        .value_at(font_size)
        .and_then(NsCssValue::length)
        .is_none()
    {
        return;
    }
    if let Some(Payload::Length(length)) = style.make_mut(font_size).map(|v| &mut v.u) {
        length.v *= factor;
    }
}
