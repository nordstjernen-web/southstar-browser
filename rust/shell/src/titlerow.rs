//! Southstar — where the title bar row puts the window controls, the tab strip and the browser name.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const CONTROLS_GAP: i32 = 6;
pub const NAME_GAP: i32 = 24;
pub const TRAILING_GAP: i32 = 10;
pub const VERSION: &str = match option_env!("NS_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
pub const SHORT_NAME: &str = "Southstar";

pub fn full_name() -> String {
    format!("Southstar Browser {VERSION}")
}

pub struct Widths {
    pub start_controls: Option<i32>,
    pub end_controls: Option<i32>,
    pub tabs_min: i32,
    pub tabs_nat: i32,
    pub full_name: i32,
    pub short_name: i32,
}

fn controls_width(natural: Option<i32>) -> i32 {
    natural.map_or(0, |w| w + CONTROLS_GAP)
}

pub fn horizontal_size(w: &Widths) -> (i32, i32) {
    let edges =
        controls_width(w.start_controls) + controls_width(w.end_controls) + NAME_GAP + TRAILING_GAP;
    (
        edges + w.tabs_min + w.short_name,
        edges + w.tabs_nat + w.full_name,
    )
}

#[derive(Clone, Copy)]
pub struct Span {
    pub x: i32,
    pub width: i32,
}

pub struct Placement {
    pub start_controls: Option<Span>,
    pub end_controls: Option<Span>,
    pub tabs: Span,
    pub full: bool,
    pub name_width: i32,
    left: i32,
    right: i32,
}

pub fn place(w: &Widths, width: i32) -> Placement {
    let mut left = 0;
    let mut right = width;
    let start_controls = w.start_controls.map(|cw| {
        let span = Span { x: left, width: cw };
        left += cw + CONTROLS_GAP;
        span
    });
    let end_controls = w.end_controls.map(|cw| {
        let span = Span {
            x: right - cw,
            width: cw,
        };
        right -= cw + CONTROLS_GAP;
        span
    });
    let room = right - left - NAME_GAP - TRAILING_GAP;
    let full = w.tabs_nat + w.full_name <= room;
    let name_width = if full { w.full_name } else { w.short_name };
    let tabs_width = clamp(room - name_width, w.tabs_min, w.tabs_nat);
    Placement {
        start_controls,
        end_controls,
        tabs: Span {
            x: left,
            width: tabs_width,
        },
        full,
        name_width,
        left,
        right,
    }
}

fn clamp(value: i32, low: i32, high: i32) -> i32 {
    if value > high {
        high
    } else if value < low {
        low
    } else {
        value
    }
}

pub fn centred_name_x(span: i32, origin: i32, name_width: i32) -> i32 {
    (span - name_width) / 2 - origin
}

pub fn name_x(p: &Placement, centred: i32) -> i32 {
    if centred < p.left + p.tabs.width + NAME_GAP || centred + p.name_width > p.right - TRAILING_GAP
    {
        p.right - TRAILING_GAP - p.name_width
    } else {
        centred
    }
}

pub fn mirrored_x(rtl: bool, row_width: i32, x: i32, width: i32) -> i32 {
    if rtl { row_width - x - width } else { x }
}
