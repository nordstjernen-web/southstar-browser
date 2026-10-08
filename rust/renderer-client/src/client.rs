//! Southstar — the window's side of the renderer protocol: building requests and reading replies, frames and tiles.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_long};

use southstar_ipc::{
    Conn, Head, MAX_REPLY, atol, json_double, json_escape, json_string, json_value, request_head,
    until_nul, write_message,
};

use crate::os::{Child, Framebuffer};

pub const RASTER_SCALE: f64 = 3.0;

pub type Text = Option<Vec<u8>>;

pub struct Renderer {
    pub child: Child,
    pub sock: c_int,
    pub wfd: c_int,
    pub conn: Box<Conn>,
    pub rx: Option<Vec<u8>>,
    pub rxcap: usize,
    pub fb: Framebuffer,
    pub max_w: c_int,
    pub max_h: c_int,
    pub dpr_milli: c_int,
    pub inproc_conn: *mut core::ffi::c_void,
    pub linear: (c_int, c_int, c_int),
    pub head: Box<Head>,
}

#[derive(Default)]
pub struct Page {
    pub ok: bool,
    pub page_width: c_int,
    pub page_height: c_int,
    pub title: Option<Vec<u8>>,
    pub url: Option<Vec<u8>>,
    pub nav: Option<Vec<u8>>,
    pub security: c_int,
    pub remote_ip: Option<Vec<u8>>,
}

pub struct Frame {
    pub ok: bool,
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
    pub animating: bool,
    pub caret_blinking: bool,
    pub wheel_snapped: bool,
    pub page_w: c_int,
    pub page_h: c_int,
    pub scroll_y: c_int,
    pub scroll_x: c_int,
    pub unchanged: bool,
    pub render_rc: c_int,
    pub pixels: *const u8,
    pub nav: Option<Vec<u8>>,
    pub webgl: Option<Vec<u8>>,
    pub camera: Option<Vec<u8>>,
    pub download: Option<Vec<u8>>,
    pub audio: Option<Vec<u8>>,
    pub window_action: Option<Vec<u8>>,
    pub clipboard: bool,
    pub tiles: Option<Vec<u8>>,
}

impl Default for Frame {
    fn default() -> Frame {
        Frame {
            ok: false,
            width: 0,
            height: 0,
            stride: 0,
            animating: false,
            caret_blinking: false,
            wheel_snapped: false,
            page_w: 0,
            page_h: 0,
            scroll_y: -1,
            scroll_x: -1,
            unchanged: false,
            render_rc: 0,
            pixels: core::ptr::null(),
            nav: None,
            webgl: None,
            camera: None,
            download: None,
            audio: None,
            window_action: None,
            clipboard: false,
            tiles: None,
        }
    }
}

#[derive(Default)]
pub struct Tick {
    pub ok: bool,
    pub changed: bool,
    pub animating: bool,
    pub page_w: c_int,
    pub page_h: c_int,
    pub nav: Option<Vec<u8>>,
    pub webgl: Option<Vec<u8>>,
    pub camera: Option<Vec<u8>>,
    pub download: Option<Vec<u8>>,
    pub audio: Option<Vec<u8>>,
    pub window_action: Option<Vec<u8>>,
    pub title: Option<Vec<u8>>,
    pub url: Option<Vec<u8>>,
    pub url_pushed: bool,
}

pub struct Wheel {
    pub x: c_int,
    pub y: c_int,
    pub dx: c_int,
    pub dy: c_int,
    pub viewport: bool,
}

pub struct TilesRequest<'a> {
    pub tile_h: c_int,
    pub want_y0: c_int,
    pub want_y1: c_int,
    pub generation: c_int,
    pub vp_held: c_int,
    pub fill: c_int,
    pub have: &'a [u8],
    pub hold: &'a [u8],
}

pub struct View {
    pub width: c_int,
    pub height: c_int,
    pub scroll_x: c_int,
    pub scroll_y: c_int,
    pub scale: f64,
    pub caret: bool,
}

