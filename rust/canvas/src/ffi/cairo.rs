//! Southstar — the canvas's drawing types over southstar-cairo: a counted context, image and recording surfaces, patterns and paths.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_cairo as cairo;

pub(crate) use cairo::{Extend, Filter, LineCap, LineJoin};

fn matrix(m: [f64; 6]) -> cairo::Matrix {
    cairo::Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5])
}

pub(crate) struct Pattern(cairo::Pattern);

impl Pattern {
    pub fn linear(p0: (f64, f64), p1: (f64, f64)) -> Pattern {
        Pattern(cairo::Pattern::linear(p0.0, p0.1, p1.0, p1.1))
    }

    pub fn radial(c0: (f64, f64, f64), c1: (f64, f64, f64)) -> Pattern {
        Pattern(cairo::Pattern::radial(c0.0, c0.1, c0.2, c1.0, c1.1, c1.2))
    }

    pub fn for_surface(surface: &Surface) -> Pattern {
        Pattern(cairo::Pattern::for_surface(surface.0.as_ref()))
    }

    pub fn conic(center: (f64, f64), sectors: &[crate::raster::Sector]) -> Pattern {
        let pattern = cairo::Pattern::mesh();
        for sector in sectors {
            pattern.begin_patch();
            pattern.mesh_move_to(center.0, center.1);
            pattern.mesh_line_to(sector.edges[0].0, sector.edges[0].1);
            pattern.mesh_line_to(sector.edges[1].0, sector.edges[1].1);
            pattern.mesh_line_to(center.0, center.1);
            for (corner, color) in [(0, 0), (1, 0), (2, 1), (3, 1)] {
                let [r, g, b, a] = sector.colors[color];
                pattern.set_corner_color_rgba(corner, r, g, b, a);
            }
            pattern.end_patch();
        }
        Pattern(pattern)
    }

    pub fn set_extend(&self, extend: Extend) {
        self.0.set_extend(extend);
    }

    pub fn set_inverse_matrix(&self, m: [f64; 6]) {
        let mut inverse = matrix(m);
        if inverse.invert().is_ok() {
            self.0.set_matrix(&inverse);
        }
    }

    pub fn add_color_stop(&self, offset: f64, rgba: [f64; 4]) {
        self.0
            .add_color_stop_rgba(offset, rgba[0], rgba[1], rgba[2], rgba[3]);
    }
}

pub(crate) struct Surface(cairo::Surface);

impl Surface {
    pub fn of(surface: cairo::Surface) -> Surface {
        Surface(surface)
    }

    pub fn reference(&self) -> Surface {
        Surface(self.0.clone())
    }

    pub fn into_shared(self) -> cairo::Surface {
        self.0
    }

    pub fn image(width: i32, height: i32) -> Surface {
        Surface(cairo::Surface::image_unchecked(
            cairo::Format::Argb32,
            width,
            height,
        ))
    }

    pub fn is_ok(&self) -> bool {
        self.0.as_ref().status().is_ok()
    }

    pub fn size(&self) -> (i32, i32) {
        let surface = self.0.as_ref();
        (surface.width(), surface.height())
    }

    pub fn read_pixels<R>(&self, f: impl FnOnce(&[u8], usize, (i32, i32)) -> R) -> Option<R> {
        let size = self.size();
        self.0
            .as_ref()
            .with_data(|data, stride| f(data, stride, size))
    }

    pub fn write_pixels(&self, f: impl FnOnce(&mut [u8], usize)) {
        self.0.as_ref().with_data_mut(f);
    }

    pub fn png(&self) -> Option<Vec<u8>> {
        self.0.as_ref().to_png().ok()
    }

    pub fn fill_opaque_black(&self) {
        let cr = cairo::Context::new(self.0.as_ref());
        let cr = cr.as_ref();
        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        cr.paint();
    }

    pub fn clear(&self) {
        let cr = cairo::Context::new(self.0.as_ref());
        let cr = cr.as_ref();
        cr.set_operator(cairo::Operator::Clear);
        cr.paint();
    }

    pub fn paint_onto(&self, target: &Surface, offset: (f64, f64), replace: bool) {
        let cr = cairo::Context::new(target.0.as_ref());
        let cr = cr.as_ref();
        cr.set_source_surface(self.0.as_ref(), offset.0, offset.1);
        if replace {
            cr.set_operator(cairo::Operator::Source);
        }
        cr.paint();
    }
}

#[derive(Clone)]
pub(crate) struct Context(cairo::Context);

pub(crate) struct Path(cairo::Path);

impl Path {
    pub fn is_ok(&self) -> bool {
        self.0.status().is_ok()
    }

    pub fn transform(&mut self, m: [f64; 6]) {
        self.0.transform(&matrix(m));
    }
}

pub(crate) struct Recording {
    _surface: cairo::Surface,
    cr: Context,
}

impl Recording {
    pub fn new() -> Recording {
        let surface = cairo::Surface::recording(cairo::Content::ColorAlpha, None);
        let cr = Context::on(&Surface(surface.clone()));
        Recording {
            _surface: surface,
            cr,
        }
    }

    pub fn context(&self) -> Context {
        self.cr.clone()
    }
}

impl Context {
    pub fn on(surface: &Surface) -> Context {
        Context(cairo::Context::new(surface.0.as_ref()))
    }

