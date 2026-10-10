//! Southstar — Content-Security-Policy parser and checks (CSP1 and CSP2 subset).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_glib::{G_CHECKSUM_SHA256, G_CHECKSUM_SHA384, G_CHECKSUM_SHA512};

mod ffi;

pub const DEFAULT: usize = 0;
pub const SCRIPT: usize = 1;
pub const STYLE: usize = 2;
pub const IMG: usize = 3;
pub const MEDIA: usize = 4;
pub const CONNECT: usize = 5;
pub const FONT: usize = 6;
pub const FRAME: usize = 7;
pub const CHILD: usize = 8;
pub const WORKER: usize = 9;
pub const FRAME_ANCESTORS: usize = 10;
pub const OBJECT: usize = 11;
pub const BASE_URI: usize = 12;
pub const FORM_ACTION: usize = 13;
pub const KIND_COUNT: usize = 14;

const DIRECTIVES: [(&[u8], usize); KIND_COUNT] = [
    (b"default-src", DEFAULT),
    (b"script-src", SCRIPT),
    (b"style-src", STYLE),
    (b"img-src", IMG),
    (b"media-src", MEDIA),
    (b"connect-src", CONNECT),
    (b"font-src", FONT),
    (b"frame-src", FRAME),
    (b"child-src", CHILD),
    (b"worker-src", WORKER),
    (b"frame-ancestors", FRAME_ANCESTORS),
    (b"object-src", OBJECT),
    (b"base-uri", BASE_URI),
    (b"form-action", FORM_ACTION),
];

const SCHEME_SOURCES: [&[u8]; 6] = [b"https:", b"http:", b"wss:", b"ws:", b"data:", b"blob:"];
const NETWORK_SCHEMES: [&[u8]; 5] = [b"http:", b"https:", b"ws:", b"wss:", b"ftp:"];
const STRICT_DYNAMIC: &[u8] = b"'strict-dynamic'";
const UNSAFE_INLINE: &[u8] = b"'unsafe-inline'";
const NONCE_PREFIX: &[u8] = b"'nonce-";

pub(crate) struct UrlParts {
    pub protocol: Vec<u8>,
    pub hostname: Option<Vec<u8>>,
    pub port: Vec<u8>,
    pub pathname: Vec<u8>,
}

#[derive(Default)]
struct Policy {
    directives: [Option<Vec<Vec<u8>>>; KIND_COUNT],
}

pub struct Csp {
    policies: Vec<Policy>,
}

fn directive_kind(name: &[u8]) -> Option<usize> {
    DIRECTIVES
        .iter()
        .find(|(directive, _)| name.eq_ignore_ascii_case(directive))
        .map(|&(_, kind)| kind)
}

fn starts_with_ignore_case(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn url_scheme_matches(url: &[u8], scheme_with_colon: &[u8]) -> bool {
    let Some(&last) = scheme_with_colon.last() else {
        return false;
    };
    let scheme = if last == b':' {
        &scheme_with_colon[..scheme_with_colon.len() - 1]
    } else {
        scheme_with_colon
    };
    starts_with_ignore_case(url, scheme) && url.get(scheme.len()) == Some(&b':')
}

fn scheme_part_matches(scheme: &[u8], url: &[u8]) -> bool {
    if starts_with_ignore_case(url, scheme) && url.get(scheme.len()) == Some(&b':') {
        return true;
    }
    if scheme.eq_ignore_ascii_case(b"http") {
        return starts_with_ignore_case(url, b"https:");
    }
    if scheme.eq_ignore_ascii_case(b"ws") {
        return starts_with_ignore_case(url, b"wss:");
    }
    false
}

fn is_network_scheme_url(url: &[u8]) -> bool {
    NETWORK_SCHEMES
        .iter()
        .any(|scheme| url_scheme_matches(url, scheme))
}

fn default_port_for_scheme(scheme: &[u8]) -> &'static [u8] {
    let name = scheme.strip_suffix(b":").unwrap_or(scheme);
    if name.eq_ignore_ascii_case(b"https") || name.eq_ignore_ascii_case(b"wss") {
        b"443"
    } else if name.eq_ignore_ascii_case(b"http") || name.eq_ignore_ascii_case(b"ws") {
        b"80"
    } else if name.eq_ignore_ascii_case(b"ftp") {
        b"21"
    } else {
        b""
    }
}

fn port_matches(source_port: Option<&[u8]>, resource_port: &[u8], resource_scheme: &[u8]) -> bool {
    match source_port {
        Some(b"*") => true,
        None | Some(b"") => {
            if resource_port.is_empty() {
                return true;
            }
            let default = default_port_for_scheme(resource_scheme);
            !default.is_empty() && resource_port == default
        }
        Some(source_port) => {
            let port = if resource_port.is_empty() {
                default_port_for_scheme(resource_scheme)
            } else {
                resource_port
            };
            !port.is_empty() && source_port == port
        }
    }
}

