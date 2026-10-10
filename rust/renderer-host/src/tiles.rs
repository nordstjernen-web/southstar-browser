//! Southstar — painting a page into shared-memory tiles and viewport layers, and the text that describes them to the window.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_long};
use std::collections::HashMap;
use std::io::Write;
use std::time::{Duration, Instant};

use southstar_ipc::{atol, json_string, json_value, strtol};

const PREFETCH_MAX: i32 = 4;
const PREFETCH_BUDGET: Duration = Duration::from_micros(8000);
const LIST_MAX: usize = 128;
const SCROLLERS_MAX: c_int = 96;

#[repr(C)]
pub struct View {
    pub sx: c_long,
    pub sy: c_long,
    pub vw: c_int,
    pub vh: c_int,
    pub scale: f64,
    pub page_h: c_int,
}

#[derive(Clone, Copy, Default)]
pub struct Sticky {
    pub has_top: bool,
    pub has_bottom: bool,
    pub top_start: f64,
    pub top_cap: f64,
    pub bottom_start: f64,
    pub bottom_cap: f64,
}

#[derive(Clone, Copy, Default)]
pub struct VpLayer {
    pub kind: c_int,
    pub top: f64,
    pub bottom: f64,
    pub x_offset: f64,
    pub sticky: Sticky,
}

pub trait LayerPlan {
    fn vp_count(&self) -> usize;
}

pub struct TileTarget<'a, 'b> {
    pub sx: c_int,
    pub tile_y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub scale: f64,
    pub bufs: &'a mut [&'b mut [u8]],
    pub stride: c_int,
    pub upper_used: &'a mut [c_int],
}

pub struct LayerTarget<'a> {
    pub sx: c_int,
    pub sy: c_int,
    pub origin_y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub scale: f64,
    pub out: &'a mut [u8],
    pub stride: c_int,
}

pub trait Page {
    type Plan: LayerPlan;
    fn note_viewport(&mut self, sx: c_int, sy: c_int, height: c_int, scale: f64);
    fn prepare_layers(
        &mut self,
        plan: &mut Self::Plan,
        sx: c_int,
        sy: c_int,
        width: c_int,
        height: c_int,
        scale: f64,
    ) -> c_int;
    fn vp_layer(&mut self, plan: &Self::Plan, index: usize) -> Option<VpLayer>;
    fn render_doc_tile(&mut self, plan: &Self::Plan, target: TileTarget<'_, '_>) -> c_int;
    fn render_vp_layer(
        &mut self,
        plan: &Self::Plan,
        index: usize,
        target: LayerTarget<'_>,
    ) -> c_int;
    fn canvas_color(&mut self, rgba: &mut [f64; 4]);
    fn scroller_rects(&mut self, out: &mut Vec<u8>, max_rects: c_int);
    fn flush_video_rects(&mut self);
}

struct Request {
    tile_h: c_long,
    want_y0: c_long,
    want_y1: c_long,
    generation: c_long,
    vp_held: c_long,
    have: Vec<c_long>,
    hold: Vec<c_long>,
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

fn long_field(body: &[u8], key: &str, default: c_long) -> c_long {
    json_value(body, key.as_bytes()).map_or(default, atol)
}

fn list_field(body: &[u8], key: &str) -> Vec<c_long> {
    let Some(text) = json_value(body, key.as_bytes()).and_then(json_string) else {
        return Vec::new();
    };
    let text = until_nul(&text);
    let mut list = Vec::new();
    let mut at = 0;
    while at < text.len() && list.len() < LIST_MAX {
        let (value, used) = strtol(&text[at..]);
        if used == 0 {
            break;
        }
        list.push(value);
        at += used;
        if text.get(at) == Some(&b',') {
            at += 1;
        }
    }
    list
}

pub fn tiles_requested(body: &[u8]) -> bool {
    long_field(body, "tiles", 0) != 0
}

impl Request {
    fn parse(body: &[u8]) -> Option<Request> {
        let request = Request {
            tile_h: long_field(body, "tile_h", 0),
            want_y0: long_field(body, "want_y0", 0),
            want_y1: long_field(body, "want_y1", 0),
            generation: long_field(body, "gen", -1),
            vp_held: long_field(body, "vp_held", -1),
            have: list_field(body, "have"),
            hold: list_field(body, "hold"),
        };
        (64..=2048).contains(&request.tile_h).then_some(request)
    }

