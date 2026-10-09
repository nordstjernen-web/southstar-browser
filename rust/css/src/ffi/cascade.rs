//! Southstar — the C ABI of applying matched declarations: css.c's match_entry array read through a mirror, each property's winning value shared into the element's ns_style, CSS-wide keywords and inheritance resolved, and its colors, display and units fixed up.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_double, c_int, c_void};
use core::mem::size_of;
use core::ptr;
use core::slice;

use southstar_glib::{GArray, GBoolean};

use super::computed_units::{ComputedStyle, PROP_COUNT, RawStyle, StyleView, cow};
use super::property::to_c;
use super::value::{
    KIND_COLOR, KIND_KEYWORD, KIND_LENGTH, KIND_SHADOW, NsCssValue, RawColor, alloc, new_keyword,
    ns_css_value_free,
};
use crate::cascade::{self, CURRENTCOLOR_PROPS, DisplayContext, Entry, OverflowFix, Revert};
use crate::computed_units;
use crate::display::{self, Display};
use crate::font;
use crate::initial;
use crate::prop::Prop;
use crate::property;

#[repr(C)]
pub(super) struct RawMatch {
    pub(super) origin: c_int,
    pub(super) spec_a: c_int,
    pub(super) spec_b: c_int,
    pub(super) spec_c: c_int,
    pub(super) sheet_index: c_int,
    pub(super) layer_order: c_int,
    pub(super) scope_order: c_int,
    pub(super) source_order: c_int,
    pub(super) decl_order: c_int,
    pub(super) important: GBoolean,
    pub(super) inline_style: GBoolean,
    pub(super) rule: *const c_void,
    pub(super) value: *mut NsCssValue,
    pub(super) prop: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawMatch>() == 72);

struct InitialValues(Vec<Option<*mut NsCssValue>>);

impl Drop for InitialValues {
    fn drop(&mut self) {
        for &value in self.0.iter().flatten() {
            unsafe { ns_css_value_free(value) };
        }
    }
}

thread_local! {
    static INITIAL: RefCell<InitialValues> = const { RefCell::new(InitialValues(Vec::new())) };
}

fn keyword<'a>(v: *const NsCssValue) -> Option<&'a [u8]> {
    let v = unsafe { v.as_ref() }?;
    if v.kind != KIND_KEYWORD {
        return None;
    }
    let kw = unsafe { v.u.keyword };
    (!kw.is_null()).then(|| unsafe { CStr::from_ptr(kw) }.to_bytes())
}

fn dup(v: *mut NsCssValue) -> *mut NsCssValue {
    if let Some(value) = unsafe { v.as_mut() } {
        value.ref_count += 1;
    }
    v
}

fn initial_value_of(prop: usize) -> *mut NsCssValue {
    let parsed = INITIAL.with(|cache| {
        let cache = &mut cache.borrow_mut().0;
        if cache.len() < PROP_COUNT {
            cache.resize(PROP_COUNT, None);
        }
        *cache[prop].get_or_insert_with(|| {
            Prop::from_id(prop)
                .and_then(|p| initial::initial_value(p.name()))
                .and_then(|text| property::parse_for(Prop::from_id(prop), text.to_bytes()))
                .map_or(ptr::null_mut(), to_c)
        })
    });
    dup(parsed)
}

fn revert_of(v: *const NsCssValue) -> Option<Revert> {
    match keyword(v)? {
        b"revert" => Some(Revert::Origin),
        b"revert-layer" => Some(Revert::Layer),
        b"revert-rule" => Some(Revert::Rule),
        _ => None,
    }
}

unsafe fn entries<'a>(array: *const GArray) -> &'a [RawMatch] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.data.is_null() && a.len > 0 => unsafe {
            slice::from_raw_parts(a.data.cast::<RawMatch>(), a.len as usize)
        },
        _ => &[],
    }
}

struct Out<'a> {
    style: &'a mut RawStyle,
}

