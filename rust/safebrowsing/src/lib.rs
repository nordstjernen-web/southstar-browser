//! Southstar — local phishing/malware blocklist and warning interstitial.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::BTreeSet;
use std::ffi::{CStr, CString};
use std::sync::{Mutex, PoisonError};

mod ffi;

pub const UNSAFE_CONTINUE_SCHEME: &[u8] = b"southstar-unsafe-continue:";

struct Lists {
    blocked: BTreeSet<Vec<u8>>,
    allowed: BTreeSet<Vec<u8>>,
}

static LISTS: Mutex<Option<Lists>> = Mutex::new(None);

const BUNDLED_LISTS: [&CStr; 7] = [
    c"../Resources/share/southstar/safebrowsing.list",
    c"../share/southstar/safebrowsing.list",
    c"share/southstar/safebrowsing.list",
    c"data/safebrowsing.list",
    c"../data/safebrowsing.list",
    c"../../data/safebrowsing.list",
    c"../../../data/safebrowsing.list",
];

const WORKING_DIR_LIST: &CStr = c"data/safebrowsing.list";

const PAGE_HEAD: &str = concat!(
    "<!doctype html><html><head><meta charset=\"utf-8\">",
    "<title>Security warning — Southstar</title><style>",
    "body{font-family:system-ui,-apple-system,\"Segoe UI\",Helvetica,",
    "Arial,sans-serif;background:#7a1212;color:#1b1b22;margin:0;padding:0;",
    "min-height:100vh;display:flex;align-items:center;justify-content:center}",
    ".card{background:#fff;border:1px solid #e3e3e8;border-top:6px solid ",
    "#c0271c;border-radius:10px;box-shadow:0 6px 30px rgba(0,0,0,0.25);",
    "padding:36px 40px;max-width:640px;margin:32px 16px;line-height:1.5}",
    ".icon{font-size:52px;line-height:1;margin-bottom:14px}",
    "h1{font-size:23px;margin:0 0 12px 0;color:#a01b12}",
    "p.summary{font-size:16px;color:#33333d;margin:0 0 16px 0}",
    ".host{font-family:ui-monospace,\"SF Mono\",Menlo,Consolas,monospace;",
    "background:#fbeeec;border:1px solid #f0cfcb;border-radius:6px;",
    "padding:8px 12px;font-size:14px;color:#a01b12;font-weight:600;",
    "overflow-wrap:anywhere;margin:0 0 18px 0}",
    ".actions{display:flex;gap:10px;flex-wrap:wrap}",
    ".btn{display:inline-block;padding:10px 18px;border-radius:6px;",
    "text-decoration:none;font-size:14px;font-weight:600;",
    "border:1px solid transparent;font-family:inherit}",
    ".btn.primary{background:#2f7d36;color:#fff;border-color:#2f7d36}",
    ".btn.primary:hover{background:#286b2e}",
    ".btn.danger{background:#fff;color:#a01b12;border-color:#e0b6b2}",
    ".btn.danger:hover{background:#fbeeec}",
    ".tips{margin-top:24px;padding-top:18px;border-top:1px solid #ececf0;",
    "color:#555;font-size:13px}",
    "</style></head><body><div class=\"card\">",
    "<div class=\"icon\">\u{26a0}\u{fe0f}</div>",
    "<h1>Deceptive site ahead</h1>",
    "<p class=\"summary\">Southstar blocked this page because the site ",
    "below is on your local list of known phishing or malware hosts. ",
    "Attackers there may try to trick you into revealing passwords or ",
    "payment details, or to install harmful software.</p>",
    "<p class=\"host\">",
);

const PAGE_ACTIONS: &str = concat!(
    "</p>",
    "<div class=\"actions\">",
    "<a class=\"btn primary\" href=\"about:start\">Back to safety</a>",
    "<a class=\"btn danger\" href=\"",
);

