//! Southstar — a parsed style sheet's storage: the rules and at-rules the Rust parser builds as it reads a sheet, the owning handle that frees them unless a cache holds the sheet, resolving a sheet's URLs and moving it into a layer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, OnceCell};
use core::ffi::{CStr, c_int, c_uint, c_void};
use core::mem::{ManuallyDrop, size_of};
use core::ops::Deref;
use core::ptr::{self, NonNull};
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use southstar_css_prop_syntax as prop_syntax;
use southstar_glib::{self as glib, GBoolean};

use super::declarations::{Rule, RuleSink, lookup_key, until_nul};
use super::keyframes::{KeyframeStop, Keyframes};
use super::rule_index::RuleIndex;
use super::selector::SelectorGroup;
use super::value::{KIND_URL, NsCssValue, c_dup, c_free};
use crate::declarations;
use crate::nesting;
use crate::selector::{self, RuleSelectors};
use crate::sheet::{self, FontFace, ScopeText, Stop, layer_join};

pub(super) struct SheetData {
    pub(super) rules: Vec<Rule>,
    imports: Vec<Import>,
    pub(super) layer_names: Vec<Vec<u8>>,
    layers: HashSet<Vec<u8>>,
    font_faces: Vec<FontFaceData>,
    keyframes: Vec<Keyframes>,
    property_rules: Vec<Arc<PropertyRule>>,
    page_rule: Option<PageRule>,
    has_container_rules: bool,
    has_container_units: bool,
    has_hover_rules: bool,
    has_active_rules: bool,
    cached: Cell<bool>,
    pub(super) pseudo_mask: Cell<c_uint>,
    serial: u64,
    resolved_base: Option<Vec<u8>>,
    pub(super) index: OnceCell<RuleIndex>,
}

impl SheetData {
    fn new(serial: u64, has_container_units: bool) -> SheetData {
        SheetData {
            rules: Vec::new(),
            imports: Vec::new(),
            layer_names: Vec::new(),
            layers: HashSet::new(),
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
            index: OnceCell::new(),
        }
    }

    pub(super) fn serial(&self) -> u64 {
        self.serial
    }

    pub(super) fn property_rules(&self) -> &[Arc<PropertyRule>] {
        &self.property_rules
    }

    fn layer_register(&mut self, name: &[u8]) {
        if name.is_empty() || self.layers.contains(lookup_key(name)) {
            return;
        }
        let owned = until_nul(name).to_vec();
        self.layer_names.push(owned.clone());
        self.layers.insert(owned);
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

pub(super) struct Scope {
    pub(super) roots: SelectorGroup,
    pub(super) limits: Option<SelectorGroup>,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<PageRule>() == 72);

static SERIAL_NEXT: AtomicU64 = AtomicU64::new(1);

fn c_text(text: &[u8]) -> CString {
    CString::new(until_nul(text)).unwrap_or_default()
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
    sheet: Box<SheetData>,
}

pub(crate) struct RuleBuilder {
    rule: Rule,
}

impl RuleBuilder {
    pub(crate) fn apply_scopes(&mut self, scopes: &[ScopeText]) -> bool {
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
            self.rule.scopes.push(Scope {
                roots: SelectorGroup::new(&roots),
                limits: limits.as_deref().map(SelectorGroup::new),
            });
        }
        true
    }

    pub(crate) fn has_scopes(&self) -> bool {
        !self.rule.scopes.is_empty()
    }

    pub(crate) fn parse_selectors(&mut self, text: &[u8]) -> RuleSelectors {
        let parsed = selector::parse_rule_selectors(text);
        for sel in &parsed.selectors {
            self.rule.selectors.push(sel);
        }
        parsed
    }

    pub(crate) fn parse_declarations(&mut self, s: &[u8], p: usize) -> usize {
        if p >= s.len() {
            return p;
        }
        let mut sink = RuleSink::new(&mut self.rule);
        p + declarations::parse_block(&s[p..], 0, &mut sink)
    }
}

impl SheetBuilder {
    pub(crate) fn serial(&self) -> u64 {
        self.sheet.serial
    }

    pub(crate) fn layer_count(&self) -> u32 {
        self.sheet.layer_names.len() as u32
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
        self.sheet.property_rules.push(Arc::new(PropertyRule {
            name: c_text(name),
            initial_value: initial.map(c_text),
            syntax: syntax.0,
            inherits,
            has_initial,
        }));
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
        self.sheet.rules.len()
    }

