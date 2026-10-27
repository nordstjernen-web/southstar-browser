//! Southstar — Cairo, southstar-text, GLib and DOM calls behind SVG rendering.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int, c_void};
use core::ptr;

use southstar_dom::Node;
use southstar_glib::GBoolean;
use southstar_text::font::CairoContext;
use southstar_text::font_description::FontDescription;
use southstar_text::fontmap::FontMap;
use southstar_text::layout::{Layout, shared_context};
use southstar_text::{Style, Weight};
use southstar_texture::TextureRef;

pub const PANGO_SCALE: c_int = 1024;
const CAIRO_STATUS_SUCCESS: c_int = 0;
pub const FORMAT_ARGB32: c_int = 0;
pub const FORMAT_A8: c_int = 2;
pub const CAP_BUTT: c_int = 0;
pub const CAP_ROUND: c_int = 1;
pub const CAP_SQUARE: c_int = 2;
pub const JOIN_MITER: c_int = 0;
pub const JOIN_ROUND: c_int = 1;
pub const JOIN_BEVEL: c_int = 2;
pub const FILL_WINDING: c_int = 0;
pub const FILL_EVEN_ODD: c_int = 1;
pub const EXTEND_REPEAT: c_int = 1;
pub const EXTEND_REFLECT: c_int = 2;
pub const EXTEND_PAD: c_int = 3;
const PATH_MOVE_TO: c_int = 0;
const PATH_LINE_TO: c_int = 1;
const PATH_CURVE_TO: c_int = 2;
const PATH_CLOSE_PATH: c_int = 3;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Matrix {
    pub xx: f64,
    pub yx: f64,
    pub xy: f64,
    pub yy: f64,
    pub x0: f64,
    pub y0: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PathHeader {
    kind: c_int,
    length: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PathPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
union PathData {
    header: PathHeader,
    point: PathPoint,
}

#[repr(C)]
struct CairoPath {
    status: c_int,
    data: *mut PathData,
    num_data: c_int,
}

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

unsafe extern "C" {
    fn cairo_save(cr: *mut c_void);
    fn cairo_restore(cr: *mut c_void);
    fn cairo_new_path(cr: *mut c_void);
    fn cairo_new_sub_path(cr: *mut c_void);
    fn cairo_move_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_line_to(cr: *mut c_void, x: f64, y: f64);
    fn cairo_curve_to(cr: *mut c_void, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64);
    fn cairo_close_path(cr: *mut c_void);
    fn cairo_rectangle(cr: *mut c_void, x: f64, y: f64, w: f64, h: f64);
    fn cairo_arc(cr: *mut c_void, xc: f64, yc: f64, r: f64, a1: f64, a2: f64);
    fn cairo_translate(cr: *mut c_void, tx: f64, ty: f64);
    fn cairo_scale(cr: *mut c_void, sx: f64, sy: f64);
    fn cairo_rotate(cr: *mut c_void, angle: f64);
    fn cairo_transform(cr: *mut c_void, m: *const Matrix);
    fn cairo_set_matrix(cr: *mut c_void, m: *const Matrix);
    fn cairo_get_matrix(cr: *mut c_void, m: *mut Matrix);
    fn cairo_identity_matrix(cr: *mut c_void);
    fn cairo_clip(cr: *mut c_void);
    fn cairo_clip_extents(cr: *mut c_void, x1: *mut f64, y1: *mut f64, x2: *mut f64, y2: *mut f64);
    fn cairo_path_extents(cr: *mut c_void, x1: *mut f64, y1: *mut f64, x2: *mut f64, y2: *mut f64);
    fn cairo_has_current_point(cr: *mut c_void) -> GBoolean;
    fn cairo_fill_preserve(cr: *mut c_void);
    fn cairo_stroke_preserve(cr: *mut c_void);
    fn cairo_set_fill_rule(cr: *mut c_void, rule: c_int);
    fn cairo_set_source_rgba(cr: *mut c_void, r: f64, g: f64, b: f64, a: f64);
    fn cairo_set_source(cr: *mut c_void, pattern: *mut c_void);
    fn cairo_set_line_width(cr: *mut c_void, w: f64);
    fn cairo_set_line_cap(cr: *mut c_void, cap: c_int);
    fn cairo_set_line_join(cr: *mut c_void, join: c_int);
    fn cairo_set_miter_limit(cr: *mut c_void, limit: f64);
    fn cairo_set_dash(cr: *mut c_void, dashes: *const f64, n: c_int, offset: f64);
    fn cairo_push_group(cr: *mut c_void);
    fn cairo_pop_group_to_source(cr: *mut c_void);
    fn cairo_paint_with_alpha(cr: *mut c_void, alpha: f64);
    fn cairo_mask_surface(cr: *mut c_void, surface: *mut c_void, x: f64, y: f64);
    fn cairo_copy_path(cr: *mut c_void) -> *mut CairoPath;
    fn cairo_append_path(cr: *mut c_void, path: *const CairoPath);
    fn cairo_path_destroy(path: *mut CairoPath);
    fn cairo_get_target(cr: *mut c_void) -> *mut c_void;
    fn cairo_create(target: *mut c_void) -> *mut c_void;
    fn cairo_destroy(cr: *mut c_void);
    fn cairo_surface_get_device_offset(s: *mut c_void, x: *mut f64, y: *mut f64);
    fn cairo_image_surface_create(format: c_int, w: c_int, h: c_int) -> *mut c_void;
    fn cairo_image_surface_get_data(s: *mut c_void) -> *mut u8;
    fn cairo_image_surface_get_stride(s: *mut c_void) -> c_int;
    fn cairo_surface_status(s: *mut c_void) -> c_int;
    fn cairo_surface_flush(s: *mut c_void);
    fn cairo_surface_mark_dirty(s: *mut c_void);
    fn cairo_surface_destroy(s: *mut c_void);
    fn cairo_pattern_create_linear(x0: f64, y0: f64, x1: f64, y1: f64) -> *mut c_void;
    fn cairo_pattern_create_radial(
        cx0: f64,
        cy0: f64,
        r0: f64,
        cx1: f64,
        cy1: f64,
        r1: f64,
    ) -> *mut c_void;
    fn cairo_pattern_add_color_stop_rgba(p: *mut c_void, off: f64, r: f64, g: f64, b: f64, a: f64);
    fn cairo_pattern_set_extend(p: *mut c_void, extend: c_int);
    fn cairo_pattern_set_matrix(p: *mut c_void, m: *const Matrix);
    fn cairo_pattern_destroy(p: *mut c_void);
    fn cairo_matrix_init(m: *mut Matrix, xx: f64, yx: f64, xy: f64, yy: f64, x0: f64, y0: f64);
    fn cairo_matrix_init_identity(m: *mut Matrix);
    fn cairo_matrix_init_translate(m: *mut Matrix, tx: f64, ty: f64);
    fn cairo_matrix_init_scale(m: *mut Matrix, sx: f64, sy: f64);
    fn cairo_matrix_init_rotate(m: *mut Matrix, radians: f64);
    fn cairo_matrix_translate(m: *mut Matrix, tx: f64, ty: f64);
    fn cairo_matrix_scale(m: *mut Matrix, sx: f64, sy: f64);
    fn cairo_matrix_rotate(m: *mut Matrix, radians: f64);
    fn cairo_matrix_multiply(r: *mut Matrix, a: *const Matrix, b: *const Matrix);
    fn cairo_matrix_invert(m: *mut Matrix) -> c_int;
    fn cairo_matrix_transform_point(m: *const Matrix, x: *mut f64, y: *mut f64);
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

impl Matrix {
    pub fn identity() -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init_identity(&mut m) };
        m
    }

    fn zero() -> Matrix {
        Matrix {
            xx: 0.0,
            yx: 0.0,
            xy: 0.0,
            yy: 0.0,
            x0: 0.0,
            y0: 0.0,
        }
    }

    pub fn new(xx: f64, yx: f64, xy: f64, yy: f64, x0: f64, y0: f64) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init(&mut m, xx, yx, xy, yy, x0, y0) };
        m
    }

    pub fn translation(tx: f64, ty: f64) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init_translate(&mut m, tx, ty) };
        m
    }

    pub fn scaling(sx: f64, sy: f64) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init_scale(&mut m, sx, sy) };
        m
    }

    pub fn rotation(radians: f64) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init_rotate(&mut m, radians) };
        m
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        unsafe { cairo_matrix_translate(self, tx, ty) };
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        unsafe { cairo_matrix_scale(self, sx, sy) };
    }

    pub fn rotate(&mut self, radians: f64) {
        unsafe { cairo_matrix_rotate(self, radians) };
    }

    pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
        let mut r = Matrix::zero();
        unsafe { cairo_matrix_multiply(&mut r, a, b) };
        r
    }

    pub fn invert(&mut self) -> bool {
        unsafe { cairo_matrix_invert(self) == CAIRO_STATUS_SUCCESS }
    }

    pub fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        let (mut x, mut y) = (x, y);
        unsafe { cairo_matrix_transform_point(self, &mut x, &mut y) };
        (x, y)
    }
}

