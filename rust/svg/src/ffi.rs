//! Southstar — the cairo canvas, paths, patterns, surfaces and text layouts SVG rendering draws with, over southstar-cairo and southstar-text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int};

use southstar_cairo::{
    Context, ContextRef, Extend, FillRule, Format, LineCap, LineJoin, PathSegment,
};
use southstar_dom::Node;
use southstar_glib::GBoolean;
use southstar_text::font::CairoContext;
use southstar_text::font_description::FontDescription;
use southstar_text::layout::{Layout, shared_context};
use southstar_text::{Style, Weight};
use southstar_texture::TextureRef;

pub use southstar_cairo::Matrix;

pub const PANGO_SCALE: c_int = 1024;
pub const FORMAT_ARGB32: Format = Format::Argb32;
pub const FORMAT_A8: Format = Format::A8;
pub const CAP_BUTT: c_int = 0;
pub const CAP_ROUND: c_int = 1;
pub const CAP_SQUARE: c_int = 2;
pub const JOIN_MITER: c_int = 0;
pub const JOIN_ROUND: c_int = 1;
pub const JOIN_BEVEL: c_int = 2;
pub const FILL_WINDING: c_int = 0;
pub const FILL_EVEN_ODD: c_int = 1;
pub const EXTEND_REPEAT: Extend = Extend::Repeat;
pub const EXTEND_REFLECT: Extend = Extend::Reflect;
pub const EXTEND_PAD: Extend = Extend::Pad;

#[derive(Clone, Copy, Default)]
pub struct SvgSize {
    pub width: f64,
    pub height: f64,
    pub ratio: f64,
    pub has_width: bool,
    pub has_height: bool,
    pub has_ratio: bool,
}

#[repr(C)]
pub struct SvgGeometry {
    pub found: GBoolean,
    pub rendered: GBoolean,
    pub has_box: GBoolean,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub to_root: Matrix,
    pub to_viewport: Matrix,
}

pub fn strtod(terminated: &[u8], pos: usize) -> (f64, usize) {
    let Some(rest) = terminated.get(pos..).filter(|r| r.last() == Some(&0)) else {
        return (0.0, 0);
    };
    southstar_glib::ascii_strtod_prefix(&rest[..rest.len() - 1])
}

fn hex_value(c: u8) -> Option<u8> {
    char::from(c)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

pub fn uri_unescape(raw: &[u8]) -> Option<Vec<u8>> {
    let raw = &raw[..raw.iter().position(|&c| c == 0).unwrap_or(raw.len())];
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' {
            let high = raw.get(i + 1).copied().and_then(hex_value)?;
            let low = raw.get(i + 2).copied().and_then(hex_value)?;
            out.push(high << 4 | low);
            i += 3;
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    (!out.contains(&0)).then_some(out)
}

pub enum Segment {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    CurveTo([f64; 6]),
    Close,
}

pub struct Path(southstar_cairo::Path);

impl Path {
    pub fn ok(&self) -> bool {
        self.0.status().is_ok()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn segments(&self) -> Vec<Segment> {
        self.0
            .segments()
            .into_iter()
            .map(|segment| match segment {
                PathSegment::MoveTo(x, y) => Segment::MoveTo(x, y),
                PathSegment::LineTo(x, y) => Segment::LineTo(x, y),
                PathSegment::CurveTo(points) => Segment::CurveTo(points),
                PathSegment::ClosePath => Segment::Close,
            })
            .collect()
    }
}

pub struct Surface(southstar_cairo::Surface);

impl Surface {
    pub fn image(format: Format, w: i32, h: i32) -> Option<Surface> {
        southstar_cairo::Surface::image(format, w, h).map(Surface)
    }

    pub fn flush(&self) {
        self.0.as_ref().flush();
    }

    pub fn with_data<R>(&self, f: impl FnOnce(&[u8], usize) -> R) -> Option<R> {
        self.0.as_ref().with_data(f)
    }

    pub fn with_data_mut<R>(&self, f: impl FnOnce(&mut [u8], usize) -> R) -> Option<R> {
        self.0.as_ref().with_data_mut(f)
    }

    pub fn into_texture(self, w: i32, h: i32) -> Option<TextureRef> {
        self.with_data(|data, stride| TextureRef::new(w, h, data, stride))?
    }
}

pub struct Pattern(southstar_cairo::Pattern);

impl Pattern {
    pub fn linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Pattern {
        Pattern(southstar_cairo::Pattern::linear(x0, y0, x1, y1))
    }

    pub fn radial(fx: f64, fy: f64, cx: f64, cy: f64, r: f64) -> Pattern {
        Pattern(southstar_cairo::Pattern::radial(fx, fy, 0.0, cx, cy, r))
    }

    pub fn add_stop(&self, off: f64, r: f64, g: f64, b: f64, a: f64) {
        self.0.add_color_stop_rgba(off, r, g, b, a);
    }

    pub fn set_extend(&self, extend: Extend) {
        self.0.set_extend(extend);
    }

    pub fn set_matrix(&self, m: &Matrix) {
        self.0.set_matrix(m);
    }
}

pub struct TextLayout(RefCell<Layout>);

pub struct Font<'a> {
    pub family: Option<&'a CStr>,
    pub size: f64,
    pub weight: i32,
    pub italic: bool,
}

#[derive(Clone)]
pub struct Canvas(Context);

impl From<ContextRef<'_>> for Canvas {
    fn from(cr: ContextRef<'_>) -> Canvas {
        Canvas(cr.reference())
    }
}

