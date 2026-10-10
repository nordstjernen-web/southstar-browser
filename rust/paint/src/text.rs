//! Southstar — page text setup: the shared Pango context, fonts from computed styles, line heights, OpenType features and variations, language and direction, alignment, and the font oracle the style engine measures with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::CStr;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_layout::{BoxRef, inline_kind};
use southstar_style::{Kind, PropId as P, StyleRef, ValueRef};

use crate::ffi::engine::{self, FontMetrics};
use crate::ffi::pango::{
    self, AttrList, Attribute, Context, FontDescription, FontMap, Layout, TabArray,
};
use crate::util::{
    UNIT_CAP, UNIT_CH, UNIT_CQH, UNIT_CQMAX, UNIT_CQMIN, UNIT_CQW, UNIT_EM, UNIT_EX, UNIT_IC,
    UNIT_LH, UNIT_NUMBER, UNIT_PERCENT, UNIT_PX, UNIT_RCAP, UNIT_RCH, UNIT_REM, UNIT_REX, UNIT_RIC,
    UNIT_RLH, UNIT_VH, UNIT_VMAX, UNIT_VMIN, UNIT_VW, cmax, cmin, get, inherited_style, keyword,
    keyword_is, length_or, skip_spaces, style_keyword, style_of,
};

pub const CSS_LINE_HEIGHT_KEY: &CStr = c"ns-css-line-height";

type FamilyCache = Option<(u32, HashSet<Vec<u8>>)>;
type MetricsCache = (u32, HashMap<MetricsKey, FontMetrics>);

thread_local! {
    static TEXT_CONTEXT: Context = Context::new_text_context();
    static FONT_FAMILIES: RefCell<FamilyCache> = RefCell::new(empty_family_cache());
    static FONT_METRICS: RefCell<MetricsCache> = RefCell::new((0, HashMap::new()));
}

fn empty_family_cache() -> FamilyCache {
    None
}

#[derive(PartialEq, Eq, Hash)]
struct MetricsKey {
    family: Option<Vec<u8>>,
    size_bits: u64,
    weight: i32,
    italic: bool,
}

pub fn context() -> Context {
    TEXT_CONTEXT.with(|c| *c)
}

pub fn create_layout() -> Layout {
    Layout::new(context())
}

pub fn weight_from_css(weight: i32) -> i32 {
    match weight {
        w if w <= 100 => pango::WEIGHT_THIN,
        w if w <= 200 => pango::WEIGHT_ULTRALIGHT,
        w if w <= 300 => pango::WEIGHT_LIGHT,
        w if w <= 400 => pango::WEIGHT_NORMAL,
        w if w <= 500 => pango::WEIGHT_MEDIUM,
        w if w <= 600 => pango::WEIGHT_SEMIBOLD,
        w if w <= 700 => pango::WEIGHT_BOLD,
        w if w <= 800 => pango::WEIGHT_ULTRABOLD,
        w if w <= 900 => pango::WEIGHT_HEAVY,
        w => w,
    }
}

pub fn pango_font_size(size_px: f64) -> i32 {
    if size_px.is_nan() || size_px <= 0.0 {
        return 0;
    }
    let size_px = if size_px > 65535.0 { 65535.0 } else { size_px };
    (size_px * pango::SCALE_F) as i32
}

pub fn stretch_from_css(rank: i32) -> i32 {
    rank.clamp(0, 8)
}

pub fn wrap_mode_for(style: Option<StyleRef<'_>>) -> i32 {
    if let Some(s) = style {
        match keyword(s.get(P::WordBreak)).map(CStr::to_bytes) {
            Some(b"break-all") => return pango::WRAP_CHAR,
            Some(b"keep-all") => return pango::WRAP_WORD,
            _ => {}
        }
        match keyword(s.get(P::OverflowWrap)).map(CStr::to_bytes) {
            Some(b"normal") => return pango::WRAP_WORD,
            Some(b"break-word" | b"anywhere") => return pango::WRAP_WORD_CHAR,
            _ => {}
        }
    }
    pango::WRAP_WORD
}

