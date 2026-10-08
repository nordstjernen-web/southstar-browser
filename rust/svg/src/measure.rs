//! Southstar — SVG element geometry: the bounding box and the transforms to the root and to the nearest viewport.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;

use southstar_dom::{Node, children};
use southstar_glib::boolean;
use southstar_style::{StyleRef, StyleTable};

use crate::ffi::{FORMAT_A8, Matrix, OwnedCanvas, Segment, Surface, SvgGeometry};
use crate::state::{State, cmax, cmin};
use crate::{
    Ctx, HEIGHT, MAX_DEPTH, MAX_NESTING, MAX_NODES, SVG_X, SVG_Y, WIDTH, attr_length, is_element,
    name_is, never_rendered, switch_choice, tag, viewbox_matrix, viewbox_size,
};

#[derive(Default, Clone, Copy)]
struct Extent {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    any: bool,
}

impl Extent {
    fn add_point(&mut self, x: f64, y: f64) {
        if !self.any {
            *self = Extent {
                x0: x,
                y0: y,
                x1: x,
                y1: y,
                any: true,
            };
            return;
        }
        self.x0 = cmin(self.x0, x);
        self.x1 = cmax(self.x1, x);
        self.y0 = cmin(self.y0, y);
        self.y1 = cmax(self.y1, y);
    }

    fn add_mapped(&mut self, from: &Extent, to_outer: &Matrix) {
        if !from.any {
            return;
        }
        let xs = [from.x0, from.x1];
        let ys = [from.y0, from.y1];
        for corner in 0..4 {
            let (x, y) = to_outer.transform_point(xs[corner & 1], ys[corner >> 1]);
            self.add_point(x, y);
        }
    }
}

#[derive(Clone, Copy)]
struct Frame {
    to_root: Matrix,
    to_viewport: Matrix,
}

impl Frame {
    fn inside(&self, local: &Matrix) -> Frame {
        Frame {
            to_root: Matrix::multiply(local, &self.to_root),
            to_viewport: Matrix::multiply(local, &self.to_viewport),
        }
    }
}

struct Measure<'a> {
    target: Node<'a>,
    path: HashSet<*const southstar_dom::NsNode>,
    inside: bool,
    boxless: bool,
    out: SvgGeometry,
}

fn empty_geometry() -> SvgGeometry {
    SvgGeometry {
        found: 0,
        rendered: 0,
        has_box: 0,
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        to_root: Matrix::identity(),
        to_viewport: Matrix::identity(),
    }
}

fn same(a: Node<'_>, b: Node<'_>) -> bool {
    a.as_ptr() == b.as_ptr()
}

impl<'a> Measure<'a> {
    fn report(&mut self, frame: &Frame, own: &Extent) {
        let g = &mut self.out;
        g.found = boolean(true);
        g.to_root = frame.to_root;
        g.to_viewport = frame.to_viewport;
        g.has_box = boolean(own.any);
        if !own.any {
            return;
        }
        g.x = own.x0;
        g.y = own.y0;
        g.width = own.x1 - own.x0;
        g.height = own.y1 - own.y0;
    }

    fn children(
        &mut self,
        ctx: &mut Ctx<'a>,
        n: Node<'a>,
        st: &State,
        frame: &Frame,
        own: &mut Extent,
    ) {
        for c in children(n) {
            if self.out.found != 0 {
                return;
            }
            if !is_element(c) {
                continue;
            }
            if !self.inside && !self.path.contains(&c.as_ptr()) {
                continue;
            }
            self.node(ctx, c, st, frame, own);
        }
    }

    fn shape(&mut self, ctx: &mut Ctx<'a>, n: Node<'a>, st: &State, own: &mut Extent) {
        let cr = ctx.cr;
        cr.new_path();
        if ctx.shape_path(n, st) && cr.has_current_point() {
            let (x0, y0, mut x1, mut y1) = cr.path_extents();
            let (mut x0, mut y0) = (x0, y0);
            if x0 == x1 && y0 == y1 {
                if let Some((px, py)) = lone_point(ctx) {
                    x0 = px;
                    y0 = py;
                    x1 = x0;
                    y1 = y0;
                }
            }
            own.add_point(x0, y0);
            own.add_point(x1, y1);
        }
        cr.new_path();
    }

