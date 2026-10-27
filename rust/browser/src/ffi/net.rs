//! Southstar — the navigation response a page is opened from and the URL, safe-browsing and HTML calls of opening it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long};
use std::ffi::CString;

use southstar_dom::Tree;

use super::glib::owned_text;

pub use southstar_js_runtime::api::NavigationTiming;

pub use southstar_engine::Post;

pub struct Response(southstar_net::Response);

impl Response {
    pub fn navigate(
        url: &CStr,
        referrer: Option<&CStr>,
        initiator: Option<&CStr>,
        user_activated: bool,
        post: Option<&Post<'_>>,
    ) -> Option<Response> {
        southstar_engine::navigate_blocking(url, referrer, initiator, user_activated, post)
            .ok()
            .map(Response)
    }

    pub fn status(&self) -> c_long {
        self.0.status()
    }

    pub fn error(&self) -> Option<&CStr> {
        self.0.error()
    }

    pub fn has_body(&self) -> bool {
        self.0.body().is_some()
    }

    pub fn body(&self) -> &[u8] {
        self.0.body().unwrap_or_default()
    }

    pub fn final_url(&self) -> Option<&CStr> {
        self.0.final_url()
    }

    pub fn content_type(&self) -> Option<&CStr> {
        self.0.content_type()
    }

    pub fn refresh(&self) -> Option<&CStr> {
        self.0.refresh()
    }

    pub fn content_language(&self) -> Option<&CStr> {
        self.0.content_language()
    }

    pub fn csp_header(&self) -> Option<&CStr> {
        self.0.csp_header()
    }

    pub fn raw_headers(&self) -> Option<&CStr> {
        self.0.raw_headers()
    }

    pub fn remote_ip(&self) -> Option<&CStr> {
        self.0.remote_ip()
    }

    pub fn security(&self) -> c_int {
        self.0.security()
    }

    pub fn timing(&self) -> NavigationTiming {
        let r = self.0.timing();
        NavigationTiming {
            origin_us: r.request_start_us,
            origin_real_ms: r.request_start_real_ms,
            domain_lookup_end_ms: r.domain_lookup_ms,
            connect_start_ms: r.domain_lookup_ms,
            connect_end_ms: r.connect_ms,
            secure_connection_start_ms: if r.connect_ms < r.tls_ms {
                r.connect_ms
            } else {
                0.0
            },
            request_start_ms: r.pretransfer_ms,
            response_start_ms: r.response_start_ms,
            response_end_ms: r.response_end_ms,
            ..NavigationTiming::default()
        }
    }

    pub fn decoded_text(&self) -> Vec<u8> {
        let content_type = self.content_type().map(CStr::to_bytes);
        let (mut decoded, _) = southstar_html_util::decode_body(self.body(), content_type);
        if let Some(nul) = decoded.iter().position(|&b| b == 0) {
            decoded.truncate(nul);
        }
        decoded
    }

    pub fn pdf_document(&self, url: &CStr) -> Vec<u8> {
        southstar_pdf::document_html(self.body(), Some(url.to_bytes()))
    }

    pub fn set_body(&mut self, bytes: &[u8]) {
        self.0.set_body(bytes);
    }

    pub fn set_content_type(&mut self, value: &CStr) {
        self.0.set_content_type(value);
    }

    pub fn set_final_url(&mut self, value: &CStr) {
        self.0.set_final_url(value);
    }

    pub fn clear_error(&mut self) {
        self.0.clear_error();
    }

    pub fn set_security(&mut self, security: c_int) {
        self.0.set_security(security);
    }
}

pub fn build_error_page(
    url: &CStr,
    status: c_long,
    transport_error: Option<&CStr>,
) -> Option<CString> {
    owned_text(&southstar_net::build_error_page(
        Some(url.to_bytes()),
        status,
        transport_error.map(CStr::to_bytes),
    ))
}

