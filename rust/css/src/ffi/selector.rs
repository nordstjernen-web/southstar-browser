//! Southstar — the C ABI of the selector parser: parsed selectors built as css.c's ns_css_selector, ns_css_simple and predicate structs with the GLib arrays the matcher reads, freeing them, and the parser state and hashes css.c still asks for.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::mem::{offset_of, size_of};
use core::ptr;

use southstar_glib::{self as glib, GArray, GBoolean, GPtrArray};

use crate::selector::{
    self, AttrPred, Compound, FLAGS, PseudoPred, Selector, anb_int_strict, attr_value_hash,
    identifier_hash,
};

#[repr(C)]
pub(super) struct RawSelector {
    compounds: *mut GPtrArray,
    combinators: *mut GArray,
    pseudo_element: c_uint,
    spec_a: c_int,
    spec_b: c_int,
    spec_c: c_int,
    ancestor_hashes: [u32; 4],
    n_ancestor_hashes: c_uint,
    n_ancestor_attr_hashes: c_uint,
}

#[repr(C)]
struct RawSimple {
    type_: *mut c_char,
    id: *mut c_char,
    classes: *mut GPtrArray,
    class_lens: *mut GArray,
    attrs: *mut GArray,
    pseudos: *mut GArray,
    matches_any: *mut GPtrArray,
    matches_none: *mut GPtrArray,
    has_groups: *mut GPtrArray,
    never_match: GBoolean,
    ns_none: GBoolean,
}

#[repr(C)]
struct RawAttrPred {
    name: *mut c_char,
    op: c_uint,
    value: *mut c_char,
    case_insensitive: GBoolean,
    case_sensitive: GBoolean,
    html_ci: GBoolean,
    name_bit: u64,
}

#[repr(C)]
struct RawPseudoPred {
    kind: c_uint,
    a: c_int,
    b: c_int,
    arg: *mut c_char,
    of_group: *mut GPtrArray,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<RawSelector>() == 56
        && offset_of!(RawSelector, ancestor_hashes) == 32
        && size_of::<RawSimple>() == 80
        && offset_of!(RawSimple, never_match) == 72
        && size_of::<RawAttrPred>() == 48
        && offset_of!(RawAttrPred, name_bit) == 40
        && size_of::<RawPseudoPred>() == 32
        && offset_of!(RawPseudoPred, arg) == 16
);

unsafe extern "C" {
    fn g_array_set_clear_func(array: *mut GArray, clear_func: glib::GDestroyNotify);
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
}

fn opt_strdup(text: Option<&[u8]>) -> *mut c_char {
    text.map_or(ptr::null_mut(), glib::strdup)
}

unsafe fn new_array(element: usize) -> *mut GArray {
    unsafe { glib::g_array_new(glib::FALSE, glib::FALSE, element as c_uint) }
}

unsafe fn append<T>(array: *mut GArray, item: &T) {
    unsafe { glib::g_array_append_vals(array, ptr::from_ref(item).cast(), 1) };
}

unsafe extern "C" fn attr_pred_clear(data: *mut c_void) {
    let pred = data.cast::<RawAttrPred>();
    unsafe {
        glib::g_free((*pred).name.cast());
        glib::g_free((*pred).value.cast());
    }
}

unsafe extern "C" fn pseudo_pred_clear(data: *mut c_void) {
    let pred = data.cast::<RawPseudoPred>();
    unsafe {
        glib::g_free((*pred).arg.cast());
        if !(*pred).of_group.is_null() {
            glib::g_ptr_array_free((*pred).of_group, glib::TRUE);
        }
    }
}

unsafe extern "C" fn group_free(data: *mut c_void) {
    unsafe { glib::g_ptr_array_free(data.cast(), glib::TRUE) };
}

unsafe extern "C" fn selector_free_notify(data: *mut c_void) {
    unsafe { ns_css_selector_free(data.cast()) };
}

pub(super) fn group_to_c(group: &[Selector]) -> *mut GPtrArray {
    let array = unsafe { glib::g_ptr_array_new_with_free_func(Some(selector_free_notify)) };
    for sel in group {
        unsafe { glib::g_ptr_array_add(array, selector_to_c(sel).cast()) };
    }
    array
}