    fn wanted(&self, fresh: bool, k: c_long, distance: c_long) -> bool {
        if !fresh && self.have.contains(&k) {
            return false;
        }
        distance == 0 || !self.hold.contains(&k)
    }
}

#[derive(Clone, Copy, Default)]
struct Rect {
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
}

fn hash(bytes: &[u8], seed: u64) -> u64 {
    const K: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut lane = [
        seed,
        seed ^ K,
        seed ^ 0xc2b2_ae3d_27d4_eb4f,
        seed ^ 0x1656_67b1_9e37_79f9,
    ];
    let (blocks, tail) = bytes.as_chunks::<32>();
    for block in blocks {
        for (l, &word) in block.as_chunks::<8>().0.iter().enumerate() {
            let word = u64::from_ne_bytes(word);
            lane[l] = (lane[l] ^ word).wrapping_mul(K);
            lane[l] ^= lane[l] >> 29;
        }
    }
    for &byte in tail {
        lane[0] = (lane[0] ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    lane[0] ^ lane[1].wrapping_mul(3) ^ lane[2].wrapping_mul(5) ^ lane[3].wrapping_mul(7)
}

fn pixel_set(row: &[u8], x: usize) -> bool {
    row[x * 4..x * 4 + 4] != [0, 0, 0, 0]
}

fn crop(buf: &[u8], width: c_int, rows: c_int, row_bytes: usize) -> Rect {
    let width = usize::try_from(width).unwrap_or(0);
    let rows = usize::try_from(rows).unwrap_or(0);
    let row = |y: usize| &buf[y * row_bytes..y * row_bytes + width * 4];
    let clear = |y: usize| row(y).iter().all(|&b| b == 0);
    let mut top = 0;
    while top < rows && clear(top) {
        top += 1;
    }
    let mut bottom = rows;
    while bottom > top && clear(bottom - 1) {
        bottom -= 1;
    }
    if top == bottom {
        return Rect::default();
    }
    let (mut left, mut right) = (width, 0);
    for y in top..bottom {
        let line = row(y);
        left = (0..left.min(width))
            .find(|&x| pixel_set(line, x))
            .unwrap_or(left);
        right = (right..width)
            .rev()
            .find(|&x| pixel_set(line, x))
            .map_or(right, |x| x + 1);
    }
    Rect {
        x: left as c_int,
        y: top as c_int,
        w: right as c_int - left as c_int,
        h: (bottom - top) as c_int,
    }
}

struct Out<'a> {
    desc: &'a mut Vec<u8>,
    fb: &'a mut [u8],
    off: usize,
    row_bytes: usize,
    width: c_int,
}

impl Out<'_> {
    fn fits(&self, rows: usize) -> bool {
        rows.checked_mul(self.row_bytes)
            .and_then(|n| n.checked_add(self.off))
            .is_some_and(|end| end <= self.fb.len())
    }

    fn pack(&mut self, src: usize, r: Rect, seed: &mut u64) -> usize {
        let at = self.off;
        let line = usize::try_from(r.w).unwrap_or(0) * 4;
        let rows = usize::try_from(r.h).unwrap_or(0);
        for y in 0..rows {
            let from = src + (r.y as usize + y) * self.row_bytes + r.x as usize * 4;
            let to = at + y * line;
            if from != to {
                self.fb.copy_within(from..from + line, to);
            }
        }
        self.off += rows * line;
        self.off = (self.off + 15) & !15;
        let meta: Vec<u8> = [r.x, r.y, r.w, r.h]
            .iter()
            .flat_map(|&v| (v as u64).to_ne_bytes())
            .collect();
        *seed = hash(&self.fb[at..at + rows * line], hash(&meta, *seed));
        at
    }

    fn text(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.desc.write_fmt(args);
    }
}

fn sticky_desc(info: &VpLayer) -> String {
    let m = &info.sticky;
    format!(
        " {:.3} {} {:.3} {:.3} {} {:.3} {:.3}\n",
        info.x_offset,
        c_int::from(m.has_top),
        m.top_start,
        m.top_cap,
        c_int::from(m.has_bottom),
        m.bottom_start,
        m.bottom_cap
    )
}

pub struct Tiles<Plan> {
    state: c_int,
    generation: c_int,
    width: c_int,
    vh: c_int,
    tile_h: c_int,
    sx: c_long,
    scale: f64,
    plan: Plan,
    sent: HashMap<c_long, u64>,
    vp_sent: Vec<u64>,
}

impl<Plan: LayerPlan> Tiles<Plan> {
    pub fn new(plan: Plan) -> Tiles<Plan> {
        Tiles {
            state: 0,
            generation: 0,
            width: 0,
            vh: 0,
            tile_h: 0,
            sx: 0,
            scale: 0.0,
            plan,
            sent: HashMap::new(),
            vp_sent: Vec::new(),
        }
    }

    fn geometry_changed(&self, v: &View, r: &Request) -> bool {
        self.width != v.vw
            || self.vh != v.vh
            || c_long::from(self.tile_h) != r.tile_h
            || self.sx != v.sx
            || self.scale != v.scale
    }

