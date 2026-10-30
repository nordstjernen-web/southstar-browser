//! Southstar — the page as cached tile and viewport-layer textures, updated from the renderer's layer descriptions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use southstar_gtk_ffi::{GdkRgba, GrapheneRect};

use crate::ffi::gdk::{Canvas, Texture};

const STICKY: i32 = 2;
const MAX_VIEW_LAYERS: i32 = 256;
const MAX_PARTS: usize = 4096;
const MAX_KEEPS: usize = 4096;
const MAX_SCROLLERS: usize = 4096;
const MAX_TILES: usize = 4096;
const MAX_CACHED_BYTES: usize = 1 << 30;
const MAX_SCALE: f64 = 64.0;
const MIN_TILE_H: i32 = 16;
const MAX_TILE_H: i32 = 8192;

struct Part {
    layer: i32,
    x: i32,
    y: i32,
    tex: Texture,
    bytes: usize,
}

struct Tile {
    generation: i32,
    parts: Vec<Part>,
}

#[derive(Default)]
struct ViewLayer {
    kind: i32,
    origin: i32,
    x: i32,
    y: i32,
    keep: bool,
    x_offset: f64,
    has_top: bool,
    has_bottom: bool,
    top_start: f64,
    top_cap: f64,
    bottom_start: f64,
    bottom_cap: f64,
    tex: Option<Texture>,
    bytes: usize,
}

struct Scroller {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    axes: i32,
    fixed: bool,
}

#[derive(Default)]
pub struct LayerUpdate {
    generation: i32,
    width: i32,
    tile_h: i32,
    scale: f64,
    sx: i64,
    fresh: bool,
    n_vp: i32,
    canvas: GdkRgba,
    vp: Vec<ViewLayer>,
    tiles: Vec<(i64, Tile)>,
    keeps: Vec<i64>,
    scrollers: Vec<Scroller>,
    scrollers_all: bool,
    parts: usize,
    budget: usize,
}

#[derive(Default)]
pub struct PageLayers {
    active: bool,
    generation: i32,
    width: i32,
    tile_h: i32,
    scale: f64,
    sx: i64,
    n_vp: i32,
    canvas: GdkRgba,
    vp: Vec<ViewLayer>,
    tiles: HashMap<i64, Tile>,
    scrollers: Vec<Scroller>,
    scrollers_all: bool,
}

struct Scan<'a> {
    rest: &'a [u8],
}

impl<'a> Scan<'a> {
    fn after(line: &'a [u8], literal: &[u8]) -> Option<Scan<'a>> {
        line.strip_prefix(literal).map(|rest| Scan { rest })
    }

    fn skip_space(&mut self) {
        while let [c, rest @ ..] = self.rest {
            if !matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
                break;
            }
            self.rest = rest;
        }
    }

    fn take(&mut self, len: usize) -> &'a [u8] {
        let (taken, rest) = self.rest.split_at(len);
        self.rest = rest;
        taken
    }

    fn digits_from(&self, start: usize) -> usize {
        self.rest[start..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count()
    }

    fn int(&mut self) -> Option<i64> {
        self.skip_space();
        let sign = usize::from(matches!(self.rest.first(), Some(b'+' | b'-')));
        let digits = self.digits_from(sign);
        if digits == 0 {
            return None;
        }
        let text = self.take(sign + digits);
        let negative = text[0] == b'-';
        let magnitude = text[sign..].iter().fold(0i64, |n, &d| {
            n.wrapping_mul(10).wrapping_add(i64::from(d - b'0'))
        });
        Some(if negative {
            magnitude.wrapping_neg()
        } else {
            magnitude
        })
    }

    fn i32(&mut self) -> Option<i32> {
        self.int().map(|n| n as i32)
    }

    fn float(&mut self) -> Option<f64> {
        self.skip_space();
        let sign = usize::from(matches!(self.rest.first(), Some(b'+' | b'-')));
        let word = &self.rest[sign..];
        for special in [&b"infinity"[..], b"inf", b"nan"] {
            if word.len() >= special.len() && word[..special.len()].eq_ignore_ascii_case(special) {
                let negative = self.rest[0] == b'-';
                self.take(sign + special.len());
                let value = if special == b"nan" {
                    f64::NAN
                } else {
                    f64::INFINITY
                };
                return Some(if negative { -value } else { value });
            }
        }
        let int_digits = self.digits_from(sign);
        let mut len = sign + int_digits;
        let mut frac_digits = 0;
        if self.rest.get(len) == Some(&b'.') {
            frac_digits = self.digits_from(len + 1);
            len += 1 + frac_digits;
        }
        if int_digits == 0 && frac_digits == 0 {
            return None;
        }
        if matches!(self.rest.get(len), Some(b'e' | b'E')) {
            let exp_sign = usize::from(matches!(self.rest.get(len + 1), Some(b'+' | b'-')));
            let exp_digits = self.digits_from(len + 1 + exp_sign);
            if exp_digits > 0 {
                len += 1 + exp_sign + exp_digits;
            }
        }
        let text = self.take(len);
        core::str::from_utf8(text).ok()?.parse().ok()
    }
}