impl Out<'_> {
    fn get(&self, prop: Prop) -> *mut NsCssValue {
        self.style.values[prop.id()]
    }

    fn keyword(&self, prop: Prop) -> Option<&[u8]> {
        keyword(self.get(prop))
    }

    fn replace(&mut self, prop: usize, v: *mut NsCssValue) {
        unsafe { ns_css_value_free(self.style.values[prop]) };
        self.style.values[prop] = v;
    }
}

fn parent_value(parent: Option<&RawStyle>, prop: usize) -> *mut NsCssValue {
    parent.map_or(ptr::null_mut(), |p| p.values[prop])
}

fn apply_winners(out: &mut Out<'_>, raw: &[RawMatch]) {
    let list: Vec<Entry> = raw
        .iter()
        .map(|m| Entry {
            origin: m.origin,
            specificity: (m.spec_a, m.spec_b, m.spec_c),
            sheet_index: m.sheet_index,
            layer_order: m.layer_order,
            scope_order: m.scope_order,
            source_order: m.source_order,
            decl_order: m.decl_order,
            important: m.important != 0,
            inline_style: m.inline_style != 0,
            rule: m.rule as usize,
            prop: usize::try_from(m.prop).unwrap_or(usize::MAX),
            revert: revert_of(m.value),
        })
        .collect();
    for (prop, won) in cascade::winners(&list, PROP_COUNT).into_iter().enumerate() {
        if let Some(won) = won {
            out.replace(prop, won.map_or(ptr::null_mut(), |e| dup(raw[e].value)));
        }
    }
}

fn normalize_overflow(out: &mut Out<'_>) {
    match cascade::overflow_fix(out.keyword(Prop::OverflowX), out.keyword(Prop::OverflowY)) {
        Some(OverflowFix::XAuto) => out.replace(Prop::OverflowX.id(), new_keyword(b"auto")),
        Some(OverflowFix::YAuto) => out.replace(Prop::OverflowY.id(), new_keyword(b"auto")),
        None => {}
    }
}

fn resolve_wide_keywords(out: &mut Out<'_>, parent: Option<&RawStyle>) {
    let mut explicit_initial = [false; PROP_COUNT];
    for (prop, initial) in explicit_initial.iter_mut().enumerate() {
        let kw = keyword(out.style.values[prop]);
        match kw {
            Some(b"inherit") => out.replace(prop, dup(parent_value(parent, prop))),
            Some(b"initial") => {
                let inherits = Prop::from_id(prop).is_some_and(Prop::inherits);
                let value = if inherits {
                    initial_value_of(prop)
                } else {
                    ptr::null_mut()
                };
                out.replace(prop, value);
                *initial = true;
            }
            Some(b"unset") => out.replace(prop, ptr::null_mut()),
            _ => {}
        }
    }
    let Some(parent) = parent else {
        return;
    };
    for (prop, &initial) in explicit_initial.iter().enumerate() {
        let inherits = Prop::from_id(prop).is_some_and(Prop::inherits);
        if out.style.values[prop].is_null()
            && !initial
            && inherits
            && !parent.values[prop].is_null()
        {
            out.style.values[prop] = dup(parent.values[prop]);
        }
    }
}

