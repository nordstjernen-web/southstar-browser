//! Southstar — the southstar-text handles page text is laid out and painted with: shared layouts, attribute lists, attributes, font descriptions and line iterators.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{RefCell, RefMut};
use core::ffi::{CStr, c_int, c_void};
use std::borrow::Cow;
use std::rc::Rc;
use std::sync::{Arc, MutexGuard, PoisonError};

use southstar_cairo::ContextRef;
use southstar_text::attrs::{self as text_attrs, AttrType};
#[cfg(target_os = "macos")]
use southstar_text::font::HintStyle;
use southstar_text::font::{Antialias, CairoContext, FontOptions, HintMetrics, SubpixelOrder};
use southstar_text::fontmap::FontMap as TextFontMap;
use southstar_text::layout::{self as text_layout, SharedContext};
use southstar_text::{
    Alignment, Direction, EllipsizeMode, Overline, Stretch, Style, TabAlign, Underline, Variant,
    Weight, WrapMode,
};

pub use southstar_text::metrics::FontMetrics;
pub use southstar_text::{LogAttr, Rectangle, SCALE, SCALE_F};

use crate::render;

pub const WEIGHT_THIN: c_int = 100;
pub const WEIGHT_ULTRALIGHT: c_int = 200;
pub const WEIGHT_LIGHT: c_int = 300;
pub const WEIGHT_NORMAL: c_int = 400;
pub const WEIGHT_MEDIUM: c_int = 500;
pub const WEIGHT_SEMIBOLD: c_int = 600;
pub const WEIGHT_BOLD: c_int = 700;
pub const WEIGHT_ULTRABOLD: c_int = 800;
pub const WEIGHT_HEAVY: c_int = 900;

pub const STYLE_NORMAL: c_int = Style::Normal as c_int;
pub const STYLE_OBLIQUE: c_int = Style::Oblique as c_int;
pub const STYLE_ITALIC: c_int = Style::Italic as c_int;
pub const VARIANT_SMALL_CAPS: c_int = Variant::SmallCaps as c_int;

pub const WRAP_WORD: c_int = WrapMode::Word as c_int;
pub const WRAP_CHAR: c_int = WrapMode::Char as c_int;
pub const WRAP_WORD_CHAR: c_int = WrapMode::WordChar as c_int;

pub const ALIGN_LEFT: c_int = Alignment::Left as c_int;
pub const ALIGN_CENTER: c_int = Alignment::Center as c_int;
pub const ALIGN_RIGHT: c_int = Alignment::Right as c_int;

pub const ELLIPSIZE_END: c_int = EllipsizeMode::End as c_int;

pub const DIRECTION_LTR: c_int = Direction::Ltr as c_int;
pub const DIRECTION_RTL: c_int = Direction::Rtl as c_int;
pub const DIRECTION_WEAK_LTR: c_int = Direction::WeakLtr as c_int;
pub const DIRECTION_NEUTRAL: c_int = Direction::Neutral as c_int;

pub const UNDERLINE_SINGLE: c_int = Underline::Single as c_int;
pub const UNDERLINE_DOUBLE: c_int = Underline::Double as c_int;
pub const UNDERLINE_ERROR: c_int = Underline::Error as c_int;
pub const OVERLINE_SINGLE: c_int = Overline::Single as c_int;

pub const TAB_LEFT: c_int = TabAlign::Left as c_int;

