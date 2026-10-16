//! Southstar — the style sheet parser's GLib-backed storage: css.c's ns_css_stylesheet, ns_css_rule and at-rule structs built as the Rust parser reads a sheet, the owning handle that frees them, resolving a sheet's URLs and moving it into a layer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::mem::{ManuallyDrop, size_of};
use core::ops::Deref;
use core::ptr::{self, NonNull};
use std::ffi::CString;
use std::sync::atomic::{AtomicU64, Ordering};

use southstar_css_prop_syntax as prop_syntax;
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};

use super::container::container_query_free;
use super::declarations::{RawPending, RawRule, RuleSink};
use super::keyframes::{KeyframeStop, Keyframes};
use super::rule_index::index_free;
use super::selector::{group_to_c, selector_to_c};
use super::shorthand::RawDecl;
use super::value::{KIND_URL, NsCssValue, value_free};
use crate::declarations;
use crate::nesting;
use crate::selector::{self, RuleSelectors};
use crate::sheet::{self, FontFace, ScopeText, Stop, layer_join};

pub(super) struct RawSheet {
    pub(super) rules: *mut GPtrArray,
    imports: Vec<Import>,
    pub(super) layer_names: *mut GPtrArray,
    layers: *mut GHashTable,
    font_faces: Vec<FontFaceData>,
    keyframes: Vec<Keyframes>,
    property_rules: Vec<PropertyRule>,
    page_rule: Option<PageRule>,
    has_container_rules: bool,
    has_container_units: bool,
    has_hover_rules: bool,
    has_active_rules: bool,
    cached: Cell<bool>,
    pub(super) pseudo_mask: Cell<c_uint>,
    serial: u64,
    resolved_base: Option<Vec<u8>>,
    pub(super) index: Cell<*mut c_void>,
}

impl RawSheet {
    fn new(serial: u64, has_container_units: bool) -> RawSheet {
        RawSheet {
            // SAFETY: rule_free_notify frees the RawRule pointers this array is given.
            rules: unsafe { glib::g_ptr_array_new_with_free_func(Some(rule_free_notify)) },
            imports: Vec::new(),
            layer_names: ptr::null_mut(),
            layers: ptr::null_mut(),
            font_faces: Vec::new(),
            keyframes: Vec::new(),
            property_rules: Vec::new(),
            page_rule: None,
            has_container_rules: false,
            has_container_units,
            has_hover_rules: false,
            has_active_rules: false,
            cached: Cell::new(false),
            pseudo_mask: Cell::new(0),
            serial,
            resolved_base: None,
            index: Cell::new(ptr::null_mut()),
        }
    }

    pub(super) fn serial(&self) -> u64 {
        self.serial
    }

    pub(super) fn property_rules(&self) -> &[PropertyRule] {
        &self.property_rules
    }

    fn layer_register(&mut self, name: &[u8]) {
        if name.is_empty() {
            return;
        }
        unsafe {
            if self.layer_names.is_null() {
                self.layer_names = glib::g_ptr_array_new_with_free_func(Some(glib::g_free));
            }
            if self.layers.is_null() {
                self.layers =
                    glib::g_hash_table_new(Some(glib::g_str_hash), Some(glib::g_str_equal));
            }
            let key = c_string(name);
            if !glib::g_hash_table_lookup(self.layers, key.as_ptr().cast()).is_null() {
                return;
            }
            let rank = (*self.layer_names).len as usize;
            let owned = glib::strdup(name);
            glib::g_ptr_array_add(self.layer_names, owned.cast());
            glib::g_hash_table_insert(self.layers, owned.cast(), (rank + 1) as *mut c_void);
        }
    }
}

impl Drop for RawSheet {
    fn drop(&mut self) {
        if !self.rules.is_null() {
            // SAFETY: rules is the sheet's own array, and its free function frees each rule.
            unsafe { glib::g_ptr_array_free(self.rules, glib::TRUE) };
        }
        if !self.layers.is_null() {
            // SAFETY: layers is the sheet's own table; its keys belong to layer_names, freed next.
            unsafe { glib::g_hash_table_destroy(self.layers) };
        }
        if !self.layer_names.is_null() {
            // SAFETY: layer_names is the sheet's own array of GLib strings.
            unsafe { glib::g_ptr_array_free(self.layer_names, glib::TRUE) };
        }
        // SAFETY: index is null or the rule index built for this sheet alone.
        unsafe { index_free(self.index.get().cast()) };
    }
}

