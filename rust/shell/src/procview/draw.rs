//! Southstar — the tab view's scrolling and painting arithmetic: CSS cursors and their fallbacks, the tile window around the viewport, wheel and fling smoothing, and the busy hourglass glyph.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const TILE_H: i32 = 256;
pub const WHEEL_STEP_PX: f64 = 100.0;
pub const HOVER_AFTER_SCROLL_MS: u32 = 150;
pub const CARET_BLINK_US: i64 = 530 * 1000;
pub const ANIM_FRAME_US: i64 = 1_000_000 / 60;
const WHEEL_TAU_MS: f64 = 45.0;
const FLING_TAU_MS: f64 = 400.0;
const FLING_STOP_PX_S: f64 = 20.0;

const CURSOR_FALLBACKS: [(&str, &str); 23] = [
    ("context-menu", "default"),
    ("help", "default"),
    ("progress", "wait"),
    ("cell", "crosshair"),
    ("vertical-text", "text"),
    ("alias", "copy"),
    ("copy", "default"),
    ("move", "all-scroll"),
    ("no-drop", "not-allowed"),
    ("not-allowed", "default"),
    ("grab", "all-scroll"),
    ("grabbing", "all-scroll"),
    ("all-scroll", "move"),
    ("col-resize", "ew-resize"),
    ("row-resize", "ns-resize"),
    ("ne-resize", "nesw-resize"),
    ("sw-resize", "nesw-resize"),
    ("nw-resize", "nwse-resize"),
    ("se-resize", "nwse-resize"),
    ("nesw-resize", "crosshair"),
    ("nwse-resize", "crosshair"),
    ("zoom-in", "crosshair"),
    ("zoom-out", "crosshair"),
];

const PLAIN_CURSORS: [&str; 12] = [
    "default",
    "none",
    "pointer",
    "wait",
    "crosshair",
    "text",
    "n-resize",
    "e-resize",
    "s-resize",
    "w-resize",
    "ew-resize",
    "ns-resize",
];

pub enum Cursor {
    Unset,
    Named(Option<&'static str>),
}

pub fn cursor_for(name: &[u8]) -> Cursor {
    if name.is_empty() {
        return Cursor::Unset;
    }
    if let Some((_, fallback)) = CURSOR_FALLBACKS.iter().find(|(n, _)| n.as_bytes() == name) {
        return Cursor::Named(Some(fallback));
    }
    if PLAIN_CURSORS.iter().any(|n| n.as_bytes() == name) {
        return Cursor::Named(None);
    }
    Cursor::Unset
}

pub fn tiles_range(scroll_y: f64, viewport_h: f64, scroll_dir: i32) -> (f64, f64) {
    let ahead = viewport_h * 1.5;
    let behind = viewport_h * 0.5;
    let up = scroll_dir < 0;
    let y0 = (scroll_y - if up { ahead } else { behind }).max(0.0);
    let y1 = scroll_y + viewport_h + if up { behind } else { ahead };
    (y0, y1)
}

pub fn take_whole(pending: &mut f64) -> i32 {
    let r = pending.round();
    let whole = if (*pending - r).abs() < 1e-6 {
        r as i32
    } else {
        *pending as i32
    };
    *pending -= f64::from(whole);
    whole
}

pub struct Glide {
    pub left_x: f64,
    pub left_y: f64,
    pub fling_x: f64,
    pub fling_y: f64,
}

impl Glide {
    pub fn idle(&self) -> bool {
        self.left_x == 0.0 && self.left_y == 0.0 && self.fling_x == 0.0 && self.fling_y == 0.0
    }