pub enum Segment {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    CurveTo([f64; 6]),
    Close,
    Other,
}

pub struct Path(*mut CairoPath);

impl Path {
    pub fn ok(&self) -> bool {
        unsafe { (*self.0).status == CAIRO_STATUS_SUCCESS }
    }

    pub fn is_empty(&self) -> bool {
        unsafe { (*self.0).num_data <= 0 }
    }

    pub fn segments(&self) -> Vec<Segment> {
        let (data, n) = unsafe { ((*self.0).data, (*self.0).num_data) };
        let mut out = Vec::new();
        let mut i = 0;
        while i < n {
            let at = |k: c_int| unsafe { (*data.add((i + k) as usize)).point };
            let header = unsafe { (*data.add(i as usize)).header };
            out.push(match header.kind {
                PATH_MOVE_TO => Segment::MoveTo(at(1).x, at(1).y),
                PATH_LINE_TO => Segment::LineTo(at(1).x, at(1).y),
                PATH_CURVE_TO => {
                    Segment::CurveTo([at(1).x, at(1).y, at(2).x, at(2).y, at(3).x, at(3).y])
                }
                PATH_CLOSE_PATH => Segment::Close,
                _ => Segment::Other,
            });
            i += header.length;
        }
        out
    }
}

impl Drop for Path {
    fn drop(&mut self) {
        unsafe { cairo_path_destroy(self.0) };
    }
}

