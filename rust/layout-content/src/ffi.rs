//! Southstar — the generated-content and form-control entry points layout.c calls, and the layout.c, css, glib and pango calls they make back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{self, NonNull};

use southstar_dom::{Node, NsNode};
use southstar_glib::{GArray, GBoolean, GHashTable, boolean, g_array_append_vals, g_free, strdup};
use southstar_layout::{BoxRef, Edges, InlineAttr, NsBox, Style};
use southstar_paint::ffi as paint;
use southstar_paint::ffi::pango::{Context, Layout};
use southstar_style::{NsCssValue, StyleRef, StyleTable, ValueRef};

use crate::{controls, counters, pseudo, select, tags, text_input};

#[repr(C)]
pub struct GString {
    str_: *mut c_char,
    len: usize,
    _allocated_len: usize,
}

unsafe extern "C" {
    fn g_string_append_len(s: *mut GString, val: *const c_char, len: isize) -> *mut GString;
    fn g_utf8_strup(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_strdown(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_casefold(s: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_totitle(c: u32) -> u32;
    fn g_unichar_isspace(c: u32) -> GBoolean;
    fn g_unichar_to_utf8(c: u32, outbuf: *mut c_char) -> c_int;
    fn ns_layout_length_resolve(v: *const NsCssValue, basis: f64, fallback: f64) -> f64;
    fn ns_layout_edges_from_style(
        s: *const Style,
        basis: f64,
        margin: *mut Edges,
        padding: *mut Edges,
        border: *mut Edges,
    );
    fn ns_layout_peek_image(src: *const c_char) -> *mut c_void;
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
    fn ns_css_container_w() -> f64;
    fn ns_css_container_h() -> f64;
}

pub struct Out(NonNull<GString>);

impl Out {
    pub fn len(&self) -> usize {
        unsafe { self.0.as_ref() }.len
    }

    pub fn bytes(&self) -> &[u8] {
        let s = unsafe { self.0.as_ref() };
        if s.str_.is_null() || s.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(s.str_.cast::<u8>(), s.len) }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        unsafe {
            g_string_append_len(self.0.as_ptr(), bytes.as_ptr().cast(), bytes.len() as isize)
        };
    }
}

#[derive(Clone, Copy)]
pub struct Attrs(*mut GArray);

impl Attrs {
    pub fn push(self, attr: InlineAttr) {
        if self.0.is_null() {
            return;
        }
        unsafe { g_array_append_vals(self.0, ptr::from_ref(&attr).cast(), 1) };
    }
}

pub fn length_resolve(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    unsafe { ns_layout_length_resolve(v.map_or(ptr::null(), ValueRef::as_ptr), basis, fallback) }
}

pub fn edges_of(s: StyleRef<'_>, basis: f64) -> (Edges, Edges, Edges) {
    let (mut m, mut p, mut b) = (Edges::default(), Edges::default(), Edges::default());
    unsafe { ns_layout_edges_from_style(s.as_ptr(), basis, &mut m, &mut p, &mut b) };
    (m, p, b)
}

pub fn peek_image(src: &CStr) -> *mut c_void {
    unsafe { ns_layout_peek_image(src.as_ptr()) }
}

pub fn viewport() -> (f64, f64) {
    unsafe { (ns_css_viewport_w(), ns_css_viewport_h()) }
}

pub fn container() -> (f64, f64) {
    unsafe { (ns_css_container_w(), ns_css_container_h()) }
}

fn take_gstr(p: *mut c_char) -> Vec<u8> {
    if p.is_null() {
        return Vec::new();
    }
    let out = unsafe { CStr::from_ptr(p) }.to_bytes().to_vec();
    unsafe { g_free(p.cast()) };
    out
}

pub fn utf8_strup(s: &CStr) -> Vec<u8> {
    take_gstr(unsafe { g_utf8_strup(s.as_ptr(), -1) })
}

pub fn utf8_strdown(s: &CStr) -> Vec<u8> {
    take_gstr(unsafe { g_utf8_strdown(s.as_ptr(), -1) })
}

pub fn utf8_casefold(s: &[u8]) -> Vec<u8> {
    take_gstr(unsafe { g_utf8_casefold(s.as_ptr().cast(), s.len() as isize) })
}

pub fn utf8_get_char(s: &CStr, at: usize) -> u32 {
    let bytes = s.to_bytes_with_nul();
    if at >= bytes.len() {
        return 0;
    }
    unsafe { g_utf8_get_char(bytes.as_ptr().add(at).cast()) }
}

pub fn unichar_isspace(c: u32) -> bool {
    unsafe { g_unichar_isspace(c) != 0 }
}

pub fn unichar_title_utf8(c: u32) -> Vec<u8> {
    let mut buf = [0u8; 8];
    let n = unsafe { g_unichar_to_utf8(g_unichar_totitle(c), buf.as_mut_ptr().cast()) };
    buf[..n.clamp(0, 8) as usize].to_vec()
}

pub fn char_cell_px(s: *const Style) -> f64 {
    let layout = Layout::new(unsafe { Context::from_raw(paint::ns_paint_text_context()) });
    unsafe { paint::ns_paint_apply_inline_font(layout.raw(), s) };
    let ctx = layout.context();
    let fd = layout
        .font_description()
        .unwrap_or_else(|| ctx.font_description());
    if fd.raw().is_null() {
        return 0.0;
    }
    ctx.metrics(fd).map_or(0.0, |fm| {
        f64::from(fm.approximate_char_width()) / f64::from(paint::pango::SCALE)
    })
}

pub fn box_dom<'a>(b: BoxRef<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn box_style<'a>(b: BoxRef<'a>) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn style<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

fn value<'a>(v: *const NsCssValue) -> Option<ValueRef<'a>> {
    unsafe { ValueRef::from_ptr(v) }
}

fn out(s: *mut GString) -> Option<Out> {
    NonNull::new(s).map(Out)
}

fn styles(t: *mut GHashTable) -> StyleTable {
    unsafe { StyleTable::from_ptr(t) }
}

fn dup_or_null(bytes: Option<Vec<u8>>) -> *mut c_char {
    bytes.map_or(ptr::null_mut(), |b| strdup(&b))
}

fn ptr_or_null(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_append_inline_spacer(
    s: *mut GString,
    attrs: *mut GArray,
    width: f64,
) {
    if let Some(mut o) = out(s) {
        controls::append_inline_spacer(&mut o, Attrs(attrs), width);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_control_pad_spaces(s: *const Style, right: GBoolean) -> c_int {
    controls::pad_spaces(style(s), right != 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_text_input_leading_spaces(s: *const Style) -> c_int {
    controls::pad_spaces(style(s), false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_text_input_columns(n: *const NsNode) -> c_int {
    text_input::columns(node(n))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_text_input_align_offset(
    s: *const Style,
    n: *const NsNode,
    span: c_long,
) -> c_long {
    text_input::align_offset(style(s), node(n), span)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_text_input_align_skip_cps(
    s: *const Style,
    n: *const NsNode,
    cps: c_long,
    size: c_long,
) -> c_long {
    text_input::align_skip_cps(style(s), node(n), cps, size)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_text_input_focused_skip_cps(
    value: *const c_char,
    caret_byte: usize,
    cps: c_long,
    size: c_long,
) -> c_long {
    text_input::focused_skip_cps(c_str(value), caret_byte, cps, size)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_text_input_advance_cps(
    p: *const c_char,
    cps: c_long,
) -> *const c_char {
    match c_str(p) {
        Some(s) => unsafe { p.add(text_input::advance_cps(s.to_bytes(), cps)) },
        None => c"".as_ptr(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_input_columns_active() -> GBoolean {
    boolean(text_input::columns_active())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_input_columns_fit(root: *const NsBox) -> GBoolean {
    boolean(unsafe { BoxRef::from_ptr(root) }.is_some_and(text_input::fit_columns))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_input_columns_clear() {
    text_input::clear_columns();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_tag_is_bold(name: *const c_char) -> GBoolean {
    boolean(tags::is_bold(c_str(name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_tag_is_italic(name: *const c_char) -> GBoolean {
    boolean(tags::is_italic(c_str(name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_tag_is_monospace(name: *const c_char) -> GBoolean {
    boolean(tags::is_monospace(c_str(name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_tag_is_underline(name: *const c_char) -> GBoolean {
    boolean(tags::is_underline(c_str(name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_tag_is_strike(name: *const c_char) -> GBoolean {
    boolean(tags::is_strike(c_str(name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_tag_is_non_rendering(name: *const c_char) -> GBoolean {
    boolean(tags::is_non_rendering(c_str(name)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_node_is_non_rendering(n: *const NsNode) -> GBoolean {
    boolean(tags::node_is_non_rendering(node(n)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_attr(
    attrs: *mut GArray,
    kind: c_uint,
    start: usize,
    end: usize,
) {
    controls::emit_attr(Attrs(attrs), kind, start, end);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_attr_styled(
    attrs: *mut GArray,
    kind: c_uint,
    start: usize,
    end: usize,
    s: *const Style,
) {
    controls::emit_attr_styled(Attrs(attrs), kind, start, end, s);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_run_at_line_start(s: *const GString) -> GBoolean {
    boolean(out(s.cast_mut()).is_none_or(|o| controls::run_at_line_start(o.bytes())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_control_css_extra_w(dom: *const NsNode, s: *const Style) -> f64 {
    controls::css_extra(node(dom), style(s), false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_control_css_extra_h(dom: *const NsNode, s: *const Style) -> f64 {
    controls::css_extra(node(dom), style(s), true)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_attr_control_width(
    r: *const InlineAttr,
    b: *const NsBox,
) -> f64 {
    let Some(r) = (unsafe { r.as_ref() }) else {
        return 0.0;
    };
    controls::inline_attr_control_width(r, style(r.style()), node(r.dom_ptr().cast()), unsafe {
        BoxRef::from_ptr(b)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_form_attr_sized(
    attrs: *mut GArray,
    kind: c_uint,
    start: usize,
    end: usize,
    dom: *const NsNode,
    table: *mut GHashTable,
) {
    controls::emit_form_attr_sized(Attrs(attrs), kind, start, end, node(dom), styles(table));
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_layout_emit_control_text_style(
    attrs: *mut GArray,
    s: *const Style,
    field_start: usize,
    field_end: usize,
    val_start: usize,
    val_end: usize,
    is_placeholder: GBoolean,
    disabled: GBoolean,
) {
    controls::emit_control_text_style(
        Attrs(attrs),
        style(s),
        (field_start, field_end),
        (val_start, val_end),
        is_placeholder != 0,
        disabled != 0,
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_font_size_attr(
    attrs: *mut GArray,
    start: usize,
    end: usize,
    font_size_px: f64,
) {
    controls::emit_font_size(Attrs(attrs), start, end, font_size_px);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_font_weight_attr(
    attrs: *mut GArray,
    start: usize,
    end: usize,
    font_weight: c_int,
) {
    controls::emit_font_weight(Attrs(attrs), start, end, font_weight);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_font_stretch_attr(
    attrs: *mut GArray,
    start: usize,
    end: usize,
    font_stretch: c_int,
) {
    controls::emit_font_stretch(Attrs(attrs), start, end, font_stretch);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_font_features_attr(
    attrs: *mut GArray,
    start: usize,
    end: usize,
    font_kerning: c_int,
    font_ligatures: *const c_char,
    font_features: *const c_char,
) {
    controls::emit_font_features(
        Attrs(attrs),
        start,
        end,
        font_kerning,
        font_ligatures,
        font_features,
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_font_variations_attr(
    attrs: *mut GArray,
    start: usize,
    end: usize,
    font_variations: *const c_char,
) {
    controls::emit_font_variations(Attrs(attrs), start, end, font_variations);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_layout_emit_color_attr(
    attrs: *mut GArray,
    start: usize,
    end: usize,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    controls::emit_color(Attrs(attrs), start, end, [r, g, b, a]);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_font_family_attr(
    attrs: *mut GArray,
    start: usize,
    end: usize,
    family: *const c_char,
) {
    controls::emit_font_family(Attrs(attrs), start, end, family);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_font_kerning_from_style(s: *const Style) -> c_int {
    controls::font_kerning(style(s))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_font_ligatures_from_style(s: *const Style) -> *const c_char {
    ptr_or_null(controls::font_keyword(
        style(s),
        controls::FontKeyword::Ligatures,
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_font_feature_settings_from_style(
    s: *const Style,
) -> *const c_char {
    ptr_or_null(controls::font_keyword(
        style(s),
        controls::FontKeyword::Features,
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_font_variation_settings_from_style(
    s: *const Style,
) -> *const c_char {
    ptr_or_null(controls::font_keyword(
        style(s),
        controls::FontKeyword::Variations,
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_counters_build(root: *const NsNode, table: *mut GHashTable) {
    counters::build(node(root), styles(table));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_layout_counters_clear() {
    counters::clear();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_resolve_pseudo_content(
    raw: *const c_char,
    host: *const NsNode,
) -> *mut c_char {
    dup_or_null(c_str(raw).and_then(|raw| pseudo::resolve_content(raw.to_bytes(), node(host))))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_pseudo_content_text(
    content: *const NsCssValue,
    host: *const NsNode,
) -> *mut c_char {
    dup_or_null(
        value(content)
            .and_then(ValueRef::keyword_text)
            .and_then(|raw| pseudo::resolve_content(raw.to_bytes(), node(host))),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_append_pseudo_content(
    s: *mut GString,
    content: *const NsCssValue,
    host: *const NsNode,
    host_style: *const Style,
) {
    if let Some(mut o) = out(s) {
        pseudo::append_content(&mut o, value(content), node(host), style(host_style));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_pseudo_generates_box(ps: *const Style) -> GBoolean {
    boolean(pseudo::generates_box(style(ps)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_quotes_string_for(
    s: *const Style,
    depth: c_int,
    closing: GBoolean,
) -> *mut c_char {
    strdup(&pseudo::quotes_string(style(s), depth, closing != 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_apply_text_transform(
    src: *const c_char,
    tt: *const c_char,
) -> *mut c_char {
    match (c_str(src), c_str(tt)) {
        (Some(src), Some(tt)) => dup_or_null(pseudo::text_transform(src, tt.to_bytes())),
        _ => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_progress_fraction(n: *const NsNode) -> f64 {
    controls::progress_fraction(node(n))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_meter_fraction(n: *const NsNode, quality: *mut c_int) -> f64 {
    let (frac, q) = controls::meter_state(node(n));
    if let Some(quality) = unsafe { quality.as_mut() } {
        *quality = q;
    }
    frac
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_open_select_option(
    s: *mut GString,
    attrs: *mut GArray,
    table: *mut GHashTable,
    option: *const NsNode,
) {
    if let (Some(mut o), Some(option)) = (out(s), node(option)) {
        select::emit_option(&mut o, Attrs(attrs), styles(table), option, true);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_listbox_option(
    s: *mut GString,
    attrs: *mut GArray,
    table: *mut GHashTable,
    option: *const NsNode,
    first: GBoolean,
) {
    if let (Some(mut o), Some(option)) = (out(s), node(option)) {
        select::emit_option(&mut o, Attrs(attrs), styles(table), option, first == 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_emit_datalist_suggestions(
    s: *mut GString,
    attrs: *mut GArray,
    table: *mut GHashTable,
    input: *const NsNode,
) {
    if let (Some(mut o), Some(input)) = (out(s), node(input)) {
        select::emit_datalist_suggestions(&mut o, Attrs(attrs), styles(table), input);
    }
}
