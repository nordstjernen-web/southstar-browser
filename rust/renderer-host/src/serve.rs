//! Southstar — the renderer session: dispatching protocol requests to the open page, back/forward cache and frame bookkeeping.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long};
use std::ffi::CString;

use southstar_ipc::{
    atol, json_double, json_escape, json_string, json_value, response_head, write_message,
};

use crate::engine::{self, Browser, Plan, Post, PrintPages, PrintSetup};
use crate::tiles::{self, Tiles, View};

const BFCACHE_MAX: usize = 4;
const SCROLL_ACTIVE_US: i64 = 150_000;
const SCROLL_TICK_GAP_US: i64 = 250_000;
const ZOOM_MIN: f64 = 0.25;
const TICK_BUDGET_MS: c_int = 16;

pub trait Framebuffer {
    fn bytes(&mut self) -> &mut [u8];
}

struct StashedPost {
    url: Option<CString>,
    post: Post,
}

pub struct Session<F> {
    ctrl_w: c_int,
    fb: F,
    max_w: c_int,
    max_h: c_int,
    shm_mode: bool,
    cur: Option<Browser>,
    bf: Vec<Browser>,
    scroll_until_us: i64,
    last_tick_us: i64,
    tick_cost_us: i64,
    tick_deferred: bool,
    frame_valid: bool,
    frame_sx: c_long,
    frame_sy: c_long,
    frame_w: c_int,
    frame_h: c_int,
    frame_scale: f64,
    post: Option<StashedPost>,
    tiles: Tiles<Plan>,
}

#[derive(Default)]
struct RenderView {
    sx: c_long,
    sy: c_long,
    vw: c_int,
    vh: c_int,
    scale: f64,
    caret: bool,
    ticked: c_int,
    caret_changed: c_int,
    requested_x: c_int,
    requested_y: c_int,
    page_w: c_int,
    page_h: c_int,
    wheel_snapped: c_int,
    unchanged: bool,
    render_rc: c_int,
}

fn long(body: &[u8], key: &str) -> c_long {
    json_value(body, key.as_bytes()).map_or(0, atol)
}

fn double_or(body: &[u8], key: &str, default: f64) -> f64 {
    json_value(body, key.as_bytes()).map_or(default, json_double)
}

fn text(body: &[u8], key: &str) -> Option<CString> {
    json_value(body, key.as_bytes())
        .and_then(json_string)
        .map(|value| engine::cstring(&value))
}

fn escaped(value: Option<&[u8]>) -> String {
    String::from_utf8_lossy(&json_escape(value.unwrap_or_default())).into_owned()
}

fn request_device_pixel_ratio(body: &[u8]) -> f64 {
    let dpr = double_or(body, "dpr", 0.0);
    if dpr > 0.0 && dpr.is_finite() {
        dpr.clamp(0.25, 40.0)
    } else {
        0.0
    }
}

fn css_viewport_extent(css_px: c_long, max_device_px: c_int) -> c_int {
    let max_css_px = (f64::from(max_device_px) / ZOOM_MIN) as c_long;
    let v = if css_px > max_css_px {
        max_css_px
    } else if css_px < 1 {
        1
    } else {
        css_px
    };
    v as c_int
}

fn max_scroll(page_extent: c_int, view_px: c_int, scale: f64) -> c_int {
    let max = page_extent.wrapping_sub((f64::from(view_px) / scale).ceil() as c_int);
    max.max(0)
}

fn clamp_int(v: c_int, lo: c_int, hi: c_int) -> c_int {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

fn flatten_line(value: Option<Vec<u8>>, keep_lf: bool) -> Option<Vec<u8>> {
    value.map(|mut v| {
        for b in &mut v {
            if *b == b'\r' {
                *b = b' ';
            } else if *b == b'\n' {
                *b = if keep_lf { 0x1f } else { b' ' };
            }
        }
        v
    })
}

fn push_header(headers: &mut Vec<u8>, name: &str, value: Option<&[u8]>, max_value: usize) {
    const CAP: usize = 32768;
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return;
    };
    let value = &value[..value.len().min(max_value)];
    let need = name.len() + 2 + value.len() + 2;
    if headers.len() >= CAP || headers.len() + need >= CAP {
        return;
    }
    headers.extend_from_slice(name.as_bytes());
    headers.extend_from_slice(b": ");
    headers.extend_from_slice(value);
    headers.extend_from_slice(b"\r\n");
}

fn respond(fd: c_int, status: c_int, content_type: &str, extra: &[u8], body: &[u8]) {
    let head = response_head(status, content_type.as_bytes(), extra, body.len());
    write_message(fd, &head, body);
}