fn groups_to_c(groups: Option<&Vec<Vec<Selector>>>) -> *mut GPtrArray {
    let Some(groups) = groups else {
        return ptr::null_mut();
    };
    let array = unsafe { glib::g_ptr_array_new_with_free_func(Some(group_free)) };
    for group in groups {
        unsafe { glib::g_ptr_array_add(array, group_to_c(group).cast()) };
    }
    array
}

fn attr_to_c(pred: &AttrPred) -> RawAttrPred {
    RawAttrPred {
        name: glib::strdup(&pred.name),
        op: pred.op,
        value: opt_strdup(pred.value.as_deref()),
        case_insensitive: glib::boolean(pred.case_insensitive),
        case_sensitive: glib::boolean(pred.case_sensitive),
        html_ci: glib::boolean(pred.html_ci),
        name_bit: pred.name_bit,
    }
}

fn pseudo_to_c(pred: &PseudoPred) -> RawPseudoPred {
    RawPseudoPred {
        kind: pred.kind,
        a: pred.a,
        b: pred.b,
        arg: opt_strdup(pred.arg.as_deref()),
        of_group: pred.of_group.as_deref().map_or(ptr::null_mut(), group_to_c),
    }
}

fn simple_to_c(c: &Compound) -> *mut RawSimple {
    unsafe {
        let classes = glib::g_ptr_array_new_with_free_func(Some(glib::g_free));
        let class_lens = new_array(size_of::<usize>());
        for class in &c.classes {
            glib::g_ptr_array_add(classes, glib::strdup(class).cast());
            append(class_lens, &class.len());
        }
        let attrs = new_array(size_of::<RawAttrPred>());
        g_array_set_clear_func(attrs, Some(attr_pred_clear));
        for pred in &c.attrs {
            append(attrs, &attr_to_c(pred));
        }
        let pseudos = new_array(size_of::<RawPseudoPred>());
        g_array_set_clear_func(pseudos, Some(pseudo_pred_clear));
        for pred in &c.pseudos {
            append(pseudos, &pseudo_to_c(pred));
        }
        let raw = glib::g_malloc0(size_of::<RawSimple>()).cast::<RawSimple>();
        raw.write(RawSimple {
            type_: opt_strdup(c.type_.as_deref()),
            id: opt_strdup(c.id.as_deref()),
            classes,
            class_lens,
            attrs,
            pseudos,
            matches_any: groups_to_c(c.matches_any.as_ref()),
            matches_none: groups_to_c(c.matches_none.as_ref()),
            has_groups: groups_to_c(c.has_groups.as_ref()),
            never_match: glib::boolean(c.never_match),
            ns_none: glib::boolean(c.ns_none),
        });
        raw
    }
}

pub(super) fn selector_to_c(sel: &Selector) -> *mut RawSelector {
    unsafe {
        let compounds = glib::g_ptr_array_new();
        for c in &sel.compounds {
            glib::g_ptr_array_add(compounds, simple_to_c(c).cast());
        }
        let combinators = new_array(size_of::<c_uint>());
        for comb in &sel.combinators {
            append(combinators, comb);
        }
        let mut ancestor_hashes = [0u32; 4];
        ancestor_hashes[..sel.ancestor_hashes.len()].copy_from_slice(&sel.ancestor_hashes);
        let raw = glib::g_malloc0(size_of::<RawSelector>()).cast::<RawSelector>();
        raw.write(RawSelector {
            compounds,
            combinators,
            pseudo_element: sel.pseudo_element,
            spec_a: sel.spec[0],
            spec_b: sel.spec[1],
            spec_c: sel.spec[2],
            ancestor_hashes,
            n_ancestor_hashes: sel.ancestor_hashes.len() as c_uint,
            n_ancestor_attr_hashes: sel.n_ancestor_attr_hashes,
        });
        raw
    }
}

