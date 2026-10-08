//! Southstar — the headless run driven through a renderer connection: open, follow navigations, scripted actions and dumps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use crate::ffi::{self, Renderer, err, fmt_g, out};
use crate::text::strip;
use crate::{Dump, Opts, nonempty};

pub const KEYS: [(&[u8], &CStr, c_int); 12] = [
    (b"Enter", c"Enter", 13),
    (b"Return", c"Enter", 13),
    (b"Backspace", c"Backspace", 8),
    (b"Delete", c"Delete", 46),
    (b"Tab", c"Tab", 9),
    (b"Escape", c"Escape", 27),
    (b"Left", c"ArrowLeft", 37),
    (b"Right", c"ArrowRight", 39),
    (b"Up", c"ArrowUp", 38),
    (b"Down", c"ArrowDown", 40),
    (b"Home", c"Home", 36),
    (b"End", c"End", 35),
];

fn cstring(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap_or_default()
}

pub fn run(opts: &Opts) -> c_int {
    ffi::single_process_enable();
    let main_loop = ffi::MainLoop::new();
    let quit = main_loop.quitter();
    std::thread::scope(|scope| {
        let driver = std::thread::Builder::new()
            .name("ns-headless-drv".into())
            .spawn_scoped(scope, move || {
                let rc = drive(opts);
                quit.quit_when_idle();
                rc
            });
        let Ok(driver) = driver else {
            return 2;
        };
        main_loop.run();
        driver.join().unwrap_or(2)
    })
}

fn drive(o: &Opts) -> c_int {
    let vw = if o.viewport_width > 0 {
        o.viewport_width
    } else {
        1000
    };
    let vh = if o.viewport_height > 0 {
        o.viewport_height
    } else {
        (vw as f64 * 0.75) as c_int
    };
    let Some(r) = Renderer::spawn_shm(vw, vh) else {
        return 2;
    };
    let Some(page) = o.url.and_then(|url| r.open(url, vw, vh, o.settle_ms)) else {
        return 2;
    };
    let nav = page.nav();
    drop(page);
    if let Some(nav) = nav {
        follow_nav(&r, Some(nav), vw, vh, o.settle_ms);
    }
    drain_console(&r);

    if let Some(actions) = nonempty(o.actions) {
        run_actions(&r, actions.to_bytes(), vw, vh, o.settle_ms);
        drain_console(&r);
    }

    if let Some(src) = nonempty(o.eval) {
        if let Some(res) = r.eval(src) {
            out(&[b"eval: ", res.as_bytes(), b"\n"].concat());
        }
        drain_console(&r);
    }

    let kind = match o.dump {
        Dump::Text => Some(c"text"),
        Dump::Dom => Some(c"dom"),
        Dump::Layout => Some(c"layout"),
        _ => None,
    };
    if let Some(dump) = kind.and_then(|kind| r.dump(kind)) {
        out(dump.as_bytes());
    }
    0
}

fn drain_console(r: &Renderer) {
    let Some(log) = r.console_poll() else {
        return;
    };
    let log = log.as_bytes();
    if log.is_empty() {
        return;
    }
    err(log);
    if log.last() != Some(&b'\n') {
        err(b"\n");
    }
}

fn follow_nav(r: &Renderer, mut href: Option<Vec<u8>>, vw: c_int, vh: c_int, settle_ms: c_int) {
    let mut hops = 0;
    while let Some(url) = href.take().filter(|h| !h.is_empty() && hops < 6) {
        let Some(page) = r.open(&cstring(&url), vw, vh, settle_ms) else {
            break;
        };
        err(&[b"[headless] open -> ", url.as_slice(), b"\n"].concat());
        href = page.nav();
        hops += 1;
    }
}

fn tick_take_nav(r: &Renderer, vw: c_int, vh: c_int) -> Option<Vec<u8>> {
    r.render(vw, vh)?.nav().filter(|nav| !nav.is_empty())
}