pub struct Surface(*mut c_void);

impl Surface {
    pub fn image(format: c_int, w: i32, h: i32) -> Option<Surface> {
        let s = Surface(unsafe { cairo_image_surface_create(format, w, h) });
        (unsafe { cairo_surface_status(s.0) } == CAIRO_STATUS_SUCCESS).then_some(s)
    }

    pub fn flush(&self) {
        unsafe { cairo_surface_flush(self.0) };
    }

    pub fn mark_dirty(&self) {
        unsafe { cairo_surface_mark_dirty(self.0) };
    }

    pub fn stride(&self) -> usize {
        unsafe { cairo_image_surface_get_stride(self.0) as usize }
    }

    pub fn data(&mut self, height: usize) -> Option<&mut [u8]> {
        let p = unsafe { cairo_image_surface_get_data(self.0) };
        (!p.is_null())
            .then(|| unsafe { core::slice::from_raw_parts_mut(p, self.stride() * height) })
    }

    pub fn into_texture(mut self, w: i32, h: i32) -> Option<TextureRef> {
        let stride = self.stride();
        TextureRef::new(w, h, self.data(usize::try_from(h).ok()?)?, stride)
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { cairo_surface_destroy(self.0) };
    }
}

pub struct Pattern(*mut c_void);

impl Pattern {
    pub fn linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_linear(x0, y0, x1, y1) })
    }

    pub fn radial(fx: f64, fy: f64, cx: f64, cy: f64, r: f64) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_radial(fx, fy, 0.0, cx, cy, r) })
    }

    pub fn add_stop(&self, off: f64, r: f64, g: f64, b: f64, a: f64) {
        unsafe { cairo_pattern_add_color_stop_rgba(self.0, off, r, g, b, a) };
    }

    pub fn set_extend(&self, extend: c_int) {
        unsafe { cairo_pattern_set_extend(self.0, extend) };
    }

    pub fn set_matrix(&self, m: &Matrix) {
        unsafe { cairo_pattern_set_matrix(self.0, m) };
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        unsafe { cairo_pattern_destroy(self.0) };
    }
}

