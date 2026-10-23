//! Southstar text — paragraph layout: text and attributes broken into lines of shaped runs, with extents, alignment and position lookups.
//! Copyright 2000, 2001, 2006 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

mod breaking;
mod extents;
mod iter;
mod line;

use std::cell::Cell;
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};

use crate::attrs::{AttrList, AttrType, Attribute, attr_font_desc_new, attr_show_new};
use crate::context::{Context, SampleWidths};
use crate::font_description::FontDescription;
use crate::shape::install_layout_hooks;
use crate::tabs::TabArray;
use crate::types::{
    Alignment, Direction, EllipsizeMode, GLYPH_UNKNOWN_FLAG, LogAttr, Rectangle, SCALE, WrapMode,
    extents_to_pixels, units_round,
};
use crate::utf8::{next_char, pango_utf8_strlen, pointer_to_offset};

pub use extents::{font_get_legacy_ascent, run_css_extents};
pub use iter::LayoutIter;
pub use line::{LayoutLine, LineRef};

use extents::Extents;

#[cfg(test)]
#[path = "tests/layout.rs"]
mod tests;

pub type SharedContext = Arc<Mutex<Context>>;

pub fn shared_context(context: Context) -> SharedContext {
    Arc::new(Mutex::new(context))
}

pub(crate) fn lock_context(context: &Mutex<Context>) -> MutexGuard<'_, Context> {
    context.lock().unwrap_or_else(PoisonError::into_inner)
}

static INIT: Once = Once::new();

pub fn init() {
    INIT.call_once(|| {
        install_layout_hooks(measure_font_sample);
    });
}

fn max_glyph_width(lines: &[LayoutLine]) -> i32 {
    lines
        .iter()
        .flat_map(|line| line.runs.iter())
        .flat_map(|run| run.glyphs.glyphs.iter())
        .map(|glyph| glyph.geometry.width)
        .fold(0, i32::max)
}

fn measure_font_sample(
    context: &mut Context,
    desc: &FontDescription,
    sample: &str,
) -> SampleWidths {
    let shared = shared_context(context.clone());
    let mut layout = Layout::new(&shared);
    layout.set_font_description(Some(desc));
    layout.set_text(sample);
    let sample_width = layout.logical_extents().width;
    layout.set_text("0123456789");
    layout.ensure_lines();
    let digits_max_glyph_width = max_glyph_width(layout.lines.as_deref().unwrap_or(&[]));
    SampleWidths {
        sample_width,
        digits_max_glyph_width,
    }
}

pub(crate) fn direction_simple(direction: Direction) -> i32 {
    match direction {
        Direction::Ltr | Direction::WeakLtr | Direction::TtbRtl => 1,
        Direction::Rtl | Direction::WeakRtl | Direction::TtbLtr => -1,
        Direction::Neutral => 0,
    }
}

pub(crate) fn byte_at(text: &str, index: i32) -> u8 {
    usize::try_from(index)
        .ok()
        .and_then(|index| text.as_bytes().get(index).copied())
        .unwrap_or(0)
}

pub(crate) fn char_at(text: &str, index: i32) -> u32 {
    usize::try_from(index).map_or(0, |index| crate::utf8::char_at(text.as_bytes(), index))
}

pub(crate) fn char_offset_of(text: &str, index: i32) -> i32 {
    pointer_to_offset(text.as_bytes(), 0, usize::try_from(index).unwrap_or(0))
}

pub(crate) fn utf8_strlen_at(text: &str, start: i32, length: i32) -> i32 {
    let start = usize::try_from(start).unwrap_or(0).min(text.len());
    let length = usize::try_from(length).unwrap_or(0);
    pango_utf8_strlen(&text.as_bytes()[start..], length)
}

pub(crate) fn byte_length_of_chars(text: &str, start: i32, num_chars: i32) -> i32 {
    let start = usize::try_from(start).unwrap_or(0);
    let mut p = start;
    for _ in 0..num_chars.max(0) {
        p = next_char(text.as_bytes(), p);
    }
    (p - start) as i32
}

