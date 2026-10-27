//! Southstar — the C ABI of applying matched declarations: css.c's match_entry array read through a mirror, each property's winning value shared into the element's ns_style, CSS-wide keywords and inheritance resolved, and its colors, display and units fixed up.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicU64, Ordering};

use southstar_css_color::Color;
use southstar_glib::GBoolean;

use super::computed_units::{ComputedStyle, PROP_COUNT, RawStyle, StyleView};
use super::property::to_c;
use super::value::{NsCssValue, OwnedValue, Payload, RawColor, new_keyword};
use crate::cascade::{self, CURRENTCOLOR_PROPS, DisplayContext, Entry, OverflowFix, Revert};
use crate::color;
use crate::computed_units;
use crate::display::{self, Display};
use crate::font;
use crate::initial;
use crate::prop::Prop;
use crate::property;
use crate::property::{Body, Value};
use crate::style_query;

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
    pub(super) value: OwnedValue,
    pub(super) prop: c_int,
}

thread_local! {
    static INITIAL: RefCell<Vec<Option<Option<OwnedValue>>>> = const { RefCell::new(Vec::new()) };
}

fn keyword(v: Option<&NsCssValue>) -> Option<&[u8]> {
    v?.keyword_bytes()
}

fn initial_value_of(prop: usize) -> Option<OwnedValue> {
    INITIAL.with(|cache| {
        let cache = &mut *cache.borrow_mut();
        if cache.len() < PROP_COUNT {
            cache.resize(PROP_COUNT, None);
        }
        cache[prop]
            .get_or_insert_with(|| {
                Prop::from_id(prop)
                    .and_then(|p| initial::initial_value(p.name()))
                    .and_then(|text| property::parse_for(Prop::from_id(prop), text.to_bytes()))
                    .map(to_c)
            })
            .clone()
    })
}

fn revert_of(v: Option<&NsCssValue>) -> Option<Revert> {
    match keyword(v)? {
        b"revert" => Some(Revert::Origin),
        b"revert-layer" => Some(Revert::Layer),
        b"revert-rule" => Some(Revert::Rule),
        _ => None,
    }
}

struct Out<'a> {
    style: &'a mut RawStyle,
}

impl Out<'_> {
    fn get(&self, prop: Prop) -> Option<&NsCssValue> {
        self.style.value_at(prop.id())
    }

    fn keyword(&self, prop: Prop) -> Option<&[u8]> {
        keyword(self.get(prop))
    }

    fn replace(&mut self, prop: usize, v: Option<OwnedValue>) {
        self.style.values[prop] = v;
    }
}

fn parent_value(parent: Option<&RawStyle>, prop: usize) -> Option<OwnedValue> {
    parent.and_then(|p| p.values[prop].clone())
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
            revert: revert_of(Some(&m.value)),
        })
        .collect();
    for (prop, won) in cascade::winners(&list, PROP_COUNT).into_iter().enumerate() {
        if let Some(won) = won {
            out.replace(prop, won.map(|e| raw[e].value.clone()));
        }
    }
}

fn normalize_overflow(out: &mut Out<'_>) {
    match cascade::overflow_fix(out.keyword(Prop::OverflowX), out.keyword(Prop::OverflowY)) {
        Some(OverflowFix::XAuto) => out.replace(Prop::OverflowX.id(), Some(new_keyword(b"auto"))),
        Some(OverflowFix::YAuto) => out.replace(Prop::OverflowY.id(), Some(new_keyword(b"auto"))),
        None => {}
    }
}

static EXPLICIT_INHERIT: [AtomicU64; PROP_COUNT.div_ceil(64)] =
    [const { AtomicU64::new(0) }; PROP_COUNT.div_ceil(64)];

fn note_explicit_inherit(prop: usize) {
    if !Prop::from_id(prop).is_some_and(Prop::inherits) {
        EXPLICIT_INHERIT[prop / 64].fetch_or(1 << (prop % 64), Ordering::Relaxed);
    }
}

