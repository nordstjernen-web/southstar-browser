//! Southstar — extension request rules: Adblock filter lists, declarativeNetRequest matching and match patterns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;

use crate::ffi::{self, Regex};

const UNSUPPORTED_OPTIONS: [&[u8]; 23] = [
    b"redirect",
    b"redirect-rule",
    b"removeparam",
    b"csp",
    b"replace",
    b"rewrite",
    b"removeheader",
    b"cookie",
    b"header",
    b"empty",
    b"mp4",
    b"popup",
    b"popunder",
    b"webrtc",
    b"inline-script",
    b"inline-font",
    b"genericblock",
    b"generichide",
    b"specifichide",
    b"elemhide",
    b"ehide",
    b"document",
    b"doc",
];

const RESOURCE_TYPES: [&[u8]; 5] = [b"script", b"stylesheet", b"image", b"font", b"media"];

const EXTENSION_TYPES: [(&[u8], &[u8]); 30] = [
    (b"js", b"script"),
    (b"mjs", b"script"),
    (b"css", b"stylesheet"),
    (b"png", b"image"),
    (b"jpg", b"image"),
    (b"jpeg", b"image"),
    (b"gif", b"image"),
    (b"webp", b"image"),
    (b"svg", b"image"),
    (b"ico", b"image"),
    (b"bmp", b"image"),
    (b"avif", b"image"),
    (b"apng", b"image"),
    (b"woff", b"font"),
    (b"woff2", b"font"),
    (b"ttf", b"font"),
    (b"otf", b"font"),
    (b"eot", b"font"),
    (b"mp4", b"media"),
    (b"webm", b"media"),
    (b"mp3", b"media"),
    (b"ogg", b"media"),
    (b"oga", b"media"),
    (b"ogv", b"media"),
    (b"wav", b"media"),
    (b"m4a", b"media"),
    (b"m4v", b"media"),
    (b"mpg", b"media"),
    (b"mpeg", b"media"),
    (b"mov", b"media"),
];

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Party {
    Any,
    Third,
    First,
}

pub(crate) struct Rule {
    pub(crate) priority: i32,
    pub(crate) allow: bool,
    pub(crate) party: Party,
    pub(crate) url_filter: Option<Vec<u8>>,
    pub(crate) regex: Option<Regex>,
    pub(crate) case_sensitive: bool,
    pub(crate) request_domains: Vec<Vec<u8>>,
    pub(crate) excluded_request_domains: Vec<Vec<u8>>,
    pub(crate) initiator_domains: Vec<Vec<u8>>,
    pub(crate) excluded_initiator_domains: Vec<Vec<u8>>,
    pub(crate) resource_types: Vec<Vec<u8>>,
    pub(crate) excluded_resource_types: Vec<Vec<u8>>,
}

#[derive(Default)]
pub(crate) struct CosmeticRule {
    domains: Vec<Vec<u8>>,
    excluded_domains: Vec<Vec<u8>>,
    selector: Vec<u8>,
}

pub(crate) struct Url<'a> {
    pub(crate) scheme: Vec<u8>,
    pub(crate) host: Vec<u8>,
    pub(crate) path: &'a [u8],
}

impl Rule {
    pub(crate) fn new(allow: bool) -> Rule {
        Rule {
            priority: 1,
            allow,
            party: Party::Any,
            url_filter: None,
            regex: None,
            case_sensitive: false,
            request_domains: Vec::new(),
            excluded_request_domains: Vec::new(),
            initiator_domains: Vec::new(),
            excluded_initiator_domains: Vec::new(),
            resource_types: Vec::new(),
            excluded_resource_types: Vec::new(),
        }
    }

