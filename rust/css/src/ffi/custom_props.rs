//! Southstar — the C ABI of the custom-property cascade: css.c's matched var_match entries read through a mirror, the registered @property rules and the parent's variable map consulted, and the element's ns_var_map built, reused or taken from the per-pass cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{OnceCell, RefCell};
use core::ffi::{CStr, c_int, c_void};
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;

use southstar_css_prop_syntax::{self as prop_syntax, Context};
use southstar_glib::GBoolean;

use super::computed_units::{RawStyle, StyleView};
use super::declarations::until_nul;
use super::font::relative_unit_px;
use super::registry::RegisteredProps;
use super::sheet::PropertyRule;
use super::values::text_of;
use super::vars::{VarMap, VarTable, chain_lookup, new_map};
use crate::computed_units::{self, Font};
use crate::custom_props::{self, Inherited, Own, Registered, Registry, VarMatch};
use crate::prop::Prop;
use crate::units::{self, CAP, CH, EX, IC, VH, VW};

pub(super) struct RawVarMatch {
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
    pub(super) name: Rc<CStr>,
    pub(super) text: Rc<CStr>,
}

struct Props<'a>(&'a RegisteredProps);

struct Parent<'a>(&'a VarMap);

fn registered(rule: &PropertyRule) -> Registered<'_> {
    Registered {
        inherits: rule.inherits(),
        initial: rule.initial(),
    }
}

impl Props<'_> {
    fn rule(&self, name: &[u8]) -> Option<&PropertyRule> {
        if name.contains(&0) {
            return None;
        }
        self.0.get(name)
    }
}

impl Registry for Props<'_> {
    fn get(&self, name: &[u8]) -> Option<Registered<'_>> {
        self.rule(name).map(registered)
    }

    fn each(&self, f: &mut dyn FnMut(&[u8], Registered<'_>)) {
        for (name, rule) in self.0.iter() {
            f(name, registered(rule));
        }
    }

    fn rejects(&self, name: &[u8], value: &[u8]) -> bool {
        self.rule(name).is_some_and(|rule| rule.rejects(value))
    }
}

impl Inherited for Parent<'_> {
    fn lookup(&self, name: &[u8]) -> Option<&[u8]> {
        chain_lookup(Some(self.0), name)
    }
}

fn views(raw: &[RawVarMatch]) -> Vec<VarMatch<'_>> {
    raw.iter()
        .map(|m| VarMatch {
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
            name: m.name.to_bytes(),
            text: m.text.to_bytes(),
        })
        .collect()
}

fn c_text(text: &[u8]) -> CString {
    CString::new(until_nul(text)).unwrap_or_default()
}

fn own_table(own: Own) -> VarTable {
    own.into_iter()
        .map(|(name, value)| (c_text(&name), c_text(&value)))
        .collect()
}

type Adjusted = (Rc<VarMap>, Rc<VarMap>);

#[derive(Default)]
pub(super) struct AdjustCache(RefCell<HashMap<*const VarMap, Adjusted>>);

pub(super) fn build_vars(
    parent: Option<&Rc<VarMap>>,
    raw: &[RawVarMatch],
    registered_props: &RegisteredProps,
    adjust_cache: Option<&AdjustCache>,
) -> Option<Rc<VarMap>> {
    let have_regs = !registered_props.is_empty();
    let have_local = !raw.is_empty();
    let inherited = parent.map(|p| Parent(p));
    let inherited = inherited.as_ref().map(|p| p as &dyn Inherited);
    if parent.is_none() && !have_regs && !have_local {
        return None;
    }
    let mut matches = views(raw);
    if have_local {
        custom_props::sort(&mut matches);
    }
    if !have_regs {
        if !have_local {
            return parent.cloned();
        }
        let own = custom_props::unregistered(&matches, inherited);
        return Some(new_map(own_table(own), parent.cloned()));
    }
    let adjust_cache = adjust_cache.filter(|_| parent.is_some() && !have_local);
    let key = parent.map_or(core::ptr::null(), Rc::as_ptr);
    if let Some(cache) = adjust_cache
        && let Some((_, hit)) = cache.0.borrow().get(&key)
    {
        return Some(hit.clone());
    }
    let own = custom_props::registered(&matches, inherited, &Props(registered_props));
    let built = match parent {
        Some(parent) if !have_local && own.is_empty() => parent.clone(),
        _ => new_map(own_table(own), parent.cloned()),
    };
    if let Some(cache) = adjust_cache
        && let Some(parent) = parent
    {
        cache
            .0
            .borrow_mut()
            .insert(key, (parent.clone(), built.clone()));
    }
    Some(built)
}

fn relative(unit: u32, px: f64, font: &Font<'_>) -> f64 {
    relative_unit_px(unit, px, font.family, font.weight, font.italic)
}

fn syntax_ctx<'a>(style: &StyleView<'_>, root_px: f64, current_color: &'a [u8]) -> Context<'a> {
    let b = computed_units::syntax_basis(style, root_px);
    let plain = Font {
        family: None,
        weight: 400,
        italic: false,
    };
    Context {
        font_size: b.font_px,
        root_font_size: b.root_px,
        line_height: b.line_height,
        root_line_height: b.root_px * 1.4375,
        ex_px: relative(EX, b.font_px, &b.font),
        ch_px: relative(CH, b.font_px, &b.font),
        cap_px: relative(CAP, b.font_px, &b.font),
        ic_px: relative(IC, b.font_px, &b.font),
        root_ex_px: relative(EX, b.root_px, &plain),
        root_ch_px: relative(CH, b.root_px, &plain),
        root_cap_px: relative(CAP, b.root_px, &plain),
        root_ic_px: relative(IC, b.root_px, &plain),
        viewport_w: units::viewport_resolve(100.0, VW),
        viewport_h: units::viewport_resolve(100.0, VH),
        container_w: crate::container::unit_dims().0,
        container_h: crate::container::unit_dims().1,
        current_color: Some(current_color),
    }
}

pub(super) fn compute_registered_vars(
    style: &RawStyle,
    parent_style: Option<&RawStyle>,
    registered_props: &RegisteredProps,
    root_px: f64,
) {
    let Some(vars) = style.vars.as_deref() else {
        return;
    };
    if parent_style.is_some_and(|p| p.same_vars(style)) {
        return;
    }
    if registered_props.is_empty() || vars.computed() {
        return;
    }
    let current_color = OnceCell::new();
    let mut ctx: Option<Context<'_>> = None;
    let mut replaced = Vec::new();
    for (name, value) in vars.table() {
        let Some(syntax) = registered_props
            .get(name.to_bytes())
            .and_then(PropertyRule::typed_syntax)
        else {
            continue;
        };
        let ctx = ctx.get_or_insert_with(|| {
            let color =
                current_color.get_or_init(|| text_of(style.value_at(Prop::Color as usize), false));
            syntax_ctx(&StyleView(style), root_px, color)
        });
        if let Some(computed) = prop_syntax::compute(syntax, value.to_bytes(), Some(ctx)) {
            replaced.push((name.clone(), c_text(&computed)));
        }
    }
    if !replaced.is_empty() {
        let mut table = vars.table().clone();
        table.extend(replaced);
        vars.set_computed(table);
    }
}