    fn positioned_box(&mut self, ctx: &mut Ctx<'a>, n: Node<'a>, st: &State, own: &mut Extent) {
        let fs = st.font_size;
        let x = ctx.geom(n, c"x", &SVG_X, ctx.vw, fs, 0.0).0;
        let y = ctx.geom(n, c"y", &SVG_Y, ctx.vh, fs, 0.0).0;
        let w = ctx.geom(n, c"width", &WIDTH, ctx.vw, fs, 0.0).0;
        let h = ctx.geom(n, c"height", &HEIGHT, ctx.vh, fs, 0.0).0;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        own.add_point(x, y);
        own.add_point(x + w, y + h);
    }

    fn text(&mut self, ctx: &mut Ctx<'a>, n: Node<'a>, st: &State, own: &mut Extent) {
        let Some((layout, x, top, w, h)) = ctx.text_layout(n, st) else {
            return;
        };
        drop(layout);
        own.add_point(x, top);
        own.add_point(x + w, top + h);
    }

    fn use_element(
        &mut self,
        ctx: &mut Ctx<'a>,
        n: Node<'a>,
        st: &State,
        frame: &Frame,
        own: &mut Extent,
    ) {
        let Some(referenced) = ctx.href_target(n).filter(|&r| !same(r, n)) else {
            return;
        };
        let fs = st.font_size;
        let mut placed = Matrix::translation(
            attr_length(n, c"x", ctx.vw, fs, 0.0),
            attr_length(n, c"y", ctx.vh, fs, 0.0),
        );
        let mut content = Extent::default();
        ctx.depth += 1;
        if name_is(referenced, b"symbol") {
            let w = attr_length(n, c"width", ctx.vw, fs, ctx.vw);
            let h = attr_length(n, c"height", ctx.vh, fs, ctx.vh);
            let fitted = viewbox_matrix(referenced, w, h);
            placed = Matrix::multiply(&fitted, &placed);
            let inner = frame.inside(&placed);
            self.children(ctx, referenced, st, &inner, &mut content);
        } else {
            let inner = frame.inside(&placed);
            self.node(ctx, referenced, st, &inner, &mut content);
        }
        ctx.depth -= 1;
        own.add_mapped(&content, &placed);
    }

    fn content(
        &mut self,
        ctx: &mut Ctx<'a>,
        n: Node<'a>,
        st: &State,
        frame: &Frame,
        own: &mut Extent,
    ) {
        let t = tag(n);
        match t {
            b"switch" => {
                if let Some(choice) = switch_choice(n) {
                    self.node(ctx, choice, st, frame, own);
                }
            }
            b"use" => self.use_element(ctx, n, st, frame, own),
            b"text" | b"tspan" => self.text(ctx, n, st, own),
            b"rect" | b"image" | b"foreignObject" => self.positioned_box(ctx, n, st, own),
            _ if matches!(t, b"g" | b"a" | b"svg") || never_rendered(t) => {
                self.children(ctx, n, st, frame, own)
            }
            _ => self.shape(ctx, n, st, own),
        }
    }

    fn toward_target(
        &mut self,
        ctx: &mut Ctx<'a>,
        n: Node<'a>,
        st: &State,
        frame: &Frame,
        own: &mut Extent,
    ) {
        if name_is(n, b"switch") {
            let choice = switch_choice(n);
            if choice.is_none_or(|c| !self.path.contains(&c.as_ptr())) {
                self.out.rendered = boolean(false);
                self.boxless = true;
            }
        }
        self.children(ctx, n, st, frame, own);
    }

    fn node_nested(
        &mut self,
        ctx: &mut Ctx<'a>,
        n: Node<'a>,
        parent: &State,
        outer: &Frame,
        outer_extent: &mut Extent,
    ) {
        let never = never_rendered(tag(n));
        let hidden = ctx.is_hidden(n);
        if never || hidden {
            if self.inside {
                return;
            }
            self.out.rendered = boolean(false);
            self.boxless = self.boxless || hidden;
        }
        let mut st = parent.clone();
        ctx.apply_node(&mut st, n);

        let (outer_vw, outer_vh) = (ctx.vw, ctx.vh);
        let (local, viewport) = local_matrix(ctx, n, &st);
        let frame = outer.inside(&local);
        let mut child_frame = frame;
        if name_is(n, b"svg") {
            child_frame.to_viewport = viewport;
        }
        let mut own = Extent::default();
        let reached = !self.inside && (same(n, self.target) || name_is(n, b"text"));
        if reached {
            self.inside = true;
        }
        if !self.inside {
            self.toward_target(ctx, n, &st, &child_frame, &mut own);
        } else if !self.boxless {
            self.content(ctx, n, &st, &child_frame, &mut own);
        }
        if reached {
            self.inside = false;
            self.report(&frame, &own);
        } else if self.inside {
            outer_extent.add_mapped(&own, &local);
        }
        ctx.vw = outer_vw;
        ctx.vh = outer_vh;
    }

