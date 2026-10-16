//! Southstar — the C ABI of unit resolution in a computed style: css.c's ns_style values array read through a mirror, each value viewed by kind, and a shared value copied before it is changed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::mem::{offset_of, size_of};
use core::ptr;

use southstar_glib as glib;

use super::value::{
    KIND_CALC, KIND_KEYWORD, KIND_LENGTH, KIND_SHADOW, KIND_SIZE, KIND_TRACKS, KIND_TRANSFORM,
    Length, NsCssValue, RawCalc, RawSize,
};
use super::vars::RawVarMap;
use crate::computed_units;
use crate::display::Display;
use crate::grid::Tracks;
use crate::prop::Prop;
use crate::shadow::ShadowList;
use crate::track_text;
use crate::transform::Transform;
use crate::units::PX;

pub const PROP_COUNT: usize = Prop::ALL.len();

#[repr(C)]
pub struct RawStyle {
    pub(super) values: [*mut NsCssValue; PROP_COUNT],
    pub(super) display: Display,
    pub(super) specified_inline: u8,
    pub(super) pseudo_styles: [*mut RawStyle; 11],
    pub(super) share_id: u64,
    pub(super) ref_count: c_int,
    pub(super) currentcolor_bits: u32,
    pub(super) vars: *mut RawVarMap,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<RawStyle>() == 2336
        && offset_of!(RawStyle, pseudo_styles) == 2224
        && offset_of!(RawStyle, share_id) == 2312
        && offset_of!(RawStyle, vars) == 2328
);

/// # Safety
///
/// `style` is null or a live computed style made by this crate that nothing else is reading or writing.
pub unsafe fn style_value_slot<T>(style: *mut T, index: usize) -> Option<*mut *mut NsCssValue> {
    if style.is_null() || index >= PROP_COUNT {
        return None;
    }
    // SAFETY: the caller passes a live style, and index is within its value slots.
    Some(unsafe { ptr::addr_of_mut!((*style.cast::<RawStyle>()).values[index]) })
}

/// # Safety
///
/// `style` is null or a live computed style made by this crate.
pub unsafe fn style_retain<T>(style: *mut T) {
    // SAFETY: the caller passes null or a live style.
    if let Some(style) = unsafe { style.cast::<RawStyle>().as_mut() } {
        style.ref_count += 1;
    }
}

impl RawStyle {
    pub(super) fn value(&self, prop: usize) -> *const NsCssValue {
        self.values
            .get(prop)
            .map_or(ptr::null(), |&v| v.cast_const())
    }

    /// # Safety
    ///
    /// `style` is null or a computed style made by this crate that outlives `'a`.
    pub unsafe fn from_ptr<'a, T>(style: *const T) -> Option<&'a RawStyle> {
        // SAFETY: the caller passes null or a live computed style.
        unsafe { style.cast::<RawStyle>().as_ref() }
    }

    pub fn value_at(&self, prop: usize) -> Option<&NsCssValue> {
        let value = *self.values.get(prop)?;
        // SAFETY: a style's value slots are null or values the style holds a reference on.
        unsafe { value.as_ref() }
    }

    pub fn display_parts(&self) -> [u8; 5] {
        let d = self.display;
        [d.box_, d.outer, d.inner, d.internal, d.list_item]
    }

    pub fn specified_inline(&self) -> bool {
        self.specified_inline != 0
    }

    pub fn pseudo(&self, slot: usize) -> Option<&RawStyle> {
        let pseudo = *self.pseudo_styles.get(slot)?;
        // SAFETY: pseudo-element slots are null or styles this style owns.
        unsafe { pseudo.as_ref() }
    }

    pub fn currentcolor_bits(&self) -> u32 {
        self.currentcolor_bits
    }

    pub fn has_vars(&self) -> bool {
        !self.vars.is_null()
    }

    pub fn vars_ptr(&self) -> *const core::ffi::c_void {
        self.vars.cast_const().cast()
    }

    fn var_map(&self) -> Option<&RawVarMap> {
        // SAFETY: vars is null or a variable map this style holds a reference on.
        unsafe { self.vars.as_ref() }
    }