struct Import {
    url: CString,
    layer_name: Option<CString>,
    media: Option<CString>,
}

struct FontFaceData {
    family: CString,
    src_url: CString,
    unicode_range: Option<CString>,
    weight: c_int,
    slant: c_int,
}

pub(super) struct PropertyRule {
    name: CString,
    initial_value: Option<CString>,
    syntax: prop_syntax::SyntaxDef,
    inherits: bool,
    has_initial: bool,
}

impl PropertyRule {
    pub(super) fn registered(
        name: &[u8],
        syntax: prop_syntax::SyntaxDef,
        inherits: bool,
        initial_value: Option<&[u8]>,
        has_initial: bool,
    ) -> Self {
        PropertyRule {
            name: c_text(name),
            initial_value: initial_value.filter(|_| has_initial).map(c_text),
            syntax,
            inherits,
            has_initial,
        }
    }

    pub(super) fn name(&self) -> &CStr {
        &self.name
    }

    pub(super) fn inherits(&self) -> bool {
        self.inherits
    }

    pub(super) fn initial(&self) -> Option<&[u8]> {
        self.registered_initial().flatten()
    }

    pub(super) fn registered_initial(&self) -> Option<Option<&[u8]>> {
        self.has_initial
            .then(|| self.initial_value.as_deref().map(CStr::to_bytes))
    }

    pub(super) fn typed_syntax(&self) -> Option<&prop_syntax::SyntaxDef> {
        Some(&self.syntax).filter(|syntax| !syntax.universal())
    }