#[derive(Debug)]
pub struct Layout {
    context: SharedContext,
    attrs: Option<AttrList>,
    font_desc: Option<FontDescription>,
    tabs: Option<TabArray>,
    text: String,
    has_text: bool,
    serial: u32,
    context_serial: u32,
    n_chars: i32,
    width: i32,
    height: i32,
    indent: i32,
    spacing: i32,
    line_spacing: f32,
    strut_line_height: i32,
    justify: bool,
    justify_last_line: bool,
    alignment: Alignment,
    single_paragraph: bool,
    auto_dir: bool,
    wrap: WrapMode,
    is_wrapped: bool,
    ellipsize: EllipsizeMode,
    is_ellipsized: bool,
    unknown_glyphs_count: i32,
    logical_rect: Cell<Option<Rectangle>>,
    ink_rect: Cell<Option<Rectangle>>,
    strut_rect: Cell<Option<Rectangle>>,
    tab_width: i32,
    decimal: u32,
    log_attrs: Option<Vec<LogAttr>>,
    lines: Option<Vec<LayoutLine>>,
    user_data: Vec<(String, f64)>,
}

fn affects_itemization(attr: &Attribute) -> bool {
    matches!(
        attr.kind(),
        AttrType::Language
            | AttrType::Family
            | AttrType::Style
            | AttrType::Weight
            | AttrType::Variant
            | AttrType::Stretch
            | AttrType::Width
            | AttrType::Size
            | AttrType::FontDesc
            | AttrType::Scale
            | AttrType::Fallback
            | AttrType::AbsoluteSize
            | AttrType::Gravity
            | AttrType::GravityHint
            | AttrType::FontScale
            | AttrType::LetterSpacing
            | AttrType::WordSpacing
            | AttrType::Shape
            | AttrType::Rise
            | AttrType::BaselineShift
            | AttrType::LineHeight
            | AttrType::AbsoluteLineHeight
            | AttrType::TextTransform
    )
}

fn affects_break_or_shape(attr: &Attribute) -> bool {
    matches!(
        attr.kind(),
        AttrType::AllowBreaks
            | AttrType::Word
            | AttrType::Sentence
            | AttrType::InsertHyphens
            | AttrType::FontFeatures
            | AttrType::Show
    )
}

impl Layout {
    pub fn new(context: &SharedContext) -> Layout {
        init();
        let context_serial = lock_context(context).serial();
        Layout {
            context: Arc::clone(context),
            attrs: None,
            font_desc: None,
            tabs: None,
            text: String::new(),
            has_text: false,
            serial: 1,
            context_serial,
            n_chars: 0,
            width: -1,
            height: -1,
            indent: 0,
            spacing: 0,
            line_spacing: 0.0,
            strut_line_height: 0,
            justify: false,
            justify_last_line: false,
            alignment: Alignment::Left,
            single_paragraph: false,
            auto_dir: true,
            wrap: WrapMode::Word,
            is_wrapped: false,
            ellipsize: EllipsizeMode::None,
            is_ellipsized: false,
            unknown_glyphs_count: -1,
            logical_rect: Cell::new(None),
            ink_rect: Cell::new(None),
            strut_rect: Cell::new(None),
            tab_width: -1,
            decimal: 0,
            log_attrs: None,
            lines: None,
            user_data: Vec::new(),
        }
    }

    pub fn context(&self) -> &SharedContext {
        &self.context
    }

    pub fn serial(&self) -> u32 {
        self.serial
    }