fn resolve_color_keywords(out: &mut Out<'_>, parent: Option<&RawStyle>) {
    if out.keyword(Prop::Color) == Some(b"currentcolor") {
        let color = match parent {
            Some(p) => dup(p.values[Prop::Color.id()]),
            None => initial_value_of(Prop::Color.id()),
        };
        out.replace(Prop::Color.id(), color);
    }
    if let Some(kw @ (b"bolder" | b"lighter")) = out.keyword(Prop::FontWeight) {
        let bolder = kw == b"bolder";
        let parent_weight = parent.map_or(400, |p| {
            font::weight_number(keyword(p.values[Prop::FontWeight.id()]), 400)
        });
        let weight = font::weight_relative(parent_weight, bolder).to_string();
        out.replace(Prop::FontWeight.id(), new_keyword(weight.as_bytes()));
    }
    for (bit, prop) in CURRENTCOLOR_PROPS.iter().enumerate() {
        match out.keyword(*prop) {
            Some(b"currentcolor") => {
                let color = dup(out.get(Prop::Color));
                out.replace(prop.id(), color);
                out.style.currentcolor_bits |= 1 << bit;
            }
            Some(b"transparent") => {
                let clear = alloc(KIND_COLOR);
                unsafe {
                    (*clear).u.color = RawColor {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    }
                };
                out.replace(prop.id(), clear);
            }
            _ => {}
        }
    }
    let current = unsafe { out.get(Prop::Color).as_ref() }
        .filter(|v| v.kind == KIND_COLOR)
        .map(|v| unsafe { v.u.color });
    for prop in [Prop::BoxShadow, Prop::TextShadow] {
        let Some(v) = (unsafe { out.get(prop).as_ref() }) else {
            continue;
        };
        if v.kind != KIND_SHADOW {
            continue;
        }
        let list = unsafe { &v.u.shadow };
        let n = usize::try_from(list.n).unwrap_or(0).min(list.s.len());
        if !list.s[..n].iter().any(|sh| sh.currentcolor != 0) {
            continue;
        }
        let Some(v) = (unsafe { cow(out.style, prop.id()).as_mut() }) else {
            continue;
        };
        let list = unsafe { &mut v.u.shadow };
        for sh in list.s[..n].iter_mut().filter(|sh| sh.currentcolor != 0) {
            sh.currentcolor = 0;
            let (r, g, b, a) = current.map_or((0, 0, 0, 255), |c| (c.r, c.g, c.b, c.a));
            (sh.r, sh.g, sh.b, sh.a) = (r, g, b, a);
        }
    }
}

fn legacy_webkit_box(out: &mut Out<'_>, mut d: Display) -> Display {
    if !out
        .keyword(Prop::Display)
        .is_some_and(|k| k.starts_with(b"-webkit-"))
    {
        return d;
    }
    if !matches!(
        out.keyword(Prop::WebkitBoxOrient),
        Some(b"vertical" | b"block-axis")
    ) {
        return d;
    }
    let clamp = unsafe { out.get(Prop::LineClamp).as_ref() };
    if clamp.is_some_and(|c| c.kind == KIND_LENGTH && unsafe { c.u.length.v } >= 1.0) {
        d.inner = display::INNER_FLOW_ROOT;
        return d;
    }
    out.replace(Prop::FlexDirection.id(), new_keyword(b"column"));
    d
}

fn out_of_flow(out: &Out<'_>) -> bool {
    matches!(out.keyword(Prop::Position), Some(b"absolute" | b"fixed"))
        || out.keyword(Prop::Float).is_some_and(|k| k != b"none")
}

fn fix_display(out: &mut Out<'_>, layout_parent: Option<&RawStyle>, is_root: bool) {
    let specified = out
        .keyword(Prop::Display)
        .map_or_else(Display::default, |k| display::from_keyword(Some(k)));
    out.style.specified_inline =
        u8::from(specified.box_ == display::BOX_NORMAL && specified.outer == display::OUTER_INLINE);
    let legacy = legacy_webkit_box(out, specified);
    let cx = DisplayContext {
        out_of_flow: out_of_flow(out),
        layout_parent: layout_parent.map_or_else(Display::default, |p| p.display),
        is_root,
    };
    let used = cascade::blockify(legacy, &cx);
    if specified != used {
        out.replace(Prop::Display.id(), new_keyword(&display::serialize(used)));
    }
    out.style.display = used;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_cascade_apply(
    matches: *const GArray,
    out: *mut RawStyle,
    parent_style: *const RawStyle,
    layout_parent: *const RawStyle,
    is_root: GBoolean,
    root_px: c_double,
) {
    let Some(style) = (unsafe { out.as_mut() }) else {
        return;
    };
    let parent = unsafe { parent_style.as_ref() };
    let layout_parent = unsafe { layout_parent.as_ref() };
    let mut out = Out { style };
    apply_winners(&mut out, unsafe { entries(matches) });
    normalize_overflow(&mut out);
    resolve_wide_keywords(&mut out, parent);
    resolve_color_keywords(&mut out, parent);
    fix_display(&mut out, layout_parent, is_root != 0);
    let parent_view = parent.map(StyleView);
    computed_units::resolve(&mut ComputedStyle(out.style), parent_view.as_ref(), root_px);
}
