//! Southstar — the C ABI of the custom-property cascade: css.c's matched var_match entries read through a mirror, the registered @property rules and the parent's variable map consulted, and the element's ns_var_map built, reused or taken from the per-pass cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::OnceCell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::size_of;
use core::ptr;
use std::ffi::CString;

use southstar_css_prop_syntax::{self as prop_syntax, Context};
use southstar_glib::{self as glib, GBoolean, GHashTable};

use super::computed_units::{RawStyle, StyleView};
use super::font::relative_unit_px;
use super::sheet::RawPropertyRule;
use super::values::text_of;
use super::vars::{RawVarMap, chain_lookup, map_ref, new_map};
use crate::computed_units::{self, Font};
use crate::custom_props::{self, Inherited, Own, Registered, Registry, VarMatch};
use crate::prop::Prop;
use crate::units::{self, CAP, CH, EX, IC, VH, VW};

#[repr(C)]
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
    pub(super) name: *const c_char,
    pub(super) text: *const c_char,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawVarMatch>() == 72);

unsafe extern "C" {
    fn g_hash_table_iter_replace(iter: *mut glib::GHashTableIter, value: *mut c_void);
}

struct Props(*mut GHashTable);

struct Parent<'a>(&'a RawVarMap);

fn registered(rule: &RawPropertyRule) -> Registered<'_> {
    Registered {
        inherits: rule.inherits(),
        initial: rule.initial(),
    }
}

impl Props {
    fn rule(&self, name: &[u8]) -> Option<&RawPropertyRule> {
        let key = CString::new(name).ok()?;
        let rule = unsafe { glib::g_hash_table_lookup(self.0, key.as_ptr().cast()) };
        unsafe { rule.cast::<RawPropertyRule>().as_ref() }
    }
}

impl Registry for Props {
    fn get(&self, name: &[u8]) -> Option<Registered<'_>> {
        self.rule(name).map(registered)
    }

    fn each(&self, f: &mut dyn FnMut(&[u8], Registered<'_>)) {
        for (key, value) in unsafe { glib::hash_table_entries(self.0) } {
            let rule = unsafe { value.cast::<RawPropertyRule>().as_ref() };
            if let (false, Some(rule)) = (key.is_null(), rule) {
                f(
                    unsafe { CStr::from_ptr(key.cast()) }.to_bytes(),
                    registered(rule),
                );
            }
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
        .filter(|m| !m.name.is_null() && !m.text.is_null())
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
            name: unsafe { CStr::from_ptr(m.name) }.to_bytes(),
            text: unsafe { CStr::from_ptr(m.text) }.to_bytes(),
        })
        .collect()
}

fn own_table(own: Own) -> *mut GHashTable {
    let table = unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            Some(glib::g_free),
        )
    };
    for (name, value) in own {
        unsafe {
            glib::g_hash_table_insert(
                table,
                glib::strdup(&name).cast(),
                glib::strdup(&value).cast(),
            )
        };
    }
    table
}

pub(super) fn build_vars(
    parent: *mut RawVarMap,
    raw: &[RawVarMatch],
    registered_props: *mut GHashTable,
    adjust_cache: *mut GHashTable,
) -> *mut RawVarMap {
    let have_regs =
        !registered_props.is_null() && unsafe { glib::g_hash_table_size(registered_props) } > 0;
    let have_local = !raw.is_empty();
    let inherited = unsafe { parent.as_ref() }.map(Parent);
    let inherited = inherited.as_ref().map(|p| p as &dyn Inherited);
    if parent.is_null() && !have_regs && !have_local {
        return ptr::null_mut();
    }
    let mut matches = views(raw);
    if have_local {
        custom_props::sort(&mut matches);
    }
    if !have_regs {
        if !have_local {
            return unsafe { map_ref(parent) };
        }
        let own = custom_props::unregistered(&matches, inherited);
        return unsafe { new_map(own_table(own), map_ref(parent)) };
    }
    let cacheable = !parent.is_null() && !have_local && !adjust_cache.is_null();
    if cacheable {
        let hit = unsafe { glib::g_hash_table_lookup(adjust_cache, parent.cast()) };
        if !hit.is_null() {
            return unsafe { map_ref(hit.cast()) };
        }
    }
    let own = custom_props::registered(&matches, inherited, &Props(registered_props));
    let built = if !parent.is_null() && !have_local && own.is_empty() {
        unsafe { map_ref(parent) }
    } else {
        unsafe { new_map(own_table(own), map_ref(parent)) }
    };
    if cacheable {
        unsafe {
            glib::g_hash_table_insert(adjust_cache, map_ref(parent).cast(), map_ref(built).cast())
        };
    }
    built
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
    registered_props: *mut GHashTable,
    root_px: f64,
) {
    let Some(vars) = (unsafe { style.vars.as_ref() }) else {
        return;
    };
    if registered_props.is_null() || vars.own.is_null() {
        return;
    }
    if parent_style.is_some_and(|p| p.vars == style.vars) {
        return;
    }
    if unsafe { glib::g_hash_table_size(registered_props) } == 0 {
        return;
    }
    let current_color = OnceCell::new();
    let mut ctx: Option<Context<'_>> = None;
    let mut iter = glib::GHashTableIter::new();
    unsafe { glib::g_hash_table_iter_init(&mut iter, vars.own) };
    let (mut key, mut value) = (ptr::null_mut(), ptr::null_mut());
    while unsafe { glib::g_hash_table_iter_next(&mut iter, &mut key, &mut value) } != 0 {
        let rule = unsafe { glib::g_hash_table_lookup(registered_props, key) };
        let Some(syntax) = unsafe { rule.cast::<RawPropertyRule>().as_ref() }
            .and_then(RawPropertyRule::typed_syntax)
        else {
            continue;
        };
        let ctx = ctx.get_or_insert_with(|| {
            let color = current_color
                .get_or_init(|| unsafe { text_of(style.value(Prop::Color as usize), false) });
            syntax_ctx(&StyleView(style), root_px, color)
        });
        let Some(value) = (unsafe { glib::bytes(value.cast()) }) else {
            continue;
        };
        if let Some(computed) = prop_syntax::compute(syntax, value, Some(ctx)) {
            unsafe { g_hash_table_iter_replace(&mut iter, glib::strdup(&computed).cast()) };
        }
    }
}