#[derive(Default)]
pub struct PrintReply {
    pub pages: c_long,
    pub scale: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub margins: [Option<f64>; 4],
}

pub struct Favicon {
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
    pub pixels: Vec<u8>,
}

pub fn long(body: &[u8], key: &str) -> c_long {
    json_value(until_nul(body), key.as_bytes()).map_or(0, atol)
}

fn double(body: &[u8], key: &str) -> Option<f64> {
    json_value(until_nul(body), key.as_bytes()).map(json_double)
}

pub fn text(body: &[u8], key: &str) -> Option<Vec<u8>> {
    json_value(until_nul(body), key.as_bytes()).and_then(json_string)
}

fn non_empty(value: Option<Vec<u8>>) -> Option<Vec<u8>> {
    value.filter(|v| !until_nul(v).is_empty())
}

fn escaped(value: &[u8]) -> String {
    String::from_utf8_lossy(&json_escape(until_nul(value))).into_owned()
}

fn milli(value: c_int) -> String {
    format!("{}.{:03}", value / 1000, value % 1000)
}

fn head_text(field: &[u8]) -> Option<Vec<u8>> {
    let value = until_nul(field);
    (!value.is_empty()).then(|| value.to_vec())
}

fn frame_from_head(head: &Head) -> Frame {
    Frame {
        ok: true,
        animating: head.x_anim & 1 != 0,
        caret_blinking: head.x_anim & 2 != 0,
        wheel_snapped: head.x_anim & 4 != 0,
        page_w: head.x_page_w as c_int,
        page_h: head.x_page_h as c_int,
        scroll_y: head.x_scroll_y as c_int,
        scroll_x: head.x_scroll_x as c_int,
        render_rc: head.x_render_rc as c_int,
        nav: head_text(&head.x_nav),
        webgl: head_text(&head.x_webgl),
        camera: head_text(&head.x_camera),
        download: head_text(&head.x_download),
        audio: head_text(&head.x_audio),
        window_action: head_text(&head.x_window_action),
        clipboard: head.x_clipboard > 0,
        ..Frame::default()
    }
}

impl Renderer {
    fn shm(&self) -> bool {
        !matches!(self.fb, Framebuffer::None)
    }

    fn dpr(&self) -> c_int {
        if self.dpr_milli > 0 {
            self.dpr_milli
        } else {
            1000
        }
    }

    pub fn set_device_pixel_ratio(&mut self, dpr: f64) {
        if dpr > 0.0 {
            self.dpr_milli = (dpr * 1000.0 + 0.5) as c_int;
        }
    }

    pub fn map_size(&self) -> usize {
        if self.shm() {
            self.max_w as usize * self.max_h as usize * 4
        } else {
            0
        }
    }

    fn send(&mut self, path: &str, content_type: &str, body: &[u8]) -> bool {
        request_head(
            b"POST",
            path.as_bytes(),
            content_type.as_bytes(),
            body.len(),
        )
        .is_some_and(|head| write_message(self.wfd, &head, body))
    }

    pub fn request(&mut self, path: &str, json: &str) -> Option<Vec<u8>> {
        if !self.send(path, "application/json", json.as_bytes())
            || !self.conn.read_head(&mut self.head)
        {
            return None;
        }
        let n = self.head.content_length;
        if !(0..=MAX_REPLY).contains(&n) {
            return None;
        }
        let mut body = vec![0u8; n as usize];
        if n > 0 && !self.conn.read_body(&mut body) {
            return None;
        }
        Some(body)
    }

    pub fn open(
        &mut self,
        url: &[u8],
        width: c_int,
        height: c_int,
        settle_ms: c_int,
        history: bool,
        user_activated: bool,
    ) -> Option<Page> {
        let json = format!(
            "{{\"url\":\"{}\",\"width\":{width},\"height\":{height},\"settle_ms\":{settle_ms},\
             \"history\":{},\"user_activated\":{},\"dpr\":{}}}",
            escaped(url),
            c_int::from(history),
            c_int::from(user_activated),
            milli(self.dpr())
        );
        let body = self.request("/open", &json)?;
        Some(Page {
            ok: long(&body, "ok") != 0,
            page_width: long(&body, "page_width") as c_int,
            page_height: long(&body, "page_height") as c_int,
            title: text(&body, "title"),
            url: text(&body, "url"),
            nav: non_empty(text(&body, "nav")),
            security: long(&body, "security") as c_int,
            remote_ip: non_empty(text(&body, "ip")),
        })
    }

