//! Southstar — SVG geometry: getBBox, getCTM and getScreenCTM over the measured node, client rects of SVG content, path length and point-at-length over a flattened path, SVGPoint.matrixTransform and the createSVG* factories.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;
use std::ffi::CStr;

use southstar_dom::FLAG_SVG_NS;
use southstar_js_engine::{Scope, Value};
use southstar_layout::BoxRef;
use southstar_svg::{Matrix, SvgGeometry};

use crate::boxes::{accumulate_transform, border_box, box_for_this, point_to_client};
use crate::{Element, JsResult, Rect, arg, cmax, cmin, ffi};

const ANIM_ATTR_MAX: usize = 63;

fn is_svg_name(n: Element) -> bool {
    n.name()
        .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"svg"))
}

fn owner_root(n: Element) -> Option<Element> {
    let mut svg = None;
    let mut p = Some(n);
    while let Some(cur) = p {
        if is_svg_name(cur) {
            svg = Some(cur);
        }
        p = cur.parent();
    }
    svg
}

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    Matrix {
        xx: a.xx * b.xx + a.yx * b.xy,
        yx: a.xx * b.yx + a.yx * b.yy,
        xy: a.xy * b.xx + a.yy * b.xy,
        yy: a.xy * b.yx + a.yy * b.yy,
        x0: a.x0 * b.xx + a.y0 * b.xy + b.x0,
        y0: a.x0 * b.yx + a.y0 * b.yy + b.y0,
    }
}

fn transform_point(m: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (m.xx * x + m.xy * y + m.x0, m.yx * x + m.yy * y + m.y0)
}

fn identity() -> Matrix {
    Matrix {
        xx: 1.0,
        yx: 0.0,
        xy: 0.0,
        yy: 1.0,
        x0: 0.0,
        y0: 0.0,
    }
}

fn page_matrix(svg_box: BoxRef<'_>, to_root: &Matrix) -> Matrix {
    let r = border_box(svg_box);
    let border = svg_box.border();
    let padding = svg_box.padding();
    let placed = Matrix {
        x0: r.x + border.left + padding.left,
        y0: r.y + border.top + padding.top,
        ..identity()
    };
    let to_page = multiply(to_root, &placed);
    let Some(css) = accumulate_transform(svg_box) else {
        return to_page;
    };
    let flat = Matrix {
        xx: css.m[0],
        yx: css.m[4],
        xy: css.m[1],
        yy: css.m[5],
        x0: css.m[3],
        y0: css.m[7],
    };
    multiply(&to_page, &flat)
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
        to_root: identity(),
        to_viewport: identity(),
    }
}

fn measure_node(
    scope: &mut Scope<'_>,
    node: Option<Element>,
) -> (Option<BoxRef<'static>>, SvgGeometry) {
    let Some(node) = node.filter(|n| n.flags() & FLAG_SVG_NS != 0) else {
        return (None, empty_geometry());
    };
    let js = ffi::js_of(scope);
    let Some(svg) = owner_root(node).filter(|_| !js.is_null()) else {
        return (None, empty_geometry());
    };
    ffi::flush_layout(js);
    let Some(svg_box) = ffi::layout_root(js).and_then(|root| ffi::find_by_dom(root, svg)) else {
        return (None, empty_geometry());
    };
    let geometry = southstar_svg::node_geometry(
        Some(svg),
        Some(node),
        svg_box.content_width(),
        svg_box.content_height(),
        ffi::svg_styles(svg_box),
        ffi::box_style(svg_box),
    );
    (Some(svg_box), geometry)
}

fn mapped_bounds(geometry: &SvgGeometry, matrix: &Matrix) -> Rect {
    let xs = [geometry.x, geometry.x + geometry.width];
    let ys = [geometry.y, geometry.y + geometry.height];
    let (mut left, mut top) = (f64::MAX, f64::MAX);
    let (mut right, mut bottom) = (-f64::MAX, -f64::MAX);
    for corner in 0..4 {
        let (px, py) = transform_point(matrix, xs[corner & 1], ys[corner >> 1]);
        left = cmin(left, px);
        right = cmax(right, px);
        top = cmin(top, py);
        bottom = cmax(bottom, py);
    }
    Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    }
}

pub(crate) fn client_rect(scope: &mut Scope<'_>, node: Option<Element>) -> Option<Rect> {
    let (svg_box, geometry) = measure_node(scope, node);
    let svg_box = svg_box?;
    if geometry.found == 0 || geometry.rendered == 0 {
        return None;
    }
    let to_page = page_matrix(svg_box, &geometry.to_root);
    Some(mapped_bounds(&geometry, &to_page))
}