    pub(crate) fn matches(
        &self,
        url: &[u8],
        host: &[u8],
        initiator: Option<&[u8]>,
        party: i32,
    ) -> bool {
        match self.party {
            Party::Third if party != 1 => return false,
            Party::First if party != 0 => return false,
            _ => {}
        }
        if !self.request_domains.is_empty() && !domain_list_match(&self.request_domains, Some(host))
        {
            return false;
        }
        if domain_list_match(&self.excluded_request_domains, Some(host)) {
            return false;
        }
        if !self.initiator_domains.is_empty()
            && !domain_list_match(&self.initiator_domains, initiator)
        {
            return false;
        }
        if domain_list_match(&self.excluded_initiator_domains, initiator) {
            return false;
        }
        if let Some(filter) = &self.url_filter {
            if !url_filter_match(filter, url, self.case_sensitive) {
                return false;
            }
        }
        if let Some(regex) = &self.regex {
            if !regex.is_match(url) {
                return false;
            }
        }
        if !self.resource_types.is_empty() || !self.excluded_resource_types.is_empty() {
            let kind = infer_type(url);
            if !self.resource_types.is_empty() && !type_in_list(&self.resource_types, kind) {
                return false;
            }
            if type_in_list(&self.excluded_resource_types, kind) {
                return false;
            }
        }
        true
    }
}

impl CosmeticRule {
    pub(crate) fn applies_to(&self, host: &[u8]) -> bool {
        !domain_list_match(&self.excluded_domains, Some(host))
            && (self.domains.is_empty() || domain_list_match(&self.domains, Some(host)))
    }

    pub(crate) fn selector(&self) -> &[u8] {
        &self.selector
    }
}

pub(crate) fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(crate) fn split_url(url: &[u8]) -> Option<Url<'_>> {
    let separator = find(url, b"://")?;
    let rest = &url[separator + 3..];
    let host_end = rest
        .iter()
        .position(|&c| matches!(c, b'/' | b'?' | b'#'))
        .unwrap_or(rest.len());
    let path = if rest.get(host_end) == Some(&b'/') {
        let from = &rest[host_end..];
        let end = from
            .iter()
            .position(|&c| c == b'?' || c == b'#')
            .unwrap_or(from.len());
        &from[..end]
    } else {
        b"/"
    };
    Some(Url {
        scheme: url[..separator].to_ascii_lowercase(),
        host: rest[..host_end].to_ascii_lowercase(),
        path,
    })
}

pub(crate) fn strip_port(host: &[u8]) -> &[u8] {
    host.iter()
        .position(|&c| c == b':')
        .map_or(host, |colon| &host[..colon])
}

fn wildcard(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut s) = (0, 0);
    while p < pattern.len() {
        if pattern[p] == b'*' {
            p += 1;
            return p == pattern.len()
                || (s..text.len()).any(|at| wildcard(&pattern[p..], &text[at..]));
        }
        if text.get(s) != Some(&pattern[p]) {
            return false;
        }
        p += 1;
        s += 1;
    }
    s == text.len()
}

fn generic_scheme(scheme: &[u8]) -> bool {
    matches!(scheme, b"http" | b"https" | b"ws" | b"wss")
}

fn host_match(pattern: &[u8], host: &[u8]) -> bool {
    if pattern == b"*" {
        return true;
    }
    match pattern.strip_prefix(b"*.") {
        Some(suffix) => domain_match(suffix, host),
        None => pattern == host,
    }
}

pub(crate) fn pattern_match(pattern: &[u8], url: &[u8]) -> bool {
    let Some(target) = split_url(url) else {
        return false;
    };
    if pattern == b"<all_urls>" {
        return generic_scheme(&target.scheme)
            || matches!(&target.scheme[..], b"ftp" | b"file" | b"data");
    }
    let Some(wanted) = split_url(pattern) else {
        return false;
    };
    let scheme_ok = if wanted.scheme == b"*" {
        generic_scheme(&target.scheme)
    } else {
        wanted.scheme == target.scheme
    };
    scheme_ok && host_match(&wanted.host, &target.host) && wildcard(wanted.path, target.path)
}

fn is_separator(c: u8) -> bool {
    !(c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'%'))
}

fn match_here(pattern: &[u8], text: &[u8], end_anchor: bool) -> bool {
    let (mut p, mut s) = (0, 0);
    while p < pattern.len() {
        match pattern[p] {
            b'*' => {
                p += 1;
                if p == pattern.len() {
                    return true;
                }
                loop {
                    if match_here(&pattern[p..], &text[s..], end_anchor) {
                        return true;
                    }
                    if s == text.len() {
                        return false;
                    }
                    s += 1;
                }
            }
            b'^' => {
                if s == text.len() {
                    p += 1;
                } else if is_separator(text[s]) {
                    p += 1;
                    s += 1;
                } else {
                    return false;
                }
            }
            c => {
                if text.get(s) != Some(&c) {
                    return false;
                }
                p += 1;
                s += 1;
            }
        }
    }
    !end_anchor || s == text.len()
}