fn json_reply(fd: c_int, json: &str) {
    respond(fd, 200, "application/json", b"", json.as_bytes());
}

fn reply_str(fd: c_int, key: &str, value: Option<&[u8]>) {
    json_reply(fd, &format!("{{\"{key}\":\"{}\"}}", escaped(value)));
}

fn empty_ok(fd: c_int) {
    respond(fd, 200, "text/plain", b"", b"");
}

impl<F: Framebuffer> Session<F> {
    pub fn new(ctrl_w: c_int, fb: F, max_w: c_int, max_h: c_int, shm_mode: bool) -> Session<F> {
        Session {
            ctrl_w,
            fb,
            max_w,
            max_h,
            shm_mode,
            cur: None,
            bf: Vec::new(),
            scroll_until_us: 0,
            last_tick_us: 0,
            tick_cost_us: 0,
            tick_deferred: false,
            frame_valid: false,
            frame_sx: 0,
            frame_sy: 0,
            frame_w: 0,
            frame_h: 0,
            frame_scale: 1.0,
            post: None,
            tiles: Tiles::new(Plan::new()),
        }
    }

    pub fn busy(&mut self) -> bool {
        self.cur.as_mut().is_some_and(Browser::busy)
    }

    pub fn print(&mut self, setup: &mut PrintSetup) -> Option<PrintPages> {
        let cur = self.cur.as_mut()?;
        self.frame_valid = false;
        cur.print_pages(setup)
    }

    fn apply_device_pixel_ratio(&mut self, body: &[u8]) {
        let dpr = request_device_pixel_ratio(body);
        if dpr > 0.0 && engine::set_device_pixel_ratio(self.cur.as_mut(), dpr) > 0 {
            self.frame_valid = false;
        }
    }

    fn bfcache_park_or_close(&mut self, mut b: Browser) {
        if !b.bfcache_eligible() {
            return;
        }
        b.bfcache_park();
        if self.bf.len() >= BFCACHE_MAX {
            self.bf.remove(0);
        }
        self.bf.push(b);
    }

    fn bfcache_take(&mut self, url: &CStr) -> Option<Browser> {
        let index = (0..self.bf.len())
            .rev()
            .find(|&i| self.bf[i].url().is_some_and(|u| u == url.to_bytes()))?;
        Some(self.bf.remove(index))
    }

    fn stash_post(&mut self, href: Option<&[u8]>) {
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let Some(post) = cur.take_post() else {
            return;
        };
        let url = href.filter(|h| !h.is_empty()).map(engine::cstring);
        self.post = Some(StashedPost { url, post });
    }

    fn tick_deferred_at(&self, now: i64) -> bool {
        if now >= self.scroll_until_us {
            return false;
        }
        let gap = (self.tick_cost_us.wrapping_mul(4)).max(SCROLL_TICK_GAP_US);
        now - self.last_tick_us < gap
    }

    fn tick(&mut self) -> c_int {
        let now = engine::monotonic_us();
        if self.tick_deferred_at(now) {
            self.tick_deferred = true;
            return 0;
        }
        let Some(cur) = self.cur.as_mut() else {
            return 0;
        };
        let changed = cur.tick(TICK_BUDGET_MS);
        self.last_tick_us = engine::monotonic_us();
        self.tick_cost_us = self.last_tick_us - now;
        self.tick_deferred = false;
        changed
    }

    fn animating(&mut self) -> bool {
        self.cur.as_mut().is_some_and(Browser::animating) || self.tick_deferred
    }

    fn note_scroll(&mut self, sx: c_long, sy: c_long, wheel: bool) {
        if wheel || (self.frame_valid && (sx != self.frame_sx || sy != self.frame_sy)) {
            self.scroll_until_us = engine::monotonic_us() + SCROLL_ACTIVE_US;
        }
    }

    fn note_frame(&mut self, rv: &RenderView) {
        self.frame_valid = true;
        self.frame_sx = rv.sx;
        self.frame_sy = rv.sy;
        self.frame_w = rv.vw;
        self.frame_h = rv.vh;
        self.frame_scale = rv.scale;
    }

    fn view_parse(&self, body: &[u8]) -> RenderView {
        let mut scale = double_or(body, "scale", 1.0);
        if scale.is_nan() || scale <= 0.0 {
            scale = 1.0;
        }
        RenderView {
            sx: long(body, "scroll_x"),
            sy: long(body, "scroll_y"),
            vw: clamp_int(long(body, "width") as c_int, 1, self.max_w),
            vh: clamp_int(long(body, "height") as c_int, 1, self.max_h),
            scale,
            caret: long(body, "caret") != 0,
            requested_x: -1,
            requested_y: -1,
            ..RenderView::default()
        }
    }