pub(super) fn passed_to_children(prop: usize) -> bool {
    Prop::from_id(prop).is_some_and(Prop::inherits)
        || EXPLICIT_INHERIT[prop / 64].load(Ordering::Relaxed) & (1 << (prop % 64)) != 0
}

fn resolve_wide_keywords(out: &mut Out<'_>, parent: Option<&RawStyle>) {
    let mut explicit_initial = [false; PROP_COUNT];
    for (prop, initial) in explicit_initial.iter_mut().enumerate() {
        match keyword(out.style.value_at(prop)) {
            Some(b"inherit") => {
                note_explicit_inherit(prop);
                out.replace(prop, parent_value(parent, prop));
            }
            Some(b"initial") => {
                let inherits = Prop::from_id(prop).is_some_and(Prop::inherits);
                let value = if inherits {
                    initial_value_of(prop)
                } else {
                    None
                };
                out.replace(prop, value);
                *initial = true;
            }
            Some(b"unset") => out.replace(prop, None),
            _ => {}
        }
    }
    let Some(parent) = parent else {
        return;
    };
    for (prop, &initial) in explicit_initial.iter().enumerate() {
        let inherits = Prop::from_id(prop).is_some_and(Prop::inherits);
        if out.style.values[prop].is_none() && !initial && inherits && parent.values[prop].is_some()
        {
            out.style.values[prop].clone_from(&parent.values[prop]);
        }
    }
}

fn raw_color(v: Option<&NsCssValue>) -> Option<RawColor> {
    v?.raw_color()
}

fn new_color(color: &Color) -> OwnedValue {
    to_c(Value::of(Body::Color(*color)))
}

fn current_color_function(kw: &[u8], current: Option<&NsCssValue>) -> Option<OwnedValue> {
    if !color::needs_current_color(kw) {
        return None;
    }
    let current =
        raw_color(current).map_or_else(|| Color::from_rgba8([0, 0, 0, 255]), RawColor::to_color);
    color::parse_with_current(kw, Some(current)).map(|c| new_color(&c))
}

fn resolve_color_keywords(out: &mut Out<'_>, parent: Option<&RawStyle>) {
    match out.keyword(Prop::Color) {
        Some(b"currentcolor") => {
            let color = match parent {
                Some(p) => p.values[Prop::Color.id()].clone(),
                None => initial_value_of(Prop::Color.id()),
            };
            out.replace(Prop::Color.id(), color);
        }
        Some(kw) => {
            let inherited = parent.and_then(|p| p.value_at(Prop::Color.id()));
            if let Some(color) = current_color_function(kw, inherited) {
                out.replace(Prop::Color.id(), Some(color));
            }
        }
        None => {}
    }
    if let Some(kw @ (b"bolder" | b"lighter")) = out.keyword(Prop::FontWeight) {
        let bolder = kw == b"bolder";
        let parent_weight = parent.map_or(400, |p| {
            font::weight_number(keyword(p.value_at(Prop::FontWeight.id())), 400)
        });
        let weight = font::weight_relative(parent_weight, bolder).to_string();
        out.replace(Prop::FontWeight.id(), Some(new_keyword(weight.as_bytes())));
    }
    for (bit, prop) in CURRENTCOLOR_PROPS.iter().enumerate() {
        match out.keyword(*prop) {
            Some(b"currentcolor") => {
                let color = out.style.values[Prop::Color.id()].clone();
                out.replace(prop.id(), color);
                out.style.currentcolor_bits |= 1 << bit;
            }
            Some(b"transparent") => {
                out.replace(prop.id(), Some(new_color(&Color::transparent())));
            }
            Some(kw) => {
                if let Some(color) = current_color_function(kw, out.get(Prop::Color)) {
                    out.replace(prop.id(), Some(color));
                }
            }
            None => {}
        }
    }
    let current = raw_color(out.get(Prop::Color));
    for prop in [Prop::BoxShadow, Prop::TextShadow] {
        let Some(list) = out.get(prop).and_then(NsCssValue::shadows) else {
            continue;
        };
        let n = usize::try_from(list.n).unwrap_or(0).min(list.s.len());
        if !list.s[..n].iter().any(|sh| sh.currentcolor != 0) {
            continue;
        }
        let Some(Payload::Shadow(list)) = out.style.make_mut(prop.id()).map(|v| &mut v.u) else {
            continue;
        };
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
    let clamp = out.get(Prop::LineClamp).and_then(NsCssValue::length);
    if clamp.is_some_and(|(v, _)| v >= 1.0) {
        d.inner = display::INNER_FLOW_ROOT;
        return d;
    }
    out.replace(Prop::FlexDirection.id(), Some(new_keyword(b"column")));
    d
}

fn out_of_flow(out: &Out<'_>) -> bool {
    matches!(out.keyword(Prop::Position), Some(b"absolute" | b"fixed"))
        || out.keyword(Prop::Float).is_some_and(|k| k != b"none")
}

fn legacy_white_space(collapse: &[u8], nowrap: bool) -> &'static [u8] {
    match collapse {
        b"preserve" | b"preserve-spaces" | b"break-spaces" if nowrap => b"pre",
        b"preserve" | b"preserve-spaces" => b"pre-wrap",
        b"break-spaces" => b"break-spaces",
        b"preserve-breaks" => b"pre-line",
        _ if nowrap => b"nowrap",
        _ => b"normal",
    }
}