fn run_actions(r: &Renderer, spec: &[u8], mut vw: c_int, mut vh: c_int, settle_ms: c_int) {
    for action in spec.split(|&b| b == b';') {
        let a = strip(action);
        if a.is_empty() {
            continue;
        }
        if let Some(rest) = a.strip_prefix(b"click ") {
            let Some((x, y)) = ffi::scan_point(rest) else {
                continue;
            };
            err(format!("[headless] click {},{}\n", fmt_g(x), fmt_g(y)).as_bytes());
            drop(r.click(x as c_int, y as c_int));
            if let Some(href) = r.release_full().filter(|href| !href.is_empty()) {
                follow_nav(r, Some(href.as_bytes().to_vec()), vw, vh, settle_ms);
            }
        } else if let Some(rest) = a.strip_prefix(b"select ") {
            let Some((kind, x, y)) = ffi::scan_select(rest) else {
                continue;
            };
            let text = r.select(kind, x as c_int, y as c_int);
            if kind == 4 || kind == 7 {
                let text = text.as_ref().map_or(&b""[..], |t| t.as_bytes());
                out(&[b"act-select: ", text, b"\n"].concat());
            }
        } else if let Some(rest) = a.strip_prefix(b"rightclick ") {
            let Some((x, y)) = ffi::scan_point(rest) else {
                continue;
            };
            let (prevented, edit) = r.contextmenu(x as c_int, y as c_int);
            err(format!(
                "[headless] rightclick {},{} prevented={prevented} edit={edit}\n",
                fmt_g(x),
                fmt_g(y)
            )
            .as_bytes());
        } else if let Some(text) = a.strip_prefix(b"type ") {
            err(&[b"[headless] type \"", text, b"\"\n"].concat());
            drop(r.key(2, &cstring(text), c"", 0));
        } else if let Some(raw) = a.strip_prefix(b"paste ") {
            let text = ffi::strcompress(raw);
            err(&[b"[headless] paste \"", raw, b"\"\n"].concat());
            drop(r.key(4, &text, c"", 0));
        } else if let Some(rest) = a.strip_prefix(b"key ") {
            let name = strip(rest);
            err(&[b"[headless] key ", name, b"\n"].concat());
            let named = cstring(name);
            let (key, code) = KEYS
                .iter()
                .find(|(n, _, _)| name.eq_ignore_ascii_case(n))
                .map_or((named.as_c_str(), 0), |&(_, key, code)| (key, code));
            if let Some(h) = r.key(0, key, key, code).filter(|h| !h.is_empty()) {
                follow_nav(r, Some(h.as_bytes().to_vec()), vw, vh, settle_ms);
            }
            drop(r.key(1, key, key, code));
        } else if let Some(src) = a.strip_prefix(b"eval ") {
            eval_and_follow(r, &cstring(src), vw, vh, settle_ms);
        } else if let Some(rest) = a.strip_prefix(b"evalfile ") {
            let path = strip(rest);
            match ffi::file_contents(path) {
                Some(src) => eval_and_follow(r, &src, vw, vh, settle_ms),
                None => err(&[b"[headless] evalfile: cannot read ", path, b"\n"].concat()),
            }
        } else if let Some(rest) = a.strip_prefix(b"shot ") {
            shot(r, strip(rest), vw, vh);
        } else if let Some(rest) = a.strip_prefix(b"viewport ") {
            let Some((nw, nh)) = ffi::scan_size(rest).filter(|&(w, h)| w > 0 && h > 0) else {
                continue;
            };
            err(format!("[headless] viewport {nw}x{nh}\n").as_bytes());
            vw = nw;
            vh = nh;
            drop(r.set_viewport(vw, vh));
            drop(r.render(vw, vh));
        } else if let Some(rest) = a.strip_prefix(b"wait ") {
            let ms = ffi::ascii_strtoll(rest);
            err(format!("[headless] wait {ms}ms\n").as_bytes());
            let end = ffi::monotonic_us().wrapping_add(ms.wrapping_mul(1000));
            while ffi::monotonic_us() < end {
                if let Some(nav) = r.render(vw, vh).and_then(|f| f.nav()) {
                    if !nav.is_empty() {
                        follow_nav(r, Some(nav), vw, vh, settle_ms);
                        continue;
                    }
                }
                ffi::usleep(33000);
            }
        }
    }
}

fn eval_and_follow(r: &Renderer, src: &CStr, vw: c_int, vh: c_int, settle_ms: c_int) {
    let res = r.eval(src);
    let shown = res.as_ref().map_or(&b"(null)"[..], |res| res.as_bytes());
    out(&[b"act-eval: ", shown, b"\n"].concat());
    drop(res);
    if let Some(nav) = tick_take_nav(r, vw, vh) {
        follow_nav(r, Some(nav), vw, vh, settle_ms);
    }
}

fn shot(r: &Renderer, path: &[u8], vw: c_int, vh: c_int) {
    drop(r.eval(c"0"));
    if path.is_empty() {
        return;
    }
    let Some(frame) = r.render(vw, vh) else {
        return;
    };
    let Some((width, height)) = frame.size().filter(|&(w, h)| w > 0 && h > 0) else {
        return;
    };
    if let Some(png) = frame.encode_png() {
        let b64 = ffi::base64(&png);
        out(&[b"shot-b64:", path, b":", b64.to_bytes(), b"\n"].concat());
        ffi::flush();
    }
    err(format!("[headless] shot {width}x{height} emitted\n").as_bytes());
}