    pub fn set_width(&mut self, width: i32) {
        let width = if width < 0 { -1 } else { width };
        if width != self.width {
            self.width = width;
            self.layout_changed();
        }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn set_height(&mut self, height: i32) {
        if height != self.height {
            self.height = height;
            let fits = self.lines.is_some()
                && !self.is_ellipsized
                && height < 0
                && self.line_count_value() <= height.wrapping_neg() as u32;
            if self.ellipsize != EllipsizeMode::None && !fits {
                self.layout_changed();
            }
        }
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn set_wrap(&mut self, wrap: WrapMode) {
        if self.wrap != wrap {
            self.wrap = wrap;
            if self.width != -1 {
                self.layout_changed();
            }
        }
    }

    pub fn wrap(&self) -> WrapMode {
        self.wrap
    }

    pub fn set_indent(&mut self, indent: i32) {
        if indent != self.indent {
            self.indent = indent;
            self.layout_changed();
        }
    }

    pub fn indent(&self) -> i32 {
        self.indent
    }

    pub fn set_strut_line_height(&mut self, line_height: i32) {
        let line_height = line_height.max(0);
        if line_height != self.strut_line_height {
            self.strut_line_height = line_height;
            self.layout_changed();
        }
    }

    pub fn strut_line_height(&self) -> i32 {
        self.strut_line_height
    }

    pub fn set_spacing(&mut self, spacing: i32) {
        if spacing != self.spacing {
            self.spacing = spacing;
            self.layout_changed();
        }
    }

    pub fn spacing(&self) -> i32 {
        self.spacing
    }

    pub fn set_line_spacing(&mut self, factor: f32) {
        if factor != self.line_spacing {
            self.line_spacing = factor;
            self.layout_changed();
        }
    }

    pub fn line_spacing(&self) -> f32 {
        self.line_spacing
    }

    pub fn set_attributes(&mut self, attrs: Option<&AttrList>) {
        if attrs.is_none() && self.attrs.is_none() {
            return;
        }
        if let (Some(current), Some(new)) = (self.attrs.as_ref(), attrs)
            && current.equal(new)
        {
            return;
        }
        self.attrs = attrs.cloned();
        self.log_attrs = None;
        self.layout_changed();
        self.tab_width = -1;
    }

    pub fn attributes(&self) -> Option<&AttrList> {
        self.attrs.as_ref()
    }

    pub fn set_font_description(&mut self, desc: Option<&FontDescription>) {
        let changed = match (desc, self.font_desc.as_ref()) {
            (None, None) => false,
            (Some(desc), Some(current)) => !desc.equal(current),
            _ => true,
        };
        if changed {
            self.font_desc = desc.cloned();
            self.layout_changed();
            self.tab_width = -1;
        }
    }

    pub fn font_description(&self) -> Option<&FontDescription> {
        self.font_desc.as_ref()
    }

    pub fn set_justify(&mut self, justify: bool) {
        if justify != self.justify {
            self.justify = justify;
            if self.is_ellipsized || self.is_wrapped || self.justify_last_line {
                self.layout_changed();
            }
        }
    }

    pub fn justify(&self) -> bool {
        self.justify
    }

    pub fn set_justify_last_line(&mut self, justify: bool) {
        if justify != self.justify_last_line {
            self.justify_last_line = justify;
            if self.justify {
                self.layout_changed();
            }
        }
    }

    pub fn justify_last_line(&self) -> bool {
        self.justify_last_line
    }

    pub fn set_auto_dir(&mut self, auto_dir: bool) {
        if auto_dir != self.auto_dir {
            self.auto_dir = auto_dir;
            self.layout_changed();
        }
    }

    pub fn auto_dir(&self) -> bool {
        self.auto_dir
    }

    pub fn set_alignment(&mut self, alignment: Alignment) {
        if alignment != self.alignment {
            self.alignment = alignment;
            self.layout_changed();
        }
    }

    pub fn alignment(&self) -> Alignment {
        self.alignment
    }

    pub fn set_tabs(&mut self, tabs: Option<&TabArray>) {
        if tabs.is_none() && self.tabs.is_none() {
            return;
        }
        self.tabs = tabs.map(|tabs| {
            let mut copy = tabs.clone();
            copy.sort();
            copy
        });
        self.layout_changed();
    }

    pub fn tabs(&self) -> Option<TabArray> {
        self.tabs.clone()
    }

    pub fn set_single_paragraph_mode(&mut self, setting: bool) {
        if self.single_paragraph != setting {
            self.single_paragraph = setting;
            self.layout_changed();
        }
    }

    pub fn single_paragraph_mode(&self) -> bool {
        self.single_paragraph
    }

    pub fn set_ellipsize(&mut self, ellipsize: EllipsizeMode) {
        if ellipsize != self.ellipsize {
            self.ellipsize = ellipsize;
            if self.is_ellipsized || self.is_wrapped {
                self.layout_changed();
            }
        }
    }

    pub fn ellipsize(&self) -> EllipsizeMode {
        self.ellipsize
    }

    pub fn set_text(&mut self, text: &str) {
        let text = text.find('\0').map_or(text, |end| &text[..end]);
        self.text = text.to_owned();
        self.has_text = true;
        self.n_chars = utf8_strlen_at(&self.text, 0, self.text.len() as i32);
        self.log_attrs = None;
        self.layout_changed();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn character_count(&self) -> i32 {
        self.n_chars
    }

    pub fn context_changed(&mut self) {
        self.layout_changed();
        self.tab_width = -1;
    }

    pub fn set_css_line_height(&mut self, key: &str, line_height: f64) {
        if let Some(slot) = self.user_data.iter_mut().find(|(k, _)| k == key) {
            slot.1 = line_height;
        } else {
            self.user_data.push((key.to_owned(), line_height));
        }
    }

    pub fn css_line_height(&self, key: &str) -> Option<f64> {
        self.user_data
            .iter()
            .find(|(k, _)| k == key)
            .map(|&(_, value)| value)
    }

    pub fn log_attrs(&mut self) -> &[LogAttr] {
        self.ensure_lines();
        match self.log_attrs.as_deref() {
            Some(attrs) => &attrs[..attrs.len().min(self.n_chars as usize + 1)],
            None => &[],
        }
    }

    pub fn line_count(&mut self) -> i32 {
        self.ensure_lines();
        self.line_count_value() as i32
    }

    pub fn lines(&mut self) -> Vec<LineRef<'_>> {
        self.ensure_lines();
        let layout: &Layout = self;
        (0..layout.computed_lines().len())
            .map(|index| LineRef::new(layout, index))
            .collect()
    }

    pub fn line(&mut self, index: i32) -> Option<LineRef<'_>> {
        if index < 0 {
            return None;
        }
        self.ensure_lines();
        let layout: &Layout = self;
        let index = index as usize;
        (index < layout.computed_lines().len()).then(|| LineRef::new(layout, index))
    }

    pub fn is_wrapped(&mut self) -> bool {
        self.ensure_lines();
        self.is_wrapped
    }

    pub fn is_ellipsized(&mut self) -> bool {
        self.ensure_lines();
        self.is_ellipsized
    }

    pub fn unknown_glyphs_count(&mut self) -> i32 {
        self.ensure_lines();
        if self.unknown_glyphs_count >= 0 {
            return self.unknown_glyphs_count;
        }
        let count = self
            .computed_lines()
            .iter()
            .flat_map(|line| line.runs.iter())
            .flat_map(|run| run.glyphs.glyphs.iter())
            .filter(|glyph| glyph.glyph & GLYPH_UNKNOWN_FLAG != 0)
            .count() as i32;
        self.unknown_glyphs_count = count;
        count
    }

    pub fn extents(&mut self) -> (Rectangle, Rectangle) {
        self.ensure_lines();
        let context = Arc::clone(&self.context);
        let ctx = lock_context(&context);
        let (ink, logical, _) = self.extents_internal(&ctx, true, true, false);
        (ink.unwrap_or_default(), logical.unwrap_or_default())
    }

    pub fn ink_extents(&mut self) -> Rectangle {
        self.ensure_lines();
        let context = Arc::clone(&self.context);
        let ctx = lock_context(&context);
        self.extents_internal(&ctx, true, false, false)
            .0
            .unwrap_or_default()
    }

    pub fn logical_extents(&mut self) -> Rectangle {
        self.ensure_lines();
        let context = Arc::clone(&self.context);
        let ctx = lock_context(&context);
        self.extents_internal(&ctx, false, true, false)
            .1
            .unwrap_or_default()
    }

    pub fn pixel_extents(&mut self) -> (Rectangle, Rectangle) {
        let (mut ink, mut logical) = self.extents();
        extents_to_pixels(Some(&mut ink), None);
        extents_to_pixels(Some(&mut logical), None);
        (ink, logical)
    }

    pub fn size(&mut self) -> (i32, i32) {
        let logical = self.logical_extents();
        (logical.width, logical.height)
    }

    pub fn pixel_size(&mut self) -> (i32, i32) {
        let mut logical = self.logical_extents();
        extents_to_pixels(Some(&mut logical), None);
        (logical.width, logical.height)
    }

    pub fn baseline(&mut self) -> i32 {
        self.ensure_lines();
        let context = Arc::clone(&self.context);
        let ctx = lock_context(&context);
        let (_, _, lines) = self.extents_internal(&ctx, false, false, true);
        lines
            .and_then(|lines| lines.first().map(|extents| extents.baseline))
            .unwrap_or(0)
    }

    pub fn iter(&mut self) -> LayoutIter<'_> {
        self.ensure_lines();
        let layout: &Layout = self;
        let ctx = lock_context(&layout.context);
        LayoutIter::new(layout, &ctx)
    }