    fn plan_usable<P: Page<Plan = Plan>>(&self, page: &mut P) -> bool {
        (0..self.plan.vp_count()).all(|j| page.vp_layer(&self.plan, j).is_some())
    }

    fn prepare<P: Page<Plan = Plan>>(
        &mut self,
        page: &mut P,
        v: &View,
        r: &Request,
        invalid: bool,
    ) {
        let geometry = self.geometry_changed(v, r);
        if self.state != 0 && !invalid && !geometry {
            page.note_viewport(v.sx as c_int, v.sy as c_int, v.vh, v.scale);
            return;
        }
        let old_vp = self.plan.vp_count();
        self.generation = self.generation.wrapping_add(1);
        self.width = v.vw;
        self.vh = v.vh;
        self.tile_h = r.tile_h as c_int;
        self.sx = v.sx;
        self.scale = v.scale;
        let rc = page.prepare_layers(
            &mut self.plan,
            v.sx as c_int,
            v.sy as c_int,
            v.vw,
            v.vh,
            v.scale,
        );
        self.state = if rc == 0 && self.plan_usable(page) {
            1
        } else {
            -1
        };
        if geometry || old_vp != self.plan.vp_count() {
            self.sent.clear();
            self.vp_sent.clear();
        }
        self.vp_sent.resize(self.plan.vp_count(), 0);
    }

    fn write_vp_layer<P: Page<Plan = Plan>>(
        &mut self,
        page: &mut P,
        v: &View,
        r: &Request,
        o: &mut Out<'_>,
        j: usize,
    ) {
        let info = page.vp_layer(&self.plan, j).unwrap_or_default();
        let origin = (info.top * v.scale).floor() as c_int;
        let rows = ((info.bottom * v.scale).ceil() as c_int).wrapping_sub(origin);
        let src = o.off;
        let mut rect = Rect::default();
        if rows > 0 && o.fits(rows as usize) {
            let end = src + rows as usize * o.row_bytes;
            let target = LayerTarget {
                sx: v.sx as c_int,
                sy: v.sy as c_int,
                origin_y: origin,
                width: v.vw,
                height: rows,
                scale: v.scale,
                out: &mut o.fb[src..end],
                stride: o.row_bytes as c_int,
            };
            if page.render_vp_layer(&self.plan, j, target) == 0 {
                rect = crop(&o.fb[src..], v.vw, rows, o.row_bytes);
            }
        }
        let mut digest = 0x51ed27;
        let at = o.pack(src, rect, &mut digest);
        let keep = r.vp_held == self.plan.vp_count() as c_long && self.vp_sent[j] == digest;
        if keep {
            o.off = src;
        }
        self.vp_sent[j] = digest;
        let offset = if keep { -1 } else { at as i64 };
        o.text(format_args!(
            "vp {j} {} {origin} {} {} {} {} {offset}",
            info.kind, rect.x, rect.y, rect.w, rect.h
        ));
        o.text(format_args!("{}", sticky_desc(&info)));
    }

    fn write_upper(
        o: &mut Out<'_>,
        lines: &mut String,
        k: c_long,
        layer: usize,
        src: usize,
        th: c_int,
        seed: &mut u64,
    ) {
        let r = crop(&o.fb[src..], o.width, th, o.row_bytes);
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let at = o.pack(src, r, seed);
        lines.push_str(&format!(
            "tile {k} {layer} {} {} {} {} {at}\n",
            r.x, r.y, r.w, r.h
        ));
    }

    fn paint_one<P: Page<Plan = Plan>>(
        &mut self,
        page: &mut P,
        v: &View,
        r: &Request,
        o: &mut Out<'_>,
        k: c_long,
    ) -> bool {
        let n_upper = self.plan.vp_count();
        let th = self.tile_h;
        let th_rows = usize::try_from(th).unwrap_or(0);
        if !th_rows
            .checked_mul(n_upper + 1)
            .is_some_and(|rows| o.fits(rows))
        {
            return false;
        }
        let start = o.off;
        let chunk = th_rows * o.row_bytes;
        let mut used = vec![0; n_upper.max(1)];
        let rc = {
            let mut rest = &mut o.fb[start..start + chunk * (n_upper + 1)];
            let mut bufs: Vec<&mut [u8]> = Vec::with_capacity(n_upper + 1);
            for _ in 0..=n_upper {
                let (head, tail) = core::mem::take(&mut rest).split_at_mut(chunk);
                bufs.push(head);
                rest = tail;
            }
            let target = TileTarget {
                sx: v.sx as c_int,
                tile_y: k.wrapping_mul(c_long::from(th)) as c_int,
                width: v.vw,
                height: th,
                scale: v.scale,
                bufs: &mut bufs,
                stride: o.row_bytes as c_int,
                upper_used: &mut used,
            };
            page.render_doc_tile(&self.plan, target)
        };
        if rc == -2 {
            self.state = -1;
        }
        if rc == 0 {
            let mut digest = 0x7e57;
            let full = Rect {
                x: 0,
                y: 0,
                w: v.vw,
                h: th,
            };
            let at = o.pack(start, full, &mut digest);
            let mut lines = format!("tile {k} 0 0 0 {} {th} {at}\n", v.vw);
            for (i, &upper) in used.iter().enumerate().take(n_upper) {
                if upper != 0 {
                    Self::write_upper(
                        o,
                        &mut lines,
                        k,
                        i + 1,
                        start + (i + 1) * chunk,
                        th,
                        &mut digest,
                    );
                }
            }
            if self.sent.get(&k) == Some(&digest) && r.hold.contains(&k) {
                o.off = start;
                o.text(format_args!("keep {k}\n"));
            } else {
                o.desc.extend_from_slice(lines.as_bytes());
                self.sent.insert(k, digest);
            }
        }
        rc == 0
    }