const PAGE_TIPS: &str = concat!(
    "\">Continue anyway ",
    "(not recommended)</a>",
    "</div>",
    "<div class=\"tips\">This check runs entirely on your device against a ",
    "local list — nothing about the page you visited was sent anywhere. ",
    "The blocked address was <span style=\"font-family:ui-monospace,",
    "monospace;overflow-wrap:anywhere\">",
);

const PAGE_TAIL: &str = concat!("</span>.</div>", "</div></body></html>");

fn load_file(blocked: &mut BTreeSet<Vec<u8>>, path: &CStr) {
    let Some(text) = ffi::file_text(path) else {
        return;
    };
    for line in text.split(|&byte| byte == b'\n') {
        let line = line.trim_ascii();
        if line.first().is_none_or(|&byte| byte == b'#') {
            continue;
        }
        let end = line
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(line.len());
        if end >= 16 {
            blocked.insert(line[..end].to_ascii_lowercase());
        }
    }
}

fn load_bundled(blocked: &mut BTreeSet<Vec<u8>>) {
    let beside_exe = ffi::self_exe_dir().and_then(|dir| {
        BUNDLED_LISTS
            .iter()
            .map(|rel| ffi::build_filename(&[&dir, rel]))
            .find(|path| ffi::exists(path))
    });
    let path =
        beside_exe.or_else(|| ffi::exists(WORKING_DIR_LIST).then(|| WORKING_DIR_LIST.to_owned()));
    if let Some(path) = path {
        load_file(blocked, &path);
    }
}

fn load() -> Lists {
    let mut blocked = BTreeSet::new();
    match ffi::getenv(c"NS_SAFEBROWSING_LIST").filter(|path| !path.is_empty()) {
        Some(path) => load_file(&mut blocked, &path),
        None => {
            let config = ffi::user_config_dir();
            let user = ffi::build_filename(&[&config, c"southstar", c"safebrowsing.list"]);
            load_file(&mut blocked, &user);
            load_bundled(&mut blocked);
        }
    }
    Lists {
        blocked,
        allowed: BTreeSet::new(),
    }
}

fn with_lists<R>(f: impl FnOnce(&mut Lists) -> R) -> R {
    let mut lists = LISTS.lock().unwrap_or_else(PoisonError::into_inner);
    f(lists.get_or_insert_with(load))
}

fn sha256_hex(host: &[u8]) -> Option<Vec<u8>> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = ffi::sha256(host)?;
    Some(
        digest
            .iter()
            .flat_map(|&byte| [HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 15)]])
            .collect(),
    )
}

pub fn allow_host(host: &[u8]) {
    if host.is_empty() {
        return;
    }
    with_lists(|lists| {
        lists.allowed.insert(host.to_ascii_lowercase());
    });
}

pub fn blocked(host: &[u8]) -> bool {
    if host.is_empty() {
        return false;
    }
    with_lists(|lists| {
        if lists.blocked.is_empty() {
            return false;
        }
        let mut low = host.to_ascii_lowercase();
        while low.last() == Some(&b'.') {
            low.pop();
        }
        if lists.allowed.contains(&low) {
            return false;
        }
        let mut suffix = &low[..];
        while let Some(dot) = suffix.iter().position(|&byte| byte == b'.') {
            if sha256_hex(suffix).is_some_and(|hex| lists.blocked.contains(&hex)) {
                return true;
            }
            suffix = &suffix[dot + 1..];
        }
        false
    })
}

pub fn interstitial(url: Option<&CStr>, host: Option<&CStr>) -> Vec<u8> {
    let url = url.unwrap_or_default();
    let host = host.unwrap_or_default();
    let continue_href =
        CString::new([UNSAFE_CONTINUE_SCHEME, url.to_bytes()].concat()).unwrap_or_default();
    [
        PAGE_HEAD.as_bytes(),
        &ffi::markup_escape(host),
        PAGE_ACTIONS.as_bytes(),
        &ffi::markup_escape(&continue_href),
        PAGE_TIPS.as_bytes(),
        &ffi::markup_escape(url),
        PAGE_TAIL.as_bytes(),
    ]
    .concat()
}