    pub fn step(&mut self, dt_ms: f64) -> (f64, f64) {
        let share = 1.0 - (-dt_ms / WHEEL_TAU_MS).exp();
        let mut mx = self.left_x * share;
        let mut my = self.left_y * share;
        if (self.left_x - mx).abs() < 0.5 {
            mx = self.left_x;
        }
        if (self.left_y - my).abs() < 0.5 {
            my = self.left_y;
        }
        self.left_x -= mx;
        self.left_y -= my;
        mx += self.fling_x * dt_ms / 1000.0;
        my += self.fling_y * dt_ms / 1000.0;
        let decay = (-dt_ms / FLING_TAU_MS).exp();
        self.fling_x *= decay;
        self.fling_y *= decay;
        if self.fling_x.hypot(self.fling_y) < FLING_STOP_PX_S {
            self.fling_x = 0.0;
            self.fling_y = 0.0;
        }
        (mx, my)
    }
}

pub fn frame_interval_ms(now_us: i64, last_us: i64) -> f64 {
    let dt = if last_us > 0 {
        (now_us - last_us) as f64 / 1000.0
    } else {
        1000.0 / 60.0
    };
    dt.clamp(1.0, 100.0)
}

pub enum PathOp {
    Move(f64, f64),
    Line(f64, f64),
    Curve(f64, f64, f64, f64, f64, f64),
    Close,
}

pub fn hourglass_bulbs(ox: f64, oy: f64) -> [PathOp; 14] {
    use PathOp::{Close, Curve, Move};
    [
        Move(14.6 + ox, 16.0 + oy),
        Curve(
            12.0 + ox,
            13.2 + oy,
            9.9 + ox,
            10.6 + oy,
            10.1 + ox,
            8.0 + oy,
        ),
        Curve(
            10.0 + ox,
            6.8 + oy,
            10.8 + ox,
            6.1 + oy,
            12.2 + ox,
            6.1 + oy,
        ),
        Curve(
            14.2 + ox,
            5.6 + oy,
            17.8 + ox,
            5.6 + oy,
            19.8 + ox,
            6.1 + oy,
        ),
        Curve(
            21.2 + ox,
            6.1 + oy,
            22.0 + ox,
            6.8 + oy,
            21.9 + ox,
            8.0 + oy,
        ),
        Curve(
            22.1 + ox,
            10.6 + oy,
            20.0 + ox,
            13.2 + oy,
            17.4 + ox,
            16.0 + oy,
        ),
        Close,
        Move(14.6 + ox, 16.0 + oy),
        Curve(
            12.0 + ox,
            18.8 + oy,
            9.9 + ox,
            21.4 + oy,
            10.1 + ox,
            24.0 + oy,
        ),
        Curve(
            10.0 + ox,
            25.2 + oy,
            10.8 + ox,
            25.9 + oy,
            12.2 + ox,
            25.9 + oy,
        ),
        Curve(
            14.2 + ox,
            26.4 + oy,
            17.8 + ox,
            26.4 + oy,
            19.8 + ox,
            25.9 + oy,
        ),
        Curve(
            21.2 + ox,
            25.9 + oy,
            22.0 + ox,
            25.2 + oy,
            21.9 + ox,
            24.0 + oy,
        ),
        Curve(
            22.1 + ox,
            21.4 + oy,
            20.0 + ox,
            18.8 + oy,
            17.4 + ox,
            16.0 + oy,
        ),
        Close,
    ]
}

pub fn hourglass_caps(ox: f64, oy: f64) -> [PathOp; 4] {
    [
        PathOp::Move(9.7 + ox, 6.2 + oy),
        PathOp::Line(22.3 + ox, 6.2 + oy),
        PathOp::Move(9.7 + ox, 25.8 + oy),
        PathOp::Line(22.3 + ox, 25.8 + oy),
    ]
}

pub fn hourglass_sand() -> [PathOp; 10] {
    use PathOp::{Close, Curve, Move};
    [
        Move(11.7, 8.0),
        Curve(14.0, 7.6, 18.0, 7.6, 20.3, 8.0),
        Curve(18.9, 10.9, 17.4, 13.3, 16.0, 15.3),
        Curve(14.6, 13.3, 13.1, 10.9, 11.7, 8.0),
        Close,
        Move(11.7, 24.0),
        Curve(13.5, 21.6, 14.8, 20.9, 16.0, 20.9),
        Curve(17.2, 20.9, 18.5, 21.6, 20.3, 24.0),
        Curve(17.6, 24.7, 14.4, 24.7, 11.7, 24.0),
        Close,
    ]
}