fn utf8(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

#[derive(Clone)]
pub struct FontMap(TextFontMap);

impl FontMap {
    pub fn default_map() -> Option<FontMap> {
        Some(FontMap(TextFontMap::default_for_thread()))
    }

    pub fn serial(&self) -> u32 {
        self.0.serial()
    }

    pub fn family_names(&self) -> Vec<Vec<u8>> {
        self.0
            .list_families()
            .iter()
            .map(|family| family.name().as_bytes().to_vec())
            .collect()
    }
}

pub fn font_map_serial() -> u32 {
    TextFontMap::default_for_thread().serial()
}

#[derive(Clone)]
pub struct Context(SharedContext);

impl Context {
    pub fn new_text_context() -> Context {
        text_layout::init();
        let mut ctx = TextFontMap::default_for_thread().create_context();
        let mut fo = FontOptions::new();
        if let Some(base) = ctx.font_options() {
            fo.merge(base);
        }
        fo.set_antialias(Antialias::Gray);
        fo.set_subpixel_order(SubpixelOrder::Default);
        fo.set_hint_metrics(HintMetrics::Off);
        #[cfg(target_os = "macos")]
        fo.set_hint_style(HintStyle::None);
        ctx.set_font_options(Some(&fo));
        ctx.set_round_glyph_positions(false);
        ctx.set_resolution(72.0);
        Context(text_layout::shared_context(ctx))
    }

    fn lock(&self) -> MutexGuard<'_, southstar_text::context::Context> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn serial(&self) -> u32 {
        self.lock().serial()
    }

    pub fn base_dir(&self) -> c_int {
        self.lock().base_dir() as c_int
    }

    pub fn set_base_dir(&self, direction: c_int) {
        if let Some(direction) = Direction::from_i32(direction) {
            self.lock().set_base_dir(direction);
        }
    }

    pub fn metrics(&self, desc: &FontDescription) -> Option<FontMetrics> {
        Some(self.lock().metrics(Some(&desc.0), None))
    }

    pub fn font_description(&self) -> FontDescription {
        FontDescription(self.lock().font_description().clone())
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct FontDescription(southstar_text::font_description::FontDescription);

impl FontDescription {
    pub fn new() -> FontDescription {
        FontDescription::default()
    }

    pub fn from_string(text: &CStr) -> FontDescription {
        FontDescription(
            southstar_text::font_description::FontDescription::from_string(&utf8(text.to_bytes())),
        )
    }

    pub fn set_family(&mut self, family: &CStr) {
        self.0.set_family(Some(&utf8(family.to_bytes())));
    }

    pub fn set_weight(&mut self, weight: c_int) {
        self.0.set_weight(Weight(weight));
    }

    pub fn set_style(&mut self, style: c_int) {
        if let Some(style) = Style::from_i32(style) {
            self.0.set_style(style);
        }
    }

    pub fn set_stretch(&mut self, stretch: c_int) {
        if let Some(stretch) = Stretch::from_i32(stretch) {
            self.0.set_stretch(stretch);
        }
    }

    pub fn set_absolute_size(&mut self, size: f64) {
        self.0.set_absolute_size(size);
    }

    pub fn set_variations(&mut self, variations: &CStr) {
        self.0.set_variations(Some(&utf8(variations.to_bytes())));
    }

    pub fn hash_value(&self) -> u32 {
        self.0.hash()
    }

    pub fn equal(&self, other: &FontDescription) -> bool {
        self.0.equal(&other.0)
    }
}

#[derive(Clone)]
pub struct Layout(Rc<RefCell<text_layout::Layout>>);

impl Layout {
    pub fn new(ctx: Context) -> Layout {
        Layout(Rc::new(RefCell::new(text_layout::Layout::new(&ctx.0))))
    }

    fn get(&self) -> RefMut<'_, text_layout::Layout> {
        self.0.borrow_mut()
    }

    pub fn into_raw(self) -> *mut c_void {
        Rc::into_raw(self.0).cast_mut().cast()
    }

    /// # Safety
    ///
    /// `raw` is null or came from `Layout::into_raw` on this thread and has not been released;
    /// the returned handle takes over that reference.
    pub unsafe fn from_raw(raw: *mut c_void) -> Option<Layout> {
        if raw.is_null() {
            return None;
        }
        // SAFETY: the caller passes a pointer from Rc::into_raw that still holds its reference.
        Some(Layout(unsafe { Rc::from_raw(raw.cast_const().cast()) }))
    }

    /// # Safety
    ///
    /// `raw` is null or came from `Layout::into_raw` on this thread and has not been released;
    /// the reference stays with the caller and the returned handle holds a new one.
    pub unsafe fn clone_from_raw(raw: *mut c_void) -> Option<Layout> {
        // SAFETY: the caller's contract is the one from_raw needs.
        let borrowed = core::mem::ManuallyDrop::new(unsafe { Layout::from_raw(raw) }?);
        Some(Layout::clone(&borrowed))
    }

    pub fn context(&self) -> Context {
        Context(Arc::clone(self.get().context()))
    }

    pub fn set_font_description(&self, desc: &FontDescription) {
        self.get().set_font_description(Some(&desc.0));
    }

    pub fn font_description(&self) -> Option<FontDescription> {
        self.get().font_description().cloned().map(FontDescription)
    }

    pub fn set_text(&self, text: &CStr) {
        self.get().set_text(&utf8(text.to_bytes()));
    }

    pub fn set_text_bytes(&self, text: &[u8]) {
        self.get().set_text(&utf8(text));
    }

    pub fn text(&self) -> String {
        self.get().text().to_owned()
    }

    pub fn clear_attributes(&self) {
        self.get().set_attributes(None);
    }

    pub fn set_width(&self, width: c_int) {
        self.get().set_width(width);
    }

    pub fn width(&self) -> c_int {
        self.get().width()
    }

    pub fn set_height(&self, height: c_int) {
        self.get().set_height(height);
    }

    pub fn set_wrap(&self, wrap: c_int) {
        if let Some(wrap) = WrapMode::from_i32(wrap) {
            self.get().set_wrap(wrap);
        }
    }

    pub fn set_indent(&self, indent: c_int) {
        self.get().set_indent(indent);
    }

    pub fn set_ellipsize(&self, ellipsize: c_int) {
        if let Some(ellipsize) = EllipsizeMode::from_i32(ellipsize) {
            self.get().set_ellipsize(ellipsize);
        }
    }

    pub fn set_alignment(&self, alignment: c_int) {
        if let Some(alignment) = Alignment::from_i32(alignment) {
            self.get().set_alignment(alignment);
        }
    }

    pub fn alignment(&self) -> c_int {
        self.get().alignment() as c_int
    }

    pub fn set_justify(&self, justify: bool) {
        self.get().set_justify(justify);
    }

    pub fn set_auto_dir(&self, auto_dir: bool) {
        self.get().set_auto_dir(auto_dir);
    }

    pub fn set_tabs(&self, tabs: &TabArray) {
        self.get().set_tabs(Some(&tabs.0));
    }

    pub fn size(&self) -> (c_int, c_int) {
        self.get().size()
    }

    pub fn pixel_size(&self) -> (c_int, c_int) {
        self.get().pixel_size()
    }

    pub fn pixel_ink_extents(&self) -> Rectangle {
        self.get().pixel_extents().0
    }

    pub fn line_count(&self) -> c_int {
        self.get().line_count()
    }

    pub fn line(&self, index: c_int) -> Option<Line<'_>> {
        self.get().line(index).is_some().then_some(Line {
            layout: self,
            index,
        })
    }

    pub fn index_to_pos(&self, index: c_int) -> Rectangle {
        self.get().index_to_pos(index)
    }

    pub fn index_to_box(&self, index: c_int) -> Rectangle {
        let mut pos = self.index_to_pos(index);
        if pos.width < 0 {
            pos.x += pos.width;
            pos.width = -pos.width;
        }
        pos
    }

    pub fn xy_to_index(&self, x: c_int, y: c_int) -> (c_int, c_int) {
        let (_, index, trailing) = self.get().xy_to_index(x, y);
        (index, trailing)
    }

    pub fn xy_to_index_inside(&self, x: c_int, y: c_int) -> Option<c_int> {
        let (inside, index, _) = self.get().xy_to_index(x, y);
        inside.then_some(index)
    }

    pub fn log_attrs(&self) -> Vec<LogAttr> {
        self.get().log_attrs().to_vec()
    }

    pub fn iter(&self) -> LayoutIter<'_> {
        let mut lines = Vec::new();
        {
            let mut layout = self.get();
            let mut iter = layout.iter();
            loop {
                lines.push(IterLine {
                    line: iter.line_index() as c_int,
                    logical: iter.line_logical_extents(),
                    baseline: iter.baseline(),
                    index: iter.index(),
                });
                if !iter.next_line() {
                    break;
                }
            }
        }
        LayoutIter {
            layout: self,
            lines,
            pos: 0,
        }
    }

    pub fn set_attributes(&self, attrs: &AttrList) {
        self.get().set_attributes(Some(&attrs.0.borrow()));
    }

    pub fn attributes_string(&self) -> Option<String> {
        self.get().attributes().map(ToString::to_string)
    }

    pub fn has_tabs(&self) -> bool {
        self.get().tabs().is_some()
    }

    pub fn height(&self) -> c_int {
        self.get().height()
    }

    pub fn indent(&self) -> c_int {
        self.get().indent()
    }

    pub fn spacing(&self) -> c_int {
        self.get().spacing()
    }

    pub fn line_spacing(&self) -> f32 {
        self.get().line_spacing()
    }

    pub fn justify(&self) -> bool {
        self.get().justify()
    }

    pub fn single_paragraph_mode(&self) -> bool {
        self.get().single_paragraph_mode()
    }

    pub fn auto_dir(&self) -> bool {
        self.get().auto_dir()
    }

    pub fn wrap(&self) -> c_int {
        self.get().wrap() as c_int
    }

    pub fn ellipsize(&self) -> c_int {
        self.get().ellipsize() as c_int
    }

    pub fn logical_extents(&self) -> Rectangle {
        self.get().logical_extents()
    }

    pub fn baseline(&self) -> c_int {
        self.get().baseline()
    }

    pub fn index_to_line(&self, index: c_int) -> c_int {
        self.get()
            .index_to_line_x(index, false)
            .map_or(0, |(line, _)| line)
    }

    pub fn context_changed(&self) {
        self.get().context_changed();
    }

    pub fn css_line_height(&self, key: &CStr) -> Option<f64> {
        self.get().css_line_height(&utf8(key.to_bytes()))
    }

    pub fn set_css_line_height(&self, key: &CStr, line_height: f64) {
        self.get()
            .set_css_line_height(&utf8(key.to_bytes()), line_height);
    }

    pub fn show(&self, cr: ContextRef) {
        render::show_layout(cr, &mut self.get());
    }

    pub fn show_on(&self, cr: &CairoContext) {
        southstar_text::render::show_layout(cr, &mut self.get());
    }

    pub fn path_on(&self, cr: &CairoContext) {
        southstar_text::render::layout_path(cr, &mut self.get());
    }
}

