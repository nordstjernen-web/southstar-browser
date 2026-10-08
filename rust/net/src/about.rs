//! Southstar — about: pages: the new tab, about:southstar with its diagnostics, the license texts, history and settings with its JSON endpoints, behind the chrome check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod diagnostics;
mod settings;
mod templates;

use core::ffi::c_long;
use std::sync::OnceLock;

use southstar_about_style::base_css;
use southstar_html_util::escape_text;

use crate::ffi::host as sys;

pub struct Page {
    pub status: c_long,
    pub content_type: &'static [u8],
    pub body: Vec<u8>,
}

const HTML: &[u8] = b"text/html; charset=utf-8";
const JSON: &[u8] = b"application/json; charset=utf-8";
const BLANK: &str = "<!doctype html><title>Blank</title>";
const FALLBACK: &str = "<!doctype html><title>Southstar</title>";
const WEB_CONTENT_DENIED: &str = "about: pages are not available to web content";

const MOZILLA: &str = concat!(
    "<!doctype html><html><head><meta charset=\"utf-8\">",
    "<title>The Book of Mozilla, 11:9</title><style>",
    "body{background:maroon;color:#fff;margin:0;height:100vh;",
    "display:flex;align-items:center;justify-content:center}",
    "p{font-family:serif;font-style:italic;font-size:1.3em;",
    "max-width:34em;text-align:center;padding:0 1em}",
    "</style></head><body><p>",
    "And the beast begat a bird of fire, and the bird flew out among ",
    "the people and gave the web back to them. Those who came after ",
    "walked by its light, and kindled fires of their own, so that no ",
    "one power should ever again hold the roads alone.",
    "<br><br>from <strong>The Book of Mozilla,</strong> 11:9",
    "</p></body></html>",
);

const BOOK: &str = concat!(
    "<!doctype html><html><head><meta charset=\"utf-8\">",
    "<title>The Book of Southstar</title><style>",
    "body{background:#fff;color:#7a0000;margin:0;height:100vh;",
    "display:flex;align-items:center;justify-content:center}",
    "p{font-family:serif;font-style:italic;font-size:1.3em;",
    "max-width:34em;text-align:center;padding:0 1em}",
    "</style></head><body><p>",
    "And when the great engines had grown vast beyond all reckoning, ",
    "a small light rose in the north, written by hand and beholden ",
    "to no one. It asked for nothing, reported to nowhere, and ",
    "carried the travellers over the wire by the old free roads.",
    "<br><br>from <strong>The Book of Southstar,</strong> 1:1",
    "</p></body></html>",
);

const TAGLINES: [&str; 5] = [
    "Southstar the unique web browser",
    "S\u{f8}rstjernen the unique web browser",
    "Sydstj\u{e4}rnan the unique web browser",
    "\u{c9}toile du Sud the unique web browser",
    "S\u{fc}dstern the unique web browser",
];

const LOGO_HEADER: &[u8] = include_bytes!("../../../src/about_logo_gif.h");
const SPLASH_HEADER: &[u8] = include_bytes!("../../../src/about_splash_gif.h");

const LOGO_PATHS: [&str; 4] = [
    "share/icons/hicolor/scalable/apps/southstar.gif",
    "../share/icons/hicolor/scalable/apps/southstar.gif",
    "../../data/icons/hicolor/scalable/apps/southstar.gif",
    "data/icons/hicolor/scalable/apps/southstar.gif",
];

struct Document {
    file: &'static str,
    title: &'static str,
}

const LICENSE: Document = Document {
    file: "License.md",
    title: "Southstar License",
};
const GPL: Document = Document {
    file: "COPYING",
    title: "GNU General Public License, version 3",
};
const THIRD_PARTY: Document = Document {
    file: "THIRD-PARTY-LICENSES.md",
    title: "Third-party software notices",
};

pub fn request_from_chrome(top_url: Option<&[u8]>) -> bool {
    let Some(top_url) = top_url.filter(|t| !t.is_empty()) else {
        return true;
    };
    let Some(page) = top_url.strip_prefix(b"about:") else {
        return false;
    };
    let page = &page[..page
        .iter()
        .position(|&c| c == b'?' || c == b'#')
        .unwrap_or(page.len())];
    !page.eq_ignore_ascii_case(b"blank") && !page.eq_ignore_ascii_case(b"srcdoc")
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| from + i)
}