fn derive_white_space(out: &mut Out<'_>) {
    let collapse = out.keyword(Prop::WhiteSpaceCollapse).unwrap_or(b"collapse");
    let nowrap = out.keyword(Prop::TextWrapMode) == Some(b"nowrap");
    let legacy = legacy_white_space(collapse, nowrap);
    if out.keyword(Prop::WhiteSpace) == Some(legacy) {
        return;
    }
    let value = if legacy == b"normal" {
        initial_value_of(Prop::WhiteSpace.id())
    } else {
        Some(new_keyword(legacy))
    };
    out.replace(Prop::WhiteSpace.id(), value);
}

fn resolve_emphasis_style(out: &mut Out<'_>) {
    let fill: &[u8] = match out.keyword(Prop::TextEmphasisStyle) {
        Some(b"filled") => b"",
        Some(b"open") => b"open ",
        _ => return,
    };
    let horizontal = style_query::writing_mode_of(out.style.keyword(Prop::WritingMode)) == 0;
    let shape: &[u8] = if horizontal { b"circle" } else { b"sesame" };
    let value = new_keyword(&[fill, shape].concat());
    out.replace(Prop::TextEmphasisStyle.id(), Some(value));
}

fn resolve_text_size_adjust(out: &mut Out<'_>) {
    if out.keyword(Prop::TextSizeAdjust) == Some(b"none") {
        out.replace(Prop::TextSizeAdjust.id(), Some(new_keyword(b"100%")));
    }
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
        out.replace(
            Prop::Display.id(),
            Some(new_keyword(&display::serialize(used))),
        );
    }
    out.style.display = used;
}

pub(super) struct Parents<'a> {
    pub(super) style: Option<&'a RawStyle>,
    pub(super) layout: Option<&'a RawStyle>,
}

pub(super) fn cascade_apply(
    matches: &[RawMatch],
    style: &mut RawStyle,
    parents: &Parents<'_>,
    is_root: bool,
    root_px: f64,
) {
    let parent = parents.style;
    let mut out = Out { style };
    apply_winners(&mut out, matches);
    normalize_overflow(&mut out);
    resolve_wide_keywords(&mut out, parent);
    resolve_color_keywords(&mut out, parent);
    derive_white_space(&mut out);
    resolve_emphasis_style(&mut out);
    resolve_text_size_adjust(&mut out);
    fix_display(&mut out, parents.layout, is_root);
    let parent_view = parent.map(StyleView);
    computed_units::resolve(&mut ComputedStyle(out.style), parent_view.as_ref(), root_px);
}