pub fn extents_to_pixels(rect: &mut Rectangle) {
    southstar_text::extents_to_pixels(Some(rect), None);
}

#[derive(Clone, Copy)]
pub struct Line<'a> {
    layout: &'a Layout,
    index: c_int,
}

pub struct Run {
    pub offset: c_int,
    pub length: c_int,
    pub y_offset: c_int,
    pub is_spacer: bool,
}

impl Line<'_> {
    fn with<R: Default>(self, f: impl FnOnce(text_layout::LineRef<'_>) -> R) -> R {
        self.layout
            .get()
            .line(self.index)
            .map(f)
            .unwrap_or_default()
    }

    pub fn start_index(self) -> c_int {
        self.with(|line| line.start_index)
    }

    pub fn length(self) -> c_int {
        self.with(|line| line.length)
    }

    pub fn index_to_x(self, index: c_int, trailing: bool) -> c_int {
        self.with(|line| line.index_to_x(index, trailing))
    }

    pub fn x_ranges(self, start: c_int, end: c_int) -> Vec<(c_int, c_int)> {
        self.with(|line| line.x_ranges(start, end))
    }

    pub fn runs(self) -> Vec<Run> {
        self.with(|line| {
            line.runs
                .iter()
                .map(|run| Run {
                    offset: run.item.offset,
                    length: run.item.length,
                    y_offset: run.y_offset,
                    is_spacer: run
                        .item
                        .analysis
                        .extra_attrs
                        .iter()
                        .any(|attr| attr.kind() == AttrType::Shape),
                })
                .collect()
        })
    }

    pub fn show(self, cr: ContextRef) {
        self.with(|line| render::show_layout_line(cr, line));
    }

    pub fn range_path(self, cr: ContextRef, x: f64, baseline: f64, range: core::ops::Range<c_int>) {
        self.with(|line| render::line_range_path(cr, line, x, baseline, range));
    }
}