fn substitute(text: &[u8], placeholder: &[u8], value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + value.len());
    let mut at = 0;
    while let Some(hit) = find(text, placeholder, at) {
        out.extend_from_slice(&text[at..hit]);
        out.extend_from_slice(value);
        at = hit + placeholder.len();
    }
    out.extend_from_slice(&text[at..]);
    out
}

fn join_path(dir: &[u8], rel: &str) -> Vec<u8> {
    crate::ffi::sys::build_filename(dir, rel.as_bytes())
}

fn read_first(rel_paths: &[&str]) -> Option<Vec<u8>> {
    let exe_dir =
        sys::self_exe().map_or_else(|| b".".to_vec(), |exe| crate::ffi::sys::path_dirname(&exe));
    let user_data_dir = sys::user_data_dir().filter(|d| !d.is_empty());
    let system_data_dirs = sys::system_data_dirs();
    rel_paths.iter().find_map(|rel| {
        sys::read_file(&join_path(&exe_dir, rel))
            .or_else(|| {
                user_data_dir
                    .as_ref()
                    .and_then(|dir| sys::read_file(&join_path(dir, rel)))
            })
            .or_else(|| {
                system_data_dirs
                    .iter()
                    .find_map(|dir| sys::read_file(&join_path(dir, rel)))
            })
    })
}

fn header_literal(header: &[u8]) -> Vec<u8> {
    let start = find(header, b"[] =", 0).map_or(header.len(), |i| i + 4);
    let mut out = Vec::new();
    let mut in_string = false;
    for &b in &header[start..] {
        match (in_string, b) {
            (false, b';') => break,
            (_, b'"') => in_string = !in_string,
            (true, _) => out.push(b),
            _ => {}
        }
    }
    out
}

fn logo_data_uri() -> &'static [u8] {
    static URI: OnceLock<Vec<u8>> = OnceLock::new();
    URI.get_or_init(|| {
        let encoded = match read_first(&LOGO_PATHS) {
            Some(gif) => sys::base64(&gif),
            None => header_literal(LOGO_HEADER),
        };
        [&b"data:image/gif;base64,"[..], &encoded].concat()
    })
}

fn logo_markup() -> Vec<u8> {
    [
        &b"<img class=\"mark-img\" src=\""[..],
        logo_data_uri(),
        b"\" alt=\"Southstar\" aria-hidden=\"true\">",
    ]
    .concat()
}

fn splash_markup() -> Vec<u8> {
    static SPLASH: OnceLock<Vec<u8>> = OnceLock::new();
    let encoded = SPLASH.get_or_init(|| header_literal(SPLASH_HEADER));
    [
        &b"<img class=\"splash\" src=\"data:image/gif;base64,"[..],
        encoded,
        b"\" alt=\"Southstar ",
        templates::VERSION.as_bytes(),
        b" splash\">",
    ]
    .concat()
}

fn document_page(doc: &Document) -> Vec<u8> {
    let paths = [
        format!("southstar/{}", doc.file),
        format!("share/southstar/{}", doc.file),
        format!("../share/southstar/{}", doc.file),
        format!("../../../{}", doc.file),
        format!("../../{}", doc.file),
        doc.file.to_owned(),
    ];
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let Some(text) = read_first(&paths) else {
        return format!(
            "<!doctype html><meta charset=utf-8><title>{title}</title><p>{file} is missing from the install \u{2014} reinstall the package or copy <code>{file}</code> next to the binary.</p>",
            title = doc.title,
            file = doc.file
        )
        .into_bytes();
    };
    let mut out = Vec::new();
    out.extend_from_slice(b"<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"color-scheme\" content=\"light dark\"><title>");
    out.extend_from_slice(doc.title.as_bytes());
    out.extend_from_slice(b"</title><style>");
    out.extend_from_slice(base_css().as_bytes());
    out.extend_from_slice(DOCUMENT_STYLE.as_bytes());
    out.extend_from_slice(b"</style></head><body><main class=\"wrap\"><a class=\"crumb\" href=\"about:southstar\">&larr; About Southstar</a><h1>");
    out.extend_from_slice(doc.title.as_bytes());
    out.extend_from_slice(b"</h1><div class=\"card\"><pre>");
    out.extend_from_slice(&escape_text(until_nul(&text)));
    out.extend_from_slice(b"</pre></div></main></body></html>");
    out
}

const DOCUMENT_STYLE: &str = concat!(
    ".wrap{max-width:860px;margin:0 auto;padding:28px 24px 56px}",
    "h1{margin:22px 4px 20px;font-size:28px;letter-spacing:-.02em}",
    ".card{padding:26px 30px}",
    "pre{margin:0;white-space:pre-wrap;word-wrap:break-word;",
    "font-family:var(--mono);font-size:13px;line-height:1.65;",
    "color:var(--muted)}"
);

