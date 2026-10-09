//! Southstar — the C ABI of the style sheet parser: css.c's ns_css_stylesheet, ns_css_rule and at-rule structs built as the Rust parser reads a sheet, freeing them, resolving a sheet's URLs and moving it into a layer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::mem::{offset_of, size_of};
use core::ptr;
use std::ffi::CString;
use std::sync::atomic::{AtomicU64, Ordering};

use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};

use super::declarations::{RawPending, RawRule, RuleSink};
use super::selector::{group_to_c, selector_to_c};
use super::shorthand::RawDecl;
use super::value::{KIND_URL, NsCssValue};
use crate::declarations;
use crate::nesting;
use crate::selector::{self, RuleSelectors};
use crate::sheet::{self, FontFace, ScopeText, Stop, layer_join};
use crate::transform::Transform;

#[repr(C)]
pub(super) struct RawSheet {
    pub(super) rules: *mut GPtrArray,
    imports: *mut GArray,
    layer_names: *mut GPtrArray,
    layers: *mut GHashTable,
    font_faces: *mut GArray,
    keyframes: *mut GArray,
    property_rules: *mut GArray,
    page_rule: *mut PageRule,
    has_container_rules: GBoolean,
    has_container_units: GBoolean,
    has_hover_rules: GBoolean,
    has_active_rules: GBoolean,
    cached: GBoolean,
    pseudo_mask: c_uint,
    serial: u64,
    resolved_base: *mut c_char,
    index: *mut c_void,
}

#[repr(C)]
struct RawImport {
    url: *mut c_char,
    layer_name: *mut c_char,
    media: *mut c_char,
}

#[repr(C)]
struct RawFontFace {
    family: *mut c_char,
    src_url: *mut c_char,
    unicode_range: *mut c_char,
    weight: c_int,
    slant: c_int,
}

#[repr(C)]
struct RawStop {
    pct: f64,
    opacity: f64,
    has_opacity: GBoolean,
    transform: Transform,
    has_transform: GBoolean,
    color: [u8; 4],
    has_color: GBoolean,
    bg_color: [u8; 4],
    has_bg_color: GBoolean,
    raw_props: *mut c_char,
}

#[repr(C)]
struct RawKeyframes {
    name: *mut c_char,
    n_stops: c_int,
    stops: *mut RawStop,
}

#[repr(C)]
struct RawPropertyRule {
    name: *mut c_char,
    initial_value: *mut c_char,
    syntax_text: *mut c_char,
    syntax: *mut c_void,
    inherits: GBoolean,
    has_initial: GBoolean,
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct PageRule {
    pub width: f64,
    pub height: f64,
    pub has_size: GBoolean,
    pub landscape: GBoolean,
    pub margin: [f64; 4],
    pub has_margin: [GBoolean; 4],
}

#[repr(C)]
pub(super) struct RawScope {
    pub(super) roots: *mut GPtrArray,
    pub(super) limits: *mut GPtrArray,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<RawSheet>() == 112
        && offset_of!(RawSheet, has_container_rules) == 64
        && offset_of!(RawSheet, serial) == 88
        && offset_of!(RawSheet, index) == 104
        && size_of::<RawImport>() == 24
        && size_of::<RawFontFace>() == 32
        && size_of::<RawStop>() == 2176
        && offset_of!(RawStop, has_transform) == 2144
        && offset_of!(RawStop, raw_props) == 2168
        && size_of::<RawKeyframes>() == 24
        && size_of::<RawPropertyRule>() == 40
        && size_of::<PageRule>() == 72
        && size_of::<RawScope>() == 16
);

unsafe extern "C" {
    fn g_array_set_clear_func(array: *mut GArray, clear_func: glib::GDestroyNotify);
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
    fn g_array_sort(
        array: *mut GArray,
        compare: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> c_int>,
    );
    fn g_hash_table_iter_replace(iter: *mut glib::GHashTableIter, value: *mut c_void);
    fn ns_css_value_free(v: *mut NsCssValue);
    fn ns_css_container_query_free(query: *mut c_void);
    fn ns_css_syntax_def_parse(text: *const c_char) -> *mut c_void;
    fn ns_css_syntax_def_free(syntax: *mut c_void);
    fn ns_css_syntax_def_universal(syntax: *const c_void) -> GBoolean;
    fn ns_css_syntax_def_initial_valid(syntax: *const c_void, value: *const c_char) -> GBoolean;
    fn ns_css_syntax_def_matches(syntax: *const c_void, value: *const c_char) -> GBoolean;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
}

static SERIAL_NEXT: AtomicU64 = AtomicU64::new(1);

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

fn opt_strdup(text: Option<&[u8]>) -> *mut c_char {
    text.map_or(ptr::null_mut(), glib::strdup)
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

unsafe fn new_array(element: usize, clear: glib::GDestroyNotify) -> *mut GArray {
    unsafe {
        let array = glib::g_array_new(glib::FALSE, glib::FALSE, element as c_uint);
        if clear.is_some() {
            g_array_set_clear_func(array, clear);
        }
        array
    }
}

unsafe fn append<T>(array: *mut GArray, item: &T) {
    unsafe { glib::g_array_append_vals(array, ptr::from_ref(item).cast(), 1) };
}

unsafe fn elements<'a, T>(array: *mut GArray) -> &'a mut [T] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.data.is_null() && a.len > 0 => unsafe {
            core::slice::from_raw_parts_mut(a.data.cast::<T>(), a.len as usize)
        },
        _ => &mut [],
    }
}