struct IterLine {
    line: c_int,
    logical: Rectangle,
    baseline: c_int,
    index: c_int,
}

pub struct LayoutIter<'a> {
    layout: &'a Layout,
    lines: Vec<IterLine>,
    pos: usize,
}

impl<'a> LayoutIter<'a> {
    fn current(&self) -> &IterLine {
        &self.lines[self.pos]
    }

    pub fn next_line(&mut self) -> bool {
        if self.pos + 1 < self.lines.len() {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub fn line(&self) -> Option<Line<'a>> {
        Some(Line {
            layout: self.layout,
            index: self.current().line,
        })
    }

    pub fn line_logical_extents(&self) -> Rectangle {
        self.current().logical
    }

    pub fn baseline(&self) -> c_int {
        self.current().baseline
    }

    pub fn index(&self) -> c_int {
        self.current().index
    }
}

pub struct TabArray(southstar_text::tabs::TabArray);

impl TabArray {
    pub fn new(size: c_int, in_pixels: bool) -> TabArray {
        TabArray(southstar_text::tabs::TabArray::new(
            usize::try_from(size).unwrap_or(0),
            in_pixels,
        ))
    }

    pub fn set_tab(&mut self, index: c_int, alignment: c_int, location: c_int) {
        let alignment = TabAlign::from_i32(alignment).unwrap_or(TabAlign::Left);
        self.0.set_tab(index, alignment, location);
    }
}

#[derive(Default)]
pub struct AttrList(RefCell<text_attrs::AttrList>);

impl AttrList {
    pub fn new() -> AttrList {
        AttrList::default()
    }