    fn write_doc<P: Page<Plan = Plan>>(
        &mut self,
        page: &mut P,
        v: &View,
        r: &Request,
        fresh: bool,
        o: &mut Out<'_>,
    ) {
        let th = c_long::from(self.tile_h);
        let thf = th as f64;
        let last = ((f64::from(v.page_h) * v.scale / thf).ceil() as c_long - 1).max(0);
        let v0 = (v.sy as f64 * v.scale / thf).floor() as c_long;
        let v1 = ((v.sy as f64 * v.scale + f64::from(v.vh) - 1.0) / thf).floor() as c_long;
        let v1 = v1.min(last);
        let w0 = ((r.want_y0 as f64 * v.scale / thf).floor() as c_long)
            .max(0)
            .min(v0);
        let w1 = ((r.want_y1 as f64 * v.scale / thf).floor() as c_long)
            .min(last)
            .max(v1);
        let t0 = Instant::now();
        let mut prefetched = 0;
        for d in 0..=(v0 - w0).max(w1 - v1) {
            for k in w0..=w1 {
                let distance = if k < v0 {
                    v0 - k
                } else if k > v1 {
                    k - v1
                } else {
                    0
                };
                if distance != d || !r.wanted(fresh, k, d) {
                    continue;
                }
                if d > 0 && !(prefetched < PREFETCH_MAX && t0.elapsed() <= PREFETCH_BUDGET) {
                    return;
                }
                if !self.paint_one(page, v, r, o, k) {
                    return;
                }
                if d > 0 {
                    prefetched += 1;
                }
            }
        }
    }

    fn write_header<P: Page<Plan = Plan>>(
        &self,
        page: &mut P,
        v: &View,
        fresh: bool,
        desc: &mut Vec<u8>,
    ) {
        let mut bg = [1.0; 4];
        page.canvas_color(&mut bg);
        let channel = |c: f64| (c * 255.0).round() as c_long as c_int;
        let _ = writeln!(
            desc,
            "gen {} {} {} {:.6} {} {} {} {} {} {}",
            self.generation,
            v.vw,
            self.tile_h,
            v.scale,
            v.sx,
            c_int::from(fresh),
            self.plan.vp_count(),
            channel(bg[0]),
            channel(bg[1]),
            channel(bg[2])
        );
    }

    pub fn render<P: Page<Plan = Plan>>(
        &mut self,
        page: &mut P,
        body: &[u8],
        v: &View,
        invalid: bool,
        fb: &mut [u8],
        desc: &mut Vec<u8>,
    ) -> bool {
        let Some(r) = Request::parse(body) else {
            return false;
        };
        let t0 = Instant::now();
        self.prepare(page, v, &r, invalid);
        if self.state < 0 {
            return false;
        }
        let fresh = r.generation != c_long::from(self.generation);
        self.write_header(page, v, fresh, desc);
        let mut o = Out {
            desc,
            fb,
            off: 0,
            row_bytes: usize::try_from(v.vw).unwrap_or(0) * 4,
            width: v.vw,
        };
        if fresh {
            for j in 0..self.plan.vp_count() {
                self.write_vp_layer(page, v, &r, &mut o, j);
            }
            page.scroller_rects(o.desc, SCROLLERS_MAX);
        }
        self.write_doc(page, v, &r, fresh, &mut o);
        if self.state < 0 {
            return false;
        }
        page.flush_video_rects();
        if std::env::var_os("NS_PROFILE").is_some() {
            let _ = writeln!(
                std::io::stderr(),
                "[profile] tiles {:6.1}ms gen={} fresh={} bytes={}",
                t0.elapsed().as_micros() as f64 / 1000.0,
                self.generation,
                c_int::from(fresh),
                o.off
            );
        }
        true
    }
}