fn url_filter_match(filter: &[u8], url: &[u8], case_sensitive: bool) -> bool {
    if filter.is_empty() {
        return true;
    }
    let (url, filter) = if case_sensitive {
        (url.to_vec(), filter.to_vec())
    } else {
        (url.to_ascii_lowercase(), filter.to_ascii_lowercase())
    };
    let (domain_anchor, start_anchor, mut body) = if let Some(rest) = filter.strip_prefix(b"||") {
        (true, false, rest)
    } else if let Some(rest) = filter.strip_prefix(b"|") {
        (false, true, rest)
    } else {
        (false, false, &filter[..])
    };
    let end_anchor = body.last() == Some(&b'|');
    if end_anchor {
        body = &body[..body.len() - 1];
    }
    if domain_anchor {
        let host_start = find(&url, b"://").map_or(0, |at| at + 3);
        let host_end = host_start
            + url[host_start..]
                .iter()
                .position(|&c| matches!(c, b'/' | b'?' | b'#'))
                .unwrap_or(url.len() - host_start);
        return (host_start..=host_end).any(|at| {
            (at == host_start || url[at - 1] == b'.') && match_here(body, &url[at..], end_anchor)
        });
    }
    if start_anchor {
        return match_here(body, &url, end_anchor);
    }
    (0..=url.len()).any(|at| match_here(body, &url[at..], end_anchor))
}

fn domain_match(domain: &[u8], host: &[u8]) -> bool {
    domain == host
        || (host.len() > domain.len()
            && host[host.len() - domain.len() - 1] == b'.'
            && host.ends_with(domain))
}

pub(crate) fn domain_list_match(list: &[Vec<u8>], host: Option<&[u8]>) -> bool {
    host.is_some_and(|host| list.iter().any(|domain| domain_match(domain, host)))
}

fn infer_type(url: &[u8]) -> Option<&'static [u8]> {
    let path_end = url
        .iter()
        .position(|&c| c == b'?' || c == b'#')
        .unwrap_or(url.len());
    let mut dot = None;
    for (i, &c) in url[..path_end].iter().enumerate() {
        if c == b'.' {
            dot = Some(i);
        } else if c == b'/' {
            dot = None;
        }
    }
    let extension = url[dot? + 1..path_end].to_ascii_lowercase();
    EXTENSION_TYPES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|&(_, kind)| kind)
}

fn type_in_list(list: &[Vec<u8>], kind: Option<&[u8]>) -> bool {
    kind.is_some_and(|kind| list.iter().any(|entry| entry == kind))
}

pub(crate) fn third_party(request_host: &[u8], initiator_host: Option<&[u8]>) -> i32 {
    let Some(initiator_host) = initiator_host else {
        return -1;
    };
    let request = strip_port(request_host);
    let initiator = strip_port(initiator_host);
    if request.is_empty() || initiator.is_empty() {
        return -1;
    }
    if request == initiator {
        return 0;
    }
    match (
        ffi::registrable_domain(request),
        ffi::registrable_domain(initiator),
    ) {
        (Some(a), Some(b)) if a == b => 0,
        _ => 1,
    }
}

pub(crate) fn host_indexed(hosts: &HashSet<Vec<u8>>, host: &[u8]) -> bool {
    if hosts.is_empty() {
        return false;
    }
    let mut rest = strip_port(host);
    while !rest.is_empty() {
        if hosts.contains(rest) {
            return true;
        }
        match rest.iter().position(|&c| c == b'.') {
            Some(dot) => rest = &rest[dot + 1..],
            None => break,
        }
    }
    false
}

fn split_domains(
    spec: &[u8],
    separator: u8,
    included: &mut Vec<Vec<u8>>,
    excluded: &mut Vec<Vec<u8>>,
) {
    if spec.is_empty() {
        return;
    }
    for part in spec.split(|&c| c == separator) {
        let domain = part.trim_ascii();
        if domain.is_empty() {
            continue;
        }
        match domain.strip_prefix(b"~") {
            Some(b"") => {}
            Some(rest) => excluded.push(rest.to_ascii_lowercase()),
            None => included.push(domain.to_ascii_lowercase()),
        }
    }
}