pub fn is_nowrap(style: Option<StyleRef<'_>>) -> bool {
    matches!(
        keyword(get(style, P::WhiteSpace)).map(CStr::to_bytes),
        Some(b"nowrap" | b"pre")
    )
}

fn style_is_italic(s: Option<StyleRef<'_>>) -> bool {
    let fs = get(s, P::FontStyle);
    keyword_is(fs, c"italic") || keyword_is(fs, c"oblique")
}

fn normal_line_height_from_metrics(s: Option<StyleRef<'_>>, family: &CStr, font_size: f64) -> f64 {
    let italic = style_is_italic(s);
    let weight = engine::font_weight_number(get(s, P::FontWeight), 400);
    let mut m = FontMetrics::default();
    font_metrics(Some(family), font_size, weight, italic, &mut m);
    m.line_px
}

fn normal_line_height_fallback(family: Option<&CStr>, font_size: f64) -> f64 {
    const KNOWN: [(&[u8], f64, bool); 7] = [
        (b"Arial", 1.1, false),
        (b"Helvetica", 1.1, false),
        (b"Times New Roman", 1.125, false),
        (b"Times", 1.125, false),
        (b"serif", 1.125, false),
        (b"Menlo", 1.164, true),
        (b"System Font", 1.19, true),
    ];
    let resolved = family.map(|f| engine::font_family_for_pango(Some(f)));
    let mut factor = 1.2;
    let mut rounded = false;
    if let Some(resolved) = &resolved {
        if let Some(&(_, f, r)) = KNOWN
            .iter()
            .find(|(name, _, _)| resolved.to_bytes().eq_ignore_ascii_case(name))
        {
            factor = f;
            rounded = r;
        }
    }
    if rounded {
        (font_size * factor).round()
    } else {
        (font_size * factor).ceil()
    }
}

fn family_keyword(s: Option<StyleRef<'_>>) -> Option<&CStr> {
    get(s, P::FontFamily)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
}

pub fn normal_line_height_px(s: Option<StyleRef<'_>>) -> f64 {
    let font_size = length_or(get(s, P::FontSize), 16.0);
    let family = family_keyword(s);
    if font_size > 0.0 {
        let line = normal_line_height_from_metrics(s, family.unwrap_or(c"sans-serif"), font_size);
        if line > 0.0 {
            return line;
        }
    }
    normal_line_height_fallback(family, font_size)
}

pub fn css_line_height_px(s: Option<StyleRef<'_>>) -> f64 {
    let Some(st) = s else {
        return -1.0;
    };
    let lh = st.get(P::LineHeight);
    let font_size = length_or(st.get(P::FontSize), 16.0);
    if lh.is_none() || keyword_is(lh, c"normal") {
        return normal_line_height_px(s);
    }
    let Some((v, unit)) = lh.and_then(ValueRef::length) else {
        return -1.0;
    };
    match unit {
        UNIT_PX => v,
        UNIT_NUMBER | UNIT_EM => v * font_size,
        UNIT_PERCENT => v / 100.0 * font_size,
        UNIT_REM => v * 16.0,
        UNIT_LH => v * font_size * 1.5,
        UNIT_RLH => v * 24.0,
        UNIT_EX => v * font_size * 0.5,
        UNIT_REX => v * 8.0,
        UNIT_CH => v * font_size * 0.5,
        UNIT_RCH => v * 8.0,
        UNIT_CAP => v * font_size * 0.7,
        UNIT_RCAP => v * 11.2,
        UNIT_IC => v * font_size,
        UNIT_RIC => v * 16.0,
        UNIT_VH | UNIT_CQH => v * engine::viewport_h() / 100.0,
        UNIT_VW | UNIT_CQW => v * engine::viewport_w() / 100.0,
        UNIT_VMIN | UNIT_CQMIN => {
            let m = cmin(engine::viewport_w(), engine::viewport_h());
            v * m / 100.0
        }
        UNIT_VMAX | UNIT_CQMAX => {
            let m = cmax(engine::viewport_w(), engine::viewport_h());
            v * m / 100.0
        }
        _ => -1.0,
    }
}