pub fn url_host_from(url: &CStr) -> Option<CString> {
    southstar_url::bytes::host_from(url.to_bytes()).and_then(|host| CString::new(host).ok())
}

pub fn url_strip_tracking_params(url: &CStr) -> Option<CString> {
    southstar_net::strip_tracking_params(url.to_bytes()).and_then(|url| owned_text(&url))
}

pub fn https_first_upgrade(url: &CStr) -> Option<CString> {
    southstar_net::https_first_upgrade(url.to_bytes()).and_then(|url| owned_text(&url))
}

pub fn safebrowsing_blocked(host: &CStr) -> bool {
    southstar_safebrowsing::blocked(host.to_bytes())
}

pub fn safebrowsing_allow_host(host: &CStr) {
    southstar_safebrowsing::allow_host(host.to_bytes());
}

pub fn safebrowsing_interstitial(url: &CStr, host: &CStr) -> Option<CString> {
    owned_text(&southstar_safebrowsing::interstitial(Some(url), Some(host)))
}

pub struct ParsedDoc(pub(super) Option<Tree>);

pub fn html_parse_bytes(input: &[u8], scripting: bool) -> ParsedDoc {
    ParsedDoc(Some(southstar_html_parser::parse_document(
        input, scripting,
    )))
}

pub fn html_parse(input: Option<&CStr>, scripting: bool) -> ParsedDoc {
    ParsedDoc(input.map(|html| southstar_html_parser::parse_document(html.to_bytes(), scripting)))
}

pub fn css_set_color_scheme(dark: bool) {
    southstar_css_media::set_prefers_dark(dark);
}

pub fn css_set_reduced_motion(reduce: bool) {
    southstar_css_media::set_prefers_reduced_motion(reduce);
}

#[cfg(unix)]
fn os_path(path: &CStr) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(std::ffi::OsStr::from_bytes(path.to_bytes()).into())
}

#[cfg(not(unix))]
fn os_path(path: &CStr) -> Option<std::path::PathBuf> {
    path.to_str().ok().map(Into::into)
}

pub fn file_exists(path: &CStr) -> bool {
    os_path(path).is_some_and(|p| p.exists())
}

#[cfg(unix)]
fn absolute_path(path: &CStr) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    let mut full = Vec::new();
    if !path.to_bytes().starts_with(b"/") {
        full.extend_from_slice(std::env::current_dir().ok()?.as_os_str().as_bytes());
        full.push(b'/');
    }
    full.extend_from_slice(path.to_bytes());
    let slashes = full.iter().take_while(|&&b| b == b'/').count();
    let mut out: Vec<u8> = if slashes == 2 {
        b"//".to_vec()
    } else {
        b"/".to_vec()
    };
    let root = out.len();
    for part in full.split(|&b| b == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                let cut = out[root..]
                    .iter()
                    .rposition(|&b| b == b'/')
                    .map_or(root, |i| root + i);
                out.truncate(cut);
            }
            _ => {
                if out.len() > root {
                    out.push(b'/');
                }
                out.extend_from_slice(part);
            }
        }
    }
    Some(out)
}

#[cfg(not(unix))]
fn absolute_path(path: &CStr) -> Option<Vec<u8>> {
    let abs = std::path::absolute(os_path(path)?).ok()?;
    let abs = abs.to_str()?.replace('\\', "/");
    Some(abs.into_bytes())
}

fn uri_path_byte_allowed(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!$&'()*+,-./:=@_~".contains(&b)
}

pub fn local_file_uri(path: &CStr) -> Option<CString> {
    let abs = absolute_path(path)?;
    let mut uri = b"file://".to_vec();
    if abs.first() != Some(&b'/') {
        uri.push(b'/');
    }
    for &b in &abs {
        if uri_path_byte_allowed(b) {
            uri.push(b);
        } else {
            uri.extend_from_slice(format!("%{b:02X}").as_bytes());
        }
    }
    owned_text(&uri)
}