    fn apply_pending_scroll(cur: &mut Browser, rv: &mut RenderView) {
        cur.take_pending_scroll(&mut rv.requested_x, &mut rv.requested_y);
        cur.page_size(&mut rv.page_w, &mut rv.page_h);
        if rv.requested_y >= 0 {
            rv.requested_y = rv.requested_y.min(max_scroll(rv.page_h, rv.vh, rv.scale));
            rv.sy = c_long::from(rv.requested_y);
        }
        if rv.requested_x >= 0 {
            rv.requested_x = rv.requested_x.min(max_scroll(rv.page_w, rv.vw, rv.scale));
            rv.sx = c_long::from(rv.requested_x);
        }
    }

    fn apply_wheel(&mut self, body: &[u8], rv: &mut RenderView, dx: c_long, dy: c_long) {
        if dx == 0 && dy == 0 {
            return;
        }
        let wheel_x = long(body, "wheel_x");
        let wheel_y = long(body, "wheel_y");
        let viewport = long(body, "wheel_viewport");
        if let Some(cur) = self.cur.as_mut() {
            if viewport == 0
                && cur.scroll_at_full(
                    wheel_x as c_int,
                    wheel_y as c_int,
                    dx as c_int,
                    dy as c_int,
                    &mut rv.wheel_snapped,
                )
            {
                self.frame_valid = false;
                return;
            }
        }
        let max_x = max_scroll(rv.page_w, rv.vw, rv.scale);
        let max_y = max_scroll(rv.page_h, rv.vh, rv.scale);
        let nx = c_long::from(clamp_int(rv.sx.wrapping_add(dx) as c_int, 0, max_x));
        let ny = c_long::from(clamp_int(rv.sy.wrapping_add(dy) as c_int, 0, max_y));
        if nx != rv.sx {
            rv.sx = nx;
            rv.requested_x = nx as c_int;
        }
        if ny != rv.sy {
            rv.sy = ny;
            rv.requested_y = ny as c_int;
        }
    }

    fn apply_snap(&mut self, rv: &mut RenderView, wheeled: bool) {
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let (mut x, mut y) = (rv.sx as c_int, rv.sy as c_int);
        let previous = (self.frame_sx as c_int, self.frame_sy as c_int);
        let vw = f64::from(rv.vw) / rv.scale;
        let vh = f64::from(rv.vh) / rv.scale;
        if !cur.snap_document(vw, vh, previous, &mut x, &mut y) {
            return;
        }
        if x != rv.sx as c_int {
            rv.sx = c_long::from(x);
            rv.requested_x = x;
        }
        if y != rv.sy as c_int {
            rv.sy = c_long::from(y);
            rv.requested_y = y;
        }
        if wheeled {
            rv.wheel_snapped = 1;
        }
    }

    fn render_frame(&mut self, rv: &mut RenderView) {
        rv.unchanged = self.frame_valid
            && rv.ticked == 0
            && rv.caret_changed == 0
            && rv.sx == self.frame_sx
            && rv.sy == self.frame_sy
            && rv.vw == self.frame_w
            && rv.vh == self.frame_h
            && rv.scale == self.frame_scale;
        if rv.unchanged {
            return;
        }
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let fb = self.fb.bytes();
        rv.render_rc =
            cur.render_argb32(rv.sx as c_int, rv.sy as c_int, rv.vw, rv.vh, rv.scale, fb);
        if rv.render_rc == 0 {
            self.note_frame(rv);
        } else {
            let n = (rv.vw as usize * 4 * rv.vh as usize).min(fb.len());
            fb[..n].fill(0xff);
            self.frame_valid = false;
        }
    }