fn path_matches(source_path: Option<&[u8]>, resource_path: &[u8]) -> bool {
    let Some(source_path) = source_path.filter(|path| !path.is_empty() && *path != b"/") else {
        return true;
    };
    let path = if resource_path.is_empty() {
        b"/"
    } else {
        resource_path
    };
    if source_path.ends_with(b"/") {
        path.starts_with(source_path)
    } else {
        path == source_path
    }
}

fn host_matches(source_host: &[u8], hostname: &[u8]) -> bool {
    if source_host.starts_with(b"*.") {
        let suffix = &source_host[1..];
        hostname.len() > suffix.len()
            && hostname[hostname.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    } else {
        hostname.eq_ignore_ascii_case(source_host)
    }
}

fn document_scheme_allows(
    resource_scheme: &[u8],
    document_url: Option<&CStr>,
    resource: &[u8],
) -> bool {
    let document_scheme = document_url
        .and_then(ffi::url_parts)
        .map(|parts| parts.protocol)
        .filter(|scheme| !scheme.is_empty());
    match document_scheme {
        Some(scheme) => {
            resource_scheme.eq_ignore_ascii_case(&scheme)
                || (scheme.eq_ignore_ascii_case(b"http:")
                    && resource_scheme.eq_ignore_ascii_case(b"https:"))
                || (scheme.eq_ignore_ascii_case(b"ws:")
                    && resource_scheme.eq_ignore_ascii_case(b"wss:"))
        }
        None => is_network_scheme_url(resource),
    }
}

fn source_matches(source: &[u8], resource_url: &CStr, document_url: Option<&CStr>) -> bool {
    let resource = resource_url.to_bytes();
    match source {
        b"" | b"'none'" | b"'unsafe-inline'" | b"'unsafe-eval'" | b"'strict-dynamic'" => {
            return false;
        }
        b"*" => return is_network_scheme_url(resource),
        b"'self'" => return ffi::same_origin(resource_url, document_url),
        _ => {}
    }
    if SCHEME_SOURCES
        .iter()
        .any(|scheme| source.eq_ignore_ascii_case(scheme))
    {
        return url_scheme_matches(resource, source);
    }

    let scheme_end = source.windows(3).position(|w| w == b"://");
    let host_source = match scheme_end {
        Some(end) => {
            if !scheme_part_matches(&source[..end], resource) {
                return false;
            }
            &source[end + 3..]
        }
        None => source,
    };

    let Some(parts) = ffi::url_parts(resource_url) else {
        return false;
    };
    let Some(hostname) = parts.hostname else {
        return false;
    };
    if scheme_end.is_none() && !document_scheme_allows(&parts.protocol, document_url, resource) {
        return false;
    }

    let port_at = host_source.iter().position(|&c| c == b':');
    let path_at = host_source.iter().position(|&c| c == b'/');
    let port_at = port_at.filter(|&port| path_at.is_none_or(|path| port < path));
    let host_end = port_at.or(path_at).unwrap_or(host_source.len());
    if !host_matches(&host_source[..host_end], &hostname) {
        return false;
    }

    let source_port =
        port_at.map(|port| &host_source[port + 1..path_at.unwrap_or(host_source.len())]);
    if !port_matches(source_port, &parts.port, &parts.protocol) {
        return false;
    }
    path_matches(path_at.map(|path| &host_source[path..]), &parts.pathname)
}

fn has_token(list: &[Vec<u8>], token: &[u8]) -> bool {
    list.iter().any(|source| source == token)
}

fn has_nonce_or_hash(list: &[Vec<u8>]) -> bool {
    list.iter().any(|source| {
        source.starts_with(NONCE_PREFIX)
            || source.starts_with(b"'sha256-")
            || source.starts_with(b"'sha384-")
            || source.starts_with(b"'sha512-")
    })
}

fn nonce_token_matches(source: &[u8], nonce: &[u8]) -> bool {
    source.len() > NONCE_PREFIX.len() + 1
        && source.ends_with(b"'")
        && &source[NONCE_PREFIX.len()..source.len() - 1] == nonce
}

fn hash_token_matches(source: &[u8], body: &[u8]) -> bool {
    let checksum_type = if source.starts_with(b"'sha256-") {
        G_CHECKSUM_SHA256
    } else if source.starts_with(b"'sha384-") {
        G_CHECKSUM_SHA384
    } else if source.starts_with(b"'sha512-") {
        G_CHECKSUM_SHA512
    } else {
        return false;
    };
    let Some(want) = source[8..].strip_suffix(b"'") else {
        return false;
    };
    if want.is_empty() {
        return false;
    }
    let got = ffi::base64_digest(checksum_type, body);
    let got_url_safe: Vec<u8> = got
        .iter()
        .map(|&c| match c {
            b'+' => b'-',
            b'/' => b'_',
            other => other,
        })
        .collect();
    want == got.as_slice() || want == got_url_safe.as_slice()
}

fn inline_fallback_allows(list: &[Vec<u8>]) -> bool {
    !has_token(list, STRICT_DYNAMIC) && !has_nonce_or_hash(list) && has_token(list, UNSAFE_INLINE)
}

impl Policy {
    fn parse(text: &[u8]) -> Policy {
        let mut policy = Policy::default();
        for clause in text.split(|&c| c == b';').map(<[u8]>::trim_ascii) {
            if clause.is_empty() {
                continue;
            }
            let mut tokens = clause.split(|&c| c == b' ' || c == b'\t');
            let Some(kind) = tokens.next().and_then(directive_kind) else {
                continue;
            };
            if policy.directives[kind].is_some() {
                continue;
            }
            policy.directives[kind] = Some(
                tokens
                    .map(<[u8]>::trim_ascii)
                    .filter(|token| !token.is_empty())
                    .map(<[u8]>::to_vec)
                    .collect(),
            );
        }
        policy
    }

    fn is_set(&self, kind: usize) -> bool {
        self.directives[kind].is_some()
    }

    fn effective_kind(&self, kind: usize) -> Option<usize> {
        if self.is_set(kind) {
            Some(kind)
        } else if kind == BASE_URI || kind == FORM_ACTION {
            None
        } else if kind == WORKER && self.is_set(CHILD) {
            Some(CHILD)
        } else if kind == WORKER && self.is_set(SCRIPT) {
            Some(SCRIPT)
        } else if kind == FRAME && self.is_set(CHILD) {
            Some(CHILD)
        } else if self.is_set(DEFAULT) {
            Some(DEFAULT)
        } else {
            None
        }
    }

    fn frame_ancestors_allow(&self, parent_url: &CStr, document_url: Option<&CStr>) -> bool {
        match self.directives[FRAME_ANCESTORS].as_deref() {
            None => true,
            Some(list) => list
                .iter()
                .any(|source| source_matches(source, parent_url, document_url)),
        }
    }

    fn allows(
        &self,
        kind: usize,
        resource_url: &CStr,
        document_url: Option<&CStr>,
        nonce: Option<&[u8]>,
        parser_inserted: bool,
    ) -> bool {
        if kind == FRAME_ANCESTORS {
            return self.frame_ancestors_allow(resource_url, document_url);
        }
        let Some(effective) = self.effective_kind(kind) else {
            return true;
        };
        let list = match self.directives[effective].as_deref() {
            Some(list) if !list.is_empty() => list,
            _ => return false,
        };
        let script_like = kind == SCRIPT || (kind == DEFAULT && effective == DEFAULT);
        let strict_dynamic = script_like && has_token(list, STRICT_DYNAMIC);
        for source in list {
            if source == STRICT_DYNAMIC {
                continue;
            }
            if let Some(nonce) = nonce.filter(|_| source.starts_with(NONCE_PREFIX)) {
                if nonce_token_matches(source, nonce) {
                    return true;
                }
                continue;
            }
            if strict_dynamic {
                continue;
            }
            if source_matches(source, resource_url, document_url) {
                return true;
            }
        }
        strict_dynamic && !parser_inserted
    }

    fn inline_script_sources(&self) -> Option<&[Vec<u8>]> {
        self.directives[SCRIPT]
            .as_deref()
            .or(self.directives[DEFAULT].as_deref())
    }

    fn inline_script_allowed(&self, body: &[u8], nonce: Option<&[u8]>) -> bool {
        let Some(list) = self.inline_script_sources() else {
            return true;
        };
        if let Some(nonce) = nonce.filter(|nonce| !nonce.is_empty())
            && has_token(list, &[NONCE_PREFIX, nonce, b"'"].concat())
        {
            return true;
        }
        if !body.is_empty() && list.iter().any(|source| hash_token_matches(source, body)) {
            return true;
        }
        inline_fallback_allows(list)
    }

    fn inline_event_handler_allowed(&self) -> bool {
        self.inline_script_sources()
            .is_none_or(inline_fallback_allows)
    }
}

impl Csp {
    pub fn parse(header: &[u8]) -> Option<Csp> {
        let policies: Vec<Policy> = header
            .split(|&c| c == b',')
            .map(<[u8]>::trim_ascii)
            .filter(|text| !text.is_empty())
            .map(Policy::parse)
            .collect();
        (!policies.is_empty()).then_some(Csp { policies })
    }

    pub fn merge(&mut self, src: &mut Csp) {
        self.policies.append(&mut src.policies);
    }

    pub fn allows(
        &self,
        kind: usize,
        resource_url: &CStr,
        document_url: Option<&CStr>,
        nonce: Option<&[u8]>,
        parser_inserted: bool,
    ) -> bool {
        kind >= KIND_COUNT
            || self.policies.iter().all(|policy| {
                policy.allows(kind, resource_url, document_url, nonce, parser_inserted)
            })
    }

    pub fn inline_script_allowed(&self, body: &[u8], nonce: Option<&[u8]>) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.inline_script_allowed(body, nonce))
    }

    pub fn inline_event_handler_allowed(&self) -> bool {
        self.policies
            .iter()
            .all(Policy::inline_event_handler_allowed)
    }
}