pub fn apply_css_line_spacing(layout: Option<&Layout>, s: Option<StyleRef<'_>>) {
    let lh_px = css_line_height_px(s);
    let Some(layout) = layout else {
        return;
    };
    if lh_px <= 0.0 {
        return;
    }
    layout.set_css_line_height(CSS_LINE_HEIGHT_KEY, lh_px);
}

fn nearest_node_attr<'a>(n: Node<'a>, attr: &CStr) -> Option<&'a CStr> {
    let mut p = Some(n);
    while let Some(node) = p {
        if node.is_element() {
            if let Some(v) = node.attr(attr).filter(|v| !v.is_empty()) {
                return Some(v);
            }
        }
        p = node.parent();
    }
    None
}

fn box_node(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn apply_i18n(layout: Option<&Layout>, attrs: Option<&AttrList>, b: Option<BoxRef<'_>>) {
    let Some(b) = b else {
        return;
    };
    let mut dn = box_node(b);
    let mut st = style_of(b);
    let mut p = b.parent();
    while let Some(parent) = p {
        if dn.is_some() && st.is_some() {
            break;
        }
        if dn.is_none() {
            dn = box_node(parent);
        }
        if st.is_none() {
            st = style_of(parent);
        }
        p = parent.parent();
    }
    if dn.is_none() && st.is_none() {
        return;
    }
    if let Some(attrs) = attrs {
        let auto_hyphens = keyword_is(get(st, P::Hyphens), c"auto");
        attrs.insert(Attribute::insert_hyphens(auto_hyphens), 0, u32::MAX);
    }
    let mut lang = dn.and_then(|n| nearest_node_attr(n, c"lang"));
    if lang.is_none() {
        lang = dn.and_then(|n| nearest_node_attr(n, c"xml:lang"));
    }
    if let (Some(lang), Some(attrs)) = (lang, attrs) {
        attrs.insert(Attribute::language(lang), 0, u32::MAX);
    }
    let dir = dn.and_then(|n| nearest_node_attr(n, c"dir"));
    let mut bd = pango::DIRECTION_NEUTRAL;
    if let Some(dir) = dir {
        if dir.to_bytes().eq_ignore_ascii_case(b"rtl") {
            bd = pango::DIRECTION_RTL;
        } else if dir.to_bytes().eq_ignore_ascii_case(b"ltr") {
            bd = pango::DIRECTION_LTR;
        }
    }
    if bd == pango::DIRECTION_NEUTRAL && keyword_is(get(st, P::Direction), c"rtl") {
        bd = pango::DIRECTION_RTL;
    }
    if bd != pango::DIRECTION_NEUTRAL {
        if let Some(layout) = layout {
            layout.set_auto_dir(false);
            layout.context().set_base_dir(bd);
        }
    }
}

fn append_feature(out: &mut Vec<u8>, feature: &[u8]) {
    if feature.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.extend_from_slice(b", ");
    }
    out.extend_from_slice(feature);
}

fn append_kerning_features(out: &mut Vec<u8>, v: Option<ValueRef<'_>>) {
    match keyword(v).map(CStr::to_bytes) {
        Some(b"none") => append_feature(out, b"kern=0"),
        Some(b"normal" | b"auto") => append_feature(out, b"kern=1"),
        _ => {}
    }
}

fn append_ligature_token(out: &mut Vec<u8>, token: &[u8]) {
    let features: &[&[u8]] = match token {
        b"none" => &[b"liga=0", b"clig=0", b"dlig=0", b"hlig=0", b"calt=0"],
        b"normal" => &[b"liga=1", b"clig=1", b"dlig=0", b"hlig=0", b"calt=1"],
        b"common-ligatures" => &[b"liga=1", b"clig=1"],
        b"no-common-ligatures" => &[b"liga=0", b"clig=0"],
        b"discretionary-ligatures" => &[b"dlig=1"],
        b"no-discretionary-ligatures" => &[b"dlig=0"],
        b"historical-ligatures" => &[b"hlig=1"],
        b"no-historical-ligatures" => &[b"hlig=0"],
        b"contextual" => &[b"calt=1"],
        b"no-contextual" => &[b"calt=0"],
        _ => &[],
    };
    for f in features {
        append_feature(out, f);
    }
}