    fn render_reply(&mut self, rv: &RenderView, tiles: Option<&[u8]>) {
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let nav = flatten_line(cur.take_pending_nav(), false);
        if nav.is_some() {
            self.stash_post(nav.as_deref());
        }
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let webgl = flatten_line(cur.take_pending_webgl(), false);
        let camera = flatten_line(cur.take_pending_camera(), false);
        let download = flatten_line(cur.take_pending_download(), false);
        let audio = flatten_line(cur.take_pending_audio(), true);
        let window_action = flatten_line(cur.take_pending_window_action(), false);
        let mut animating = c_int::from(self.animating());
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        if cur.caret_blinking() {
            animating |= 2;
        }
        if rv.wheel_snapped != 0 {
            animating |= 4;
        }
        let clipboard = c_int::from(cur.has_pending_clipboard());
        let mut headers = format!(
            "X-W: {}\r\nX-H: {}\r\nX-Stride: {}\r\nX-Anim: {animating}\r\nX-PageW: {}\r\nX-PageH: {}\r\n\
             X-ScrollY: {}\r\nX-ScrollX: {}\r\nX-Render-RC: {}\r\nX-Clipboard: {clipboard}\r\n{}{}",
            rv.vw,
            rv.vh,
            rv.vw.wrapping_mul(4),
            rv.page_w,
            rv.page_h,
            rv.requested_y,
            rv.requested_x,
            rv.render_rc,
            if rv.unchanged { "X-Unchanged: 1\r\n" } else { "" },
            if tiles.is_some() { "X-Tiles: 1\r\n" } else { "" },
        )
        .into_bytes();
        push_header(&mut headers, "X-Nav", nav.as_deref(), 2000);
        push_header(&mut headers, "X-WebGL", webgl.as_deref(), 2000);
        push_header(&mut headers, "X-Camera", camera.as_deref(), 2000);
        push_header(&mut headers, "X-Download", download.as_deref(), 3000);
        push_header(&mut headers, "X-Audio", audio.as_deref(), 16000);
        push_header(
            &mut headers,
            "X-Window-Action",
            window_action.as_deref(),
            31,
        );
        let fd = self.ctrl_w;
        if let Some(tiles) = tiles {
            respond(fd, 200, "text/plain", &headers, tiles);
        } else if self.shm_mode || rv.unchanged {
            respond(fd, 200, "application/octet-stream", &headers, b"");
        } else {
            let fb = self.fb.bytes();
            let n = (rv.vw as usize * 4 * rv.vh as usize).min(fb.len());
            respond(fd, 200, "application/octet-stream", &headers, &fb[..n]);
        }
    }

    fn render_tiles(&mut self, body: &[u8], rv: &RenderView) -> bool {
        let view = View {
            sx: rv.sx,
            sy: rv.sy,
            vw: rv.vw,
            vh: rv.vh,
            scale: rv.scale,
            page_h: rv.page_h,
        };
        let invalid = !self.frame_valid || rv.ticked != 0 || rv.caret_changed != 0;
        let mut desc = Vec::with_capacity(4096);
        let Some(cur) = self.cur.as_mut() else {
            return false;
        };
        let size = (self.max_w as usize * self.max_h as usize * 4).min(self.fb.bytes().len());
        let fb = &mut self.fb.bytes()[..size];
        if !self.tiles.render(cur, body, &view, invalid, fb, &mut desc) {
            return false;
        }
        self.note_frame(rv);
        self.render_reply(rv, Some(&desc));
        true
    }

    fn render(&mut self, body: &[u8]) {
        let mut rv = self.view_parse(body);
        self.apply_device_pixel_ratio(body);
        if self.cur.is_none() {
            respond(
                self.ctrl_w,
                200,
                "application/octet-stream",
                b"X-W: 0\r\nX-H: 0\r\nX-Stride: 0\r\nX-Anim: 0\r\n",
                b"",
            );
            return;
        }
        let dx = long(body, "wheel_dx");
        let dy = long(body, "wheel_dy");
        let wheeled = dx != 0 || dy != 0;
        self.note_scroll(rv.sx, rv.sy, wheeled);
        let fill = long(body, "fill");
        rv.ticked = if self.frame_valid && fill == 0 {
            self.tick()
        } else {
            0
        };
        if let Some(cur) = self.cur.as_mut() {
            Self::apply_pending_scroll(cur, &mut rv);
        }
        self.apply_wheel(body, &mut rv, dx, dy);
        self.apply_snap(&mut rv, wheeled);
        if let Some(cur) = self.cur.as_mut() {
            rv.caret_changed = cur.set_caret_blink_active(rv.caret);
        }
        if self.shm_mode && tiles::tiles_requested(body) && self.render_tiles(body, &rv) {
            return;
        }
        self.render_frame(&mut rv);
        self.render_reply(&rv, None);
    }

