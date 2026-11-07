//! Southstar — phishing and malware checks (the local host list and Google Safe Browsing) and the warning interstitial.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::BTreeSet;
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

mod canon;
mod ffi;
mod google;
mod listfile;
mod protobuf;
mod status;

pub use google::start;
pub use status::rows as status_rows;

pub const UNSAFE_CONTINUE_SCHEME: &[u8] = b"southstar-unsafe-continue:";

struct Lists {
    blocked: BTreeSet<Vec<u8>>,
    allowed: BTreeSet<Vec<u8>>,
}

static LISTS: Mutex<Option<Lists>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Safe,
    LocalList,
    Malware,
    SocialEngineering,
    UnwantedSoftware,
}

impl Verdict {
    fn rank(self) -> u8 {
        match self {
            Verdict::Safe => 0,
            Verdict::UnwantedSoftware => 1,
            Verdict::SocialEngineering => 2,
            Verdict::LocalList => 3,
            Verdict::Malware => 4,
        }
    }

    pub(crate) fn worse(self, other: Verdict) -> Verdict {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Verdict::Malware => "Dangerous site ahead",
            Verdict::UnwantedSoftware => "Site ahead offers harmful programs",
            _ => "Deceptive site ahead",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Verdict::SocialEngineering => concat!(
                "Google Safe Browsing reports this site for phishing. It may pretend ",
                "to be a site you trust to get your passwords, payment details or ",
                "other personal information."
            ),
            Verdict::Malware => concat!(
                "Google Safe Browsing reports that this site spreads malware. Visiting ",
                "it may install programs that steal or damage the information on your device."
            ),
            Verdict::UnwantedSoftware => concat!(
                "Google Safe Browsing reports that this site pushes unwanted software. ",
                "It may try to get you to install programs that change your browser or ",
                "slow down your device."
            ),
            _ => concat!(
                "Southstar blocked this page because the site below is on your local list ",
                "of known phishing or malware hosts. Attackers there may try to trick you ",
                "into revealing passwords or payment details, or to install harmful software."
            ),
        }
    }

    fn details(self) -> &'static str {
        match self {
            Verdict::Safe | Verdict::LocalList => concat!(
                "This check runs entirely on your device against a local list \u{2014} ",
                "nothing about the page you visited was sent anywhere. "
            ),
            _ => concat!(
                "Southstar checks addresses against a list of hash prefixes kept on your ",
                "device. Only when one matches does it send Google a short prefix of the ",
                "address\u{2019}s hash \u{2014} never the address itself \u{2014} to confirm ",
                "the match. Advisory provided by Google. <a href=\"",
                "https://developers.google.com/safe-browsing/v4/advisory\">",
                "About Safe Browsing advisories</a>. "
            ),
        }
    }
}

struct Counters {
    checked: AtomicU64,
    local_blocks: AtomicU64,
    prefix_matches: AtomicU64,
    searches: AtomicU64,
    search_failures: AtomicU64,
    google_blocks: AtomicU64,
}

static COUNTERS: Counters = Counters {
    checked: AtomicU64::new(0),
    local_blocks: AtomicU64::new(0),
    prefix_matches: AtomicU64::new(0),
    searches: AtomicU64::new(0),
    search_failures: AtomicU64::new(0),
    google_blocks: AtomicU64::new(0),
};

pub fn enabled() -> bool {
    southstar_config::get().is_none_or(|c| c.safe_browsing != 0)
}

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
    "color:#555;font-size:13px}.tips a{color:#2f5fa8}",
    "</style></head><body><div class=\"card\">",
    "<div class=\"icon\">\u{26a0}\u{fe0f}</div>",
    "<h1>",
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
    "<div class=\"tips\">",
);

const PAGE_ADDRESS: &str = concat!(
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

pub fn allow_host(host: &[u8]) {
    if host.is_empty() {
        return;
    }
    with_lists(|lists| {
        lists.allowed.insert(host.to_ascii_lowercase());
    });
}

fn host_allowed(host: &[u8]) -> bool {
    let mut low = host.to_ascii_lowercase();
    while low.last() == Some(&b'.') {
        low.pop();
    }
    with_lists(|lists| lists.allowed.contains(&low))
}

pub(crate) fn local_host_count() -> usize {
    with_lists(|lists| lists.blocked.len())
}

pub fn check_url(url: &[u8]) -> Verdict {
    if !enabled() {
        return Verdict::Safe;
    }
    let Some(host) = southstar_url::bytes::host_from(url) else {
        return Verdict::Safe;
    };
    COUNTERS.checked.fetch_add(1, Ordering::Relaxed);
    if host_allowed(&host) {
        return Verdict::Safe;
    }
    if blocked(&host) {
        COUNTERS.local_blocks.fetch_add(1, Ordering::Relaxed);
        return Verdict::LocalList;
    }
    let verdict = google::check_url(url);
    if verdict != Verdict::Safe {
        COUNTERS.google_blocks.fetch_add(1, Ordering::Relaxed);
    }
    verdict
}

pub fn blocked(host: &[u8]) -> bool {
    if host.is_empty() || !enabled() {
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
            if lists
                .blocked
                .contains(southstar_digest::sha256_hex(suffix).as_bytes())
            {
                return true;
            }
            suffix = &suffix[dot + 1..];
        }
        false
    })
}

pub fn interstitial(url: Option<&CStr>, host: Option<&CStr>, verdict: Verdict) -> Vec<u8> {
    let url = url.unwrap_or_default();
    let host = host.unwrap_or_default();
    let continue_href =
        CString::new([UNSAFE_CONTINUE_SCHEME, url.to_bytes()].concat()).unwrap_or_default();
    [
        PAGE_HEAD.as_bytes(),
        verdict.heading().as_bytes(),
        b"</h1><p class=\"summary\">",
        verdict.summary().as_bytes(),
        b"</p><p class=\"host\">",
        &ffi::markup_escape(host),
        PAGE_ACTIONS.as_bytes(),
        &ffi::markup_escape(&continue_href),
        PAGE_TIPS.as_bytes(),
        verdict.details().as_bytes(),
        PAGE_ADDRESS.as_bytes(),
        &ffi::markup_escape(url),
        PAGE_TAIL.as_bytes(),
    ]
    .concat()
}