fn append_ligature_features(out: &mut Vec<u8>, ligatures: Option<&CStr>) {
    let Some(ligatures) = ligatures else {
        return;
    };
    for token in ligatures
        .to_bytes()
        .split(|c| matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0c))
    {
        if !token.is_empty() {
            append_ligature_token(out, token);
        }
    }
}

fn read_tag(text: &[u8], pos: &mut usize) -> Option<[u8; 4]> {
    let mut p = skip_spaces(text, *pos);
    let quote = *text.get(p)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    p += 1;
    let s = p;
    while p < text.len() && text[p] != quote {
        p += 1;
    }
    if p >= text.len() || p - s != 4 {
        return None;
    }
    *pos = p + 1;
    Some([text[s], text[s + 1], text[s + 2], text[s + 3]])
}

fn strtoll_digits(text: &[u8]) -> (i64, usize) {
    let mut v: i64 = 0;
    let mut n = 0;
    while n < text.len() && text[n].is_ascii_digit() {
        v = v
            .saturating_mul(10)
            .saturating_add(i64::from(text[n] - b'0'));
        n += 1;
    }
    (v, n)
}

fn read_feature_value(text: &[u8], pos: &mut usize) -> Option<i32> {
    let mut p = skip_spaces(text, *pos);
    match text.get(p) {
        None | Some(b',') => {
            *pos = p;
            return Some(1);
        }
        Some(c) if c.is_ascii_alphabetic() => {
            let s = p;
            while p < text.len() && (text[p].is_ascii_alphabetic() || text[p] == b'-') {
                p += 1;
            }
            let out = match text[s..p].to_ascii_lowercase().as_slice() {
                b"on" => 1,
                b"off" => 0,
                _ => return None,
            };
            *pos = skip_spaces(text, p);
            return Some(out);
        }
        Some(c) if c.is_ascii_digit() => {}
        Some(_) => return None,
    }
    let (v, n) = strtoll_digits(&text[p..]);
    if n == 0 {
        return None;
    }
    let v = v.clamp(0, i64::from(i32::MAX)) as i32;
    *pos = skip_spaces(text, p + n);
    Some(v)
}

fn append_feature_settings(out: &mut Vec<u8>, settings: Option<&CStr>) {
    let Some(settings) = settings.map(CStr::to_bytes) else {
        return;
    };
    if settings.is_empty() || settings == b"normal" {
        return;
    }
    let mut p = skip_spaces(settings, 0);
    while p < settings.len() {
        let Some(tag) = read_tag(settings, &mut p) else {
            return;
        };
        let Some(value) = read_feature_value(settings, &mut p) else {
            return;
        };
        let mut feature = tag.to_vec();
        feature.extend_from_slice(format!("={value}").as_bytes());
        append_feature(out, &feature);
        if settings.get(p) != Some(&b',') {
            break;
        }
        p = skip_spaces(settings, p + 1);
    }
}

fn read_variation_value(text: &[u8], pos: &mut usize) -> Option<Vec<u8>> {
    let p = skip_spaces(text, *pos);
    match text.get(p) {
        None | Some(b',') => return None,
        _ => {}
    }
    let (v, n) = southstar_glib::ascii_strtod_prefix(&text[p..]);
    if n == 0 || !v.is_finite() {
        return None;
    }
    *pos = skip_spaces(text, p + n);
    Some(engine::format_g8(v))
}