    fn open(&mut self, body: &[u8]) {
        let url = text(body, "url");
        let vw = css_viewport_extent(long(body, "width"), self.max_w);
        let vh = css_viewport_extent(long(body, "height"), self.max_h);
        let settle = long(body, "settle_ms") as c_int;
        let history = long(body, "history") != 0;
        let user_activated = long(body, "user_activated") != 0;
        let dpr = request_device_pixel_ratio(body);
        if dpr > 0.0 {
            engine::set_device_pixel_ratio(None, dpr);
        }
        let restored = match (&url, history) {
            (Some(url), true) => self.bfcache_take(url),
            _ => None,
        };
        let referrer = if history {
            None
        } else {
            self.cur.as_mut().and_then(Browser::url)
        };
        if let Some(cur) = self.cur.take() {
            self.bfcache_park_or_close(cur);
        }
        if let (Some(referrer), None, Some(url)) = (&referrer, &restored, &url) {
            let referrer = engine::cstring(referrer);
            if engine::same_origin(&referrer, url) {
                engine::set_next_referrer(&referrer);
            }
        }
        engine::set_next_user_activated(user_activated);
        self.frame_valid = false;
        engine::net_log_clear();
        let posted = self.post.take();
        self.cur = if let Some(mut page) = restored {
            page.bfcache_restore(vw, f64::from(vh));
            if dpr > 0.0 {
                engine::set_device_pixel_ratio(Some(&mut page), dpr);
            }
            Some(page)
        } else if let Some(url) = &url {
            match posted.filter(|p| p.url.as_deref() == Some(url.as_c_str())) {
                Some(stashed) => {
                    Browser::open_post_viewport(url, vw, f64::from(vh), settle, &stashed.post)
                }
                None => Browser::open_viewport(url, vw, f64::from(vh), settle),
            }
        } else {
            None
        };
        let (mut pw, mut ph) = (0, 0);
        let (mut title, mut final_url, mut nav, mut security, mut ip) = (None, None, None, 0, None);
        if let Some(cur) = self.cur.as_mut() {
            cur.page_size(&mut pw, &mut ph);
            title = cur.title();
            final_url = cur.url();
            nav = cur.take_pending_nav();
            (security, ip) = cur.security();
        }
        let shown_url = final_url.or_else(|| url.map(|u| u.into_bytes()));
        json_reply(
            self.ctrl_w,
            &format!(
                "{{\"ok\":{},\"page_width\":{pw},\"page_height\":{ph},\"title\":\"{}\",\"url\":\"{}\",\
                 \"nav\":\"{}\",\"security\":{security},\"ip\":\"{}\"}}",
                c_int::from(self.cur.is_some()),
                escaped(title.as_deref()),
                escaped(shown_url.as_deref()),
                escaped(nav.as_deref()),
                escaped(ip.as_deref()),
            ),
        );
    }

    fn tick_request(&mut self) {
        if self.cur.is_none() {
            json_reply(self.ctrl_w, "{\"ok\":0}");
            return;
        }
        let changed = self.tick();
        if changed != 0 {
            self.frame_valid = false;
        }
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let nav = flatten_line(cur.take_pending_nav(), false);
        if nav.is_some() {
            self.stash_post(nav.as_deref());
        }
        let Some(cur) = self.cur.as_mut() else {
            return;
        };
        let webgl = cur.take_pending_webgl();
        let camera = cur.take_pending_camera();
        let download = cur.take_pending_download();
        let audio = flatten_line(cur.take_pending_audio(), true);
        let window_action = cur.take_pending_window_action();
        let title = cur.title();
        let page_url = cur.url();
        let url_pushed = cur.take_soft_nav_pushed();
        let (mut page_w, mut page_h) = (0, 0);
        cur.page_size(&mut page_w, &mut page_h);
        let animating = c_int::from(self.animating());
        json_reply(
            self.ctrl_w,
            &format!(
                "{{\"ok\":1,\"changed\":{},\"animating\":{animating},\"page_width\":{page_w},\
                 \"page_height\":{page_h},\"nav\":\"{}\",\"webgl\":\"{}\",\"camera\":\"{}\",\
                 \"download\":\"{}\",\"audio\":\"{}\",\"window_action\":\"{}\",\"title\":\"{}\",\
                 \"url\":\"{}\",\"url_pushed\":{url_pushed}}}",
                c_int::from(changed != 0),
                escaped(nav.as_deref()),
                escaped(webgl.as_deref()),
                escaped(camera.as_deref()),
                escaped(download.as_deref()),
                escaped(audio.as_deref()),
                escaped(window_action.as_deref()),
                escaped(title.as_deref()),
                escaped(page_url.as_deref()),
            ),
        );
    }