pub struct TextLayout(RefCell<Layout>);

pub struct Font<'a> {
    pub family: Option<&'a CStr>,
    pub size: f64,
    pub weight: i32,
    pub italic: bool,
}

#[derive(Clone, Copy)]
pub struct Canvas(*mut c_void);

pub struct OwnedCanvas(Canvas);

impl OwnedCanvas {
    pub fn on(surface: &Surface) -> OwnedCanvas {
        OwnedCanvas(Canvas(unsafe { cairo_create(surface.0) }))
    }

    pub fn canvas(&self) -> Canvas {
        self.0
    }
}

impl Drop for OwnedCanvas {
    fn drop(&mut self) {
        unsafe { cairo_destroy(self.0.0) };
    }
}

impl Canvas {
    /// # Safety
    ///
    /// `cr` is a live cairo context that outlives every use of the returned canvas.
    pub unsafe fn from_raw(cr: *mut c_void) -> Canvas {
        Canvas(cr)
    }

    pub fn save(self) {
        unsafe { cairo_save(self.0) };
    }

    pub fn restore(self) {
        unsafe { cairo_restore(self.0) };
    }

    pub fn new_path(self) {
        unsafe { cairo_new_path(self.0) };
    }

    pub fn new_sub_path(self) {
        unsafe { cairo_new_sub_path(self.0) };
    }

    pub fn move_to(self, x: f64, y: f64) {
        unsafe { cairo_move_to(self.0, x, y) };
    }

    pub fn line_to(self, x: f64, y: f64) {
        unsafe { cairo_line_to(self.0, x, y) };
    }

    pub fn curve_to(self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        unsafe { cairo_curve_to(self.0, x1, y1, x2, y2, x3, y3) };
    }

    pub fn close_path(self) {
        unsafe { cairo_close_path(self.0) };
    }

    pub fn rectangle(self, x: f64, y: f64, w: f64, h: f64) {
        unsafe { cairo_rectangle(self.0, x, y, w, h) };
    }

    pub fn arc(self, xc: f64, yc: f64, r: f64, a1: f64, a2: f64) {
        unsafe { cairo_arc(self.0, xc, yc, r, a1, a2) };
    }

    pub fn translate(self, tx: f64, ty: f64) {
        unsafe { cairo_translate(self.0, tx, ty) };
    }

    pub fn scale(self, sx: f64, sy: f64) {
        unsafe { cairo_scale(self.0, sx, sy) };
    }

    pub fn rotate(self, angle: f64) {
        unsafe { cairo_rotate(self.0, angle) };
    }

    pub fn transform(self, m: &Matrix) {
        unsafe { cairo_transform(self.0, m) };
    }

    pub fn set_matrix(self, m: &Matrix) {
        unsafe { cairo_set_matrix(self.0, m) };
    }

