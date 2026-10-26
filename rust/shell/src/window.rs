//! Southstar — the browser window's text rules: address display and completion, session lines, tab stepping and labels.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const APP_ID: &core::ffi::CStr = c"org.southstar.WebBrowser";
pub const PROJECT_URL: &core::ffi::CStr = c"https://github.com/nordstjernen-web/southstar-browser";
pub const FULLSCREEN_NOTICE_SECONDS: u32 = 5;
pub const TAB_TITLE_BYTES: usize = 40;
pub const SECURITY_SECURE: i32 = 1;
pub const SECURITY_INVALID: i32 = 2;
pub const SECURITY_PLAIN: i32 = 3;

pub const ICON_SEARCH_PATHS: [&core::ffi::CStr; 7] = [
    c"share/icons",
    c"../share/icons",
    c"data/icons",
    c"../data/icons",
    c"../../data/icons",
    c"../../../data/icons",
    c"../../../../data/icons",
];

fn ascii_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

pub fn strip(text: &[u8]) -> &[u8] {
    let start = text
        .iter()
        .position(|&c| !ascii_space(c))
        .unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|&c| !ascii_space(c))
        .map_or(start, |e| e + 1);
    &text[start..end.max(start)]
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

pub fn is_explicit_url(text: &[u8]) -> bool {
    [&b"about:"[..], b"file:", b"data:", b"view-source:"]
        .iter()
        .any(|p| text.starts_with(p))
        || contains(text, b"://")
}

pub fn https_url(text: &[u8]) -> Vec<u8> {
    [b"https://", text].concat()
}

pub fn display_is_safe(decoded: &str) -> bool {
    !decoded.chars().any(|c| {
        let c = u32::from(c);
        c < 0x20
            || c == 0x7f
            || (0x200e..=0x200f).contains(&c)
            || (0x202a..=0x202e).contains(&c)
            || (0x2066..=0x2069).contains(&c)
    })
}

pub fn completed_host(text: &[u8], ctrl: bool, shift: bool) -> Option<Vec<u8>> {
    let text = strip(text);
    if text.is_empty() || contains(text, b"://") || text.contains(&b' ') || is_explicit_url(text) {
        return None;
    }
    let suffix: &[u8] = if ctrl && shift {
        b".org"
    } else if shift {
        b".net"
    } else {
        b".com"
    };
    let prefix: &[u8] = if text.starts_with(b"www.") {
        b""
    } else {
        b"www."
    };
    Some(if text.ends_with(suffix) {
        [prefix, text].concat()
    } else {
        [prefix, text, suffix].concat()
    })
}

pub fn session_url_recoverable(u: &[u8]) -> bool {
    [&b"http://"[..], b"https://", b"ftp://", b"file://"]
        .iter()
        .any(|p| u.starts_with(p))
}

pub fn step_tab(current: i32, delta: i32, n: i32) -> i32 {
    ((current + delta) % n + n) % n
}

pub fn clip_title(t: &[u8]) -> &[u8] {
    &t[..t.len().min(TAB_TITLE_BYTES)]
}

pub fn split_download(text: &[u8]) -> (&[u8], Option<&[u8]>) {
    match text.iter().position(|&c| c == b'\t') {
        Some(tab) => (&text[..tab], Some(&text[tab + 1..])),
        None => (text, None),
    }
}

pub fn window_title(title: Option<&[u8]>, brand: &[u8]) -> Vec<u8> {
    match title.filter(|t| !t.is_empty()) {
        Some(t) => [t, " — ".as_bytes(), brand].concat(),
        None => brand.to_vec(),
    }
}

pub fn status_text(base: &[u8], webgl: Option<&[u8]>) -> Vec<u8> {
    match webgl {
        Some(note) if !base.is_empty() => [base, " · ".as_bytes(), note].concat(),
        Some(note) => note.to_vec(),
        None => base.to_vec(),
    }
}

pub fn prefers_dark(red: f32, green: f32, blue: f32) -> bool {
    let luma = 0.2126 * f64::from(red) + 0.7152 * f64::from(green) + 0.0722 * f64::from(blue);
    luma > 0.5
}

pub struct Shortcut {
    pub name: &'static core::ffi::CStr,
    pub accels: &'static [&'static core::ffi::CStr],
}

pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        name: c"back",
        accels: &[c"<Alt>Left"],
    },
    Shortcut {
        name: c"forward",
        accels: &[c"<Alt>Right"],
    },
    Shortcut {
        name: c"reload",
        accels: &[c"<Ctrl>r", c"F5"],
    },
    Shortcut {
        name: c"hard-reload",
        accels: &[c"<Ctrl><Shift>r", c"<Ctrl>F5"],
    },
    Shortcut {
        name: c"find",
        accels: &[c"<Ctrl>f"],
    },
    Shortcut {
        name: c"console",
        accels: &[c"<Ctrl><Shift>j", c"F12"],
    },
    Shortcut {
        name: c"view-source",
        accels: &[c"<Ctrl>u"],
    },
    Shortcut {
        name: c"bookmark-page",
        accels: &[c"<Ctrl>d"],
    },
    Shortcut {
        name: c"home",
        accels: &[c"<Alt>Home"],
    },
    Shortcut {
        name: c"new-tab",
        accels: &[c"<Ctrl>t"],
    },
    Shortcut {
        name: c"new-private-tab",
        accels: &[c"<Ctrl><Alt>p"],
    },
    Shortcut {
        name: c"close-tab",
        accels: &[c"<Ctrl>w"],
    },
    Shortcut {
        name: c"focus-address",
        accels: &[c"<Ctrl>l"],
    },
    Shortcut {
        name: c"focus-page",
        accels: &[c"Escape"],
    },
    Shortcut {
        name: c"zoom-in",
        accels: &[c"<Ctrl>plus", c"<Ctrl>equal", c"<Ctrl>KP_Add"],
    },
    Shortcut {
        name: c"zoom-out",
        accels: &[c"<Ctrl>minus", c"<Ctrl>KP_Subtract"],
    },
    Shortcut {
        name: c"zoom-reset",
        accels: &[c"<Ctrl>0", c"<Ctrl>KP_0"],
    },
    Shortcut {
        name: c"next-tab",
        accels: &[c"<Ctrl>Page_Down", c"<Ctrl>Tab"],
    },
    Shortcut {
        name: c"prev-tab",
        accels: &[c"<Ctrl>Page_Up", c"<Ctrl><Shift>Tab"],
    },
    Shortcut {
        name: c"task-manager",
        accels: &[c"<Shift>Escape"],
    },
    Shortcut {
        name: c"downloads",
        accels: &[c"<Ctrl>j"],
    },
    Shortcut {
        name: c"about",
        accels: &[],
    },
    Shortcut {
        name: c"history",
        accels: &[c"<Ctrl>h"],
    },
    Shortcut {
        name: c"print",
        accels: &[c"<Ctrl>p"],
    },
    Shortcut {
        name: c"save-pdf",
        accels: &[],
    },
    Shortcut {
        name: c"save-image",
        accels: &[],
    },
    Shortcut {
        name: c"fullscreen",
        accels: &[],
    },
    Shortcut {
        name: c"settings",
        accels: &[c"<Ctrl>comma"],
    },
    Shortcut {
        name: c"quit",
        accels: &[c"<Ctrl>q"],
    },
];

pub struct MenuItem {
    pub label: &'static core::ffi::CStr,
    pub action: &'static core::ffi::CStr,
    pub accel: Option<&'static core::ffi::CStr>,
}

const fn item(
    label: &'static core::ffi::CStr,
    action: &'static core::ffi::CStr,
    accel: Option<&'static core::ffi::CStr>,
) -> MenuItem {
    MenuItem {
        label,
        action,
        accel,
    }
}

pub const MENU: &[&[MenuItem]] = &[
    &[
        item(c"New Tab", c"win.new-tab", None),
        item(c"New Private Tab", c"win.new-private-tab", None),
    ],
    &[
        item(c"Zoom In", c"win.zoom-in", Some(c"<Ctrl>plus")),
        item(c"Zoom Out", c"win.zoom-out", Some(c"<Ctrl>minus")),
        item(c"Reset Zoom", c"win.zoom-reset", Some(c"<Ctrl>0")),
        item(c"Full Screen", c"win.fullscreen", None),
        item(c"Find in Page", c"win.find", None),
    ],
    &[
        item(
            c"Bookmark This Page",
            c"win.bookmark-page",
            Some(c"<Ctrl>d"),
        ),
        item(c"History", c"win.history", None),
        item(c"Downloads", c"win.downloads", None),
        item(c"Print…", c"win.print", Some(c"<Ctrl>P")),
        item(c"Save Page as PDF…", c"win.save-pdf", None),
        item(c"Save Page as Image…", c"win.save-image", None),
    ],
    &[
        item(c"Page Source", c"win.view-source", Some(c"<Ctrl>u")),
        item(
            c"JavaScript Console",
            c"win.console",
            Some(c"<Ctrl><Shift>j"),
        ),
        item(c"Task Manager", c"win.task-manager", None),
        item(c"Settings", c"win.settings", None),
    ],
];

pub const CACHE_OBJECT_DIRS: [&core::ffi::CStr; 4] = [c"cache", c"jsbc", c"webfonts", c"frames"];
pub const CACHE_STREAM_DIRS: [&core::ffi::CStr; 2] = [c"msaudio", c"msvideo"];
pub const CACHE_MIN_AGE_SECONDS: i64 = 3600;
pub const CACHE_WALK_LIMIT: u32 = 100_000;