fn is_simple_host(pattern: &[u8]) -> bool {
    pattern.len() > 3
        && pattern.starts_with(b"||")
        && pattern.ends_with(b"^")
        && pattern[2..pattern.len() - 1]
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
}

fn parse_network(line: &[u8], rules: &mut Vec<Rule>, hosts: &mut HashSet<Vec<u8>>) {
    let (allow, rest) = match line.strip_prefix(b"@@") {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (pattern, options) = if rest.first() == Some(&b'/') {
        match rest.iter().rposition(|&c| c == b'/') {
            Some(close) if close != 0 => (
                &rest[..=close],
                (rest.get(close + 1) == Some(&b'$')).then(|| &rest[close + 2..]),
            ),
            _ => (rest, None),
        }
    } else {
        match rest.iter().position(|&c| c == b'$') {
            Some(dollar) => (&rest[..dollar], Some(&rest[dollar + 1..])),
            None => (rest, None),
        }
    };
    if pattern.is_empty() {
        return;
    }
    if !allow && options.is_none() && is_simple_host(pattern) {
        hosts.insert(pattern[2..pattern.len() - 1].to_ascii_lowercase());
        return;
    }
    let mut rule = Rule::new(allow);
    if pattern[0] == b'/' && pattern.len() > 1 && pattern.ends_with(b"/") {
        match Regex::new(&pattern[1..pattern.len() - 1], false) {
            Some(regex) => rule.regex = Some(regex),
            None => return,
        }
    } else {
        rule.url_filter = Some(pattern.to_vec());
    }
    for option in options
        .into_iter()
        .flat_map(|options| options.split(|&c| c == b','))
    {
        let option = option.trim_ascii();
        let (negated, option) = match option.strip_prefix(b"~") {
            Some(rest) => (true, rest),
            None => (false, option),
        };
        if let Some(spec) = option.strip_prefix(b"domain=") {
            split_domains(
                spec,
                b'|',
                &mut rule.initiator_domains,
                &mut rule.excluded_initiator_domains,
            );
        } else if option == b"third-party" {
            rule.party = if negated { Party::First } else { Party::Third };
        } else if option == b"match-case" {
            rule.case_sensitive = true;
        } else if RESOURCE_TYPES.contains(&option) {
            if negated {
                rule.excluded_resource_types.push(option.to_vec());
            } else {
                rule.resource_types.push(option.to_vec());
            }
        } else if UNSUPPORTED_OPTIONS.contains(&option) {
            return;
        }
    }
    rules.push(rule);
}

fn parse_cosmetic(line: &[u8], at: usize, cosmetic: &mut Vec<CosmeticRule>) {
    let mut selector = &line[at + 2..];
    while let Some(rest) = selector.strip_prefix(b" ") {
        selector = rest;
    }
    if selector.is_empty() || selector.contains(&b'{') {
        return;
    }
    if selector.starts_with(b"+js") || selector.starts_with(b"script:") {
        return;
    }
    let mut rule = CosmeticRule::default();
    split_domains(
        &line[..at],
        b',',
        &mut rule.domains,
        &mut rule.excluded_domains,
    );
    rule.selector = selector.trim_ascii().to_vec();
    cosmetic.push(rule);
}

pub(crate) fn parse_filter_list(
    text: &[u8],
    rules: &mut Vec<Rule>,
    cosmetic: &mut Vec<CosmeticRule>,
    hosts: &mut HashSet<Vec<u8>>,
) {
    for line in text.split(|&c| c == b'\n') {
        let line = line.trim_ascii();
        if line.is_empty() || line[0] == b'!' || line[0] == b'[' {
            continue;
        }
        if [b"#@#", b"#?#", b"#$#", b"#%#"]
            .iter()
            .any(|marker| find(line, *marker).is_some())
        {
            continue;
        }
        match find(line, b"##") {
            Some(at) => parse_cosmetic(line, at, cosmetic),
            None => parse_network(line, rules, hosts),
        }
    }
}