pub(crate) fn get_bbox(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let (_, geometry) = measure_node(scope, ffi::unwrap_node(this));
    if geometry.found != 0 {
        let r = Rect {
            x: geometry.x,
            y: geometry.y,
            w: geometry.width,
            h: geometry.height,
        };
        return Ok(ffi::dom_rect(scope, r));
    }
    let r = box_for_this(scope, this).map_or_else(Rect::default, border_box);
    Ok(ffi::dom_rect(scope, r))
}

fn matrix_parts(m: &Matrix) -> [f64; 6] {
    [m.xx, m.yx, m.xy, m.yy, m.x0, m.y0]
}

pub(crate) fn get_screen_ctm(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let node = ffi::unwrap_node(this);
    let (svg_box, geometry) = measure_node(scope, node);
    let mut m = geometry.to_root;
    if let Some(svg_box) = svg_box {
        m = page_matrix(svg_box, &geometry.to_root);
        point_to_client(scope, node, &mut m.x0, &mut m.y0);
    }
    Ok(ffi::dom_matrix(scope, matrix_parts(&m)))
}

pub(crate) fn get_ctm(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let (_, geometry) = measure_node(scope, ffi::unwrap_node(this));
    Ok(ffi::dom_matrix(scope, matrix_parts(&geometry.to_viewport)))
}

pub(crate) fn get_owner_svg_element(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    let Some(n) = ffi::unwrap_node(this).filter(|n| n.flags() & FLAG_SVG_NS != 0) else {
        return Ok(Value::undefined());
    };
    let svg = core::iter::successors(n.parent(), |p| p.parent()).find(|p| is_svg_name(*p));
    Ok(svg.map_or(Value::null(), |svg| ffi::wrap_node(scope, svg)))
}

#[derive(Clone, Copy)]
struct Pt {
    x: f64,
    y: f64,
    moveto: bool,
}

fn line_pt(x: f64, y: f64) -> Pt {
    Pt {
        x,
        y,
        moveto: false,
    }
}

fn is_separator(b: u8) -> bool {
    matches!(b, b' ' | b',' | b'\t' | b'\n' | b'\r')
}

struct PathReader<'a> {
    text: &'a CStr,
    at: usize,
}

impl PathReader<'_> {
    fn bytes(&self) -> &[u8] {
        self.text.to_bytes()
    }

    fn skip_separators(&mut self) {
        while self.bytes().get(self.at).is_some_and(|b| is_separator(*b)) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes().get(self.at).copied()
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_separators();
        let (v, next) = ffi::ascii_strtod(self.text, self.at)?;
        self.at = next;
        Some(v)
    }

    fn numbers<const N: usize>(&mut self) -> Option<[f64; N]> {
        let mut out = [0.0; N];
        for slot in &mut out {
            *slot = self.number()?;
        }
        Some(out)
    }
}

fn flatten_cubic(out: &mut Vec<Pt>, p: [f64; 8]) {
    let [x0, y0, x1, y1, x2, y2, x3, y3] = p;
    let steps = 24;
    for i in 1..=steps {
        let t = f64::from(i) / f64::from(steps);
        let u = 1.0 - t;
        let bx = u * u * u * x0 + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x3;
        let by = u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3;
        out.push(line_pt(bx, by));
    }
}

fn flatten_quad(out: &mut Vec<Pt>, p: [f64; 6]) {
    let [x0, y0, x1, y1, x2, y2] = p;
    let steps = 18;
    for i in 1..=steps {
        let t = f64::from(i) / f64::from(steps);
        let u = 1.0 - t;
        let bx = u * u * x0 + 2.0 * u * t * x1 + t * t * x2;
        let by = u * u * y0 + 2.0 * u * t * y1 + t * t * y2;
        out.push(line_pt(bx, by));
    }
}

struct Arc {
    rx: f64,
    ry: f64,
    phi: f64,
    large: bool,
    sweep: bool,
}