    fn node(
        &mut self,
        ctx: &mut Ctx<'a>,
        n: Node<'a>,
        parent: &State,
        outer: &Frame,
        outer_extent: &mut Extent,
    ) {
        if n.name().is_none() || ctx.depth >= MAX_DEPTH || ctx.nesting >= MAX_NESTING {
            return;
        }
        ctx.nodes += 1;
        if ctx.nodes > MAX_NODES {
            return;
        }
        ctx.nesting += 1;
        self.node_nested(ctx, n, parent, outer, outer_extent);
        ctx.nesting -= 1;
    }
}

fn lone_point(ctx: &Ctx<'_>) -> Option<(f64, f64)> {
    let path = ctx.cr.copy_path()?;
    let segs = path.segments();
    if path.is_empty() {
        return None;
    }
    let mut last = None;
    for s in segs {
        match s {
            Segment::MoveTo(x, y) => last = Some((x, y)),
            _ => return None,
        }
    }
    last
}

fn local_matrix(ctx: &mut Ctx<'_>, n: Node<'_>, st: &State) -> (Matrix, Matrix) {
    let (mut local, _) = crate::parse::transform(n.attr(c"transform"));
    let mut viewport = Matrix::identity();
    if !name_is(n, b"svg") {
        return (local, viewport);
    }
    let fs = st.font_size;
    let w = attr_length(n, c"width", ctx.vw, fs, ctx.vw);
    let h = attr_length(n, c"height", ctx.vh, fs, ctx.vh);
    let placed = Matrix::translation(
        attr_length(n, c"x", ctx.vw, fs, 0.0),
        attr_length(n, c"y", ctx.vh, fs, 0.0),
    );
    viewport = viewbox_matrix(n, w, h);
    viewport = Matrix::multiply(&viewport, &placed);
    local = Matrix::multiply(&viewport, &local);
    (ctx.vw, ctx.vh) = viewbox_size(n, w, h);
    (local, viewport)
}

fn path_to<'a>(svg: Node<'a>, target: Node<'a>) -> Option<HashSet<*const southstar_dom::NsNode>> {
    let mut path = HashSet::new();
    let mut n = Some(target);
    while let Some(cur) = n.filter(|&c| !same(c, svg)) {
        path.insert(cur.as_ptr());
        n = cur.parent();
    }
    n.map(|_| path)
}

pub fn node_geometry(
    svg: Option<Node<'_>>,
    target: Option<Node<'_>>,
    width: f64,
    height: f64,
    styles: StyleTable,
    inherited: Option<StyleRef<'_>>,
) -> SvgGeometry {
    let (Some(svg), Some(target)) = (svg, target) else {
        return empty_geometry();
    };
    let Some(path) = path_to(svg, target) else {
        return empty_geometry();
    };
    let Some(surface) = Surface::image(FORMAT_A8, 1, 1) else {
        return empty_geometry();
    };
    let cr = OwnedCanvas::on(&surface);
    let mut m = Measure {
        target,
        path,
        inside: same(target, svg),
        boxless: false,
        out: empty_geometry(),
    };
    let mut ctx = Ctx::new(cr.canvas(), svg, styles, 0.0, 0.0);
    m.out.rendered = boolean(true);
    let to_root = viewbox_matrix(svg, width, height);
    let frame = Frame {
        to_root,
        to_viewport: to_root,
    };
    (ctx.vw, ctx.vh) = viewbox_size(svg, width, height);
    let mut st = State::inherited(inherited);
    ctx.apply_node(&mut st, svg);
    let mut own = Extent::default();
    m.children(&mut ctx, svg, &st, &frame, &mut own);
    if same(target, svg) {
        m.report(&frame, &own);
    }
    m.out
}