    fn cr(&self) -> cairo::ContextRef<'_> {
        self.0.as_ref()
    }

    pub fn same(&self, other: &Context) -> bool {
        self.cr() == other.cr()
    }

    pub fn cairo(&self) -> cairo::ContextRef<'_> {
        self.0.as_ref()
    }

    pub fn clip(&self) {
        self.cr().clip();
    }

    pub fn paint_with_alpha(&self, alpha: f64) {
        self.cr().paint_with_alpha(alpha);
    }

    pub fn set_source_filter(&self, filter: Filter) {
        self.cr().set_source_filter(filter);
    }

    pub fn clip_preserve(&self) {
        self.cr().clip_preserve();
    }

    pub fn reset_clip(&self) {
        self.cr().reset_clip();
    }

    pub fn in_fill(&self, x: f64, y: f64) -> bool {
        self.cr().in_fill(x, y)
    }

    pub fn in_stroke(&self, x: f64, y: f64) -> bool {
        self.cr().in_stroke(x, y)
    }

    pub fn move_to(&self, x: f64, y: f64) {
        self.cr().move_to(x, y);
    }

    pub fn line_to(&self, x: f64, y: f64) {
        self.cr().line_to(x, y);
    }

    pub fn curve_to(&self, c1: (f64, f64), c2: (f64, f64), to: (f64, f64)) {
        self.cr().curve_to(c1.0, c1.1, c2.0, c2.1, to.0, to.1);
    }

    pub fn close_path(&self) {
        self.cr().close_path();
    }

    pub fn new_path(&self) {
        self.cr().new_path();
    }

    pub fn new_sub_path(&self) {
        self.cr().new_sub_path();
    }

    pub fn save(&self) {
        self.cr().save();
    }

    pub fn restore(&self) {
        self.cr().restore();
    }

    pub fn translate(&self, x: f64, y: f64) {
        self.cr().translate(x, y);
    }

    pub fn rotate(&self, angle: f64) {
        self.cr().rotate(angle);
    }

    pub fn scale(&self, x: f64, y: f64) {
        self.cr().scale(x, y);
    }

    pub fn set_source_pattern(&self, pattern: &Pattern) {
        self.cr().set_source(&pattern.0);
    }

    pub fn set_source_rgba(&self, rgba: [f64; 4]) {
        self.cr()
            .set_source_rgba(rgba[0], rgba[1], rgba[2], rgba[3]);
    }

    pub fn set_source_surface(&self, surface: &Surface, x: f64, y: f64) {
        self.cr().set_source_surface(surface.0.as_ref(), x, y);
    }

    pub fn paint(&self) {
        self.cr().paint();
    }

    pub fn set_line_width(&self, width: f64) {
        self.cr().set_line_width(width);
    }

    pub fn set_fill_rule(&self, rule: i32) {
        self.cr().set_fill_rule(cairo::FillRule::from_raw(rule));
    }

    pub fn fill_rule(&self) -> i32 {
        self.cr().fill_rule() as i32
    }

    pub fn fill(&self) {
        self.cr().fill();
    }

    pub fn stroke(&self) {
        self.cr().stroke();
    }

    pub fn fill_preserve(&self) {
        self.cr().fill_preserve();
    }

    pub fn stroke_preserve(&self) {
        self.cr().stroke_preserve();
    }

    pub fn set_operator(&self, op: i32) {
        self.cr().set_operator(cairo::Operator::from_raw(op));
    }

    pub fn set_line_cap(&self, cap: LineCap) {
        self.cr().set_line_cap(cap);
    }

    pub fn set_line_join(&self, join: LineJoin) {
        self.cr().set_line_join(join);
    }

    pub fn set_miter_limit(&self, limit: f64) {
        self.cr().set_miter_limit(limit);
    }

    pub fn set_dash(&self, dashes: &[f64], offset: f64) {
        self.cr().set_dash(dashes, offset);
    }

    pub fn transform(&self, m: [f64; 6]) {
        self.cr().transform(&matrix(m));
    }

    pub fn set_matrix(&self, m: [f64; 6]) {
        self.cr().set_matrix(&matrix(m));
    }

    pub fn matrix(&self) -> [f64; 6] {
        self.cr().matrix().to_array()
    }

    pub fn identity_matrix(&self) {
        self.cr().identity_matrix();
    }

    pub fn rectangle(&self, x: f64, y: f64, w: f64, h: f64) {
        self.cr().rectangle(x, y, w, h);
    }

    pub fn arc(&self, center: (f64, f64), radius: f64, angles: (f64, f64), negative: bool) {
        let (xc, yc) = center;
        let (a1, a2) = angles;
        if negative {
            self.cr().arc_negative(xc, yc, radius, a1, a2);
        } else {
            self.cr().arc(xc, yc, radius, a1, a2);
        }
    }

    pub fn has_current_point(&self) -> bool {
        self.cr().has_current_point()
    }

    pub fn current_point(&self) -> (f64, f64) {
        self.cr().current_point()
    }

    pub fn copy_path(&self) -> Path {
        Path(self.cr().copy_path())
    }

    pub fn append_path(&self, path: &Path) {
        if path.is_ok() {
            self.cr().append_path(&path.0);
        }
    }
}

impl crate::path::PathSink for Context {
    fn move_to(&mut self, x: f64, y: f64) {
        Context::move_to(self, x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        Context::line_to(self, x, y);
    }

    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        Context::curve_to(self, (x1, y1), (x2, y2), (x, y));
    }

    fn close_path(&mut self) {
        Context::close_path(self);
    }

    fn unit_arc(
        &mut self,
        center: (f64, f64),
        phi: f64,
        radii: (f64, f64),
        angles: (f64, f64),
        sweep: bool,
    ) {
        self.save();
        self.translate(center.0, center.1);
        self.rotate(phi);
        self.scale(radii.0, radii.1);
        self.arc((0.0, 0.0), 1.0, angles, !sweep);
        self.restore();
    }
}