    fn favicon(&mut self) {
        let icon = self
            .cur
            .as_mut()
            .and_then(Browser::favicon_url)
            .filter(|url| !url.is_empty())
            .and_then(|url| engine::fetch_favicon(&engine::cstring(&url)));
        match icon {
            Some(icon) => {
                let headers = format!(
                    "X-W: {}\r\nX-H: {}\r\nX-Stride: {}\r\n",
                    icon.width,
                    icon.height,
                    icon.width * 4
                );
                respond(
                    self.ctrl_w,
                    200,
                    "application/octet-stream",
                    headers.as_bytes(),
                    &icon.pixels,
                );
            }
            None => respond(
                self.ctrl_w,
                200,
                "application/octet-stream",
                b"X-W: 0\r\nX-H: 0\r\nX-Stride: 0\r\n",
                b"",
            ),
        }
    }

    fn print_request(&mut self, body: &[u8]) {
        let prefix = text(body, "prefix");
        let mut scale = double_or(body, "scale", 0.0);
        if scale.is_nan() || scale <= 0.1 || scale > 8.0 {
            scale = 2.0;
        }
        let mut setup = engine::print_setup_default();
        self.frame_valid = false;
        let pages = if prefix.is_some() {
            self.print(&mut setup)
        } else {
            None
        };
        let mut written = 0usize;
        if let (Some(pages), Some(prefix)) = (pages, &prefix) {
            let w = (setup.width * scale).ceil() as c_int;
            let h = (setup.height * scale).ceil() as c_int;
            for i in 0..pages.len() {
                if written == i && w > 0 && h > 0 {
                    let mut path = prefix.as_bytes().to_vec();
                    path.extend_from_slice(format!("-{i}.png").as_bytes());
                    if pages.write_png(i, w, h, scale, &engine::cstring(&path)) {
                        written = i + 1;
                    }
                }
            }
        }
        json_reply(
            self.ctrl_w,
            &format!(
                "{{\"pages\":{written},\"scale\":{scale:.6},\"width\":{:.6},\"height\":{:.6},\
                 \"mt\":{:.6},\"mr\":{:.6},\"mb\":{:.6},\"ml\":{:.6}}}",
                setup.width,
                setup.height,
                setup.margin_top,
                setup.margin_right,
                setup.margin_bottom,
                setup.margin_left
            ),
        );
    }

    fn dump(&mut self, body: &[u8]) {
        let kind = text(body, "kind");
        let kind = kind.as_ref().map(|k| k.to_bytes());
        let mut result = None;
        if let (Some(cur), Some(kind)) = (self.cur.as_mut(), kind) {
            result = match kind {
                b"dom" => cur.dump_dom(),
                b"layout" => cur.dump_layout(),
                b"text" => cur.render_text(),
                b"links" => cur.links(),
                b"performance" => cur.dump_performance(),
                _ => None,
            };
        }
        if result.is_none() && kind == Some(b"network") {
            result = engine::net_log_dump();
        }
        reply_str(self.ctrl_w, "text", result.as_deref());
    }