    pub fn index_to_line_x(&mut self, index: i32, trailing: bool) -> Option<(i32, i32)> {
        if index < 0 || index > self.text.len() as i32 {
            return None;
        }
        self.ensure_lines();
        let (line_nr, found) = self.index_to_line(index);
        match found {
            Some(line_index) => {
                let line = LineRef::new(self, line_index);
                let end = line.start_index + line.length;
                let index = index.min(end);
                Some((line_nr, line.index_to_x(index, trailing)))
            }
            None => Some((-1, -1)),
        }
    }

    pub fn xy_to_index(&mut self, x: i32, y: i32) -> (bool, i32, i32) {
        self.ensure_lines();
        let layout: &Layout = self;
        let ctx = lock_context(&layout.context);
        let mut iter = LayoutIter::new(layout, &ctx);
        drop(ctx);

        let mut prev_line: Option<usize> = None;
        let mut found: Option<usize> = None;
        let mut found_line_x = 0;
        let mut prev_last = 0;
        let mut prev_line_x = 0;
        let mut outside = false;

        loop {
            let line_logical = iter.line_logical_extents();
            let (first_y, last_y) = iter.line_yrange();
            let current = iter.line_index();

            if y < first_y {
                if prev_line.is_some() && y < prev_last + (first_y - prev_last) / 2 {
                    found = prev_line;
                    found_line_x = prev_line_x;
                } else {
                    if prev_line.is_none() {
                        outside = true;
                    }
                    found = Some(current);
                    found_line_x = x - line_logical.x;
                }
            } else if y >= first_y && y < last_y {
                found = Some(current);
                found_line_x = x - line_logical.x;
            }

            prev_line = Some(current);
            prev_last = last_y;
            prev_line_x = x - line_logical.x;

            if found.is_some() || !iter.next_line() {
                break;
            }
        }

        if found.is_none() {
            outside = true;
            found = prev_line;
            found_line_x = prev_line_x;
        }

        let line = LineRef::new(layout, found.unwrap_or(0));
        let (inside, index, trailing) = line.x_to_index(found_line_x);
        (inside && !outside, index, trailing)
    }