pub fn variations_from_css(settings: Option<&CStr>) -> Option<Vec<u8>> {
    let settings = settings?.to_bytes();
    if settings.is_empty() {
        return None;
    }
    if settings == b"normal" {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    let mut p = skip_spaces(settings, 0);
    while p < settings.len() {
        let tag = read_tag(settings, &mut p)?;
        let value = read_variation_value(settings, &mut p)?;
        if !out.is_empty() {
            out.push(b',');
        }
        out.extend_from_slice(&tag);
        out.push(b'=');
        out.extend_from_slice(&value);
        if settings.get(p) != Some(&b',') {
            break;
        }
        p = skip_spaces(settings, p + 1);
    }
    (!out.is_empty()).then_some(out)
}

pub fn font_features_attr(
    kerning: i32,
    ligatures: Option<&CStr>,
    settings: Option<&CStr>,
) -> Option<Attribute> {
    let mut s = Vec::new();
    match kerning.cmp(&0) {
        core::cmp::Ordering::Equal => append_feature(&mut s, b"kern=0"),
        core::cmp::Ordering::Greater => append_feature(&mut s, b"kern=1"),
        core::cmp::Ordering::Less => {}
    }
    append_ligature_features(&mut s, ligatures);
    append_feature_settings(&mut s, settings);
    if s.is_empty() {
        return None;
    }
    let features = CString::new(s).ok()?;
    Some(Attribute::font_features(&features))
}

pub fn font_variations_attr(settings: Option<&CStr>) -> Option<Attribute> {
    let variations = CString::new(variations_from_css(settings)?).ok()?;
    let desc = FontDescription::new();
    desc.set_variations(&variations);
    Some(Attribute::font_desc(&desc))
}

pub fn apply_font_features(attrs: &AttrList, s: StyleRef<'_>, start: u32, end: u32) {
    let mut features = Vec::new();
    append_kerning_features(&mut features, s.get(P::FontKerning));
    if let Some(lig) = s
        .get(P::FontVariantLigatures)
        .filter(|v| v.kind() == Kind::Keyword)
    {
        append_ligature_features(&mut features, lig.keyword_text());
    }
    if let Some(settings) = s
        .get(P::FontFeatureSettings)
        .filter(|v| v.kind() == Kind::Keyword)
    {
        append_feature_settings(&mut features, settings.keyword_text());
    }
    if features.is_empty() {
        return;
    }
    let Ok(text) = CString::new(features) else {
        return;
    };
    attrs.insert(Attribute::font_features(&text), start, end);
}

pub fn font_available(family: Option<&CStr>) -> bool {
    let Some(family) = family.filter(|f| !f.is_empty()) else {
        return true;
    };
    let map = FontMap::default_map();
    let serial = map.map_or(0, FontMap::serial);
    let lower = family.to_bytes().to_ascii_lowercase();
    let has = FONT_FAMILIES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let stale = cache.as_ref().is_none_or(|(s, _)| *s != serial);
        if stale {
            let mut names = HashSet::new();
            if let Some(map) = map {
                for name in map.family_names() {
                    names.insert(name.to_ascii_lowercase());
                }
            }
            *cache = Some((serial, names));
        }
        cache
            .as_ref()
            .is_some_and(|(_, names)| names.contains(&lower))
    });
    has || engine::font_family_loaded(family)
}

fn font_has_legacy_mac_ascent(family: Option<&CStr>) -> bool {
    if cfg!(target_os = "macos") {
        family.is_some_and(|f| {
            let f = f.to_bytes();
            f.eq_ignore_ascii_case(b"Times")
                || f.eq_ignore_ascii_case(b"Helvetica")
                || f.eq_ignore_ascii_case(b"Courier")
        })
    } else {
        false
    }
}

fn font_vertical_metrics(
    ctx: Context,
    fd: &FontDescription,
    family: Option<&CStr>,
    out: &mut FontMetrics,
) {
    let Some(fm) = ctx.metrics(fd.as_ref()) else {
        return;
    };
    let raw_ascent = f64::from(fm.ascent()) / pango::SCALE_F;
    let raw_descent = f64::from(fm.descent()) / pango::SCALE_F;
    let height = f64::from(fm.height()) / pango::SCALE_F;
    drop(fm);
    let mut ascent = raw_ascent.round();
    let descent = raw_descent.round();
    let gap = if height > raw_ascent + raw_descent {
        (height - raw_ascent - raw_descent).round()
    } else {
        0.0
    };
    if font_has_legacy_mac_ascent(family) {
        ascent += ((ascent + descent) * 0.15 + 0.5).floor();
    }
    let line = ascent + descent + gap;
    if line.is_nan() || line <= 0.0 {
        return;
    }
    out.line_px = line;
    out.ascent_px = ascent;
    out.descent_px = descent;
}