impl LayerUpdate {
    fn texture(&mut self, map: &[u8], at: i64, width: i32, rows: i32) -> Option<(Texture, usize)> {
        if width <= 0 || rows <= 0 {
            return None;
        }
        let off = usize::try_from(at).ok().filter(|&off| off <= map.len())?;
        let row = width as usize * 4;
        let len = (rows as usize).checked_mul(row)?;
        if len > map.len() - off || len > self.budget {
            return None;
        }
        self.budget -= len;
        Texture::from_bytes(width, rows, &map[off..off + len], row).map(|tex| (tex, len))
    }

    pub fn parse(desc: &[u8], map: &[u8]) -> Option<LayerUpdate> {
        let mut lines = desc.split(|&c| c == b'\n');
        let mut u = LayerUpdate {
            budget: map.len(),
            ..LayerUpdate::default()
        };
        u.parse_gen(lines.next()?)?;
        for line in lines {
            u.parse_line(line, map);
        }
        if u.fresh && u.vp.len() as i64 != i64::from(u.n_vp) {
            return None;
        }
        Some(u)
    }

    fn parse_gen(&mut self, line: &[u8]) -> Option<()> {
        let mut s = Scan::after(line, b"gen")?;
        self.generation = s.i32()?;
        self.width = s.i32()?;
        self.tile_h = s.i32()?;
        self.scale = s.float()?;
        self.sx = s.int()?;
        let fresh = s.i32()?;
        self.n_vp = s.i32()?;
        let r = s.i32()?;
        let g = s.i32()?;
        let b = s.i32()?;
        self.fresh = fresh != 0;
        self.canvas = GdkRgba {
            red: r as f32 / 255.0,
            green: g as f32 / 255.0,
            blue: b as f32 / 255.0,
            alpha: 1.0,
        };
        (self.width > 0
            && (MIN_TILE_H..=MAX_TILE_H).contains(&self.tile_h)
            && self.scale > 0.0
            && self.scale <= MAX_SCALE
            && (0..=MAX_VIEW_LAYERS).contains(&self.n_vp))
        .then_some(())
    }

    fn parse_line(&mut self, line: &[u8], map: &[u8]) {
        if line.starts_with(b"tile ") {
            self.parse_tile(line, map);
        } else if line.starts_with(b"keep ") {
            self.parse_keep(line);
        } else if line.starts_with(b"vp ") {
            self.parse_vp(line, map);
        } else if line.starts_with(b"sr") {
            self.parse_scroller(line);
        }
    }

    fn parse_vp(&mut self, line: &[u8], map: &[u8]) -> Option<()> {
        if self.vp.len() >= MAX_VIEW_LAYERS as usize {
            return None;
        }
        let mut s = Scan::after(line, b"vp")?;
        let mut l = ViewLayer::default();
        s.i32()?;
        l.kind = s.i32()?;
        l.origin = s.i32()?;
        l.x = s.i32()?;
        l.y = s.i32()?;
        let w = s.i32()?;
        let h = s.i32()?;
        let at = s.int()?;
        l.x_offset = s.float()?;
        l.has_top = s.i32()? != 0;
        l.top_start = s.float()?;
        l.top_cap = s.float()?;
        l.has_bottom = s.i32()? != 0;
        l.bottom_start = s.float()?;
        l.bottom_cap = s.float()?;
        l.keep = at < 0;
        if let Some((tex, bytes)) = self.texture(map, at, w, h) {
            l.tex = Some(tex);
            l.bytes = bytes;
        }
        self.vp.push(l);
        Some(())
    }

    fn tile(&mut self, index: i64) -> &mut Tile {
        let at = match self.tiles.iter().position(|(k, _)| *k == index) {
            Some(at) => at,
            None => {
                self.tiles.push((
                    index,
                    Tile {
                        generation: self.generation,
                        parts: Vec::new(),
                    },
                ));
                self.tiles.len() - 1
            }
        };
        &mut self.tiles[at].1
    }