    pub fn handle(&mut self, path: &[u8], body: &[u8]) -> bool {
        let fd = self.ctrl_w;
        let xy = |body: &[u8]| (long(body, "x") as c_int, long(body, "y") as c_int);
        match path {
            b"/quit" => {
                empty_ok(fd);
                return true;
            }
            b"/webgl" | b"/camera" => {
                let origin = text(body, "origin");
                let allow = long(body, "allow") as c_int;
                if let (Some(cur), Some(origin)) = (self.cur.as_mut(), &origin) {
                    if path == b"/webgl" {
                        cur.resolve_webgl(origin, allow);
                    } else {
                        cur.resolve_camera(origin, allow);
                    }
                }
                self.frame_valid = false;
                empty_ok(fd);
            }
            b"/colorscheme" => {
                engine::set_color_scheme(long(body, "dark") as c_int);
                self.frame_valid = false;
                empty_ok(fd);
            }
            b"/open" => self.open(body),
            b"/tick" => self.tick_request(),
            b"/render" => self.render(body),
            b"/favicon" => self.favicon(),
            b"/link" => {
                let (x, y) = xy(body);
                match self.cur.as_mut() {
                    Some(cur) => {
                        let href = cur.link_at(x, y);
                        let cursor = cur.cursor_at(x, y);
                        json_reply(
                            fd,
                            &format!(
                                "{{\"href\":\"{}\",\"cursor\":\"{}\"}}",
                                escaped(href.as_deref()),
                                escaped(cursor.as_deref())
                            ),
                        );
                    }
                    None => {
                        self.frame_valid = false;
                        reply_str(fd, "href", None);
                    }
                }
            }
            b"/click" | b"/select" => {
                let (x, y) = xy(body);
                self.frame_valid = false;
                let href = match self.cur.as_mut() {
                    Some(cur) if path == b"/click" => {
                        let href = cur.press(x, y, long(body, "mods") as c_int);
                        self.stash_post(href.as_deref());
                        href
                    }
                    Some(cur) => cur.select(long(body, "kind") as c_int, x, y),
                    None => None,
                };
                reply_str(fd, "href", href.as_deref());
            }
            b"/key" => {
                let kind = long(body, "kind") as c_int;
                let keycode = long(body, "keycode") as c_int;
                let mods = long(body, "mods") as c_int;
                let key = text(body, "key").unwrap_or_default();
                let code = text(body, "code").unwrap_or_default();
                self.frame_valid = false;
                let mut prevented = 0;
                let href = self
                    .cur
                    .as_mut()
                    .and_then(|cur| cur.key_full(kind, &key, &code, keycode, mods, &mut prevented));
                self.stash_post(href.as_deref());
                json_reply(
                    fd,
                    &format!(
                        "{{\"href\":\"{}\",\"prevented\":{}}}",
                        escaped(href.as_deref()),
                        c_int::from(prevented != 0)
                    ),
                );
            }
            b"/clipboard" => {
                let clip = self.cur.as_mut().and_then(Browser::take_pending_clipboard);
                respond(
                    fd,
                    200,
                    "text/plain; charset=utf-8",
                    b"",
                    clip.as_deref().unwrap_or_default(),
                );
            }
            b"/release" => {
                self.frame_valid = false;
                let mut changed = 0;
                let href = self
                    .cur
                    .as_mut()
                    .and_then(|cur| cur.release_click(&mut changed));
                self.stash_post(href.as_deref());
                json_reply(
                    fd,
                    &format!(
                        "{{\"href\":\"{}\",\"changed\":{}}}",
                        escaped(href.as_deref()),
                        c_int::from(changed > 0)
                    ),
                );
            }
            b"/dropfiles" => {
                let (x, y) = xy(body);
                let paths = text(body, "paths");
                let mut changed = 0;
                if let (Some(cur), Some(paths)) = (self.cur.as_mut(), &paths) {
                    if !paths.as_bytes().is_empty() {
                        let list: Vec<CString> = paths
                            .as_bytes()
                            .split(|&b| b == b'\n')
                            .map(engine::cstring)
                            .collect();
                        changed = cur.drop_files(x, y, &list);
                    }
                }
                if changed > 0 {
                    self.frame_valid = false;
                }
                json_reply(fd, &format!("{{\"changed\":{}}}", c_int::from(changed > 0)));
            }
            b"/hover" => {
                let (x, y) = xy(body);
                let changed = self.cur.as_mut().map_or(0, |cur| cur.hover(x, y));
                if changed > 0 {
                    self.frame_valid = false;
                }
                let href = self.cur.as_mut().and_then(|cur| cur.link_under(x, y));
                let cursor = self.cur.as_mut().and_then(|cur| cur.cursor_at(x, y));
                json_reply(
                    fd,
                    &format!(
                        "{{\"changed\":{},\"href\":\"{}\",\"cursor\":\"{}\"}}",
                        c_int::from(changed > 0),
                        escaped(href.as_deref()),
                        escaped(cursor.as_deref())
                    ),
                );
            }
            b"/scroll" => {
                let (x, y) = xy(body);
                let (dx, dy) = (long(body, "dx") as c_int, long(body, "dy") as c_int);
                let consumed = self
                    .cur
                    .as_mut()
                    .map_or(0, |cur| cur.scroll_at(x, y, dx, dy));
                if consumed != 0 {
                    self.frame_valid = false;
                }
                json_reply(
                    fd,
                    &format!("{{\"consumed\":{}}}", c_int::from(consumed != 0)),
                );
            }
            b"/scrollbar-press" | b"/scrollbar-drag" => {
                let (x, y) = xy(body);
                let hit = match self.cur.as_mut() {
                    Some(cur) if path == b"/scrollbar-press" => cur.scrollbar_press(x, y),
                    Some(cur) => cur.scrollbar_drag(x, y),
                    None => 0,
                };
                if hit != 0 {
                    self.frame_valid = false;
                }
                json_reply(fd, &format!("{{\"hit\":{}}}", c_int::from(hit != 0)));
            }
            b"/scrollbar-release" => {
                if let Some(cur) = self.cur.as_mut() {
                    cur.scrollbar_release();
                }
                json_reply(fd, "{\"ok\":1}");
            }
            b"/focused-editable" => {
                let active = self.cur.as_mut().map_or(0, Browser::focused_editable);
                json_reply(fd, &format!("{{\"active\":{active}}}"));
            }
            b"/focused-editable-state" => {
                let (mut caret, mut anchor) = (0usize, 0usize);
                let value = self
                    .cur
                    .as_mut()
                    .and_then(|cur| cur.focused_editable_value(&mut caret, &mut anchor));
                json_reply(
                    fd,
                    &format!(
                        "{{\"active\":{},\"caret\":{caret},\"anchor\":{anchor},\"value\":\"{}\"}}",
                        c_int::from(value.is_some()),
                        escaped(value.as_deref())
                    ),
                );
            }
            b"/focused-editable-selection" => {
                let caret = long(body, "caret").max(0) as usize;
                let anchor = long(body, "anchor").max(0) as usize;
                let ok = self
                    .cur
                    .as_mut()
                    .map_or(0, |cur| cur.set_focused_editable_selection(caret, anchor));
                if ok != 0 {
                    self.frame_valid = false;
                }
                json_reply(fd, &format!("{{\"ok\":{}}}", c_int::from(ok != 0)));
            }
            b"/find" => {
                let options = (
                    long(body, "case_sensitive") as c_int,
                    long(body, "direction") as c_int,
                    long(body, "from_y") as c_int,
                );
                let query = text(body, "query").unwrap_or_default();
                let (mut total, mut current, mut scroll_y) = (0, 0, 0);
                self.frame_valid = false;
                if let Some(cur) = self.cur.as_mut() {
                    cur.find(&query, options, (&mut total, &mut current, &mut scroll_y));
                }
                json_reply(
                    fd,
                    &format!("{{\"total\":{total},\"current\":{current},\"scroll_y\":{scroll_y}}}"),
                );
            }
            b"/viewport" => {
                let vw = css_viewport_extent(long(body, "width"), self.max_w);
                let vh = css_viewport_extent(long(body, "height"), self.max_h);
                let (mut pw, mut ph, mut ok) = (0, 0, 0);
                self.frame_valid = false;
                self.apply_device_pixel_ratio(body);
                if let Some(cur) = self.cur.as_mut() {
                    if cur.set_viewport(vw, f64::from(vh)) == 0 {
                        cur.window_action_applied();
                        cur.page_size(&mut pw, &mut ph);
                        ok = 1;
                    }
                }
                json_reply(
                    fd,
                    &format!("{{\"ok\":{ok},\"page_width\":{pw},\"page_height\":{ph}}}"),
                );
            }
            b"/eval" => {
                let src = text(body, "src").unwrap_or_default();
                self.frame_valid = false;
                let result = self.cur.as_mut().and_then(|cur| cur.eval(&src));
                reply_str(fd, "text", result.as_deref());
            }
            b"/video-event" => {
                let token = text(body, "token");
                let kind = text(body, "kind");
                let ok = self.cur.as_mut().is_some_and(|cur| {
                    cur.video_helper_event(token.as_deref(), kind.as_deref()) != 0
                });
                self.frame_valid = false;
                json_reply(fd, &format!("{{\"ok\":{}}}", c_int::from(ok)));
            }
            b"/dump" => self.dump(body),
            b"/console" => {
                let log = self.cur.as_mut().and_then(Browser::console_drain);
                reply_str(fd, "text", log.as_deref());
            }
            b"/media" => {
                let (x, y) = xy(body);
                let (mut is_video, mut stream) = (0, 0);
                let url = self
                    .cur
                    .as_mut()
                    .and_then(|cur| cur.media_at(x, y, &mut is_video, &mut stream));
                json_reply(
                    fd,
                    &format!(
                        "{{\"url\":\"{}\",\"is_video\":{is_video},\"stream\":{stream}}}",
                        escaped(url.as_deref())
                    ),
                );
            }
            b"/contextmenu" => {
                let (x, y) = xy(body);
                let mut edit = 0;
                let prevented = self
                    .cur
                    .as_mut()
                    .map_or(0, |cur| cur.contextmenu_full(x, y, &mut edit));
                if edit != 0 {
                    self.frame_valid = false;
                }
                json_reply(
                    fd,
                    &format!("{{\"prevented\":{prevented},\"edit\":{edit}}}"),
                );
            }
            b"/export" => {
                let path = text(body, "path");
                self.frame_valid = false;
                let rc = match (self.cur.as_mut(), &path) {
                    (Some(cur), Some(path)) => cur.render_image(path),
                    _ => -1,
                };
                json_reply(fd, &format!("{{\"ok\":{rc}}}"));
            }
            b"/print" => self.print_request(body),
            _ => respond(fd, 404, "text/plain", b"", b""),
        }
        false
    }
}

impl<F> Drop for Session<F> {
    fn drop(&mut self) {
        self.post = None;
        self.bf.clear();
        self.cur = None;
    }
}