    pub(crate) fn join_container_condition(&mut self, from: usize, canon: &[u8]) {
        for rule in self.sheet.rules.iter_mut().skip(from) {
            let joined = match &rule.container_condition {
                Some(existing) => [canon, b"\x1f", existing].concat(),
                None => canon.to_vec(),
            };
            rule.container_condition = Some(until_nul(&joined).to_vec());
        }
    }

    pub(crate) fn new_rule(&self, layer: Option<&[u8]>, source_order: i32) -> RuleBuilder {
        RuleBuilder {
            rule: Rule::new(layer, source_order),
        }
    }

    pub(crate) fn push_rule(&mut self, rule: RuleBuilder) {
        self.sheet.rules.push(rule.rule);
    }
}

pub struct OwnedSheet(NonNull<SheetData>);

impl OwnedSheet {
    pub fn parse(css: &[u8]) -> OwnedSheet {
        let serial = SERIAL_NEXT.fetch_add(1, Ordering::Relaxed);
        let has_container_units = nesting::has_container_units(css);
        let mut builder = SheetBuilder {
            sheet: Box::new(SheetData::new(serial, has_container_units)),
        };
        let mut flattened = nesting::flatten(css);
        if let Some(nul) = flattened.iter().position(|&c| c == 0) {
            flattened.truncate(nul);
        }
        sheet::parse_into(&mut builder, &flattened);
        OwnedSheet(NonNull::from(Box::leak(builder.sheet)))
    }

    fn raw_mut(&mut self) -> &mut SheetData {
        // SAFETY: the handle points to a live sheet, and borrowing the handle mutably keeps its other uses out.
        unsafe { self.0.as_mut() }
    }

    pub(super) fn into_raw(self) -> *mut SheetData {
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
        for rule in &mut sheet.rules {
            rule_resolve_urls(rule, base);
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
        let old_names = core::mem::take(&mut sheet.layer_names);
        sheet.layers.clear();
        sheet.layer_register(layer);
        for old in &old_names {
            let full = layer_join(Some(layer), old);
            sheet.layer_register(&full);
        }
        for rule in &mut sheet.rules {
            let full = match &rule.layer_name {
                Some(existing) => layer_join(Some(layer), existing),
                None => layer.to_vec(),
            };
            rule.layer_name = Some(until_nul(&full).to_vec());
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
    }
}

fn rule_resolve_urls(rule: &mut Rule, base: &CStr) {
    for decl in &mut rule.decls {
        // SAFETY: a declaration's value is a live value chain it holds a reference on, and the sheet is borrowed mutably.
        unsafe { value_resolve_url(decl.value_ptr(), base) };
    }
    if let Some(vars) = &mut rule.vars {
        vars.replace_texts(|text| raw_text_resolve_urls(text, base));
    }
    for pending in &mut rule.pending {
        if let Some(resolved) = raw_text_resolve_urls(&pending.raw_text, base) {
            pending.raw_text = until_nul(&resolved).to_vec();
        }
    }
}

impl Deref for OwnedSheet {
    type Target = Sheet;

    fn deref(&self) -> &Sheet {
        // SAFETY: the handle points to a live sheet, and Sheet is a transparent SheetData.
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

/// # Safety
///
/// `v` is null or a live value chain that nothing else borrows during the call.
unsafe fn value_resolve_url(mut v: *mut NsCssValue, base: &CStr) {
    // SAFETY: the caller passes null or a live value chain that nothing else borrows.
    while let Some(value) = unsafe { v.as_mut() } {
        if value.kind == KIND_URL {
            // SAFETY: a url value's payload is its url string.
            let url = unsafe { value.u.url };
            // SAFETY: url is null or a NUL-terminated string the value owns.
            if let Some(resolved) = unsafe { glib::bytes(url) }
                .filter(|url| url_should_resolve(url))
                .and_then(|url| url_resolve(base, url))
            {
                // SAFETY: url is the value's own GLib string, replaced right after.
                unsafe { c_free(url) };
                value.u.url = c_dup(&resolved);
            }
        }
        v = value.next_layer;
    }
}

#[repr(transparent)]
pub struct Sheet(SheetData);

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
        // SAFETY: the caller passes null or a live sheet, and Sheet is a transparent SheetData.
        unsafe { sheet.cast::<Sheet>().as_ref() }
    }

    pub(super) fn raw(&self) -> &SheetData {
        &self.0
    }

    pub(super) fn rules(&self) -> &[Rule] {
        &self.0.rules
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
        self.rules().iter().flat_map(|rule| {
            rule.decls
                .iter()
                .map(|decl| (decl.prop() as usize, decl.value()))
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