    fn render_json(&self, view: &View, wheel: Option<&Wheel>) -> String {
        let scale_milli = (view.scale * 1000.0 + 0.5) as c_int;
        let mut json = format!(
            "{{\"width\":{},\"height\":{},\"scroll_x\":{},\"scroll_y\":{},\"scale\":{},\"caret\":{},\"dpr\":{}",
            view.width,
            view.height,
            view.scroll_x,
            view.scroll_y,
            milli(scale_milli),
            c_int::from(view.caret),
            milli(self.dpr())
        );
        if let Some(w) = wheel.filter(|w| w.dx != 0 || w.dy != 0) {
            json.push_str(&format!(
                ",\"wheel_x\":{},\"wheel_y\":{},\"wheel_dx\":{},\"wheel_dy\":{},\"wheel_viewport\":{}",
                w.x,
                w.y,
                w.dx,
                w.dy,
                c_int::from(w.viewport)
            ));
        }
        json.push('}');
        json
    }

    fn clamp_view(&self, view: &mut View) {
        view.width = view.width.min(self.max_w);
        view.height = view.height.min(self.max_h);
        if view.scale.is_nan() || view.scale <= 0.0 {
            view.scale = 1.0;
        }
    }

    fn head_fits(&self, width: c_int, height: c_int) -> bool {
        let h = &self.head;
        if h.x_w < 1
            || h.x_w > c_long::from(self.max_w)
            || h.x_w > c_long::from(width)
            || h.x_h < 1
            || h.x_h > c_long::from(self.max_h)
            || h.x_h > c_long::from(height)
            || h.x_stride < h.x_w * 4
            || h.x_stride > c_long::from(self.max_w) * 4
        {
            return false;
        }
        self.shm() || h.content_length as u64 >= (h.x_stride as u64).wrapping_mul(h.x_h as u64)
    }

    fn note_linear(&mut self, out: &Frame) {
        if self.shm() {
            self.linear = (out.width, out.height, out.stride);
        }
    }

    fn attach_unchanged(&self, out: &mut Frame) {
        let (w, h, stride) = self.linear;
        if !self.shm()
            || w <= 0
            || self.head.x_w != c_long::from(w)
            || self.head.x_h != c_long::from(h)
        {
            return;
        }
        out.width = w;
        out.height = h;
        out.stride = stride;
        out.pixels = self.fb.pixels();
    }

    fn read_frame_reply(&mut self) -> bool {
        if !self.conn.read_head(&mut self.head) {
            return false;
        }
        let n = self.head.content_length;
        if n < 0 || n as u64 > self.rxcap as u64 {
            if n > 0 {
                self.conn.skip_body(n as u64);
            }
            return false;
        }
        if n == 0 {
            return true;
        }
        match self.rx.as_mut() {
            Some(rx) => self.conn.read_body(&mut rx[..n as usize]),
            None => false,
        }
    }

    fn take_frame(&mut self, width: c_int, height: c_int) -> Option<Frame> {
        if self.head.x_unchanged > 0 {
            let mut out = frame_from_head(&self.head);
            out.unchanged = true;
            self.attach_unchanged(&mut out);
            return Some(out);
        }
        if !self.head_fits(width, height) {
            return None;
        }
        let mut out = frame_from_head(&self.head);
        out.width = self.head.x_w as c_int;
        out.height = self.head.x_h as c_int;
        out.stride = self.head.x_stride as c_int;
        out.pixels = match (&self.fb, &self.rx) {
            (Framebuffer::None, Some(rx)) => rx.as_ptr(),
            (Framebuffer::None, None) => core::ptr::null(),
            (fb, _) => fb.pixels(),
        };
        self.note_linear(&out);
        Some(out)
    }