fn flatten_arc(out: &mut Vec<Pt>, from: (f64, f64), arc: Arc, to: (f64, f64)) {
    let ((x0, y0), (x1, y1)) = (from, to);
    let Arc {
        mut rx,
        mut ry,
        phi,
        large,
        sweep,
    } = arc;
    if rx == 0.0 || ry == 0.0 || (x0 == x1 && y0 == y1) {
        out.push(line_pt(x1, y1));
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let cosp = phi.cos();
    let sinp = phi.sin();
    let dx = (x0 - x1) / 2.0;
    let dy = (y0 - y1) / 2.0;
    let x1p = cosp * dx + sinp * dy;
    let y1p = -sinp * dx + cosp * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let sign = if large != sweep { 1.0 } else { -1.0 };
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let co = sign * (if num > 0.0 { num / den } else { 0.0 }).sqrt();
    let cxp = co * rx * y1p / ry;
    let cyp = -co * ry * x1p / rx;
    let cx = cosp * cxp - sinp * cyp + (x0 + x1) / 2.0;
    let cy = sinp * cxp + cosp * cyp + (y0 + y1) / 2.0;
    let t0 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
    let t1 = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx);
    let mut dtheta = t1 - t0;
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * PI;
    }
    let steps = 32;
    for i in 1..=steps {
        let t = t0 + dtheta * f64::from(i) / f64::from(steps);
        let ex = cx + rx * t.cos() * cosp - ry * t.sin() * sinp;
        let ey = cy + rx * t.cos() * sinp + ry * t.sin() * cosp;
        out.push(line_pt(ex, ey));
    }
}

fn reflect(prev: u8, kinds: &[u8], cur: f64, last: f64) -> f64 {
    if kinds.contains(&prev) {
        2.0 * cur - last
    } else {
        cur
    }
}