unsafe fn simple_free(s: *mut RawSimple) {
    let Some(simple) = (unsafe { s.as_mut() }) else {
        return;
    };
    unsafe {
        glib::g_free(simple.type_.cast());
        glib::g_free(simple.id.cast());
        glib::g_ptr_array_free(simple.classes, glib::TRUE);
        g_array_free(simple.class_lens, glib::TRUE);
        if !simple.attrs.is_null() {
            g_array_free(simple.attrs, glib::TRUE);
        }
        if !simple.pseudos.is_null() {
            g_array_free(simple.pseudos, glib::TRUE);
        }
        for groups in [simple.matches_any, simple.matches_none, simple.has_groups] {
            if !groups.is_null() {
                glib::g_ptr_array_free(groups, glib::TRUE);
            }
        }
        glib::g_free(s.cast());
    }
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

unsafe fn range<'a>(p: *const c_char, len: usize) -> &'a [u8] {
    if p.is_null() || len == 0 {
        return &[];
    }
    unsafe { core::slice::from_raw_parts(p.cast(), len) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_selector_free(sel: *mut c_void) {
    let raw = sel.cast::<RawSelector>();
    let Some(selector) = (unsafe { raw.as_mut() }) else {
        return;
    };
    unsafe {
        let compounds = &*selector.compounds;
        for i in 0..compounds.len as usize {
            simple_free((*compounds.pdata.add(i)).cast());
        }
        glib::g_ptr_array_free(selector.compounds, glib::TRUE);
        g_array_free(selector.combinators, glib::TRUE);
        glib::g_free(sel);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_selector_list(text: *const c_char) -> *mut GPtrArray {
    let list = unsafe { bytes(text) }
        .map(selector::parse_list)
        .unwrap_or_default();
    group_to_c(&list)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_selector_list_checked(
    text: *const c_char,
    out_valid: *mut GBoolean,
) -> *mut GPtrArray {
    let (list, valid) = selector::parse_list_checked(unsafe { bytes(text) });
    if let Some(out) = unsafe { out_valid.as_mut() } {
        *out = glib::boolean(valid);
    }
    group_to_c(&list)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_selector_group(
    text: *const c_char,
    len: usize,
    depth: c_int,
) -> *mut GPtrArray {
    group_to_c(&selector::parse_group(
        unsafe { range(text, len) },
        depth,
        false,
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_rule_selectors(
    p: *const c_char,
    end: *const c_char,
    out: *mut GPtrArray,
    has_hover: *mut GBoolean,
    has_active: *mut GBoolean,
) -> GBoolean {
    let len = if p.is_null() || end <= p {
        0
    } else {
        unsafe { end.offset_from(p) as usize }
    };
    let parsed = selector::parse_rule_selectors(unsafe { range(p, len) });
    for sel in &parsed.selectors {
        unsafe { glib::g_ptr_array_add(out, selector_to_c(sel).cast()) };
    }
    unsafe {
        *has_hover = glib::boolean(parsed.has_hover);
        *has_active = glib::boolean(parsed.has_active);
    }
    glib::boolean(parsed.ok)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_supports_selector(text: *const c_char, len: usize) -> GBoolean {
    let text = unsafe { range(text, len) };
    let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
    glib::boolean(selector::supports_selector(text))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_selector_attr_ancestor_hashes() -> GBoolean {
    glib::boolean(selector::get(&FLAGS.attr_ancestor_hashes))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_identifier_hash(
    kind: c_char,
    name: *const c_char,
    len: usize,
) -> u32 {
    identifier_hash(kind as u8, unsafe { range(name, len) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_attr_value_hash(
    name: *const c_char,
    value: *const c_char,
    value_len: usize,
) -> u32 {
    attr_value_hash(unsafe { bytes(name) }.unwrap_or_default(), unsafe {
        range(value, value_len)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_anb_int_strict(text: *const c_char, out: *mut c_int) -> GBoolean {
    match unsafe { bytes(text) }.and_then(anb_int_strict) {
        Some(value) => {
            unsafe { *out = value };
            glib::TRUE
        }
        None => glib::FALSE,
    }
}