fn font_metrics_measure(
    family: Option<&CStr>,
    size_px: f64,
    weight: i32,
    italic: bool,
    out: &mut FontMetrics,
) {
    let l = create_layout();
    let fd = FontDescription::new();
    let pango_family = family.map(|f| engine::font_family_for_pango(Some(f)));
    if let Some(pf) = pango_family.as_deref().filter(|f| !f.is_empty()) {
        fd.set_family(pf);
    }
    if weight > 0 {
        fd.set_weight(weight);
    }
    if italic {
        fd.set_style(pango::STYLE_ITALIC);
    }
    fd.set_absolute_size(f64::from(pango_font_size(size_px)));
    l.set_font_description(fd.as_ref());
    font_vertical_metrics(l.context(), &fd, pango_family.as_deref(), out);
    drop(pango_family);

    l.set_text(c"x");
    let ink = l.pixel_ink_extents();
    if ink.height > 0 {
        out.ex_px = f64::from(ink.height);
    }
    l.set_text(c"H");
    let ink = l.pixel_ink_extents();
    if ink.height > 0 {
        out.cap_px = f64::from(ink.height);
    }
    l.set_text(c"0");
    let (w, _) = l.pixel_size();
    if w > 0 {
        out.ch_px = f64::from(w);
    }
    l.set_text(c"\xe6\xb0\xb4");
    let (w, _) = l.pixel_size();
    if w > 0 {
        out.ic_px = f64::from(w);
    }
}

pub fn font_metrics(
    family: Option<&CStr>,
    size_px: f64,
    weight: i32,
    italic: bool,
    out: &mut FontMetrics,
) {
    if size_px <= 0.0 {
        return;
    }
    let serial = pango::font_map_serial();
    let key = MetricsKey {
        family: family.map(|f| f.to_bytes().to_vec()),
        size_bits: size_px.to_bits(),
        weight,
        italic,
    };
    let hit = FONT_METRICS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.0 != serial {
            cache.1.clear();
            cache.0 = serial;
        }
        cache.1.get(&key).copied()
    });
    if let Some(hit) = hit {
        *out = hit;
        return;
    }
    let mut m = FontMetrics {
        ex_px: size_px * 0.5,
        ch_px: size_px * 0.5,
        cap_px: size_px * 0.7,
        ic_px: size_px,
        ..FontMetrics::default()
    };
    font_metrics_measure(family, size_px, weight, italic, &mut m);
    FONT_METRICS.with(|cache| cache.borrow_mut().1.insert(key, m));
    *out = m;
}

pub fn font_generation() -> u64 {
    let serial = pango::font_map_serial();
    (u64::from(serial) << 32) | u64::from(engine::font_generation())
}