    pub fn index_to_pos(&mut self, index: i32) -> Rectangle {
        let mut pos = Rectangle::default();
        if index < 0 {
            return pos;
        }
        self.ensure_lines();
        let layout: &Layout = self;
        let ctx = lock_context(&layout.context);
        let mut iter = LayoutIter::new(layout, &ctx);
        drop(ctx);

        let mut index = index;
        let mut line_logical_rect = Rectangle::default();
        let mut run_logical_rect = Rectangle::default();
        let mut layout_line: Option<usize> = None;

        loop {
            let tmp_line = iter.line_index();
            let tmp = &layout.computed_lines()[tmp_line];
            if tmp.start_index > index {
                if let Some(previous) = layout_line {
                    let previous = &layout.computed_lines()[previous];
                    index = previous.start_index + previous.length;
                }
                break;
            }

            line_logical_rect = iter.line_logical_extents();
            layout_line = Some(tmp_line);

            if tmp.start_index + tmp.length >= index {
                loop {
                    let run = iter.run();
                    run_logical_rect = iter.run_extents().1;
                    let Some(run) = run else {
                        break;
                    };
                    if run.item.offset <= index && index < run.item.offset + run.item.length {
                        break;
                    }
                    if !iter.next_run() {
                        break;
                    }
                }
                if tmp.start_index + tmp.length > index {
                    break;
                }
            }

            if !iter.next_line() {
                index = tmp.start_index + tmp.length;
                break;
            }
        }

        let Some(line_index) = layout_line else {
            return pos;
        };
        pos.y = run_logical_rect.y;
        pos.height = run_logical_rect.height;

        let line = LineRef::new(layout, line_index);
        let x_pos = line.index_to_x(index, false);
        pos.x = line_logical_rect.x + x_pos;

        if index < line.start_index + line.length {
            let x_pos = line.index_to_x(index, true);
            pos.width = (line_logical_rect.x + x_pos) - pos.x;
        } else {
            pos.width = 0;
        }
        pos
    }

