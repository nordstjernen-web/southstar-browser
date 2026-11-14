//! Southstar — which shape of an inline svg lies under a point: the render walk run without painting, testing each path's fill and stroke.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_style::{Prop, StyleRef, StyleTable};

use crate::ffi::{Canvas, FILL_EVEN_ODD, FORMAT_A8, Surface};
use crate::parse::eq_ci;
use crate::smil::PathPoint;
use crate::state::{PaintKind, State};
use crate::{Ctx, current_polyline};

static POINTER_EVENTS: Prop = Prop::new(c"pointer-events");

pub struct Hit<'a> {
    x: f64,
    y: f64,
    node: Option<Node<'a>>,
    owner: Option<Node<'a>>,
}

fn subpaths(outline: &[PathPoint]) -> impl Iterator<Item = &[PathPoint]> {
    outline.chunk_by(|_, next| !next.starts_subpath)
}

fn winding(outline: &[PathPoint], x: f64, y: f64) -> i32 {
    let mut count = 0;
    for sub in subpaths(outline) {
        for (i, a) in sub.iter().enumerate() {
            let b = &sub[(i + 1) % sub.len()];
            let side = (b.x - a.x) * (y - a.y) - (x - a.x) * (b.y - a.y);
            if a.y <= y && b.y > y && side > 0.0 {
                count += 1;
            } else if a.y > y && b.y <= y && side < 0.0 {
                count -= 1;
            }
        }
    }
    count
}

fn segment_distance(a: &PathPoint, b: &PathPoint, x: f64, y: f64) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((x - a.x) * dx + (y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (x - (a.x + t * dx)).hypot(y - (a.y + t * dy))
}

fn on_stroke(outline: &[PathPoint], x: f64, y: f64, width: f64) -> bool {
    subpaths(outline).any(|sub| {
        sub.windows(2)
            .any(|w| segment_distance(&w[0], &w[1], x, y) <= width / 2.0)
    })
}

fn inside(outline: &[PathPoint], st: &State, x: f64, y: f64) -> bool {
    let turns = winding(outline, x, y);
    let filled = if st.fill_rule == FILL_EVEN_ODD {
        turns % 2 != 0
    } else {
        turns != 0
    };
    (st.fill.kind != PaintKind::None && filled)
        || (st.stroke.kind != PaintKind::None
            && st.stroke_width > 0.0
            && on_stroke(outline, x, y, st.stroke_width))
}

impl<'a> Ctx<'a> {
    fn hit_ignores(&self, n: Node<'a>) -> bool {
        match self.style(n).and_then(|s| s.keyword(&POINTER_EVENTS)) {
            Some(keyword) => eq_ci(keyword.to_bytes(), b"none"),
            None => self
                .prop(n, c"pointer-events")
                .is_some_and(|v| eq_ci(v.to_bytes(), b"none")),
        }
    }

    pub(crate) fn hit_current_path(&mut self, n: Node<'a>, st: &State) -> bool {
        let Some((x, y)) = self.hit.as_ref().map(|hit| (hit.x, hit.y)) else {
            return false;
        };
        let mut to_user = self.cr.matrix();
        let found = to_user.invert().is_ok() && !st.hidden && !self.hit_ignores(n) && {
            let (x, y) = to_user.transform_point(x, y);
            inside(&current_polyline(&self.cr), st, x, y)
        };
        if found && let Some(hit) = self.hit.as_mut() {
            hit.node = Some(hit.owner.unwrap_or(n));
        }
        self.cr.new_path();
        true
    }

    pub(crate) fn hit_enter_use(&mut self, instance: Node<'a>) -> Option<Node<'a>> {
        let hit = self.hit.as_mut()?;
        let outer = hit.owner;
        hit.owner = outer.or(Some(instance));
        outer
    }

    pub(crate) fn hit_leave_use(&mut self, outer: Option<Node<'a>>) {
        if let Some(hit) = self.hit.as_mut() {
            hit.owner = outer;
        }
    }
}

pub fn hit_node(
    svg: Node<'_>,
    width: f64,
    height: f64,
    styles: StyleTable,
    inherited: Option<StyleRef<'_>>,
    x: f64,
    y: f64,
) -> Option<Node<'static>> {
    if width <= 0.0 || height <= 0.0 || x < 0.0 || y < 0.0 || x > width || y > height {
        return None;
    }
    let surface = Surface::image(FORMAT_A8, 1, 1)?;
    let mut ctx = Ctx::new(Canvas::on(&surface), svg, styles, width, height);
    ctx.hit = Some(Hit {
        x,
        y,
        node: None,
        owner: None,
    });
    ctx.render_tree(inherited);
    ctx.hit?.node?.downgrade().get()
}