pub fn apply_inline_font(layout: &Layout, s: Option<StyleRef<'_>>) {
    let desc = FontDescription::new();
    let font_size = length_or(get(s, P::FontSize), 16.0);
    let fam = get(s, P::FontFamily);
    let family = match fam {
        Some(v) if v.kind() == Kind::Keyword => v.keyword_text(),
        _ => Some(c"sans-serif"),
    };
    let pango_family = engine::font_family_for_pango(family);
    desc.set_family(&pango_family);
    drop(pango_family);
    desc.set_absolute_size(f64::from(pango_font_size(font_size)));
    let font_weight = engine::font_weight_number(get(s, P::FontWeight), -1);
    if font_weight > 0 {
        desc.set_weight(weight_from_css(font_weight));
    }
    desc.set_stretch(stretch_from_css(engine::font_stretch_rank(get(
        s,
        P::FontStretch,
    ))));
    if keyword_is(get(s, P::FontStyle), c"italic") {
        desc.set_style(pango::STYLE_ITALIC);
    } else if keyword_is(get(s, P::FontStyle), c"oblique") {
        desc.set_style(pango::STYLE_OBLIQUE);
    }
    if let Some(variations) = variations_from_css(style_keyword(s, P::FontVariationSettings))
        .and_then(|v| CString::new(v).ok())
    {
        desc.set_variations(&variations);
    }
    layout.set_font_description(desc.as_ref());
    drop(desc);

    let Some((tab_v, tab_unit)) = get(s, P::TabSize).and_then(ValueRef::length) else {
        return;
    };
    if tab_v.is_nan() || tab_v <= 0.0 {
        return;
    }
    let tab_w = if tab_unit == UNIT_NUMBER {
        let probe = Layout::new(layout.context());
        if let Some(desc) = layout.font_description() {
            probe.set_font_description(desc);
        }
        probe.set_text_bytes(b" ");
        let (sw, _) = probe.size();
        drop(probe);
        tab_v * (f64::from(sw) / pango::SCALE_F)
    } else {
        tab_v
    };
    if tab_w > 0.0 {
        let n = 32;
        let tabs = TabArray::new(n, true);
        for i in 0..n {
            tabs.set_tab(i, pango::TAB_LEFT, (f64::from(i + 1) * tab_w + 0.5) as i32);
        }
        layout.set_tabs(&tabs);
    }
}

pub fn apply_text_align(layout: &Layout, s: Option<StyleRef<'_>>) {
    let ta = get(s, P::TextAlign);
    let rtl = layout.context().base_dir() == pango::DIRECTION_RTL;
    if keyword_is(ta, c"center") {
        layout.set_alignment(pango::ALIGN_CENTER);
    } else if keyword_is(ta, c"right")
        || (keyword_is(ta, c"end") && !rtl)
        || (keyword_is(ta, c"start") && rtl)
        || (ta.is_none() && rtl)
    {
        layout.set_alignment(pango::ALIGN_RIGHT);
    } else if keyword_is(ta, c"justify") {
        layout.set_justify(true);
    } else {
        layout.set_alignment(pango::ALIGN_LEFT);
    }
}

pub fn start_align_overflow(layout: &Layout) {
    let width = layout.width();
    if width < 0 {
        return;
    }
    let rtl = layout.context().base_dir() == pango::DIRECTION_RTL;
    let start = if rtl {
        pango::ALIGN_RIGHT
    } else {
        pango::ALIGN_LEFT
    };
    if layout.alignment() == start {
        return;
    }
    let mut all_overflow = true;
    let mut iter = layout.iter();
    loop {
        if iter.line_logical_extents().width <= width {
            all_overflow = false;
        }
        if !(all_overflow && iter.next_line()) {
            break;
        }
    }
    drop(iter);
    if all_overflow {
        layout.set_alignment(start);
    }
}

pub fn apply_nowrap_align_width(layout: &Layout, b: BoxRef<'_>) {
    if layout.width() >= 0 {
        return;
    }
    if layout.alignment() == pango::ALIGN_LEFT {
        return;
    }
    let (pw, _) = layout.pixel_size();
    if f64::from(pw) <= b.content_width() {
        layout.set_width((b.content_width() * pango::SCALE_F) as i32);
    }
}

fn inline_has_form_controls(b: BoxRef<'_>) -> bool {
    b.attrs().iter().any(|r| {
        matches!(
            r.kind,
            inline_kind::INPUT_FIELD | inline_kind::INPUT_FIELD_FOCUSED | inline_kind::BUTTON
        )
    })
}

pub fn inline_y_offset_for_layout(b: BoxRef<'_>, layout: &Layout) -> f64 {
    let (_, ph) = layout.size();
    let mut y_offset = (b.content_height() - f64::from(ph) / pango::SCALE_F) * 0.5;
    if inline_has_form_controls(b) {
        y_offset = 0.0;
    }
    if y_offset < 0.0 && css_line_height_px(inherited_style(b)) <= 0.0 {
        y_offset = 0.0;
    }
    y_offset
}