    fn index_to_line(&self, index: i32) -> (i32, Option<usize>) {
        let lines = self.computed_lines();
        let mut found = None;
        let mut line_nr = -1;
        for (i, line) in lines.iter().enumerate() {
            if line.start_index > index {
                break;
            }
            found = Some(i);
            line_nr += 1;
            if line.start_index + line.length > index {
                break;
            }
        }
        (line_nr, found)
    }

    pub(crate) fn computed_lines(&self) -> &[LayoutLine] {
        self.lines.as_deref().unwrap_or(&[])
    }

    pub(crate) fn computed_log_attrs(&self) -> &[LogAttr] {
        self.log_attrs.as_deref().unwrap_or(&[])
    }

    fn line_count_value(&self) -> u32 {
        self.lines.as_ref().map_or(0, |lines| lines.len() as u32)
    }

    fn layout_changed(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        if self.serial == 0 {
            self.serial = 1;
        }
        self.clear_lines();
    }

    fn clear_lines(&mut self) {
        self.lines = None;
        self.unknown_glyphs_count = -1;
        self.logical_rect.set(None);
        self.ink_rect.set(None);
        self.strut_rect.set(None);
        self.is_ellipsized = false;
        self.is_wrapped = false;
    }

    fn check_context_changed(&mut self, ctx: &mut Context) {
        let old_serial = self.context_serial;
        self.context_serial = ctx.serial();
        if old_serial != self.context_serial {
            self.context_changed();
        }
    }

    pub(crate) fn ensure_lines(&mut self) {
        let context = Arc::clone(&self.context);
        let mut ctx = lock_context(&context);
        self.check_context_changed(&mut ctx);
        if self.lines.is_some() {
            return;
        }
        if !self.has_text {
            self.set_text("");
        }
        breaking::check_lines(self, &mut ctx);
    }

    fn effective_attributes(&self) -> Option<AttrList> {
        let mut attrs = self.attrs.clone();
        if let Some(desc) = self.font_desc.as_ref() {
            attrs
                .get_or_insert_with(AttrList::new)
                .insert_before(attr_font_desc_new(desc));
        }
        if self.single_paragraph {
            attrs
                .get_or_insert_with(AttrList::new)
                .insert_before(attr_show_new(crate::types::show_flags::LINE_BREAKS));
        }
        attrs
    }

    pub(crate) fn alignment_for(&self, line: &LayoutLine, base_dir: Direction) -> Alignment {
        let mut alignment = self.alignment;
        if alignment != Alignment::Center
            && self.auto_dir
            && direction_simple(line.resolved_dir) == -direction_simple(base_dir)
        {
            alignment = match alignment {
                Alignment::Left => Alignment::Right,
                Alignment::Right => Alignment::Left,
                Alignment::Center => Alignment::Center,
            };
        }
        alignment
    }

    pub(crate) fn x_offset(
        &self,
        line: &LayoutLine,
        base_dir: Direction,
        layout_width: i32,
        line_width: i32,
    ) -> i32 {
        let alignment = self.alignment_for(line, base_dir);
        let mut x_offset = if layout_width == 0 {
            0
        } else if alignment == Alignment::Right {
            layout_width - line_width
        } else if alignment == Alignment::Center {
            let offset = (layout_width - line_width) / 2;
            if ((layout_width | line_width) & (SCALE - 1)) == 0 {
                units_round(offset)
            } else {
                offset
            }
        } else {
            0
        };

        if alignment == Alignment::Center {
            return x_offset;
        }

        if line.is_paragraph_start {
            if self.indent > 0 {
                if alignment == Alignment::Left {
                    x_offset += self.indent;
                } else {
                    x_offset -= self.indent;
                }
            }
        } else if self.indent < 0 {
            if alignment == Alignment::Left {
                x_offset -= self.indent;
            } else {
                x_offset += self.indent;
            }
        }
        x_offset
    }

    pub(crate) fn line_extents_layout_coords(
        &self,
        ctx: &Context,
        line_index: usize,
        layout_width: i32,
        y_offset: i32,
        baseline: Option<&mut i32>,
    ) -> (Rectangle, Rectangle) {
        let line = &self.computed_lines()[line_index];
        let first_line = line_index == 0;
        let (line_ink, line_logical, height) = extents::line_extents_and_height(self, ctx, line);
        let x_offset = self.x_offset(line, ctx.base_dir(), layout_width, line_logical.width);

        let new_baseline = match baseline.as_deref() {
            Some(&previous) if !first_line && self.line_spacing != 0.0 => {
                (previous as f32 + self.line_spacing * height as f32) as i32
            }
            _ => y_offset - line_logical.y,
        };

        let mut ink = line_ink;
        ink.x = line_ink.x + x_offset;
        ink.y = new_baseline + line_ink.y;

        let mut logical = line_logical;
        logical.x = line_logical.x + x_offset;
        logical.y = new_baseline + line_logical.y;

        if let Some(baseline) = baseline {
            *baseline = new_baseline;
        }
        (ink, logical)
    }