    pub fn insert(&self, attr: Attribute, start: u32, end: u32) {
        self.0.borrow_mut().insert(attr.0.with_range(start, end));
    }

    pub fn copy(&self) -> AttrList {
        AttrList(RefCell::new(self.0.borrow().clone()))
    }

    pub fn insert_range(&self, attr: Option<Attribute>, start: usize, len: usize) {
        if let Some(attr) = attr {
            self.insert(attr, start as u32, (start + len) as u32);
        }
    }
}

#[derive(Clone)]
pub struct Attribute(text_attrs::Attribute);

impl Attribute {
    pub fn copy(&self) -> Attribute {
        self.clone()
    }

    pub fn insert_hyphens(insert: bool) -> Attribute {
        Attribute(text_attrs::attr_insert_hyphens_new(insert))
    }

    pub fn language(lang: &CStr) -> Attribute {
        let language = southstar_text::language::Language::from_string(&utf8(lang.to_bytes()));
        Attribute(text_attrs::attr_language_new(Some(language)))
    }

    pub fn font_features(features: &CStr) -> Attribute {
        Attribute(text_attrs::attr_font_features_new(&utf8(
            features.to_bytes(),
        )))
    }

    pub fn font_desc(desc: &FontDescription) -> Attribute {
        Attribute(text_attrs::attr_font_desc_new(&desc.0))
    }