fn flatten_path(d: Option<&CStr>) -> Vec<Pt> {
    let mut pts = Vec::new();
    let Some(d) = d else {
        return pts;
    };
    let mut r = PathReader { text: d, at: 0 };
    let (mut cx, mut cy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
    let (mut lcx, mut lcy) = (0.0, 0.0);
    let (mut cmd, mut prev) = (0u8, 0u8);
    while r.peek().is_some() {
        r.skip_separators();
        let Some(b) = r.peek() else {
            break;
        };
        if b.is_ascii_alphabetic() {
            cmd = b;
            r.at += 1;
        } else if cmd == b'M' {
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let c = cmd.to_ascii_uppercase();
        let (ox, oy) = (cx, cy);
        let dx = |v: f64| if rel { v + ox } else { v };
        let dy = |v: f64| if rel { v + oy } else { v };
        match c {
            b'M' | b'L' | b'T' => {
                let Some([a, b]) = r.numbers::<2>() else {
                    break;
                };
                let (a, b) = (dx(a), dy(b));
                if c == b'T' {
                    let qx = reflect(prev, b"QT", cx, lcx);
                    let qy = reflect(prev, b"QT", cy, lcy);
                    flatten_quad(&mut pts, [cx, cy, qx, qy, a, b]);
                    lcx = qx;
                    lcy = qy;
                } else {
                    pts.push(Pt {
                        x: a,
                        y: b,
                        moveto: c == b'M',
                    });
                    if c == b'M' {
                        sx = a;
                        sy = b;
                    }
                }
                cx = a;
                cy = b;
            }
            b'H' | b'V' => {
                let Some(a) = r.number() else {
                    break;
                };
                if c == b'H' {
                    cx = dx(a);
                } else {
                    cy = dy(a);
                }
                pts.push(line_pt(cx, cy));
            }
            b'C' | b'S' => {
                let (c1x, c1y) = if c == b'C' {
                    let Some([a, b]) = r.numbers::<2>() else {
                        break;
                    };
                    (dx(a), dy(b))
                } else {
                    (reflect(prev, b"CS", cx, lcx), reflect(prev, b"CS", cy, lcy))
                };
                let Some([e, f, g, h]) = r.numbers::<4>() else {
                    break;
                };
                let (e, f, g, h) = (dx(e), dy(f), dx(g), dy(h));
                flatten_cubic(&mut pts, [cx, cy, c1x, c1y, e, f, g, h]);
                lcx = e;
                lcy = f;
                cx = g;
                cy = h;
            }
            b'Q' => {
                let Some([a, b, e, f]) = r.numbers::<4>() else {
                    break;
                };
                let (a, b, e, f) = (dx(a), dy(b), dx(e), dy(f));
                flatten_quad(&mut pts, [cx, cy, a, b, e, f]);
                lcx = a;
                lcy = b;
                cx = e;
                cy = f;
            }
            b'A' => {
                let Some([rx, ry, rot, lg, sw, e, f]) = r.numbers::<7>() else {
                    break;
                };
                let (e, f) = (dx(e), dy(f));
                let arc = Arc {
                    rx,
                    ry,
                    phi: rot * PI / 180.0,
                    large: lg != 0.0,
                    sweep: sw != 0.0,
                };
                flatten_arc(&mut pts, (cx, cy), arc, (e, f));
                cx = e;
                cy = f;
            }
            b'Z' => {
                pts.push(line_pt(sx, sy));
                cx = sx;
                cy = sy;
            }
            _ => break,
        }
        prev = c;
    }
    pts
}

fn path_length(pts: &[Pt], at: Option<f64>) -> (f64, (f64, f64)) {
    let mut total = 0.0;
    let mut point = None;
    for pair in pts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b.moveto {
            continue;
        }
        let seg = (b.x - a.x).hypot(b.y - a.y);
        if let Some(at) = at
            && point.is_none()
            && at <= total + seg
            && seg > 0.0
        {
            let t = (at - total) / seg;
            point = Some((a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
        total += seg;
    }
    let point = point
        .or_else(|| pts.last().map(|last| (last.x, last.y)))
        .unwrap_or((0.0, 0.0));
    (total, point)
}

fn path_of(this: &Value) -> Vec<Pt> {
    flatten_path(ffi::unwrap_node(this).and_then(|n| n.attr(c"d")))
}

pub(crate) fn get_total_length(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::number(path_length(&path_of(this), None).0))
}

fn point_object(scope: &mut Scope<'_>, x: f64, y: f64) -> JsResult {
    let pt = scope.new_object();
    scope.set(&pt, "x", Value::number(x))?;
    scope.set(&pt, "y", Value::number(y))?;
    Ok(pt)
}

pub(crate) fn get_point_at_length(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let pts = path_of(this);
    let at = match args.first() {
        Some(v) => scope.to_number(v)?,
        None => 0.0,
    };
    let (_, (px, py)) = path_length(&pts, Some(at));
    point_object(scope, px, py)
}

fn number_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> JsResult<f64> {
    let v = scope.get(object, key)?;
    scope.to_number(&v)
}

fn with_matrix_transform(scope: &mut Scope<'_>, object: &Value) -> JsResult<()> {
    let f = scope.function("matrixTransform", 1, matrix_transform);
    scope.set(object, "matrixTransform", f)
}

fn matrix_transform(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let x = number_prop(scope, this, "x")?;
    let y = number_prop(scope, this, "y")?;
    let matrix = arg(args, 0);
    let mut m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    if matrix.is_object() {
        for (slot, key) in m.iter_mut().zip(["a", "b", "c", "d", "e", "f"]) {
            *slot = number_prop(scope, &matrix, key)?;
        }
    }
    let [a, b, c, d, e, f] = m;
    let out = point_object(scope, a * x + c * y + e, b * x + d * y + f)?;
    with_matrix_transform(scope, &out)?;
    Ok(out)
}

pub(crate) fn create_svg_point(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let p = point_object(scope, 0.0, 0.0)?;
    with_matrix_transform(scope, &p)?;
    Ok(p)
}

pub(crate) fn create_svg_rect(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let r = scope.new_object();
    for key in ["x", "y", "width", "height"] {
        scope.set(&r, key, Value::number(0.0))?;
    }
    Ok(r)
}

const IDENTITY_PARTS: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

pub(crate) fn create_svg_matrix(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(ffi::dom_matrix(scope, IDENTITY_PARTS))
}

fn noop(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn create_svg_transform(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> JsResult {
    let t = scope.new_object();
    scope.set(&t, "type", Value::int(0))?;
    scope.set(&t, "angle", Value::number(0.0))?;
    let matrix = ffi::dom_matrix(scope, IDENTITY_PARTS);
    scope.set(&t, "matrix", matrix)?;
    for (name, arity) in [
        ("setMatrix", 1),
        ("setTranslate", 2),
        ("setScale", 2),
        ("setRotate", 3),
        ("setSkewX", 1),
        ("setSkewY", 1),
    ] {
        let f = scope.function(name, arity, noop);
        scope.set(&t, name, f)?;
    }
    Ok(t)
}

pub(crate) fn begin_element(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(n) = ffi::unwrap_node(this).filter(|n| n.flags() & FLAG_SVG_NS != 0) else {
        return Ok(Value::undefined());
    };
    let Some(parent) = n.parent() else {
        return Ok(Value::undefined());
    };
    let attr = n.attr(c"attributeName").filter(|a| !a.is_empty());
    if let (Some(attr), Some(to)) = (attr, n.attr(c"to")) {
        let mut name = b"data-nd-anim-".to_vec();
        name.extend_from_slice(attr.to_bytes());
        name.truncate(ANIM_ATTR_MAX);
        ffi::set_attr_recorded(ffi::js_of(scope), parent, &name, to.to_bytes());
    }
    Ok(Value::undefined())
}

pub(crate) fn set_current_time(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::undefined())
}