    pub fn render(&mut self, mut view: View, wheel: Option<&Wheel>) -> Option<Frame> {
        self.clamp_view(&mut view);
        let json = self.render_json(&view, wheel);
        if !self.send("/render", "application/json", json.as_bytes()) || !self.read_frame_reply() {
            return None;
        }
        self.take_frame(view.width, view.height)
    }

    fn tiles_json(
        &self,
        view: &View,
        wheel: Option<&Wheel>,
        t: &TilesRequest<'_>,
    ) -> Option<Vec<u8>> {
        const CAP: usize = 4096;
        let mut json = self.render_json(view, wheel).into_bytes();
        if json.is_empty() || json.len() >= CAP {
            return None;
        }
        json.pop();
        json.extend_from_slice(
            format!(
                ",\"tiles\":1,\"tile_h\":{},\"want_y0\":{},\"want_y1\":{},\"gen\":{},\"vp_held\":{},\"fill\":{},\"have\":\"",
                t.tile_h, t.want_y0, t.want_y1, t.generation, t.vp_held, t.fill,
            )
            .as_bytes(),
        );
        json.extend_from_slice(until_nul(t.have));
        json.extend_from_slice(b"\",\"hold\":\"");
        json.extend_from_slice(until_nul(t.hold));
        json.extend_from_slice(b"\"}");
        (json.len() < CAP).then_some(json)
    }

    pub fn render_tiles(
        &mut self,
        mut view: View,
        wheel: Option<&Wheel>,
        tiles: &TilesRequest<'_>,
    ) -> Option<Frame> {
        self.clamp_view(&mut view);
        let json = self.tiles_json(&view, wheel, tiles)?;
        if !self.send("/render", "application/json", &json) || !self.conn.read_head(&mut self.head)
        {
            return None;
        }
        let n = self.head.content_length;
        if self.head.x_tiles > 0 {
            if n <= 0 || n > MAX_REPLY {
                return None;
            }
            let mut desc = vec![0u8; n as usize];
            if !self.conn.read_body(&mut desc) {
                return None;
            }
            let mut out = frame_from_head(&self.head);
            out.tiles = Some(desc);
            out.width = self.head.x_w as c_int;
            out.height = self.head.x_h as c_int;
            out.stride = self.head.x_stride as c_int;
            out.pixels = self.fb.pixels();
            self.linear.0 = 0;
            return Some(out);
        }
        if n > 0 {
            self.conn.skip_body(n as u64);
            return None;
        }
        if self.head.x_unchanged > 0 {
            let mut out = frame_from_head(&self.head);
            out.unchanged = true;
            return Some(out);
        }
        if !self.head_fits(view.width, view.height) {
            return None;
        }
        let mut out = frame_from_head(&self.head);
        out.width = self.head.x_w as c_int;
        out.height = self.head.x_h as c_int;
        out.stride = self.head.x_stride as c_int;
        out.pixels = self.fb.pixels();
        self.note_linear(&out);
        Some(out)
    }

    pub fn tick(&mut self) -> Option<Tick> {
        let body = self.request("/tick", "{}")?;
        Some(Tick {
            ok: long(&body, "ok") != 0,
            changed: long(&body, "changed") != 0,
            animating: long(&body, "animating") != 0,
            page_w: long(&body, "page_width") as c_int,
            page_h: long(&body, "page_height") as c_int,
            nav: text(&body, "nav"),
            webgl: text(&body, "webgl"),
            camera: text(&body, "camera"),
            download: text(&body, "download"),
            audio: text(&body, "audio"),
            window_action: text(&body, "window_action"),
            title: text(&body, "title"),
            url: text(&body, "url"),
            url_pushed: long(&body, "url_pushed") != 0,
        })
    }