fn line_cap(cap: c_int) -> LineCap {
    match cap {
        CAP_ROUND => LineCap::Round,
        CAP_SQUARE => LineCap::Square,
        _ => LineCap::Butt,
    }
}

fn line_join(join: c_int) -> LineJoin {
    match join {
        JOIN_ROUND => LineJoin::Round,
        JOIN_BEVEL => LineJoin::Bevel,
        _ => LineJoin::Miter,
    }
}

fn fill_rule(rule: c_int) -> FillRule {
    if rule == FILL_EVEN_ODD {
        FillRule::EvenOdd
    } else {
        FillRule::Winding
    }
}

impl Canvas {
    pub fn on(surface: &Surface) -> Canvas {
        Canvas(Context::new(surface.0.as_ref()))
    }

    fn cr(&self) -> ContextRef<'_> {
        self.0.as_ref()
    }

    pub fn save(&self) {
        self.cr().save();
    }

    pub fn restore(&self) {
        self.cr().restore();
    }

    pub fn new_path(&self) {
        self.cr().new_path();
    }

    pub fn new_sub_path(&self) {
        self.cr().new_sub_path();
    }

    pub fn move_to(&self, x: f64, y: f64) {
        self.cr().move_to(x, y);
    }

    pub fn line_to(&self, x: f64, y: f64) {
        self.cr().line_to(x, y);
    }

    pub fn curve_to(&self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.cr().curve_to(x1, y1, x2, y2, x3, y3);
    }

    pub fn close_path(&self) {
        self.cr().close_path();
    }

    pub fn rectangle(&self, x: f64, y: f64, w: f64, h: f64) {
        self.cr().rectangle(x, y, w, h);
    }

    pub fn arc(&self, xc: f64, yc: f64, r: f64, a1: f64, a2: f64) {
        self.cr().arc(xc, yc, r, a1, a2);
    }

    pub fn translate(&self, tx: f64, ty: f64) {
        self.cr().translate(tx, ty);
    }

    pub fn scale(&self, sx: f64, sy: f64) {
        self.cr().scale(sx, sy);
    }

    pub fn rotate(&self, angle: f64) {
        self.cr().rotate(angle);
    }

    pub fn transform(&self, m: &Matrix) {
        self.cr().transform(m);
    }

    pub fn set_matrix(&self, m: &Matrix) {
        self.cr().set_matrix(m);
    }

    pub fn matrix(&self) -> Matrix {
        self.cr().matrix()
    }

    pub fn identity_matrix(&self) {
        self.cr().identity_matrix();
    }

    pub fn clip(&self) {
        self.cr().clip();
    }

    pub fn clip_extents(&self) -> (f64, f64, f64, f64) {
        self.cr().clip_extents()
    }

    pub fn path_extents(&self) -> (f64, f64, f64, f64) {
        self.cr().path_extents()
    }

    pub fn has_current_point(&self) -> bool {
        self.cr().has_current_point()
    }

    pub fn fill_preserve(&self) {
        self.cr().fill_preserve();
    }

    pub fn stroke_preserve(&self) {
        self.cr().stroke_preserve();
    }

    pub fn set_fill_rule(&self, rule: c_int) {
        self.cr().set_fill_rule(fill_rule(rule));
    }

    pub fn set_source_rgba(&self, r: f64, g: f64, b: f64, a: f64) {
        self.cr().set_source_rgba(r, g, b, a);
    }

    pub fn set_source(&self, p: &Pattern) {
        self.cr().set_source(&p.0);
    }

    pub fn set_line_width(&self, w: f64) {
        self.cr().set_line_width(w);
    }

    pub fn set_line_cap(&self, cap: c_int) {
        self.cr().set_line_cap(line_cap(cap));
    }

    pub fn set_line_join(&self, join: c_int) {
        self.cr().set_line_join(line_join(join));
    }

    pub fn set_miter_limit(&self, limit: f64) {
        self.cr().set_miter_limit(limit);
    }

    pub fn set_dash(&self, dashes: &[f64], offset: f64) {
        self.cr().set_dash(dashes, offset);
    }

    pub fn push_group(&self) {
        self.cr().push_group();
    }

    pub fn pop_group_to_source(&self) {
        self.cr().pop_group_to_source();
    }

    pub fn paint_with_alpha(&self, alpha: f64) {
        self.cr().paint_with_alpha(alpha);
    }

    pub fn mask_surface(&self, s: &Surface) {
        self.cr().mask_surface(s.0.as_ref(), 0.0, 0.0);
    }

    pub fn copy_path(&self) -> Option<Path> {
        Some(Path(self.cr().copy_path()))
    }

    pub fn append_path(&self, p: &Path) {
        self.cr().append_path(&p.0);
    }

    pub fn target_device_offset(&self) -> (f64, f64) {
        self.cr().target().device_offset()
    }

    fn text_cairo(&self) -> CairoContext {
        CairoContext::from_cairo(self.cr())
    }

    pub fn text_layout(&self, text: &[u8], font: &Font<'_>) -> TextLayout {
        let context = southstar_text::context::Context::create_for_cairo(&self.text_cairo());
        let mut layout = Layout::new(&shared_context(context));
        let mut desc = FontDescription::new();
        let family = font.family.map(southstar_style::font_family_for_pango);
        let family = family.as_deref().unwrap_or(c"sans-serif");
        desc.set_family(Some(&family.to_string_lossy()));
        desc.set_absolute_size(f64::from(southstar_text::font_size_units(font.size)));
        desc.set_weight(Weight(font.weight));
        if font.italic {
            desc.set_style(Style::Italic);
        }
        layout.set_font_description(Some(&desc));
        layout.set_text(&String::from_utf8_lossy(text));
        TextLayout(RefCell::new(layout))
    }

    pub fn layout_path(&self, layout: &TextLayout) {
        southstar_text::render::layout_path(&self.text_cairo(), &mut layout.0.borrow_mut());
    }
}

impl TextLayout {
    pub fn pixel_size(&self) -> (i32, i32) {
        self.0.borrow_mut().pixel_size()
    }

    pub fn baseline(&self) -> i32 {
        self.0.borrow_mut().baseline()
    }
}

pub fn collect_text(n: Node<'_>) -> Vec<u8> {
    let mut text = southstar_dom::serialize::collect_text(Some(n));
    text.truncate(text.iter().position(|&b| b == 0).unwrap_or(text.len()));
    text
}

pub fn next_in_subtree<'a>(n: Node<'a>, root: Node<'a>, descend: bool) -> Option<Node<'a>> {
    southstar_dom::index::next_in_subtree(n, Some(root), descend)
}

pub fn navigator_languages() -> Vec<Vec<u8>> {
    southstar_netutil::effective_navigator_languages()
}