    pub fn family(family: &CStr) -> Attribute {
        Attribute(text_attrs::attr_family_new(&utf8(family.to_bytes())))
    }

    pub fn weight(weight: c_int) -> Attribute {
        Attribute(text_attrs::attr_weight_new(Weight(weight)))
    }

    pub fn stretch(stretch: c_int) -> Attribute {
        let stretch = Stretch::from_i32(stretch).unwrap_or(Stretch::Normal);
        Attribute(text_attrs::attr_stretch_new(stretch))
    }

    pub fn style(style: c_int) -> Attribute {
        let style = Style::from_i32(style).unwrap_or(Style::Normal);
        Attribute(text_attrs::attr_style_new(style))
    }

    pub fn variant(variant: c_int) -> Attribute {
        let variant = Variant::from_i32(variant).unwrap_or(Variant::Normal);
        Attribute(text_attrs::attr_variant_new(variant))
    }

    pub fn size_absolute(size: c_int) -> Attribute {
        Attribute(text_attrs::attr_size_new_absolute(size))
    }

    pub fn scale(scale: f64) -> Attribute {
        Attribute(text_attrs::attr_scale_new(scale))
    }

    pub fn rise(rise: c_int) -> Attribute {
        Attribute(text_attrs::attr_rise_new(rise))
    }

    pub fn letter_spacing(spacing: c_int) -> Attribute {
        Attribute(text_attrs::attr_letter_spacing_new(spacing))
    }

    pub fn foreground(r: u16, g: u16, b: u16) -> Attribute {
        Attribute(text_attrs::attr_foreground_new(r, g, b))
    }

    pub fn foreground_alpha(alpha: u16) -> Attribute {
        Attribute(text_attrs::attr_foreground_alpha_new(alpha))
    }

    pub fn background(r: u16, g: u16, b: u16) -> Attribute {
        Attribute(text_attrs::attr_background_new(r, g, b))
    }

    pub fn background_alpha(alpha: u16) -> Attribute {
        Attribute(text_attrs::attr_background_alpha_new(alpha))
    }

    pub fn underline(underline: c_int) -> Attribute {
        let underline = Underline::from_i32(underline).unwrap_or(Underline::None);
        Attribute(text_attrs::attr_underline_new(underline))
    }

    pub fn underline_color(r: u16, g: u16, b: u16) -> Attribute {
        Attribute(text_attrs::attr_underline_color_new(r, g, b))
    }

    pub fn overline(overline: c_int) -> Attribute {
        let overline = Overline::from_i32(overline).unwrap_or(Overline::None);
        Attribute(text_attrs::attr_overline_new(overline))
    }

    pub fn overline_color(r: u16, g: u16, b: u16) -> Attribute {
        Attribute(text_attrs::attr_overline_color_new(r, g, b))
    }

    pub fn strikethrough(strike: bool) -> Attribute {
        Attribute(text_attrs::attr_strikethrough_new(strike))
    }

    pub fn strikethrough_color(r: u16, g: u16, b: u16) -> Attribute {
        Attribute(text_attrs::attr_strikethrough_color_new(r, g, b))
    }

    pub fn allow_breaks(allow: bool) -> Attribute {
        Attribute(text_attrs::attr_allow_breaks_new(allow))
    }

    pub fn line_height_absolute(height: c_int) -> Attribute {
        Attribute(text_attrs::attr_line_height_new_absolute(height))
    }

    pub fn shape_rect(rect: Rectangle) -> Attribute {
        Attribute(text_attrs::attr_shape_new(&rect, &rect))
    }

    pub fn shape(width: c_int) -> Attribute {
        Attribute::shape_rect(Rectangle {
            x: 0,
            y: 0,
            width,
            height: 0,
        })
    }
}
