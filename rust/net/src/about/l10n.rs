//! Southstar — about: page translation: runs the `{{…}}` strings of a page template and the diagnostics labels through the UI catalogue.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_html_util::escape_append;

pub fn tr(text: &str) -> &str {
    CString::new(text)
        .ok()
        .and_then(|key| southstar_i18n::translate(&key))
        .and_then(|translated| translated.to_str().ok())
        .unwrap_or(text)
}

pub fn lang() -> String {
    southstar_i18n::language()
        .and_then(|tag| tag.to_str().ok())
        .filter(|tag| !tag.is_empty())
        .map_or_else(|| "en".to_owned(), |tag| tag.replace('_', "-"))
}

fn js_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\'' | '"' | '\\' | '<' | '>' | '&' | '\n' | '\r' | '\u{2028}' | '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn html(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    escape_append(&mut out, tr(text).as_bytes(), true);
    String::from_utf8_lossy(&out).into_owned()
}

pub fn localize(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        let Some(close) = rest[open + 2..].find("}}") else {
            break;
        };
        out.push_str(&rest[..open]);
        let key = &rest[open + 2..open + 2 + close];
        match key.strip_prefix("js:") {
            Some(key) => out.push_str(&js_escape(tr(key))),
            None => out.push_str(&html(key)),
        }
        rest = &rest[open + 2 + close + 2..];
    }
    out.push_str(rest);
    out.replace("__ND_LANG__", &lang())
}