    pub fn same_vars(&self, other: &RawStyle) -> bool {
        ptr::eq(self.vars, other.vars)
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
        super::vars::substitute_in(text, self.vars, super::registry::pass_registered())
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

pub(super) fn slot(v: &NsCssValue) -> Slot<'_> {
    unsafe {
        match v.kind {
            KIND_KEYWORD => {
                Slot::Keyword((!v.u.keyword.is_null()).then(|| CStr::from_ptr(v.u.keyword)))
            }
            KIND_LENGTH => Slot::Length(v.u.length),
            KIND_CALC => Slot::Calc(&v.u.calc),
            KIND_SHADOW => Slot::Shadow(&v.u.shadow),
            KIND_TRACKS => Slot::Tracks(&v.u.tracks),
            KIND_TRANSFORM => Slot::Transform(&v.u.transform),
            KIND_SIZE => Slot::Size(&v.u.size),
            _ => Slot::Other,
        }
    }
}

fn slot_mut(v: &mut NsCssValue) -> SlotMut<'_> {
    unsafe {
        match v.kind {
            KIND_LENGTH => SlotMut::Length(&mut v.u.length),
            KIND_CALC => SlotMut::Calc(&mut v.u.calc),
            KIND_SHADOW => SlotMut::Shadow(&mut v.u.shadow),
            KIND_TRACKS => SlotMut::Tracks(&mut v.u.tracks),
            KIND_TRANSFORM => SlotMut::Transform(&mut v.u.transform),
            KIND_SIZE => SlotMut::Size(&mut v.u.size),
            _ => SlotMut::Other,
        }
    }
}

pub(crate) struct ComputedStyle<'a>(pub(super) &'a mut RawStyle);

pub(crate) struct StyleView<'a>(pub(super) &'a RawStyle);

impl ComputedStyle<'_> {
    pub(crate) fn get(&self, prop: usize) -> Option<Slot<'_>> {
        unsafe { self.0.values.get(prop)?.as_ref() }.map(slot)
    }

    pub(crate) fn make_mut(&mut self, prop: usize) -> Option<SlotMut<'_>> {
        let value = unsafe { cow(self.0, prop) };
        unsafe { value.as_mut() }.map(slot_mut)
    }

    pub(crate) fn set_length_px(&mut self, prop: usize, px: f64) {
        if matches!(self.get(prop), Some(Slot::Length(_))) {
            if let Some(SlotMut::Length(length)) = self.make_mut(prop) {
                length.v = px;
                length.unit = PX;
            }
            return;
        }
        let value = unsafe { glib::g_malloc0(size_of::<NsCssValue>()) }.cast::<NsCssValue>();
        unsafe {
            (*value).kind = KIND_LENGTH;
            (*value).u.length = Length { v: px, unit: PX };
        }
        self.0.values[prop] = value;
    }
}

impl StyleView<'_> {
    pub(crate) fn get(&self, prop: usize) -> Option<Slot<'_>> {
        unsafe { self.0.values.get(prop)?.as_ref() }.map(slot)
    }
}

pub(super) unsafe fn cow(style: &mut RawStyle, prop: usize) -> *mut NsCssValue {
    let Some(&value) = style.values.get(prop) else {
        return ptr::null_mut();
    };
    let Some(shared) = (unsafe { value.as_mut() }) else {
        return value;
    };
    if shared.ref_count == 0 {
        return value;
    }
    unsafe {
        let copy = glib::g_malloc0(size_of::<NsCssValue>()).cast::<NsCssValue>();
        ptr::copy_nonoverlapping(value, copy, 1);
        (*copy).ref_count = 0;
        (*copy).image_set_text = glib::g_strdup(shared.image_set_text);
        (*copy).specified = glib::g_strdup(shared.specified);
        if let Some(next) = (*copy).next_layer.as_mut() {
            next.ref_count += 1;
        }
        shared.ref_count -= 1;
        style.values[prop] = copy;
        copy
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_tracks_computed_serialize(
    style: *const RawStyle,
    root: *const RawStyle,
    prop: c_int,
) -> *mut c_char {
    let Some(style) = (unsafe { style.as_ref() }) else {
        return ptr::null_mut();
    };
    let value = usize::try_from(prop)
        .ok()
        .and_then(|prop| style.values.get(prop))
        .and_then(|&v| unsafe { v.as_ref() });
    let Some(value) = value.filter(|v| v.kind == KIND_TRACKS && !v.specified.is_null()) else {
        return ptr::null_mut();
    };
    if unsafe { value.u.tracks.subgrid } != 0 {
        return ptr::null_mut();
    }
    let specified = unsafe { CStr::from_ptr(value.specified) }.to_bytes();
    let root = unsafe { root.as_ref() }.unwrap_or(style);
    let text = track_text::tracks_computed_text(
        specified,
        computed_units::style_font_px(&StyleView(style)),
        computed_units::style_font_px(&StyleView(root)),
    );
    glib::strdup(&text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_style_scale_font_size(s: *mut RawStyle, factor: f64) {
    let Some(style) = (unsafe { s.as_mut() }) else {
        return;
    };
    let font_size = Prop::FontSize.id();
    if unsafe { style.values[font_size].as_ref() }.is_none_or(|v| v.kind != KIND_LENGTH) {
        return;
    }
    unsafe { (*cow(style, font_size)).u.length.v *= factor };
}