    fn parse_tile(&mut self, line: &[u8], map: &[u8]) -> Option<()> {
        if self.parts >= MAX_PARTS {
            return None;
        }
        let mut s = Scan::after(line, b"tile")?;
        let index = s.int()?;
        let layer = s.i32()?;
        let x = s.i32()?;
        let y = s.i32()?;
        let w = s.i32()?;
        let h = s.i32()?;
        let at = s.int()?;
        if layer < 0 || layer > self.n_vp {
            return None;
        }
        let (tex, bytes) = self.texture(map, at, w, h)?;
        self.parts += 1;
        self.tile(index).parts.push(Part {
            layer,
            x,
            y,
            tex,
            bytes,
        });
        Some(())
    }

    fn parse_scroller(&mut self, line: &[u8]) -> Option<()> {
        if line == b"sr-all" {
            self.scrollers_all = true;
            return Some(());
        }
        if self.scrollers.len() >= MAX_SCROLLERS {
            return None;
        }
        let mut s = Scan::after(line, b"sr")?;
        let x = s.i32()?;
        let y = s.i32()?;
        let w = s.i32()?;
        let h = s.i32()?;
        let axes = s.i32()?;
        let fixed = s.i32()?;
        self.scrollers.push(Scroller {
            x: f64::from(x),
            y: f64::from(y),
            w: f64::from(w),
            h: f64::from(h),
            axes,
            fixed: fixed != 0,
        });
        Some(())
    }

    fn parse_keep(&mut self, line: &[u8]) -> Option<()> {
        if self.keeps.len() >= MAX_KEEPS {
            return None;
        }
        let index = Scan::after(line, b"keep")?.int()?;
        self.keeps.push(index);
        Some(())
    }
}

fn sticky_offset(l: &ViewLayer, scroll_y: f64) -> f64 {
    let mut dy = 0.0;
    if l.has_top && scroll_y > l.top_start {
        dy = c_min(scroll_y - l.top_start, l.top_cap);
    }
    if l.has_bottom && dy == 0.0 && scroll_y < l.bottom_start {
        dy = c_max(scroll_y - l.bottom_start, l.bottom_cap);
    }
    dy
}

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn append(canvas: &mut dyn Canvas, tex: &Texture, x: f64, y: f64, raster: f64) {
    let r = GrapheneRect::new(
        (x / raster) as f32,
        (y / raster) as f32,
        (f64::from(tex.width()) / raster) as f32,
        (f64::from(tex.height()) / raster) as f32,
    );
    canvas.append_texture(tex, &r);
}

impl PageLayers {
    pub fn reset(&mut self) {
        self.active = false;
        self.vp.clear();
        self.tiles.clear();
        self.scrollers.clear();
        self.scrollers_all = false;
    }

    fn same_geometry(&self, u: &LayerUpdate) -> bool {
        self.active
            && self.width == u.width
            && self.tile_h == u.tile_h
            && self.scale == u.scale
            && self.sx == u.sx
            && self.n_vp == u.n_vp
    }

    fn keep_view_layers(&self, u: &mut LayerUpdate) {
        for (l, old) in u.vp.iter_mut().zip(&self.vp) {
            if l.keep && l.tex.is_none() {
                l.tex = old.tex.clone();
                l.bytes = old.bytes;
            }
        }
    }

    fn adopt_geometry(&mut self, u: &mut LayerUpdate) {
        if self.same_geometry(u) {
            self.keep_view_layers(u);
        } else {
            self.tiles.clear();
        }
        self.active = true;
        self.generation = u.generation;
        self.width = u.width;
        self.tile_h = u.tile_h;
        self.scale = u.scale;
        self.sx = u.sx;
        self.n_vp = u.n_vp;
        self.canvas = u.canvas;
        core::mem::swap(&mut self.vp, &mut u.vp);
        core::mem::swap(&mut self.scrollers, &mut u.scrollers);
        self.scrollers_all = u.scrollers_all;
    }

    pub fn apply(&mut self, u: &mut LayerUpdate) -> bool {
        if u.fresh {
            self.adopt_geometry(u);
        } else if !self.active || u.generation != self.generation {
            return false;
        }
        let changed = u.fresh || !u.tiles.is_empty() || !u.keeps.is_empty();
        for (index, tile) in u.tiles.drain(..) {
            self.tiles.insert(index, tile);
        }
        for key in &u.keeps {
            if let Some(t) = self.tiles.get_mut(key) {
                t.generation = u.generation;
            }
        }
        if self.tiles.len() > MAX_TILES || self.cached_bytes() > MAX_CACHED_BYTES {
            self.reset();
        }
        changed
    }