unsafe fn pointers<'a, T>(array: *mut GPtrArray) -> &'a [*mut T] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.pdata.is_null() && a.len > 0 => unsafe {
            core::slice::from_raw_parts(a.pdata.cast::<*mut T>(), a.len as usize)
        },
        _ => &[],
    }
}

unsafe extern "C" fn import_clear(data: *mut c_void) {
    let import = data.cast::<RawImport>();
    unsafe {
        glib::g_free((*import).url.cast());
        glib::g_free((*import).layer_name.cast());
        glib::g_free((*import).media.cast());
    }
}

unsafe extern "C" fn font_face_clear(data: *mut c_void) {
    let face = data.cast::<RawFontFace>();
    unsafe {
        glib::g_free((*face).family.cast());
        glib::g_free((*face).src_url.cast());
        glib::g_free((*face).unicode_range.cast());
    }
}

unsafe extern "C" fn keyframes_clear(data: *mut c_void) {
    let kf = data.cast::<RawKeyframes>();
    unsafe {
        glib::g_free((*kf).name.cast());
        for i in 0..usize::try_from((*kf).n_stops).unwrap_or(0) {
            glib::g_free((*(*kf).stops.add(i)).raw_props.cast());
        }
        glib::g_free((*kf).stops.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_property_rule_clear(data: *mut c_void) {
    let rule = data.cast::<RawPropertyRule>();
    unsafe {
        glib::g_free((*rule).name.cast());
        glib::g_free((*rule).initial_value.cast());
        glib::g_free((*rule).syntax_text.cast());
        ns_css_syntax_def_free((*rule).syntax);
    }
}

unsafe extern "C" fn scope_free(data: *mut c_void) {
    let scope = data.cast::<RawScope>();
    let Some(raw) = (unsafe { scope.as_ref() }) else {
        return;
    };
    unsafe {
        if !raw.roots.is_null() {
            glib::g_ptr_array_free(raw.roots, glib::TRUE);
        }
        if !raw.limits.is_null() {
            glib::g_ptr_array_free(raw.limits, glib::TRUE);
        }
        glib::g_free(data);
    }
}

unsafe extern "C" fn stop_cmp(a: *const c_void, b: *const c_void) -> c_int {
    let (da, db) = unsafe { ((*a.cast::<RawStop>()).pct, (*b.cast::<RawStop>()).pct) };
    if da < db {
        -1
    } else if da > db {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_rule_free(data: *mut c_void) {
    let rule = data.cast::<RawRule>();
    let Some(raw) = (unsafe { rule.as_ref() }) else {
        return;
    };
    unsafe {
        for &sel in pointers::<c_void>(raw.selectors) {
            super::selector::ns_css_selector_free(sel);
        }
        glib::g_ptr_array_free(raw.selectors, glib::TRUE);
        for decl in elements::<RawDecl>(raw.decls) {
            ns_css_value_free(decl.value);
        }
        g_array_free(raw.decls, glib::TRUE);
        if !raw.vars.is_null() {
            glib::g_hash_table_destroy(raw.vars);
        }
        if !raw.var_important.is_null() {
            glib::g_hash_table_destroy(raw.var_important);
        }
        if !raw.pending.is_null() {
            g_array_free(raw.pending, glib::TRUE);
        }
        glib::g_free(raw.layer_name.cast());
        glib::g_free(raw.container_condition.cast());
        ns_css_container_query_free(raw.container_query);
        if !raw.scopes.is_null() {
            glib::g_ptr_array_free(raw.scopes, glib::TRUE);
        }
        glib::g_free(data);
    }
}

pub(crate) struct SyntaxDef(*mut c_void);

impl SyntaxDef {
    pub(crate) fn parse(text: &[u8]) -> Option<SyntaxDef> {
        let text = c_string(text);
        let raw = unsafe { ns_css_syntax_def_parse(text.as_ptr()) };
        (!raw.is_null()).then_some(SyntaxDef(raw))
    }

    pub(crate) fn universal(&self) -> bool {
        unsafe { ns_css_syntax_def_universal(self.0) != 0 }
    }

    pub(crate) fn initial_valid(&self, initial: Option<&[u8]>) -> bool {
        let initial = initial.map(c_string);
        let initial = initial.as_ref().map_or(ptr::null(), |text| text.as_ptr());
        unsafe { ns_css_syntax_def_initial_valid(self.0, initial) != 0 }
    }

    pub(crate) fn matches(&self, value: &[u8]) -> bool {
        let value = c_string(value);
        unsafe { ns_css_syntax_def_matches(self.0, value.as_ptr()) != 0 }
    }

    fn into_raw(self) -> *mut c_void {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }
}

impl Drop for SyntaxDef {
    fn drop(&mut self) {
        unsafe { ns_css_syntax_def_free(self.0) };
    }
}

pub(crate) struct SheetBuilder {
    raw: *mut RawSheet,
}

pub(crate) struct RuleBuilder {
    raw: *mut RawRule,
}

impl Drop for RuleBuilder {
    fn drop(&mut self) {
        unsafe { ns_css_rule_free(self.raw.cast()) };
    }
}

impl RuleBuilder {
    fn rule(&mut self) -> &mut RawRule {
        unsafe { &mut *self.raw }
    }

    pub(crate) fn apply_scopes(&mut self, scopes: &[ScopeText]) -> bool {
        if scopes.is_empty() {
            return true;
        }
        let array = unsafe { glib::g_ptr_array_new_with_free_func(Some(scope_free)) };
        self.rule().scopes = array;
        for text in scopes {
            let Some(roots) = sheet::scope_group_valid(text.start.as_deref().unwrap_or(b":root"))
            else {
                return false;
            };
            let limits = match text.end.as_deref() {
                Some(end) => match sheet::scope_group_valid(end) {
                    Some(limits) => Some(limits),
                    None => return false,
                },
                None => None,
            };
            unsafe {
                let scope = glib::g_malloc0(size_of::<RawScope>()).cast::<RawScope>();
                scope.write(RawScope {
                    roots: group_to_c(&roots),
                    limits: limits.as_deref().map_or(ptr::null_mut(), group_to_c),
                });
                glib::g_ptr_array_add(array, scope.cast());
            }
        }
        true
    }

    pub(crate) fn has_scopes(&self) -> bool {
        unsafe { !(*self.raw).scopes.is_null() }
    }

    pub(crate) fn parse_selectors(&mut self, text: &[u8]) -> RuleSelectors {
        let parsed = selector::parse_rule_selectors(text);
        let array = self.rule().selectors;
        for sel in &parsed.selectors {
            unsafe { glib::g_ptr_array_add(array, selector_to_c(sel).cast()) };
        }
        parsed
    }

    pub(crate) fn parse_declarations(&mut self, s: &[u8], p: usize) -> usize {
        if p >= s.len() {
            return p;
        }
        let raw = self.raw;
        let mut sink = RuleSink::new(unsafe { (*raw).decls }, raw);
        p + declarations::parse_block(&s[p..], 0, &mut sink)
    }
}

impl SheetBuilder {
    fn sheet(&mut self) -> &mut RawSheet {
        unsafe { &mut *self.raw }
    }

    pub(crate) fn serial(&self) -> u64 {
        unsafe { (*self.raw).serial }
    }

    pub(crate) fn layer_count(&self) -> u32 {
        unsafe { (*self.raw).layer_names.as_ref() }.map_or(0, |names| names.len)
    }

    pub(crate) fn layer_register(&mut self, name: &[u8]) {
        if name.is_empty() {
            return;
        }
        let sheet = self.sheet();
        unsafe {
            if sheet.layer_names.is_null() {
                sheet.layer_names = glib::g_ptr_array_new_with_free_func(Some(glib::g_free));
            }
            if sheet.layers.is_null() {
                sheet.layers =
                    glib::g_hash_table_new(Some(glib::g_str_hash), Some(glib::g_str_equal));
            }
            let key = c_string(name);
            if !glib::g_hash_table_lookup(sheet.layers, key.as_ptr().cast()).is_null() {
                return;
            }
            let rank = (*sheet.layer_names).len as usize;
            let owned = glib::strdup(name);
            glib::g_ptr_array_add(sheet.layer_names, owned.cast());
            glib::g_hash_table_insert(sheet.layers, owned.cast(), (rank + 1) as *mut c_void);
        }
    }

    pub(crate) fn add_import(&mut self, url: &[u8], layer: Option<&[u8]>, media: Option<&[u8]>) {
        if url.is_empty() {
            return;
        }
        let sheet = self.sheet();
        if sheet.imports.is_null() {
            sheet.imports = unsafe { new_array(size_of::<RawImport>(), Some(import_clear)) };
        }
        if let Some(layer) = layer {
            self.layer_register(layer);
        }
        let import = RawImport {
            url: glib::strdup(url),
            layer_name: opt_strdup(layer),
            media: opt_strdup(media),
        };
        unsafe { append(self.sheet().imports, &import) };
    }

    pub(crate) fn ensure_font_faces(&mut self) {
        let sheet = self.sheet();
        if sheet.font_faces.is_null() {
            sheet.font_faces =
                unsafe { new_array(size_of::<RawFontFace>(), Some(font_face_clear)) };
        }
    }

    pub(crate) fn push_font_face(&mut self, face: FontFace) {
        let raw = RawFontFace {
            family: glib::strdup(&face.family),
            src_url: glib::strdup(&face.src_url),
            unicode_range: opt_strdup(face.unicode_range.as_deref()),
            weight: face.weight,
            slant: face.slant,
        };
        unsafe { append(self.sheet().font_faces, &raw) };
    }

    pub(crate) fn push_keyframes(&mut self, name: &[u8], stops: Vec<Stop>) {
        unsafe {
            let sheet = self.sheet();
            if sheet.keyframes.is_null() {
                sheet.keyframes = new_array(size_of::<RawKeyframes>(), Some(keyframes_clear));
            }
            let array = new_array(size_of::<RawStop>(), None);
            for stop in &stops {
                let raw = RawStop {
                    pct: stop.pct,
                    opacity: stop.opacity,
                    has_opacity: glib::boolean(stop.has_opacity),
                    transform: stop.transform,
                    has_transform: glib::boolean(stop.has_transform),
                    color: stop.color,
                    has_color: glib::boolean(stop.has_color),
                    bg_color: stop.bg_color,
                    has_bg_color: glib::boolean(stop.has_bg_color),
                    raw_props: opt_strdup(stop.raw_props.as_deref()),
                };
                append(array, &raw);
            }
            g_array_sort(array, Some(stop_cmp));
            let n = (*array).len as usize;
            let stops_raw = glib::g_malloc0(size_of::<RawStop>() * n.max(1)).cast::<RawStop>();
            if n > 0 {
                ptr::copy_nonoverlapping((*array).data.cast::<RawStop>(), stops_raw, n);
            }
            g_array_free(array, glib::TRUE);
            let kf = RawKeyframes {
                name: glib::strdup(name),
                n_stops: n as c_int,
                stops: stops_raw,
            };
            append(self.sheet().keyframes, &kf);
        }
    }

    pub(crate) fn push_property_rule(
        &mut self,
        name: &[u8],
        initial: Option<&[u8]>,
        syntax_text: Option<&[u8]>,
        syntax: SyntaxDef,
        inherits: bool,
        has_initial: bool,
    ) {
        let sheet = self.sheet();
        if sheet.property_rules.is_null() {
            sheet.property_rules = unsafe {
                new_array(
                    size_of::<RawPropertyRule>(),
                    Some(ns_css_property_rule_clear),
                )
            };
        }
        let rule = RawPropertyRule {
            name: glib::strdup(name),
            initial_value: opt_strdup(initial),
            syntax_text: opt_strdup(syntax_text),
            syntax: syntax.into_raw(),
            inherits: glib::boolean(inherits),
            has_initial: glib::boolean(has_initial),
        };
        unsafe { append(self.sheet().property_rules, &rule) };
    }

    pub(crate) fn page_rule(&mut self) -> &mut PageRule {
        let sheet = self.sheet();
        if sheet.page_rule.is_null() {
            sheet.page_rule = unsafe { glib::g_malloc0(size_of::<PageRule>()) }.cast();
        }
        unsafe { &mut *sheet.page_rule }
    }

    pub(crate) fn set_has_container_rules(&mut self) {
        self.sheet().has_container_rules = glib::TRUE;
    }

    pub(crate) fn set_has_hover_rules(&mut self) {
        self.sheet().has_hover_rules = glib::TRUE;
    }

    pub(crate) fn set_has_active_rules(&mut self) {
        self.sheet().has_active_rules = glib::TRUE;
    }

    pub(crate) fn rules_len(&self) -> usize {
        unsafe { (*(*self.raw).rules).len as usize }
    }

    pub(crate) fn join_container_condition(&mut self, from: usize, canon: &[u8]) {
        let rules = unsafe { pointers::<RawRule>(self.sheet().rules) };
        for &rule in rules.iter().skip(from) {
            let rule = unsafe { &mut *rule };
            let joined = match unsafe { bytes(rule.container_condition) } {
                Some(existing) => [canon, b"\x1f", existing].concat(),
                None => canon.to_vec(),
            };
            unsafe { glib::g_free(rule.container_condition.cast()) };
            rule.container_condition = glib::strdup(&joined);
        }
    }

    pub(crate) fn new_rule(&self, layer: Option<&[u8]>, source_order: i32) -> RuleBuilder {
        unsafe {
            let raw = glib::g_malloc0(size_of::<RawRule>()).cast::<RawRule>();
            (*raw).selectors = glib::g_ptr_array_new();
            (*raw).decls = new_array(size_of::<RawDecl>(), None);
            (*raw).layer_name = opt_strdup(layer);
            (*raw).source_order = source_order;
            RuleBuilder { raw }
        }
    }

    pub(crate) fn push_rule(&mut self, rule: RuleBuilder) {
        let raw = rule.raw;
        core::mem::forget(rule);
        unsafe { glib::g_ptr_array_add(self.sheet().rules, raw.cast()) };
    }
}

unsafe extern "C" fn rule_free_notify(data: *mut c_void) {
    unsafe { ns_css_rule_free(data) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_stylesheet_parse(text: *const c_char, len: isize) -> *mut c_void {
    let raw = unsafe { glib::g_malloc0(size_of::<RawSheet>()) }.cast::<RawSheet>();
    unsafe {
        (*raw).serial = SERIAL_NEXT.fetch_add(1, Ordering::Relaxed);
        (*raw).rules = glib::g_ptr_array_new_with_free_func(Some(rule_free_notify));
    }
    if text.is_null() {
        return raw.cast();
    }
    let len =
        usize::try_from(len).unwrap_or_else(|_| unsafe { CStr::from_ptr(text) }.to_bytes().len());
    let text = unsafe { core::slice::from_raw_parts(text.cast::<u8>(), len) };
    unsafe { (*raw).has_container_units = glib::boolean(nesting::has_container_units(text)) };
    let mut flattened = nesting::flatten(text);
    if let Some(nul) = flattened.iter().position(|&c| c == 0) {
        flattened.truncate(nul);
    }
    let mut builder = SheetBuilder { raw };
    sheet::parse_into(&mut builder, &flattened);
    raw.cast()
}

fn url_should_resolve(url: &[u8]) -> bool {
    let local = url.get(..5).is_some_and(|scheme| {
        scheme.eq_ignore_ascii_case(b"data:") || scheme.eq_ignore_ascii_case(b"blob:")
    });
    !url.is_empty() && url[0] != b'#' && !local
}

fn url_resolve(base: &CStr, rel: &[u8]) -> Option<*mut c_char> {
    let rel = c_string(rel);
    let resolved = unsafe { ns_url_resolve(base.as_ptr(), rel.as_ptr()) };
    (!resolved.is_null()).then_some(resolved)
}

fn is_gspace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn raw_text_resolve_urls(text: &[u8], base: &CStr) -> Option<Vec<u8>> {
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    find(text, b"url(")?;
    let mut out = Vec::with_capacity(text.len());
    let mut changed = false;
    let mut p = 0;
    while p < text.len() {
        let Some(hit) = find(&text[p..], b"url(").map(|at| p + at) else {
            out.extend_from_slice(&text[p..]);
            break;
        };
        let Some(close) = text[hit + 4..]
            .iter()
            .position(|&c| c == b')')
            .map(|at| hit + 4 + at)
        else {
            out.extend_from_slice(&text[p..]);
            break;
        };
        out.extend_from_slice(&text[p..hit]);
        let mut s = hit + 4;
        while s < close && is_gspace(text[s]) {
            s += 1;
        }
        let mut e = close;
        while e > s && is_gspace(text[e - 1]) {
            e -= 1;
        }
        if e > s && (text[s] == b'"' || text[s] == b'\'') {
            let q = text[s];
            s += 1;
            if e > s && text[e - 1] == q {
                e -= 1;
            }
        }
        let rel = &text[s..e];
        let resolved = if url_should_resolve(rel) {
            url_resolve(base, rel)
        } else {
            None
        };
        let abs = resolved.map(|ptr| {
            let owned = unsafe { glib::GStr::take(ptr) };
            owned.map(|abs| abs.to_bytes().to_vec()).unwrap_or_default()
        });
        match abs.filter(|abs| abs.as_slice() != rel) {
            Some(abs) => {
                out.extend_from_slice(b"url(\"");
                out.extend_from_slice(&abs);
                out.extend_from_slice(b"\")");
                changed = true;
            }
            None => out.extend_from_slice(&text[hit..close + 1]),
        }
        p = close + 1;
    }
    changed.then_some(out)
}

unsafe fn value_resolve_url(mut v: *mut NsCssValue, base: &CStr) {
    while let Some(value) = unsafe { v.as_mut() } {
        if value.kind == KIND_URL {
            let url = unsafe { value.u.url };
            if unsafe { bytes(url) }.is_some_and(url_should_resolve) {
                let resolved = unsafe { ns_url_resolve(base.as_ptr(), url) };
                if !resolved.is_null() {
                    unsafe {
                        glib::g_free(url.cast());
                        value.u.url = resolved;
                    }
                }
            }
        }
        v = value.next_layer;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_stylesheet_resolve_urls(
    sheet: *mut c_void,
    base_url: *const c_char,
) {
    let (Some(sheet), false) = (
        unsafe { sheet.cast::<RawSheet>().as_mut() },
        base_url.is_null(),
    ) else {
        return;
    };
    let base = unsafe { CStr::from_ptr(base_url) };
    if unsafe { bytes(sheet.resolved_base) } == Some(base.to_bytes()) {
        return;
    }
    unsafe {
        glib::g_free(sheet.resolved_base.cast());
        sheet.resolved_base = glib::strdup(base.to_bytes());
    }
    for &rule in unsafe { pointers::<RawRule>(sheet.rules) } {
        let Some(rule) = (unsafe { rule.as_mut() }) else {
            continue;
        };
        for decl in unsafe { elements::<RawDecl>(rule.decls) } {
            unsafe { value_resolve_url(decl.value, base) };
        }
        if !rule.vars.is_null() {
            let mut iter = glib::GHashTableIter::new();
            unsafe { glib::g_hash_table_iter_init(&mut iter, rule.vars) };
            let (mut key, mut value) = (ptr::null_mut(), ptr::null_mut());
            while unsafe { glib::g_hash_table_iter_next(&mut iter, &mut key, &mut value) } != 0 {
                let text = unsafe { bytes(value.cast()) }.unwrap_or_default();
                if let Some(resolved) = raw_text_resolve_urls(text, base) {
                    unsafe { g_hash_table_iter_replace(&mut iter, glib::strdup(&resolved).cast()) };
                }
            }
        }
        for pending in unsafe { elements::<RawPending>(rule.pending) } {
            let text = unsafe { bytes(pending.raw_vtext) }.unwrap_or_default();
            if let Some(resolved) = raw_text_resolve_urls(text, base) {
                unsafe { glib::g_free(pending.raw_vtext.cast()) };
                pending.raw_vtext = glib::strdup(&resolved);
            }
        }
    }
    for face in unsafe { elements::<RawFontFace>(sheet.font_faces) } {
        if !unsafe { bytes(face.src_url) }.is_some_and(url_should_resolve) {
            continue;
        }
        let resolved = unsafe { ns_url_resolve(base.as_ptr(), face.src_url) };
        if resolved.is_null() {
            continue;
        }
        unsafe { glib::g_free(face.src_url.cast()) };
        face.src_url = resolved;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_stylesheet_force_layer(
    sheet: *mut c_void,
    layer_name: *const c_char,
) {
    let raw = sheet.cast::<RawSheet>();
    let (Some(sheet), Some(layer)) = (unsafe { raw.as_mut() }, unsafe { bytes(layer_name) }) else {
        return;
    };
    if layer.is_empty() {
        return;
    }
    sheet.serial = SERIAL_NEXT.fetch_add(1, Ordering::Relaxed);
    let old_names = sheet.layer_names;
    let old_layers = sheet.layers;
    sheet.layer_names = ptr::null_mut();
    sheet.layers = ptr::null_mut();
    let mut builder = SheetBuilder { raw };
    builder.layer_register(layer);
    for &old in unsafe { pointers::<c_char>(old_names) } {
        let full = layer_join(Some(layer), unsafe { bytes(old) }.unwrap_or_default());
        builder.layer_register(&full);
    }
    for &rule in unsafe { pointers::<RawRule>((*raw).rules) } {
        let rule = unsafe { &mut *rule };
        let full = match unsafe { bytes(rule.layer_name) } {
            Some(existing) => layer_join(Some(layer), existing),
            None => layer.to_vec(),
        };
        unsafe { glib::g_free(rule.layer_name.cast()) };
        rule.layer_name = glib::strdup(&full);
    }
    for import in unsafe { elements::<RawImport>((*raw).imports) } {
        let full = match unsafe { bytes(import.layer_name) } {
            Some(existing) => layer_join(Some(layer), existing),
            None => layer.to_vec(),
        };
        unsafe { glib::g_free(import.layer_name.cast()) };
        import.layer_name = glib::strdup(&full);
        builder.layer_register(&full);
    }
    unsafe {
        if !old_layers.is_null() {
            glib::g_hash_table_destroy(old_layers);
        }
        if !old_names.is_null() {
            glib::g_ptr_array_free(old_names, glib::TRUE);
        }
    }
}