    pub fn xy_text(
        &mut self,
        path: &str,
        x: c_int,
        y: c_int,
        extra: Option<(&str, c_int)>,
        key: &str,
    ) -> Option<Vec<u8>> {
        let json = match extra {
            Some((name, value)) => format!("{{\"x\":{x},\"y\":{y},\"{name}\":{value}}}"),
            None => format!("{{\"x\":{x},\"y\":{y}}}"),
        };
        let body = self.request(path, &json)?;
        text(&body, key)
    }

    pub fn link_cursor_at(&mut self, x: c_int, y: c_int) -> Option<(Text, Text)> {
        let body = self.request("/link", &format!("{{\"x\":{x},\"y\":{y}}}"))?;
        Some((text(&body, "href"), non_empty(text(&body, "cursor"))))
    }

    pub fn key(
        &mut self,
        kind: c_int,
        key: &[u8],
        code: &[u8],
        keycode: c_int,
        mods: c_int,
    ) -> Option<(Option<Vec<u8>>, bool)> {
        let json = format!(
            "{{\"kind\":{kind},\"key\":\"{}\",\"code\":\"{}\",\"keycode\":{keycode},\"mods\":{mods}}}",
            escaped(key),
            escaped(code)
        );
        let body = self.request("/key", &json)?;
        Some((text(&body, "href"), long(&body, "prevented") != 0))
    }

    pub fn release(&mut self) -> Option<(Option<Vec<u8>>, bool)> {
        let body = self.request("/release", "{}")?;
        Some((text(&body, "href"), long(&body, "changed") != 0))
    }

    pub fn focused_editable(&mut self) -> bool {
        self.request("/focused-editable", "{}")
            .is_some_and(|body| long(&body, "active") != 0)
    }

    pub fn focused_editable_value(&mut self) -> Option<(Option<Vec<u8>>, usize, usize)> {
        let body = self.request("/focused-editable-state", "{}")?;
        let caret = long(&body, "caret").max(0) as usize;
        let anchor = long(&body, "anchor").max(0) as usize;
        let value = (long(&body, "active") != 0).then(|| text(&body, "value").unwrap_or_default());
        Some((value, caret, anchor))
    }

    pub fn set_focused_editable_selection(&mut self, caret: usize, anchor: usize) -> bool {
        self.request(
            "/focused-editable-selection",
            &format!("{{\"caret\":{caret},\"anchor\":{anchor}}}"),
        )
        .is_some_and(|body| long(&body, "ok") != 0)
    }

    pub fn hover(&mut self, x: c_int, y: c_int) -> Option<(bool, Text, Text)> {
        let body = self.request("/hover", &format!("{{\"x\":{x},\"y\":{y}}}"))?;
        Some((
            long(&body, "changed") != 0,
            non_empty(text(&body, "href")),
            non_empty(text(&body, "cursor")),
        ))
    }

    pub fn flag(&mut self, path: &str, json: &str, key: &str) -> bool {
        self.request(path, json)
            .is_some_and(|body| long(&body, key) != 0)
    }

    pub fn find(
        &mut self,
        query: &[u8],
        case_sensitive: bool,
        direction: c_int,
        from_y: c_int,
    ) -> Option<[c_int; 3]> {
        let json = format!(
            "{{\"query\":\"{}\",\"case_sensitive\":{},\"direction\":{direction},\"from_y\":{from_y}}}",
            escaped(query),
            c_int::from(case_sensitive)
        );
        let body = self.request("/find", &json)?;
        Some([
            long(&body, "total") as c_int,
            long(&body, "current") as c_int,
            long(&body, "scroll_y") as c_int,
        ])
    }

    pub fn drop_files(&mut self, x: c_int, y: c_int, paths: &[&[u8]]) -> c_int {
        let joined = paths.join(&b'\n');
        let json = format!("{{\"x\":{x},\"y\":{y},\"paths\":\"{}\"}}", escaped(&joined));
        self.request("/dropfiles", &json)
            .map_or(0, |body| long(&body, "changed") as c_int)
    }