    fn cached_bytes(&self) -> usize {
        let tiles = self.tiles.values().flat_map(|t| &t.parts).map(|p| p.bytes);
        let layers = self.vp.iter().map(|l| l.bytes);
        tiles.chain(layers).fold(0, usize::saturating_add)
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn generation(&self) -> i32 {
        if self.active { self.generation } else { -1 }
    }

    pub fn vp_held(&self) -> i32 {
        if self.active { self.n_vp } else { -1 }
    }

    fn tile_of(&self, y: f64) -> i64 {
        (y * self.scale / f64::from(self.tile_h)).floor() as i64
    }

    pub fn have(&self, y0: f64, y1: f64, any_gen: bool) -> String {
        let mut out = String::new();
        if self.active {
            for k in self.tile_of(y0).max(0)..=self.tile_of(y1) {
                if let Some(t) = self.tiles.get(&k)
                    && (any_gen || t.generation == self.generation)
                {
                    if !out.is_empty() {
                        out.push(',');
                    }
                    out.push_str(&k.to_string());
                }
            }
        }
        out
    }

    pub fn missing(&self, y0: f64, y1: f64, view_y0: f64, view_y1: f64, page_h: f64) -> bool {
        if !self.active {
            return false;
        }
        let last = ((page_h * self.scale / f64::from(self.tile_h)).ceil() as i64 - 1).max(0);
        let k1 = self.tile_of(y1).min(last);
        let v0 = self.tile_of(view_y0);
        let v1 = self.tile_of(view_y1);
        (self.tile_of(y0).max(0)..=k1).any(|k| match self.tiles.get(&k) {
            None => true,
            Some(t) => k >= v0 && k <= v1 && t.generation != self.generation,
        })
    }

    pub fn evict(&mut self, y0: f64, y1: f64) {
        if !self.active {
            return;
        }
        let k0 = self.tile_of(y0);
        let k1 = self.tile_of(y1);
        self.tiles.retain(|&k, _| k >= k0 && k <= k1);
    }

    pub fn scroller_at(
        &self,
        x: f64,
        y: f64,
        scroll_x: f64,
        scroll_y: f64,
        vertical: bool,
    ) -> bool {
        if !self.active {
            return false;
        }
        if self.scrollers_all {
            return true;
        }
        let axis = if vertical { 2 } else { 1 };
        self.scrollers.iter().any(|s| {
            let (px, py) = if s.fixed {
                (x, y)
            } else {
                (x + scroll_x, y + scroll_y)
            };
            (s.axes & axis) != 0 && px >= s.x && px < s.x + s.w && py >= s.y && py < s.y + s.h
        })
    }

    fn snapshot_view_layer(
        &self,
        l: &ViewLayer,
        canvas: &mut dyn Canvas,
        raster: f64,
        ox: f64,
        oy: f64,
        scroll_y: f64,
    ) {
        let Some(tex) = &l.tex else {
            return;
        };
        if l.kind != STICKY {
            append(
                canvas,
                tex,
                f64::from(l.x),
                f64::from(l.origin + l.y),
                raster,
            );
            return;
        }
        let dy = (sticky_offset(l, scroll_y) * self.scale).round();
        let dx = (l.x_offset * self.scale).round();
        append(
            canvas,
            tex,
            f64::from(l.x) + dx - ox,
            f64::from(l.origin + l.y) + dy - oy,
            raster,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn snapshot_doc_layer(
        &self,
        layer: i32,
        canvas: &mut dyn Canvas,
        raster: f64,
        ox: f64,
        oy: f64,
        k0: i64,
        k1: i64,
    ) {
        for k in k0..=k1 {
            let Some(t) = self.tiles.get(&k) else {
                continue;
            };
            for part in t.parts.iter().filter(|p| p.layer == layer) {
                append(
                    canvas,
                    &part.tex,
                    f64::from(part.x) - ox,
                    k as f64 * f64::from(self.tile_h) + f64::from(part.y) - oy,
                    raster,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn snapshot(
        &self,
        canvas: &mut dyn Canvas,
        width: f64,
        height: f64,
        raster: f64,
        scroll_x: f64,
        scroll_y: f64,
    ) {
        if !self.active || raster <= 0.0 || raster.is_nan() {
            return;
        }
        let area = GrapheneRect::new(0.0, 0.0, width as f32, height as f32);
        canvas.append_color(&self.canvas, &area);
        let oy = (scroll_y * self.scale).round();
        let ox = ((scroll_x - self.sx as f64) * self.scale).round();
        let tile_h = f64::from(self.tile_h);
        let k0 = (oy / tile_h).floor() as i64;
        let k1 = ((oy + height * raster - 1.0) / tile_h).floor() as i64;
        canvas.push_clip(&area);
        canvas.under(ox, oy);
        for layer in 0..=self.n_vp {
            if layer > 0
                && let Some(l) = self.vp.get(layer as usize - 1)
            {
                self.snapshot_view_layer(l, canvas, raster, ox, oy, scroll_y);
            }
            self.snapshot_doc_layer(layer, canvas, raster, ox, oy, k0, k1);
        }
        canvas.pop();
    }
}