    pub(crate) fn extents_internal(
        &self,
        ctx: &Context,
        want_ink: bool,
        want_logical: bool,
        want_line_extents: bool,
    ) -> (Option<Rectangle>, Option<Rectangle>, Option<Vec<Extents>>) {
        let mut ink_out = None;
        let mut logical_out = None;
        let mut want_ink = want_ink;
        let mut want_logical = want_logical;

        if want_ink && let Some(cached) = self.ink_rect.get() {
            ink_out = Some(cached);
            want_ink = false;
        }
        if want_logical && let Some(cached) = self.logical_rect.get() {
            logical_out = Some(cached);
            want_logical = false;
        }
        if !want_ink && !want_logical && !want_line_extents {
            return (ink_out, logical_out, None);
        }

        let lines = self.computed_lines();
        let base_dir = ctx.base_dir();
        let mut width = self.width;
        let need_width = if self.auto_dir {
            lines
                .iter()
                .any(|line| self.alignment_for(line, base_dir) != Alignment::Left)
        } else {
            self.alignment != Alignment::Left
        };

        if width == -1 && need_width && (want_ink || want_line_extents) {
            let (_, overall_logical, _) = self.extents_internal(ctx, false, true, false);
            width = overall_logical.unwrap_or_default().width;
        }

        let mut ink_rect = Rectangle::default();
        let mut logical_rect = Rectangle::default();
        let mut line_extents: Option<Vec<Extents>> =
            (want_line_extents && !lines.is_empty()).then(|| Vec::with_capacity(lines.len()));

        let mut y_offset = 0;
        let mut baseline = 0;
        for (line_index, _) in lines.iter().enumerate() {
            let (line_ink_layout, line_logical_layout) = self.line_extents_layout_coords(
                ctx,
                line_index,
                width,
                y_offset,
                Some(&mut baseline),
            );

            if let Some(extents) = line_extents.as_mut() {
                extents.push(Extents {
                    baseline,
                    ink_rect: line_ink_layout,
                    logical_rect: line_logical_layout,
                });
            }

            if want_ink {
                if line_index == 0 {
                    ink_rect = line_ink_layout;
                } else {
                    let new_pos = ink_rect.x.min(line_ink_layout.x);
                    ink_rect.width = (ink_rect.x + ink_rect.width)
                        .max(line_ink_layout.x + line_ink_layout.width)
                        - new_pos;
                    ink_rect.x = new_pos;

                    let new_pos = ink_rect.y.min(line_ink_layout.y);
                    ink_rect.height = (ink_rect.y + ink_rect.height)
                        .max(line_ink_layout.y + line_ink_layout.height)
                        - new_pos;
                    ink_rect.y = new_pos;
                }
            }

            if want_logical {
                if self.width == -1 {
                    logical_rect.width = logical_rect.width.max(line_logical_layout.width);
                } else if line_index == 0 {
                    logical_rect.x = line_logical_layout.x;
                    logical_rect.width = line_logical_layout.width;
                } else {
                    let new_pos = logical_rect.x.min(line_logical_layout.x);
                    logical_rect.width = (logical_rect.x + logical_rect.width)
                        .max(line_logical_layout.x + line_logical_layout.width)
                        - new_pos;
                    logical_rect.x = new_pos;
                }
                logical_rect.height =
                    line_logical_layout.y + line_logical_layout.height - logical_rect.y;
            }

            y_offset = line_logical_layout.y + line_logical_layout.height + self.spacing;
        }

        if want_ink {
            self.ink_rect.set(Some(ink_rect));
            ink_out = Some(ink_rect);
        }
        if want_logical {
            self.logical_rect.set(Some(logical_rect));
            logical_out = Some(logical_rect);
        }
        (ink_out, logical_out, line_extents)
    }
}
