//! Southstar — SVG rendering onto Cairo, sharing the CSS cascade with HTML.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod measure;
mod parse;
mod state;

use core::f64::consts::PI;
use std::collections::HashMap;

use core::ffi::CStr;

pub use ffi::{Matrix, SvgGeometry};
pub use measure::node_geometry;

use ffi::{
    Canvas, EXTEND_PAD, EXTEND_REFLECT, EXTEND_REPEAT, FORMAT_A8, FORMAT_ARGB32, OwnedCanvas,
    PANGO_SCALE, Pattern, Segment, Surface, SvgSize, TextLayout, Texture,
};
use parse::{eq_ci, is_ws};
use southstar_dom::{Kind, Node, children};
use southstar_glib::boolean;
use southstar_style::{Prop, StyleRef, StyleTable, Value, parse_color};
use state::{
    DISPLAY, OPACITY, PaintKind, STOP_COLOR, STOP_OPACITY, State, clamp, cmax, cmin, css_number,
    diag_basis, unit_rgba,
};

const MAX_DEPTH: i32 = 24;
const MAX_NESTING: i32 = 256;
const MAX_NODES: i32 = 60000;
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIM_PX: f64 = 8192.0;
const MAX_PIXELS: f64 = 4096.0 * 4096.0;
const DEFAULT_DIM_PX: f64 = 512.0;
const DEFAULT_OBJECT_W: f64 = 300.0;
const DEFAULT_OBJECT_H: f64 = 150.0;
const MAX_HREF_CHAIN: usize = 16;

static SVG_X: Prop = Prop::new(c"x");
static SVG_Y: Prop = Prop::new(c"y");
static WIDTH: Prop = Prop::new(c"width");
static HEIGHT: Prop = Prop::new(c"height");
static RX: Prop = Prop::new(c"rx");
static RY: Prop = Prop::new(c"ry");
static CX: Prop = Prop::new(c"cx");
static CY: Prop = Prop::new(c"cy");
static R: Prop = Prop::new(c"r");

pub struct Ctx<'a> {
    cr: Canvas,
    root: Node<'a>,
    styles: StyleTable,
    ids: Option<HashMap<&'a [u8], Node<'a>>>,
    depth: i32,
    nesting: i32,
    nodes: i32,
    vw: f64,
    vh: f64,
}

fn name_is(n: Node<'_>, tag: &[u8]) -> bool {
    n.name().is_some_and(|name| name.to_bytes() == tag)
}

fn tag(n: Node<'_>) -> &[u8] {
    n.name().map_or(&[][..], |name| name.to_bytes())
}

fn is_element(n: Node<'_>) -> bool {
    n.kind() == Kind::Element
}

fn attr_ci(n: Node<'_>, name: &CStr, value: &[u8]) -> bool {
    n.attr(name).is_some_and(|v| eq_ci(v.to_bytes(), value))
}

fn attr_length(n: Node<'_>, name: &CStr, basis: f64, fs: f64, fallback: f64) -> f64 {
    match n.attr(name) {
        Some(s) => parse::length(Some(s), basis, fs, fallback),
        None => fallback,
    }
}

fn viewbox_matrix(n: Node<'_>, vw: f64, vh: f64) -> Matrix {
    let mut out = Matrix::identity();
    let Some([x, y, w, h]) = parse::viewbox(n.attr(c"viewBox")) else {
        return out;
    };
    if w <= 0.0 || h <= 0.0 || vw <= 0.0 || vh <= 0.0 {
        return out;
    }
    let (mut slice, mut none) = (false, false);
    let (mut ax, mut ay) = (0.5, 0.5);
    if let Some(par) = n.attr(c"preserveAspectRatio") {
        for part in par
            .to_bytes()
            .split(|c| matches!(c, b' ' | b'\t' | b'\r' | b'\n'))
        {
            if part.is_empty() {
                continue;
            }
            let has = |needle: &[u8]| part.windows(needle.len()).any(|w| w == needle);
            if eq_ci(part, b"none") {
                none = true;
            } else if eq_ci(part, b"slice") {
                slice = true;
            } else if part[0].eq_ignore_ascii_case(&b'x') {
                if has(b"xMin") {
                    ax = 0.0;
                } else if has(b"xMax") {
                    ax = 1.0;
                }
                if has(b"YMin") {
                    ay = 0.0;
                } else if has(b"YMax") {
                    ay = 1.0;
                }
            }
        }
    }
    let (mut sx, mut sy) = (vw / w, vh / h);
    if !none {
        let s = if slice { cmax(sx, sy) } else { cmin(sx, sy) };
        sx = s;
        sy = s;
    }
    let tx = (vw - w * sx) * ax;
    let ty = (vh - h * sy) * ay;
    out = Matrix::translation(tx, ty);
    out.scale(sx, sy);
    out.translate(-x, -y);
    out
}

fn viewbox_size(n: Node<'_>, w: f64, h: f64) -> (f64, f64) {
    parse::viewbox_size(n.attr(c"viewBox")).unwrap_or((w, h))
}

fn g_strip(s: &[u8]) -> &[u8] {
    let sp = |c: &u8| matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r');
    let start = s.iter().position(|c| !sp(c)).unwrap_or(s.len());
    let end = s.iter().rposition(|c| !sp(c)).map_or(start, |e| e + 1);
    &s[start..end.max(start)]
}

fn language_preferred(tag: &[u8], preferred: &[Vec<u8>]) -> bool {
    preferred.iter().any(|p| {
        let len = p.len();
        len != 0
            && len <= tag.len()
            && tag[..len].eq_ignore_ascii_case(p)
            && (tag.len() == len || tag[len] == b'-')
    })
}