    pub(super) fn rejects(&self, value: &[u8]) -> bool {
        self.typed_syntax()
            .is_some_and(|syntax| !prop_syntax::matches(syntax, value))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PageRule {
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
const _: () = assert!(size_of::<PageRule>() == 72);

unsafe extern "C" {
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
    fn g_hash_table_iter_replace(iter: *mut glib::GHashTableIter, value: *mut c_void);
}

static SERIAL_NEXT: AtomicU64 = AtomicU64::new(1);

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

fn until_nul(text: &[u8]) -> &[u8] {
    text.iter()
        .position(|&c| c == 0)
        .map_or(text, |nul| &text[..nul])
}

fn c_text(text: &[u8]) -> CString {
    CString::new(until_nul(text)).unwrap_or_default()
}

fn opt_strdup(text: Option<&[u8]>) -> *mut c_char {
    text.map_or(ptr::null_mut(), glib::strdup)
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

unsafe fn new_array(element: usize) -> *mut GArray {
    unsafe { glib::g_array_new(glib::FALSE, glib::FALSE, element as c_uint) }
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

fn sort_stops(stops: &mut Vec<KeyframeStop>) {
    let pcts: Vec<f64> = stops.iter().map(KeyframeStop::pct).collect();
    let mut order: Vec<usize> = (0..stops.len()).collect();
    merge_sort(&mut order, &mut Vec::with_capacity(stops.len()), &|a, b| {
        pcts[a] <= pcts[b] || pcts[a].partial_cmp(&pcts[b]).is_none()
    });
    let mut taken: Vec<Option<KeyframeStop>> = stops.drain(..).map(Some).collect();
    stops.extend(order.iter().filter_map(|&i| taken[i].take()));
}

fn merge_sort(
    order: &mut [usize],
    merged: &mut Vec<usize>,
    in_order: &dyn Fn(usize, usize) -> bool,
) {
    if order.len() <= 1 {
        return;
    }
    let (front, back) = order.split_at_mut(order.len() / 2);
    merge_sort(front, merged, in_order);
    merge_sort(back, merged, in_order);
    merged.clear();
    let (mut i, mut j) = (0, 0);
    while i < front.len() && j < back.len() {
        if in_order(front[i], back[j]) {
            merged.push(front[i]);
            i += 1;
        } else {
            merged.push(back[j]);
            j += 1;
        }
    }
    merged.extend_from_slice(&front[i..]);
    order[..merged.len()].copy_from_slice(merged);
}

pub(super) unsafe fn rule_free(data: *mut c_void) {
    let rule = data.cast::<RawRule>();
    let Some(raw) = (unsafe { rule.as_ref() }) else {
        return;
    };
    unsafe {
        for &sel in pointers::<c_void>(raw.selectors) {
            super::selector::selector_free(sel);
        }
        glib::g_ptr_array_free(raw.selectors, glib::TRUE);
        for decl in elements::<RawDecl>(raw.decls) {
            value_free(decl.value);
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
        container_query_free(raw.container_query);
        if !raw.scopes.is_null() {
            glib::g_ptr_array_free(raw.scopes, glib::TRUE);
        }
        glib::g_free(data);
    }
}

pub(crate) struct SyntaxDef(prop_syntax::SyntaxDef);

impl SyntaxDef {
    pub(crate) fn parse(text: &[u8]) -> Option<SyntaxDef> {
        prop_syntax::parse(text).map(SyntaxDef)
    }

    pub(crate) fn universal(&self) -> bool {
        self.0.universal()
    }

    pub(crate) fn initial_valid(&self, initial: Option<&[u8]>) -> bool {
        initial.is_some_and(|initial| prop_syntax::initial_valid(&self.0, initial))
    }

    pub(crate) fn matches(&self, value: &[u8]) -> bool {
        prop_syntax::matches(&self.0, value)
    }
}

pub(crate) struct SheetBuilder {
    sheet: Box<RawSheet>,
}

pub(crate) struct RuleBuilder {
    raw: *mut RawRule,
}

impl Drop for RuleBuilder {
    fn drop(&mut self) {
        unsafe { rule_free(self.raw.cast()) };
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
    pub(crate) fn serial(&self) -> u64 {
        self.sheet.serial
    }

    pub(crate) fn layer_count(&self) -> u32 {
        // SAFETY: layer_names is null or the sheet's own array of layer names.
        unsafe { self.sheet.layer_names.as_ref() }.map_or(0, |names| names.len)
    }

    pub(crate) fn layer_register(&mut self, name: &[u8]) {
        self.sheet.layer_register(name);
    }

    pub(crate) fn add_import(&mut self, url: &[u8], layer: Option<&[u8]>, media: Option<&[u8]>) {
        if url.is_empty() {
            return;
        }
        if let Some(layer) = layer {
            self.layer_register(layer);
        }
        self.sheet.imports.push(Import {
            url: c_text(url),
            layer_name: layer.map(c_text),
            media: media.map(c_text),
        });
    }

    pub(crate) fn push_font_face(&mut self, face: FontFace) {
        self.sheet.font_faces.push(FontFaceData {
            family: c_text(&face.family),
            src_url: c_text(&face.src_url),
            unicode_range: face.unicode_range.as_deref().map(c_text),
            weight: face.weight,
            slant: face.slant,
        });
    }

    pub(crate) fn push_keyframes(&mut self, name: &[u8], stops: Vec<Stop>) {
        let mut stops: Vec<KeyframeStop> = stops
            .into_iter()
            .map(|stop| {
                KeyframeStop::new(
                    stop.pct,
                    stop.has_transform.then_some(stop.transform),
                    stop.raw_props.as_deref().map(|raw| until_nul(raw).to_vec()),
                )
            })
            .collect();
        sort_stops(&mut stops);
        self.sheet
            .keyframes
            .push(Keyframes::new(until_nul(name).to_vec(), stops));
    }

    pub(crate) fn push_property_rule(
        &mut self,
        name: &[u8],
        initial: Option<&[u8]>,
        syntax: SyntaxDef,
        inherits: bool,
        has_initial: bool,
    ) {
        self.sheet.property_rules.push(PropertyRule {
            name: c_text(name),
            initial_value: initial.map(c_text),
            syntax: syntax.0,
            inherits,
            has_initial,
        });
    }

    pub(crate) fn page_rule(&mut self) -> &mut PageRule {
        self.sheet.page_rule.get_or_insert_with(PageRule::default)
    }

    pub(crate) fn set_has_container_rules(&mut self) {
        self.sheet.has_container_rules = true;
    }

    pub(crate) fn set_has_hover_rules(&mut self) {
        self.sheet.has_hover_rules = true;
    }

    pub(crate) fn set_has_active_rules(&mut self) {
        self.sheet.has_active_rules = true;
    }

    pub(crate) fn rules_len(&self) -> usize {
        unsafe { (*self.sheet.rules).len as usize }
    }

    pub(crate) fn join_container_condition(&mut self, from: usize, canon: &[u8]) {
        let rules = unsafe { pointers::<RawRule>(self.sheet.rules) };
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
            (*raw).decls = new_array(size_of::<RawDecl>());
            (*raw).layer_name = opt_strdup(layer);
            (*raw).source_order = source_order;
            RuleBuilder { raw }
        }
    }

    pub(crate) fn push_rule(&mut self, rule: RuleBuilder) {
        let raw = rule.raw;
        core::mem::forget(rule);
        unsafe { glib::g_ptr_array_add(self.sheet.rules, raw.cast()) };
    }
}

unsafe extern "C" fn rule_free_notify(data: *mut c_void) {
    unsafe { rule_free(data) };
}

pub struct OwnedSheet(NonNull<RawSheet>);

impl OwnedSheet {
    pub fn parse(css: &[u8]) -> OwnedSheet {
        let serial = SERIAL_NEXT.fetch_add(1, Ordering::Relaxed);
        let has_container_units = nesting::has_container_units(css);
        let mut builder = SheetBuilder {
            sheet: Box::new(RawSheet::new(serial, has_container_units)),
        };
        let mut flattened = nesting::flatten(css);
        if let Some(nul) = flattened.iter().position(|&c| c == 0) {
            flattened.truncate(nul);
        }
        sheet::parse_into(&mut builder, &flattened);
        OwnedSheet(NonNull::from(Box::leak(builder.sheet)))
    }

    fn raw_mut(&mut self) -> &mut RawSheet {
        // SAFETY: the handle points to a live sheet, and borrowing the handle mutably keeps its other uses out.
        unsafe { self.0.as_mut() }
    }

    pub(super) fn as_raw(&self) -> *mut RawSheet {
        self.0.as_ptr()
    }

    pub(super) fn into_raw(self) -> *mut RawSheet {
        ManuallyDrop::new(self).0.as_ptr()
    }

    pub(super) fn set_cached(&self, cached: bool) {
        self.raw().cached.set(cached);
    }

    pub(super) fn share(&self) -> OwnedSheet {
        OwnedSheet(self.0)
    }

    pub fn resolve_urls(&mut self, base: &CStr) {
        let sheet = self.raw_mut();
        if sheet.resolved_base.as_deref() == Some(base.to_bytes()) {
            return;
        }
        sheet.resolved_base = Some(base.to_bytes().to_vec());
        // SAFETY: rules is the sheet's array of RawRule pointers.
        for &rule in unsafe { pointers::<RawRule>(sheet.rules) } {
            // SAFETY: a sheet's rules are null or live RawRules it owns, borrowed by nothing else here.
            if let Some(rule) = unsafe { rule.as_mut() } {
                rule_resolve_urls(rule, base);
            }
        }
        for face in &mut sheet.font_faces {
            let url = face.src_url.to_bytes();
            if let Some(resolved) = Some(url)
                .filter(|url| url_should_resolve(url))
                .and_then(|url| url_resolve(base, url))
            {
                face.src_url = c_text(&resolved);
            }
        }
    }

    pub(super) fn force_layer(&mut self, layer: &[u8]) {
        if layer.is_empty() {
            return;
        }
        let sheet = self.raw_mut();
        sheet.serial = SERIAL_NEXT.fetch_add(1, Ordering::Relaxed);
        let old_names = sheet.layer_names;
        let old_layers = sheet.layers;
        sheet.layer_names = ptr::null_mut();
        sheet.layers = ptr::null_mut();
        sheet.layer_register(layer);
        // SAFETY: old_names is null or the array of layer names the sheet owned until now.
        for &old in unsafe { pointers::<c_char>(old_names) } {
            // SAFETY: a layer name is a NUL-terminated string the array owns.
            let full = layer_join(Some(layer), unsafe { bytes(old) }.unwrap_or_default());
            sheet.layer_register(&full);
        }
        // SAFETY: rules is the sheet's array of RawRule pointers.
        for &rule in unsafe { pointers::<RawRule>(sheet.rules) } {
            // SAFETY: a sheet's rules are null or live RawRules it owns, borrowed by nothing else here.
            let Some(rule) = (unsafe { rule.as_mut() }) else {
                continue;
            };
            // SAFETY: layer_name is null or a string the rule owns.
            let full = match unsafe { bytes(rule.layer_name) } {
                Some(existing) => layer_join(Some(layer), existing),
                None => layer.to_vec(),
            };
            // SAFETY: layer_name is null or a GLib string the rule owns, replaced right after.
            unsafe { glib::g_free(rule.layer_name.cast()) };
            rule.layer_name = glib::strdup(&full);
        }
        let mut imports = core::mem::take(&mut sheet.imports);
        for import in &mut imports {
            let full = match &import.layer_name {
                Some(existing) => layer_join(Some(layer), existing.to_bytes()),
                None => layer.to_vec(),
            };
            import.layer_name = Some(c_text(&full));
            sheet.layer_register(&full);
        }
        sheet.imports = imports;
        if !old_layers.is_null() {
            // SAFETY: old_layers is the layer table the sheet owned until now, and nothing refers to it.
            unsafe { glib::g_hash_table_destroy(old_layers) };
        }
        if !old_names.is_null() {
            // SAFETY: old_names is the layer name array the sheet owned until now, and nothing refers to it.
            unsafe { glib::g_ptr_array_free(old_names, glib::TRUE) };
        }
    }
}

fn rule_resolve_urls(rule: &mut RawRule, base: &CStr) {
    // SAFETY: decls is the rule's array of RawDecl.
    for decl in unsafe { elements::<RawDecl>(rule.decls) } {
        // SAFETY: a declaration's value is null or a live value chain the rule owns.
        unsafe { value_resolve_url(decl.value, base) };
    }
    if !rule.vars.is_null() {
        let mut iter = glib::GHashTableIter::new();
        // SAFETY: vars is the rule's live table of custom property texts.
        unsafe { glib::g_hash_table_iter_init(&mut iter, rule.vars) };
        let (mut key, mut value) = (ptr::null_mut(), ptr::null_mut());
        // SAFETY: iter was initialized over vars, which is not changed but through the iterator.
        while unsafe { glib::g_hash_table_iter_next(&mut iter, &mut key, &mut value) } != 0 {
            // SAFETY: the table's values are NUL-terminated strings it owns.
            let text = unsafe { bytes(value.cast()) }.unwrap_or_default();
            if let Some(resolved) = raw_text_resolve_urls(text, base) {
                // SAFETY: iter points at a live entry, and the table frees the value it replaces.
                unsafe { g_hash_table_iter_replace(&mut iter, glib::strdup(&resolved).cast()) };
            }
        }
    }
    // SAFETY: pending is null or the rule's array of RawPending.
    for pending in unsafe { elements::<RawPending>(rule.pending) } {
        // SAFETY: raw_vtext is null or a string the pending declaration owns.
        let text = unsafe { bytes(pending.raw_vtext) }.unwrap_or_default();
        if let Some(resolved) = raw_text_resolve_urls(text, base) {
            // SAFETY: raw_vtext is a GLib string the pending declaration owns, replaced right after.
            unsafe { glib::g_free(pending.raw_vtext.cast()) };
            pending.raw_vtext = glib::strdup(&resolved);
        }
    }
}

impl Deref for OwnedSheet {
    type Target = Sheet;

    fn deref(&self) -> &Sheet {
        // SAFETY: the handle points to a live sheet, and Sheet is a transparent RawSheet.
        unsafe { self.0.cast::<Sheet>().as_ref() }
    }
}

impl Drop for OwnedSheet {
    fn drop(&mut self) {
        if self.raw().cached.get() {
            return;
        }
        // SAFETY: the handle came from Box::leak, and a sheet no cache holds has no other handle.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

fn url_should_resolve(url: &[u8]) -> bool {
    let local = url.get(..5).is_some_and(|scheme| {
        scheme.eq_ignore_ascii_case(b"data:") || scheme.eq_ignore_ascii_case(b"blob:")
    });
    !url.is_empty() && url[0] != b'#' && !local
}

fn url_resolve(base: &CStr, rel: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(Some(base.to_bytes()), rel)
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
        let abs = if url_should_resolve(rel) {
            url_resolve(base, rel)
        } else {
            None
        };
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
            if let Some(resolved) = unsafe { bytes(url) }
                .filter(|url| url_should_resolve(url))
                .and_then(|url| url_resolve(base, url))
            {
                unsafe {
                    glib::g_free(url.cast());
                    value.u.url = glib::strdup(&resolved);
                }
            }
        }
        v = value.next_layer;
    }
}

#[repr(transparent)]
pub struct Sheet(RawSheet);

pub struct FontFaceRule<'a> {
    pub family: Option<&'a CStr>,
    pub src_url: Option<&'a CStr>,
    pub unicode_range: Option<&'a CStr>,
    pub weight: c_int,
    pub slant: c_int,
}

pub struct ImportRule<'a> {
    pub url: Option<&'a CStr>,
    pub layer_name: Option<&'a CStr>,
    pub media: Option<&'a CStr>,
}

impl Sheet {
    /// # Safety
    ///
    /// `sheet` is null or points to a style sheet made by this crate that outlives `'a`.
    pub unsafe fn from_ptr<'a>(sheet: *const c_void) -> Option<&'a Sheet> {
        // SAFETY: the caller passes null or a live sheet, and Sheet is a transparent RawSheet.
        unsafe { sheet.cast::<Sheet>().as_ref() }
    }

    pub(super) fn raw(&self) -> &RawSheet {
        &self.0
    }

    pub(super) fn rules(&self) -> &[*mut RawRule] {
        // SAFETY: rules is null or the sheet's array of RawRule pointers, freed with the sheet.
        unsafe { pointers(self.0.rules) }
    }

    pub fn as_ptr(&self) -> *const c_void {
        ptr::from_ref(self).cast()
    }

    pub fn has_container_rules(&self) -> bool {
        self.0.has_container_rules
    }

    pub fn has_container_units(&self) -> bool {
        self.0.has_container_units
    }

    pub fn has_hover_rules(&self) -> bool {
        self.0.has_hover_rules
    }

    pub fn has_active_rules(&self) -> bool {
        self.0.has_active_rules
    }

    pub fn page_rule(&self) -> Option<&PageRule> {
        self.0.page_rule.as_ref()
    }

    pub fn font_faces(&self) -> impl Iterator<Item = FontFaceRule<'_>> {
        self.0.font_faces.iter().map(|face| FontFaceRule {
            family: Some(&face.family),
            src_url: Some(&face.src_url),
            unicode_range: face.unicode_range.as_deref(),
            weight: face.weight,
            slant: face.slant,
        })
    }

    pub fn declared_values(&self) -> impl Iterator<Item = (usize, &NsCssValue)> {
        self.rules()
            .iter()
            .filter_map(|&rule| {
                // SAFETY: a sheet's rules are null or live RawRules freed with the sheet.
                unsafe { rule.as_ref() }
            })
            .flat_map(|rule| {
                // SAFETY: decls is null or the rule's array of RawDecl.
                let decls: &[RawDecl] = unsafe { shared_elements(rule.decls) };
                decls.iter().filter_map(|decl| {
                    // SAFETY: a declaration's value is null or a live value the rule owns.
                    let value = unsafe { decl.value.as_ref() }?;
                    Some((decl.prop as usize, value))
                })
            })
    }

    pub fn imports(&self) -> impl Iterator<Item = ImportRule<'_>> {
        self.0.imports.iter().map(|import| ImportRule {
            url: Some(&import.url),
            layer_name: import.layer_name.as_deref(),
            media: import.media.as_deref(),
        })
    }

    pub fn keyframes(&self) -> impl Iterator<Item = Keyframes> + '_ {
        self.0.keyframes.iter().cloned()
    }
}

unsafe fn shared_elements<'a, T>(array: *const GArray) -> &'a [T] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.data.is_null() && a.len > 0 => unsafe {
            core::slice::from_raw_parts(a.data.cast::<T>(), a.len as usize)
        },
        _ => &[],
    }
}