fn search_engine() -> Vec<u8> {
    let config = sys::ConfigGuard::lock();
    config
        .get()
        .and_then(|c| sys::config_text(c.search_engine))
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| southstar_config::DEFAULT_SEARCH_ENGINE.as_bytes().to_vec())
}

fn js_string_escape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for &b in text {
        if matches!(b, b'\'' | b'\\' | b'<' | b'"') {
            out.extend_from_slice(format!("\\u{b:04x}").as_bytes());
        } else {
            out.push(b);
        }
    }
    out
}

fn search_name(engine: &[u8]) -> Vec<u8> {
    let Some(host) = sys::url_host(engine).filter(|h| !h.is_empty()) else {
        return b"the web".to_vec();
    };
    match sys::registrable_domain(&host).filter(|r| !r.is_empty()) {
        Some(registrable) => registrable,
        None => host.strip_prefix(b"www.").unwrap_or(&host).to_vec(),
    }
}

fn start_page() -> Vec<u8> {
    let engine = search_engine();
    let page = substitute(
        templates::start().as_bytes(),
        b"__ND_SEARCH_URL__",
        &js_string_escape(&engine),
    );
    let page = substitute(
        &page,
        b"__ND_SEARCH_NAME__",
        &sys::markup_escape(&search_name(&engine)),
    );
    let page = substitute(&page, b"__ND_LOGO_MARK__", &logo_markup());
    substitute(
        &page,
        b"__ND_TAGLINE__",
        TAGLINES[sys::random_below(TAGLINES.len() as i32)].as_bytes(),
    )
}

fn southstar_page() -> Vec<u8> {
    let page = substitute(
        templates::southstar().as_bytes(),
        b"__ND_LOGO_MARK__",
        &logo_markup(),
    );
    let page = substitute(&page, b"__ND_DIAG__", &diagnostics::html());
    substitute(&page, b"__ND_SPLASH__", &splash_markup())
}

fn request_form(url: &[u8], method: Option<&[u8]>, body: Option<&[u8]>) -> Vec<u8> {
    let posted = method.is_some_and(|m| m.eq_ignore_ascii_case(b"POST"));
    match body.filter(|b| posted && !b.is_empty()) {
        Some(body) => until_nul(body).to_vec(),
        None => url
            .iter()
            .position(|&c| c == b'?')
            .map_or_else(Vec::new, |q| url[q + 1..].to_vec()),
    }
}

fn html(body: impl Into<Vec<u8>>) -> Page {
    Page {
        status: 200,
        content_type: HTML,
        body: body.into(),
    }
}

fn json(body: impl Into<Vec<u8>>) -> Page {
    Page {
        status: 200,
        content_type: JSON,
        body: body.into(),
    }
}

pub fn respond(
    url: &[u8],
    top_url: Option<&[u8]>,
    method: Option<&[u8]>,
    body: Option<&[u8]>,
) -> Option<Page> {
    let what = url.strip_prefix(b"about:")?;
    let chrome_only = what.starts_with(b"ai")
        || what == b"history"
        || what == b"config"
        || what.starts_with(b"settings");
    if chrome_only && !request_from_chrome(top_url) {
        return Some(Page {
            status: 403,
            content_type: b"text/plain; charset=utf-8",
            body: WEB_CONTENT_DENIED.into(),
        });
    }
    Some(match what {
        b"blank" | b"" => html(BLANK),
        b"start" | b"home" | b"newtab" => html(start_page()),
        b"southstar" | b"about" => html(southstar_page()),
        b"mozilla" => html(MOZILLA),
        b"book" => html(BOOK),
        b"license" | b"licence" => html(document_page(&LICENSE)),
        b"gpl" | b"copying" => html(document_page(&GPL)),
        b"third-party" | b"third-party-licenses" | b"credits" => html(document_page(&THIRD_PARTY)),
        b"history" => html(sys::history_page()),
        b"settings" | b"config" => html(templates::settings()),
        _ if what.starts_with(b"settings-data") => json(settings::json()),
        _ if what.starts_with(b"settings-save") => {
            settings::save(&request_form(url, method, body));
            json("{\"ok\":true}")
        }
        _ if what.starts_with(b"settings-clear") => {
            sys::clear_browsing_data();
            json("{\"ok\":true}")
        }
        _ => html(FALLBACK),
    })
}