fn system_language_allows(n: Node<'_>) -> bool {
    let Some(value) = n.attr(c"systemLanguage") else {
        return true;
    };
    let preferred = ffi::navigator_languages();
    let value = value.to_bytes();
    if value.is_empty() {
        return false;
    }
    value
        .split(|&c| c == b',')
        .any(|t| language_preferred(g_strip(t), &preferred))
}

fn never_rendered(tag: &[u8]) -> bool {
    const TAGS: [&[u8]; 14] = [
        b"defs",
        b"symbol",
        b"title",
        b"desc",
        b"metadata",
        b"style",
        b"script",
        b"clipPath",
        b"mask",
        b"marker",
        b"pattern",
        b"filter",
        b"linearGradient",
        b"radialGradient",
    ];
    TAGS.contains(&tag)
}

fn switch_choice(n: Node<'_>) -> Option<Node<'_>> {
    children(n).find(|&c| {
        is_element(c)
            && c.attr(c"requiredExtensions").is_none()
            && c.attr(c"requiredFeatures").is_none()
            && system_language_allows(c)
    })
}

fn extend_of(s: Option<&CStr>) -> i32 {
    match s.map(CStr::to_bytes) {
        Some(s) if eq_ci(s, b"reflect") => EXTEND_REFLECT,
        Some(s) if eq_ci(s, b"repeat") => EXTEND_REPEAT,
        _ => EXTEND_PAD,
    }
}

struct Vertex {
    x: f64,
    y: f64,
    in_angle: f64,
    out_angle: f64,
    has_in: bool,
    has_out: bool,
}

fn vertex(x: f64, y: f64) -> Vertex {
    Vertex {
        x,
        y,
        in_angle: 0.0,
        out_angle: 0.0,
        has_in: false,
        has_out: false,
    }
}

fn vertex_link(out: &mut [Vertex], fx: f64, fy: f64, tx: f64, ty: f64) {
    let Some(prev) = out.last_mut() else {
        return;
    };
    let (dx, dy) = (tx - fx, ty - fy);
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    if !prev.has_out {
        prev.out_angle = dy.atan2(dx);
        prev.has_out = true;
    }
}

fn path_vertices(cr: Canvas) -> Option<Vec<Vertex>> {
    let path = cr.copy_path()?;
    if !path.ok() {
        return None;
    }
    let mut out: Vec<Vertex> = Vec::new();
    let (mut cx, mut cy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
    for seg in path.segments() {
        match seg {
            Segment::MoveTo(x, y) => {
                (cx, sx, cy, sy) = (x, x, y, y);
                out.push(vertex(cx, cy));
            }
            Segment::LineTo(x, y) => {
                vertex_link(&mut out, cx, cy, x, y);
                out.push(vertex(x, y));
                if out.len() >= 2 && (x != cx || y != cy) {
                    let v = out.last_mut().expect("vertex");
                    v.in_angle = (y - cy).atan2(x - cx);
                    v.has_in = true;
                }
                (cx, cy) = (x, y);
            }
            Segment::CurveTo([c1x, c1y, c2x, c2y, ex, ey]) => {
                vertex_link(&mut out, cx, cy, c1x, c1y);
                out.push(vertex(ex, ey));
                let v = out.last_mut().expect("vertex");
                let (mut tx, mut ty) = (ex - c2x, ey - c2y);
                if tx == 0.0 && ty == 0.0 {
                    (tx, ty) = (ex - c1x, ey - c1y);
                }
                if tx == 0.0 && ty == 0.0 {
                    (tx, ty) = (ex - cx, ey - cy);
                }
                if tx != 0.0 || ty != 0.0 {
                    v.in_angle = ty.atan2(tx);
                    v.has_in = true;
                }
                (cx, cy) = (ex, ey);
            }
            Segment::Close => {
                vertex_link(&mut out, cx, cy, sx, sy);
                out.push(vertex(sx, sy));
                if sx != cx || sy != cy {
                    let v = out.last_mut().expect("vertex");
                    v.in_angle = (sy - cy).atan2(sx - cx);
                    v.has_in = true;
                }
                (cx, cy) = (sx, sy);
            }
            Segment::Other => {}
        }
    }
    Some(out)
}

fn collapsed_text(n: Node<'_>) -> Option<Vec<u8>> {
    let text = ffi::collect_text(n)?;
    let mut flat = Vec::with_capacity(text.to_bytes().len());
    let mut prev_ws = true;
    for &c in text.to_bytes() {
        if is_ws(c) {
            if !prev_ws {
                flat.push(b' ');
            }
            prev_ws = true;
        } else {
            flat.push(c);
            prev_ws = false;
        }
    }
    while flat.last() == Some(&b' ') {
        flat.pop();
    }
    (!flat.is_empty()).then_some(flat)
}

impl<'a> Ctx<'a> {
    fn new(cr: Canvas, root: Node<'a>, styles: StyleTable, vw: f64, vh: f64) -> Ctx<'a> {
        Ctx {
            cr,
            root,
            styles,
            ids: None,
            depth: 0,
            nesting: 0,
            nodes: 0,
            vw,
            vh,
        }
    }

    fn by_id(&mut self, id: &[u8]) -> Option<Node<'a>> {
        if id.is_empty() {
            return None;
        }
        let root = self.root;
        let ids = self.ids.get_or_insert_with(|| {
            let mut ids = HashMap::new();
            if let Some(rid) = root
                .attr(c"id")
                .map(CStr::to_bytes)
                .filter(|r| !r.is_empty())
            {
                ids.insert(rid, root);
            }
            let mut c = root.first_child();
            while let Some(n) = c {
                if is_element(n)
                    && let Some(id) = n.attr(c"id").map(CStr::to_bytes).filter(|r| !r.is_empty())
                {
                    ids.entry(id).or_insert(n);
                }
                c = ffi::next_in_subtree(n, root, is_element(n));
            }
            ids
        });
        ids.get(id).copied()
    }

    fn href_target(&mut self, n: Node<'a>) -> Option<Node<'a>> {
        let h = n
            .attr(c"href")
            .or_else(|| n.attr(c"xlink:href"))?
            .to_bytes();
        let start = h.iter().position(|&c| !is_ws(c)).unwrap_or(h.len());
        let h = &h[start..];
        if h.first() != Some(&b'#') {
            return None;
        }
        self.by_id(&h[1..])
    }

    fn inherited_attr(&mut self, n: Node<'a>, name: &CStr) -> Option<&'a CStr> {
        let mut cur = Some(n);
        for _ in 0..MAX_HREF_CHAIN {
            let c = cur?;
            if let Some(v) = c.attr(name) {
                return Some(v);
            }
            cur = self.href_target(c);
        }
        None
    }

    fn stops_owner(&mut self, n: Node<'a>) -> Option<Node<'a>> {
        let mut cur = Some(n);
        for _ in 0..MAX_HREF_CHAIN {
            let c = cur?;
            if children(c).any(|s| is_element(s) && name_is(s, b"stop")) {
                return Some(c);
            }
            cur = self.href_target(c);
        }
        None
    }

    fn url_target(&mut self, n: Node<'a>, prop: &CStr, tag: &[u8]) -> Option<Node<'a>> {
        let v = self.prop(n, prop)?;
        let (id, _) = parse::url_id(v.to_bytes())?;
        self.by_id(&id).filter(|&t| name_is(t, tag))
    }

    #[allow(clippy::too_many_arguments)]
    fn geom(
        &self,
        n: Node<'a>,
        attr: &CStr,
        prop: &Prop,
        basis: f64,
        fs: f64,
        fallback: f64,
    ) -> (f64, bool) {
        let s = self.style(n);
        if let Some(s) = s
            && let Some(v) = s.value(prop)
        {
            match v.get() {
                Value::Keyword(Some(k)) if eq_ci(k.to_bytes(), b"auto") => {
                    return (fallback, true);
                }
                Value::Length(..) | Value::Calc => {
                    return (css_number(s, prop, basis, fallback), false);
                }
                _ => {}
            }
        }
        let Some(a) = self.prop(n, attr) else {
            return (fallback, false);
        };
        if eq_ci(a.to_bytes(), b"auto") {
            return (fallback, true);
        }
        (parse::length(Some(&a), basis, fs, fallback), false)
    }

    fn rounded_corner(&self, x: f64, y: f64, rx: f64, ry: f64, a1: f64, a2: f64) {
        let cr = self.cr;
        cr.save();
        cr.translate(x, y);
        cr.scale(rx, ry);
        cr.arc(0.0, 0.0, 1.0, a1, a2);
        cr.restore();
    }

    fn radii(&self, n: Node<'a>, fs: f64) -> (f64, f64) {
        let (mut rx, rx_auto) = self.geom(n, c"rx", &RX, self.vw, fs, -1.0);
        let (mut ry, ry_auto) = self.geom(n, c"ry", &RY, self.vh, fs, -1.0);
        if rx_auto {
            rx = -1.0;
        }
        if ry_auto {
            ry = -1.0;
        }
        (rx, ry)
    }

    fn shape_path(&self, n: Node<'a>, st: &State) -> bool {
        let cr = self.cr;
        let fs = st.font_size;
        let tag = tag(n);
        match tag {
            b"path" => match n.attr(c"d").filter(|d| !d.is_empty()) {
                Some(d) => {
                    parse::path_data(cr, d);
                    true
                }
                None => false,
            },
            b"rect" => {
                let x = self.geom(n, c"x", &SVG_X, self.vw, fs, 0.0).0;
                let y = self.geom(n, c"y", &SVG_Y, self.vh, fs, 0.0).0;
                let w = self.geom(n, c"width", &WIDTH, self.vw, fs, 0.0).0;
                let h = self.geom(n, c"height", &HEIGHT, self.vh, fs, 0.0).0;
                if w <= 0.0 || h <= 0.0 {
                    return false;
                }
                let (mut rx, mut ry) = self.radii(n, fs);
                if rx < 0.0 && ry < 0.0 {
                    rx = 0.0;
                    ry = 0.0;
                } else if rx < 0.0 {
                    rx = ry;
                } else if ry < 0.0 {
                    ry = rx;
                }
                rx = clamp(rx, 0.0, w / 2.0);
                ry = clamp(ry, 0.0, h / 2.0);
                if rx <= 0.0 || ry <= 0.0 {
                    cr.rectangle(x, y, w, h);
                    return true;
                }
                cr.save();
                cr.new_sub_path();
                cr.translate(x + rx, y + ry);
                cr.scale(rx, ry);
                cr.arc(0.0, 0.0, 1.0, PI, 1.5 * PI);
                cr.restore();
                self.rounded_corner(x + w - rx, y + ry, rx, ry, 1.5 * PI, 2.0 * PI);
                self.rounded_corner(x + w - rx, y + h - ry, rx, ry, 0.0, 0.5 * PI);
                self.rounded_corner(x + rx, y + h - ry, rx, ry, 0.5 * PI, PI);
                cr.close_path();
                true
            }
            b"circle" => {
                let cx = self.geom(n, c"cx", &CX, self.vw, fs, 0.0).0;
                let cy = self.geom(n, c"cy", &CY, self.vh, fs, 0.0).0;
                let r = self
                    .geom(n, c"r", &R, diag_basis(self.vw, self.vh), fs, 0.0)
                    .0;
                if r <= 0.0 {
                    return false;
                }
                cr.new_sub_path();
                cr.arc(cx, cy, r, 0.0, 2.0 * PI);
                cr.close_path();
                true
            }
            b"ellipse" => {
                let cx = self.geom(n, c"cx", &CX, self.vw, fs, 0.0).0;
                let cy = self.geom(n, c"cy", &CY, self.vh, fs, 0.0).0;
                let (mut rx, mut ry) = self.radii(n, fs);
                if rx < 0.0 && ry < 0.0 {
                    return false;
                }
                if rx < 0.0 {
                    rx = ry;
                }
                if ry < 0.0 {
                    ry = rx;
                }
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                cr.save();
                cr.new_sub_path();
                cr.translate(cx, cy);
                cr.scale(rx, ry);
                cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
                cr.restore();
                cr.close_path();
                true
            }
            b"line" => {
                let x1 = attr_length(n, c"x1", self.vw, fs, 0.0);
                let y1 = attr_length(n, c"y1", self.vh, fs, 0.0);
                let x2 = attr_length(n, c"x2", self.vw, fs, 0.0);
                let y2 = attr_length(n, c"y2", self.vh, fs, 0.0);
                cr.move_to(x1, y1);
                cr.line_to(x2, y2);
                true
            }
            b"polyline" | b"polygon" => {
                let Some(pts) = n.attr(c"points") else {
                    return false;
                };
                let pts = parse::points(pts);
                let Some(&(x0, y0)) = pts.first() else {
                    return false;
                };
                cr.move_to(x0, y0);
                for &(x, y) in &pts[1..] {
                    cr.line_to(x, y);
                }
                if tag == b"polygon" {
                    cr.close_path();
                }
                true
            }
            _ => false,
        }
    }

    fn add_stops(&self, pat: &Pattern, owner: Node<'a>, st: &State, alpha: f64) {
        let mut last = 0.0;
        for c in children(owner) {
            if !is_element(c) || !name_is(c, b"stop") {
                continue;
            }
            let os = self.prop(c, c"offset");
            let mut off = match &os {
                Some(os) => parse::length(Some(os), 1.0, st.font_size, 0.0),
                None => 0.0,
            };
            if os.as_ref().is_some_and(|os| !os.to_bytes().contains(&b'%')) && off > 1.0 {
                off = 1.0;
            }
            off = clamp(off, 0.0, 1.0);
            if off < last {
                off = last;
            }
            last = off;

            let mut rgba = [0.0, 0.0, 0.0, 1.0];
            let cs = self.style(c);
            match cs.and_then(|s| s.value(&STOP_COLOR)).map(|v| v.get()) {
                Some(Value::Color(col)) => rgba = unit_rgba(col),
                _ => {
                    if let Some(sc) = self.prop(c, c"stop-color") {
                        if eq_ci(sc.to_bytes(), b"currentcolor") {
                            rgba = st.color;
                        } else if let Some(col) = parse_color(&sc) {
                            rgba = unit_rgba(col);
                        }
                    }
                }
            }
            let mut so = 1.0;
            match cs.filter(|s| s.value(&STOP_OPACITY).is_some()) {
                Some(cs) => so = clamp(css_number(cs, &STOP_OPACITY, 1.0, 1.0), 0.0, 1.0),
                None => {
                    if let Some(sos) = self.prop(c, c"stop-opacity") {
                        so = clamp(parse::length(Some(&sos), 1.0, st.font_size, 1.0), 0.0, 1.0);
                    }
                }
            }
            let [r, g, b, a] = rgba;
            pat.add_stop(off, r, g, b, a * so * alpha);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gradient_pattern(
        &mut self,
        g: Node<'a>,
        st: &State,
        alpha: f64,
        bx: f64,
        by: f64,
        bw: f64,
        bh: f64,
    ) -> Option<Pattern> {
        let linear = name_is(g, b"linearGradient");
        let units = self.inherited_attr(g, c"gradientUnits");
        let obb = units.is_none_or(|u| !eq_ci(u.to_bytes(), b"userSpaceOnUse"));
        let bw_ref = if obb { 1.0 } else { self.vw };
        let bh_ref = if obb { 1.0 } else { self.vh };
        let dref = if obb {
            1.0
        } else {
            diag_basis(self.vw, self.vh)
        };
        let fs = st.font_size;
        let len = |ctx: &mut Ctx<'a>, name: &CStr, basis: f64, fallback: f64| match ctx
            .inherited_attr(g, name)
        {
            Some(s) => parse::length(Some(s), basis, fs, fallback),
            None => fallback,
        };
        let pat = if linear {
            let x1 = len(self, c"x1", bw_ref, 0.0);
            let y1 = len(self, c"y1", bh_ref, 0.0);
            let x2 = len(self, c"x2", bw_ref, bw_ref);
            let y2 = len(self, c"y2", bh_ref, 0.0);
            Pattern::linear(x1, y1, x2, y2)
        } else {
            let cx = len(self, c"cx", bw_ref, bw_ref * 0.5);
            let cy = len(self, c"cy", bh_ref, bh_ref * 0.5);
            let r = len(self, c"r", dref, dref * 0.5);
            let mut fx = len(self, c"fx", bw_ref, cx);
            let mut fy = len(self, c"fy", bh_ref, cy);
            if r <= 0.0 {
                return None;
            }
            let (dx, dy) = (fx - cx, fy - cy);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > r * 0.999 {
                let k = r * 0.999 / dist;
                fx = cx + dx * k;
                fy = cy + dy * k;
            }
            Pattern::radial(fx, fy, cx, cy, r)
        };
        let owner = self.stops_owner(g)?;
        self.add_stops(&pat, owner, st, alpha);
        pat.set_extend(extend_of(self.inherited_attr(g, c"spreadMethod")));
        let mut m = Matrix::identity();
        if obb {
            if bw <= 0.0 || bh <= 0.0 {
                return None;
            }
            m = Matrix::translation(bx, by);
            m.scale(bw, bh);
        }
        if let Some(gt) = self.inherited_attr(g, c"gradientTransform") {
            let (t, any) = parse::transform(Some(gt));
            if any {
                m = Matrix::multiply(&t, &m);
            }
        }
        if !m.invert() {
            return None;
        }
        pat.set_matrix(&m);
        Some(pat)
    }

    fn set_paint(&mut self, paint: &state::Paint, st: &State, opacity: f64) -> bool {
        let cr = self.cr;
        match paint.kind {
            PaintKind::None => return false,
            PaintKind::Color => {
                let [r, g, b, a] = paint.rgba;
                if a * opacity <= 0.0 {
                    return false;
                }
                cr.set_source_rgba(r, g, b, a * opacity);
                return true;
            }
            PaintKind::Ref => {}
        }
        let g = paint.reference.as_deref().and_then(|id| self.by_id(id));
        if let Some(g) =
            g.filter(|&g| name_is(g, b"linearGradient") || name_is(g, b"radialGradient"))
        {
            let (bx, by, bx2, by2) = cr.path_extents();
            if let Some(pat) = self.gradient_pattern(g, st, opacity, bx, by, bx2 - bx, by2 - by) {
                cr.set_source(&pat);
                return true;
            }
        }
        if paint.have_fallback {
            let [r, g, b, a] = paint.fallback;
            if a * opacity <= 0.0 {
                return false;
            }
            cr.set_source_rgba(r, g, b, a * opacity);
            return true;
        }
        false
    }

    fn apply_stroke_params(&self, st: &State) {
        let cr = self.cr;
        cr.set_line_width(st.stroke_width);
        cr.set_line_cap(st.line_cap);
        cr.set_line_join(st.line_join);
        cr.set_miter_limit(st.miter_limit);
        match &st.dashes {
            Some(d) if !d.is_empty() => cr.set_dash(d, st.dash_offset),
            _ => cr.set_dash(&[], 0.0),
        }
    }

    fn paint_current_path(&mut self, st: &State) {
        let cr = self.cr;
        let do_stroke = st.stroke.kind != PaintKind::None && st.stroke_width > 0.0;
        for pass in 0..2 {
            let stroking = if st.stroke_first {
                pass == 0
            } else {
                pass == 1
            };
            if stroking {
                if !do_stroke || !self.set_paint(&st.stroke, st, st.stroke_opacity) {
                    continue;
                }
                if st.non_scaling_stroke {
                    let ctm = cr.matrix();
                    cr.identity_matrix();
                    self.apply_stroke_params(st);
                    cr.stroke_preserve();
                    cr.set_matrix(&ctm);
                } else {
                    self.apply_stroke_params(st);
                    cr.stroke_preserve();
                }
            } else {
                cr.set_fill_rule(st.fill_rule);
                if !self.set_paint(&st.fill, st, st.fill_opacity) {
                    continue;
                }
                cr.fill_preserve();
            }
        }
        cr.new_path();
    }

    fn apply_transform_attr(&self, n: Node<'a>) {
        let (m, any) = parse::transform(n.attr(c"transform"));
        if any {
            self.cr.transform(&m);
        }
    }

    fn clip_path_children(&mut self, clip: Node<'a>, st: &State) {
        for c in children(clip) {
            if !is_element(c) || c.name().is_none() {
                continue;
            }
            if name_is(c, b"use") {
                let t = self.href_target(c);
                if let Some(t) = t.filter(|_| self.depth < MAX_DEPTH) {
                    self.depth += 1;
                    self.clip_path_children(c, st);
                    self.cr.save();
                    self.apply_transform_attr(c);
                    self.shape_path(t, st);
                    self.cr.restore();
                    self.depth -= 1;
                }
                continue;
            }
            self.cr.save();
            self.apply_transform_attr(c);
            self.shape_path(c, st);
            self.cr.restore();
        }
    }

    fn mask_surface(&mut self, n: Node<'a>, st: &State) -> Option<Surface> {
        let mask = self.url_target(n, c"mask", b"mask")?;
        if self.depth >= MAX_DEPTH {
            return None;
        }
        let (ox, oy) = self.cr.target_device_offset();
        let (cx1, cy1, cx2, cy2) = self.cr.clip_extents();
        let ctm = self.cr.matrix();
        let (cx1, cy1) = ctm.transform_point(cx1, cy1);
        let (cx2, cy2) = ctm.transform_point(cx2, cy2);
        let w = (cmax(cx1, cx2) + ox).ceil() as i32;
        let h = (cmax(cy1, cy2) + oy).ceil() as i32;
        if w <= 0 || h <= 0 || f64::from(w) * f64::from(h) > MAX_PIXELS {
            return None;
        }
        let mut rgb = Surface::image(FORMAT_ARGB32, w, h)?;
        {
            let mcr = OwnedCanvas::on(&rgb);
            mcr.canvas().set_matrix(&ctm);
            let saved = self.cr;
            self.cr = mcr.canvas();
            self.depth += 1;
            let mut ms = st.clone();
            ms.fill.set_color([1.0; 4]);
            for c in children(mask) {
                if is_element(c) {
                    self.render_node(c, &ms);
                }
            }
            self.depth -= 1;
            self.cr = saved;
        }
        rgb.flush();
        let mut a8 = Surface::image(FORMAT_A8, w, h)?;
        let (sstride, dstride) = (rgb.stride(), a8.stride());
        let (w, h) = (w as usize, h as usize);
        let src = rgb.data(h)?.to_vec();
        let dst = a8.data(h)?;
        for y in 0..h {
            for x in 0..w {
                let o = y * sstride + x * 4;
                let px = u32::from_ne_bytes([src[o], src[o + 1], src[o + 2], src[o + 3]]);
                let r = f64::from((px >> 16) & 0xff) / 255.0;
                let g = f64::from((px >> 8) & 0xff) / 255.0;
                let b = f64::from(px & 0xff) / 255.0;
                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                dst[y * dstride + x] = clamp(lum * 255.0, 0.0, 255.0) as u8;
            }
        }
        a8.mark_dirty();
        Some(a8)
    }

    fn apply_clip(&mut self, n: Node<'a>, st: &State) {
        let Some(clip) = self.url_target(n, c"clip-path", b"clipPath") else {
            return;
        };
        if self.depth >= MAX_DEPTH {
            return;
        }
        self.depth += 1;
        let cr = self.cr;
        cr.save();
        if attr_ci(clip, c"clipPathUnits", b"objectBoundingBox") {
            cr.transform(&Matrix::scaling(self.vw, self.vh));
        }
        cr.new_path();
        self.clip_path_children(clip, st);
        cr.restore();
        cr.set_fill_rule(st.clip_rule);
        cr.clip();
        cr.new_path();
        self.depth -= 1;
    }

    fn text_layout(&self, n: Node<'a>, st: &State) -> Option<(TextLayout, f64, f64, f64, f64)> {
        let flat = collapsed_text(n)?;
        let fs = st.font_size;
        let mut x = attr_length(n, c"x", self.vw, fs, 0.0);
        let mut y = attr_length(n, c"y", self.vh, fs, 0.0);
        x += attr_length(n, c"dx", self.vw, fs, 0.0);
        y += attr_length(n, c"dy", self.vh, fs, 0.0);
        let font = ffi::Font {
            family: st.font_family.as_deref(),
            size: st.font_size,
            weight: st.font_weight,
            italic: st.font_italic,
        };
        let layout = self.cr.text_layout(&flat, &font);
        let (wpx, hpx) = layout.pixel_size();
        if st.text_anchor == 1 {
            x -= f64::from(wpx) / 2.0;
        } else if st.text_anchor == 2 {
            x -= f64::from(wpx);
        }
        let baseline = layout.baseline() / PANGO_SCALE;
        Some((
            layout,
            x,
            y - f64::from(baseline),
            f64::from(wpx),
            f64::from(hpx),
        ))
    }

    fn render_text(&mut self, n: Node<'a>, st: &State) {
        let Some((layout, x, top, _, _)) = self.text_layout(n, st) else {
            return;
        };
        self.cr.move_to(x, top);
        self.cr.layout_path(&layout);
        drop(layout);
        self.paint_current_path(st);
    }

    fn marker_ref(&mut self, n: Node<'a>, prop: &CStr) -> Option<Node<'a>> {
        let v = self.prop(n, prop).or_else(|| self.prop(n, c"marker"))?;
        let (id, _) = parse::url_id(v.to_bytes())?;
        self.by_id(&id).filter(|&m| name_is(m, b"marker"))
    }

    fn draw_marker(&mut self, marker: Node<'a>, st: &State, x: f64, y: f64, angle: f64) {
        if self.depth >= MAX_DEPTH {
            return;
        }
        let cr = self.cr;
        let fs = st.font_size;
        let mw = attr_length(marker, c"markerWidth", self.vw, fs, 3.0);
        let mh = attr_length(marker, c"markerHeight", self.vh, fs, 3.0);
        if mw <= 0.0 || mh <= 0.0 {
            return;
        }
        let stroke_units = marker
            .attr(c"markerUnits")
            .is_none_or(|u| eq_ci(u.to_bytes(), b"strokeWidth"));
        cr.save();
        cr.translate(x, y);
        match marker.attr(c"orient") {
            Some(o)
                if !eq_ci(o.to_bytes(), b"auto") && !eq_ci(o.to_bytes(), b"auto-start-reverse") =>
            {
                cr.rotate(parse::length(Some(o), 0.0, fs, 0.0) * PI / 180.0);
            }
            _ => cr.rotate(angle),
        }
        if stroke_units && st.stroke_width > 0.0 {
            cr.scale(st.stroke_width, st.stroke_width);
        }
        let vm = viewbox_matrix(marker, mw, mh);
        let rx = attr_length(marker, c"refX", self.vw, fs, 0.0);
        let ry = attr_length(marker, c"refY", self.vh, fs, 0.0);
        let (rx, ry) = vm.transform_point(rx, ry);
        cr.translate(-rx, -ry);
        let clip = self
            .prop(marker, c"overflow")
            .is_none_or(|ov| !eq_ci(ov.to_bytes(), b"visible") && !eq_ci(ov.to_bytes(), b"auto"));
        if clip {
            cr.rectangle(0.0, 0.0, mw, mh);
            cr.clip();
            cr.new_path();
        }
        cr.transform(&vm);
        let (ow, oh) = (self.vw, self.vh);
        if let Some((w, h)) = parse::viewbox_size(marker.attr(c"viewBox")) {
            self.vw = w;
            self.vh = h;
        }
        let ms = State {
            color: st.color,
            font_size: st.font_size,
            ..State::default()
        };
        self.depth += 1;
        for c in children(marker) {
            if is_element(c) {
                self.render_node(c, &ms);
            }
        }
        self.depth -= 1;
        self.vw = ow;
        self.vh = oh;
        cr.restore();
    }

    fn render_markers(&mut self, n: Node<'a>, st: &State) {
        let ms = self.marker_ref(n, c"marker-start");
        let mm = self.marker_ref(n, c"marker-mid");
        let me = self.marker_ref(n, c"marker-end");
        if ms.is_none() && mm.is_none() && me.is_none() {
            return;
        }
        let Some(verts) = path_vertices(self.cr) else {
            return;
        };
        let last = verts.len().saturating_sub(1);
        for (i, v) in verts.iter().enumerate() {
            let m = if i == 0 {
                ms
            } else if i == last {
                me
            } else {
                mm
            };
            let Some(m) = m else {
                continue;
            };
            let mut a = if v.has_in && v.has_out {
                let dx = v.in_angle.cos() + v.out_angle.cos();
                let dy = v.in_angle.sin() + v.out_angle.sin();
                if dx == 0.0 && dy == 0.0 {
                    v.in_angle
                } else {
                    dy.atan2(dx)
                }
            } else if v.has_in {
                v.in_angle
            } else if v.has_out {
                v.out_angle
            } else {
                0.0
            };
            if i == 0 && attr_ci(m, c"orient", b"auto-start-reverse") {
                a += PI;
            }
            self.draw_marker(m, st, v.x, v.y, a);
        }
    }

    fn render_children(&mut self, n: Node<'a>, st: &State) {
        for c in children(n) {
            if is_element(c) {
                self.render_node(c, st);
            }
        }
    }

    fn is_hidden(&self, n: Node<'a>) -> bool {
        if !system_language_allows(n) {
            return true;
        }
        if self
            .style(n)
            .and_then(|s| s.keyword(&DISPLAY))
            .is_some_and(|d| d.to_bytes() == b"none")
        {
            return true;
        }
        self.prop(n, c"display")
            .is_some_and(|d| eq_ci(d.to_bytes(), b"none"))
    }

    fn opacity(&self, n: Node<'a>, st: &State) -> f64 {
        match self.style(n).filter(|s| s.value(&OPACITY).is_some()) {
            Some(s) => clamp(css_number(s, &OPACITY, 1.0, 1.0), 0.0, 1.0),
            None => match self.prop(n, c"opacity") {
                Some(o) => clamp(parse::length(Some(&o), 1.0, st.font_size, 1.0), 0.0, 1.0),
                None => 1.0,
            },
        }
    }

    fn render_nested_svg(&mut self, n: Node<'a>, st: &State) {
        let cr = self.cr;
        let fs = st.font_size;
        let x = attr_length(n, c"x", self.vw, fs, 0.0);
        let y = attr_length(n, c"y", self.vh, fs, 0.0);
        let w = attr_length(n, c"width", self.vw, fs, self.vw);
        let h = attr_length(n, c"height", self.vh, fs, self.vh);
        cr.translate(x, y);
        cr.rectangle(0.0, 0.0, w, h);
        cr.clip();
        cr.new_path();
        cr.transform(&viewbox_matrix(n, w, h));
        let (ow, oh) = (self.vw, self.vh);
        match n.attr(c"viewBox") {
            Some(vb) => {
                if let Some((vw, vh)) = parse::viewbox_size(Some(vb)) {
                    self.vw = vw;
                    self.vh = vh;
                }
            }
            None => {
                self.vw = w;
                self.vh = h;
            }
        }
        self.render_children(n, st);
        self.vw = ow;
        self.vh = oh;
    }

    fn render_use(&mut self, n: Node<'a>, st: &State) {
        let Some(t) = self.href_target(n).filter(|t| t.as_ptr() != n.as_ptr()) else {
            return;
        };
        let cr = self.cr;
        let fs = st.font_size;
        let x = attr_length(n, c"x", self.vw, fs, 0.0);
        let y = attr_length(n, c"y", self.vh, fs, 0.0);
        cr.translate(x, y);
        self.depth += 1;
        if name_is(t, b"symbol") {
            let w = attr_length(n, c"width", self.vw, fs, self.vw);
            let h = attr_length(n, c"height", self.vh, fs, self.vh);
            cr.rectangle(0.0, 0.0, w, h);
            cr.clip();
            cr.new_path();
            cr.transform(&viewbox_matrix(t, w, h));
            self.render_children(t, st);
        } else {
            self.render_node(t, st);
        }
        self.depth -= 1;
    }

    fn render_shape(&mut self, n: Node<'a>, st: &State) {
        let cr = self.cr;
        cr.new_path();
        if self.shape_path(n, st) {
            let markable = matches!(tag(n), b"path" | b"line" | b"polyline" | b"polygon");
            let kept = if markable { cr.copy_path() } else { None };
            self.paint_current_path(st);
            if let Some(kept) = kept {
                cr.new_path();
                cr.append_path(&kept);
                drop(kept);
                self.render_markers(n, st);
            }
        }
        cr.new_path();
    }

    fn render_node_nested(&mut self, n: Node<'a>, parent: &State) {
        let tag = tag(n);
        if never_rendered(tag) || self.is_hidden(n) {
            return;
        }
        let mut st = parent.clone();
        self.apply_node(&mut st, n);
        let opacity = self.opacity(n, &st);
        if st.hidden && tag != b"g" && tag != b"svg" {
            return;
        }
        if opacity <= 0.0 {
            return;
        }
        let cr = self.cr;
        cr.save();
        self.apply_transform_attr(n);
        let mask = self.mask_surface(n, &st);
        let grouped = opacity < 1.0 || mask.is_some();
        if grouped {
            cr.push_group();
        }
        self.apply_clip(n, &st);
        match tag {
            b"g" | b"a" => self.render_children(n, &st),
            b"switch" => {
                if let Some(choice) = switch_choice(n) {
                    self.render_node(choice, &st);
                }
            }
            b"svg" => self.render_nested_svg(n, &st),
            b"use" => self.render_use(n, &st),
            b"text" | b"tspan" => self.render_text(n, &st),
            _ => self.render_shape(n, &st),
        }
        if grouped {
            cr.pop_group_to_source();
            match &mask {
                Some(mask) => {
                    cr.save();
                    cr.identity_matrix();
                    if opacity < 1.0 {
                        cr.push_group();
                        cr.mask_surface(mask);
                        cr.pop_group_to_source();
                        cr.paint_with_alpha(opacity);
                    } else {
                        cr.mask_surface(mask);
                    }
                    cr.restore();
                }
                None => cr.paint_with_alpha(opacity),
            }
        }
        drop(mask);
        cr.restore();
    }

    fn render_node(&mut self, n: Node<'a>, parent: &State) {
        if n.name().is_none() || self.depth >= MAX_DEPTH || self.nesting >= MAX_NESTING {
            return;
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return;
        }
        self.nesting += 1;
        self.render_node_nested(n, parent);
        self.nesting -= 1;
    }
}

pub fn is_root(n: Node<'_>) -> bool {
    is_element(n) && name_is(n, b"svg")
}

pub fn find_root(doc: Node<'_>) -> Option<Node<'_>> {
    let mut cur = Some(doc);
    while let Some(n) = cur {
        if is_root(n) {
            return Some(n);
        }
        cur = ffi::next_in_subtree(n, doc, true);
    }
    None
}

pub fn intrinsic_size(svg: Option<Node<'_>>) -> SvgSize {
    let mut out = SvgSize {
        width: 0.0,
        height: 0.0,
        ratio: 0.0,
        has_width: 0,
        has_height: 0,
        has_ratio: 0,
    };
    let Some(svg) = svg else {
        return out;
    };
    let (mut has_w, mut has_h, mut has_r) = (false, false, false);
    let dim = |name: &CStr| {
        let s = svg.attr(name).filter(|s| !s.to_bytes().contains(&b'%'))?;
        let v = parse::length(Some(s), 0.0, 16.0, 0.0);
        (v > 0.0).then_some(v)
    };
    if let Some(w) = dim(c"width") {
        out.width = w;
        has_w = true;
    }
    if let Some(h) = dim(c"height") {
        out.height = h;
        has_h = true;
    }
    if let Some((w, h)) = parse::viewbox_size(svg.attr(c"viewBox")) {
        out.ratio = w / h;
        has_r = true;
    }
    if has_w && has_h && !has_r && out.height > 0.0 {
        out.ratio = out.width / out.height;
        has_r = true;
    }
    if has_w && !has_h && has_r {
        out.height = out.width / out.ratio;
        has_h = true;
    } else if has_h && !has_w && has_r {
        out.width = out.height * out.ratio;
        has_w = true;
    }
    out.has_width = boolean(has_w);
    out.has_height = boolean(has_h);
    out.has_ratio = boolean(has_r);
    out
}

pub fn render_node(
    cr: Canvas,
    svg: Node<'_>,
    width: f64,
    height: f64,
    styles: StyleTable,
    inherited: Option<StyleRef<'_>>,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let mut ctx = Ctx::new(cr, svg, styles, width, height);
    let mut st = State::inherited(inherited);
    cr.save();
    cr.rectangle(0.0, 0.0, width, height);
    cr.clip();
    cr.new_path();
    cr.transform(&viewbox_matrix(svg, width, height));
    if let Some((w, h)) = parse::viewbox_size(svg.attr(c"viewBox")) {
        ctx.vw = w;
        ctx.vh = h;
    }
    ctx.apply_node(&mut st, svg);
    ctx.render_children(svg, &st);
    cr.restore();
}

pub fn looks_like_svg(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    let scan = data.len().min(1024);
    (0..scan.saturating_sub(4))
        .any(|i| data[i] == b'<' && data[i..i + 4].eq_ignore_ascii_case(b"<svg"))
}

fn parse_svg(data: &[u8]) -> Option<ffi::Document> {
    if data.is_empty() || data.len() > MAX_INPUT_BYTES || !looks_like_svg(data) {
        return None;
    }
    ffi::Document::parse(data)
}

pub fn render_bytes(cr: Canvas, data: &[u8], width: f64, height: f64) -> bool {
    if data.is_empty() || data.len() > MAX_INPUT_BYTES {
        return false;
    }
    if width <= 0.0 || height <= 0.0 {
        return false;
    }
    let Some(doc) = parse_svg(data) else {
        return false;
    };
    let Some(root) = find_root(doc.node()) else {
        return false;
    };
    render_node(cr, root, width, height, StyleTable::none(), None);
    true
}

pub fn decode_bytes(data: &[u8]) -> Option<(*mut Texture, i32, i32)> {
    let doc = parse_svg(data)?;
    let root = find_root(doc.node())?;
    let size = intrinsic_size(Some(root));
    let mut w = if size.has_width != 0 {
        size.width
    } else {
        DEFAULT_DIM_PX
    };
    let mut h = if size.has_height != 0 {
        size.height
    } else {
        DEFAULT_DIM_PX
    };
    if size.has_width == 0 && size.has_height == 0 && size.has_ratio != 0 && size.ratio > 0.0 {
        let k = cmin(DEFAULT_OBJECT_W / size.ratio, DEFAULT_OBJECT_H);
        w = size.ratio * k;
        h = k;
    }
    if !w.is_finite() || w <= 0.0 {
        w = DEFAULT_DIM_PX;
    }
    if !h.is_finite() || h <= 0.0 {
        h = DEFAULT_DIM_PX;
    }
    if w > MAX_DIM_PX || h > MAX_DIM_PX {
        let s = MAX_DIM_PX / cmax(w, h);
        w *= s;
        h *= s;
    }
    if w * h > MAX_PIXELS {
        let s = (MAX_PIXELS / (w * h)).sqrt();
        w *= s;
        h *= s;
    }
    let iw = clamp((w - 0.001).ceil(), 1.0, MAX_DIM_PX) as i32;
    let ih = clamp((h - 0.001).ceil(), 1.0, MAX_DIM_PX) as i32;
    let surf = Surface::image(FORMAT_ARGB32, iw, ih)?;
    {
        let cr = OwnedCanvas::on(&surf);
        render_node(
            cr.canvas(),
            root,
            f64::from(iw),
            f64::from(ih),
            StyleTable::none(),
            None,
        );
    }
    surf.flush();
    drop(doc);
    let tex = surf.into_texture(iw, ih);
    (!tex.is_null()).then_some((tex, iw, ih))
}