    pub fn matrix(self) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_get_matrix(self.0, &mut m) };
        m
    }

    pub fn identity_matrix(self) {
        unsafe { cairo_identity_matrix(self.0) };
    }

    pub fn clip(self) {
        unsafe { cairo_clip(self.0) };
    }

    pub fn clip_extents(self) -> (f64, f64, f64, f64) {
        let (mut a, mut b, mut c, mut d) = (0.0, 0.0, 0.0, 0.0);
        unsafe { cairo_clip_extents(self.0, &mut a, &mut b, &mut c, &mut d) };
        (a, b, c, d)
    }

    pub fn path_extents(self) -> (f64, f64, f64, f64) {
        let (mut a, mut b, mut c, mut d) = (0.0, 0.0, 0.0, 0.0);
        unsafe { cairo_path_extents(self.0, &mut a, &mut b, &mut c, &mut d) };
        (a, b, c, d)
    }

    pub fn has_current_point(self) -> bool {
        unsafe { cairo_has_current_point(self.0) != 0 }
    }

    pub fn fill_preserve(self) {
        unsafe { cairo_fill_preserve(self.0) };
    }

    pub fn stroke_preserve(self) {
        unsafe { cairo_stroke_preserve(self.0) };
    }

    pub fn set_fill_rule(self, rule: c_int) {
        unsafe { cairo_set_fill_rule(self.0, rule) };
    }

    pub fn set_source_rgba(self, r: f64, g: f64, b: f64, a: f64) {
        unsafe { cairo_set_source_rgba(self.0, r, g, b, a) };
    }

    pub fn set_source(self, p: &Pattern) {
        unsafe { cairo_set_source(self.0, p.0) };
    }

    pub fn set_line_width(self, w: f64) {
        unsafe { cairo_set_line_width(self.0, w) };
    }

    pub fn set_line_cap(self, cap: c_int) {
        unsafe { cairo_set_line_cap(self.0, cap) };
    }

    pub fn set_line_join(self, join: c_int) {
        unsafe { cairo_set_line_join(self.0, join) };
    }

    pub fn set_miter_limit(self, limit: f64) {
        unsafe { cairo_set_miter_limit(self.0, limit) };
    }

    pub fn set_dash(self, dashes: &[f64], offset: f64) {
        let p = if dashes.is_empty() {
            ptr::null()
        } else {
            dashes.as_ptr()
        };
        unsafe { cairo_set_dash(self.0, p, dashes.len() as c_int, offset) };
    }

    pub fn push_group(self) {
        unsafe { cairo_push_group(self.0) };
    }

    pub fn pop_group_to_source(self) {
        unsafe { cairo_pop_group_to_source(self.0) };
    }

    pub fn paint_with_alpha(self, alpha: f64) {
        unsafe { cairo_paint_with_alpha(self.0, alpha) };
    }

    pub fn mask_surface(self, s: &Surface) {
        unsafe { cairo_mask_surface(self.0, s.0, 0.0, 0.0) };
    }

    pub fn copy_path(self) -> Option<Path> {
        let p = unsafe { cairo_copy_path(self.0) };
        (!p.is_null()).then(|| Path(p))
    }

    pub fn append_path(self, p: &Path) {
        unsafe { cairo_append_path(self.0, p.0) };
    }

    pub fn target_device_offset(self) -> (f64, f64) {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { cairo_surface_get_device_offset(cairo_get_target(self.0), &mut x, &mut y) };
        (x, y)
    }

    fn text_cairo(self) -> Option<CairoContext> {
        // SAFETY: a Canvas wraps the live cairo_t SVG rendering draws on.
        unsafe { CairoContext::from_raw(self.0) }
    }

    pub fn text_layout(self, text: &[u8], font: &Font<'_>) -> TextLayout {
        let context = match self.text_cairo() {
            Some(cr) => southstar_text::context::Context::create_for_cairo(&cr),
            None => FontMap::default_for_thread().create_context(),
        };
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

    pub fn layout_path(self, layout: &TextLayout) {
        if let Some(cr) = self.text_cairo() {
            southstar_text::render::layout_path(&cr, &mut layout.0.borrow_mut());
        }
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