    pub fn set_viewport(&mut self, width: c_int, height: c_int) -> Option<Page> {
        let json = format!(
            "{{\"width\":{width},\"height\":{height},\"dpr\":{}}}",
            milli(self.dpr())
        );
        let body = self.request("/viewport", &json)?;
        Some(Page {
            ok: long(&body, "ok") != 0,
            page_width: long(&body, "page_width") as c_int,
            page_height: long(&body, "page_height") as c_int,
            ..Page::default()
        })
    }

    pub fn origin_decision(&mut self, path: &str, origin: &[u8], allow: bool) -> bool {
        let json = format!(
            "{{\"origin\":\"{}\",\"allow\":{}}}",
            escaped(origin),
            c_int::from(allow)
        );
        self.request(path, &json).is_some()
    }

    pub fn video_event(&mut self, token: &[u8], kind: &[u8]) -> bool {
        let json = format!(
            "{{\"token\":\"{}\",\"kind\":\"{}\"}}",
            escaped(token),
            escaped(kind)
        );
        self.request("/video-event", &json).is_some()
    }

    pub fn text_request(&mut self, path: &str, key: &str, value: Option<&[u8]>) -> Option<Vec<u8>> {
        let json = match value {
            Some(value) => format!("{{\"{key}\":\"{}\"}}", escaped(value)),
            None => "{}".to_owned(),
        };
        let body = self.request(path, &json)?;
        text(&body, "text")
    }

    pub fn media_at(&mut self, x: c_int, y: c_int) -> Option<(Option<Vec<u8>>, c_int, c_int)> {
        let body = self.request("/media", &format!("{{\"x\":{x},\"y\":{y}}}"))?;
        Some((
            non_empty(text(&body, "url")),
            long(&body, "is_video") as c_int,
            long(&body, "stream") as c_int,
        ))
    }

    pub fn contextmenu(&mut self, x: c_int, y: c_int) -> Option<(c_int, c_int)> {
        let body = self.request("/contextmenu", &format!("{{\"x\":{x},\"y\":{y}}}"))?;
        Some((
            long(&body, "prevented") as c_int,
            long(&body, "edit") as c_int,
        ))
    }

    pub fn export(&mut self, path: &[u8]) -> bool {
        let json = format!("{{\"path\":\"{}\"}}", escaped(path));
        let ok = self
            .request("/export", &json)
            .map(|body| json_value(until_nul(&body), b"ok").map_or(-1, atol));
        ok == Some(0)
    }

    pub fn print(&mut self, prefix: &[u8]) -> Option<PrintReply> {
        let json = format!(
            "{{\"prefix\":\"{}\",\"scale\":{RASTER_SCALE:.6}}}",
            escaped(prefix)
        );
        let body = self.request("/print", &json)?;
        Some(PrintReply {
            pages: long(&body, "pages"),
            scale: double(&body, "scale").unwrap_or(RASTER_SCALE),
            width: double(&body, "width"),
            height: double(&body, "height"),
            margins: [
                double(&body, "mt"),
                double(&body, "mr"),
                double(&body, "mb"),
                double(&body, "ml"),
            ],
        })
    }

    pub fn favicon(&mut self) -> Option<Favicon> {
        if !self.send("/favicon", "application/json", b"") || !self.conn.read_head(&mut self.head) {
            return None;
        }
        let n = self.head.content_length;
        if n <= 0 || n > MAX_REPLY {
            return None;
        }
        let mut pixels = vec![0u8; n as usize];
        if !self.conn.read_body(&mut pixels) {
            return None;
        }
        let h = &self.head;
        if !(1..=1024).contains(&h.x_w)
            || !(1..=1024).contains(&h.x_h)
            || h.x_stride < h.x_w * 4
            || h.x_stride > 1024 * 4
            || (h.x_stride as u64) * (h.x_h as u64) > n as u64
        {
            return None;
        }
        Some(Favicon {
            width: h.x_w as c_int,
            height: h.x_h as c_int,
            stride: h.x_stride as c_int,
            pixels,
        })
    }

    pub fn quit(&mut self) {
        self.send("/quit", "text/plain", b"");
    }
}
