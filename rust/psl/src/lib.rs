//! Southstar PSL — the Public Suffix List algorithm over the list the system or the bundle provides, read at first use: whether a domain is a public suffix and the registrable domain of a host.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::env;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::OnceLock;

const FILE_NAME: &str = "public_suffix_list.dat";

const SYSTEM_DIRS: [&str; 4] = [
    "/usr/share/publicsuffix",
    "/usr/local/share/publicsuffix",
    "/usr/pkg/share/publicsuffix",
    "/opt/homebrew/share/publicsuffix",
];

#[derive(Default)]
struct Rules {
    plain: HashSet<Vec<u8>>,
    wildcard: HashSet<Vec<u8>>,
    exception: HashSet<Vec<u8>>,
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(file) = env::var_os("NS_PSL_FILE").filter(|f| !f.is_empty()) {
        paths.push(PathBuf::from(file));
    }
    if let Some(exe_dir) = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
    {
        for relative in [
            "share/southstar",
            "../share/southstar",
            "../Resources/share/southstar",
            "../share/publicsuffix",
        ] {
            paths.push(exe_dir.join(relative).join(FILE_NAME));
        }
    }
    paths.extend(
        SYSTEM_DIRS
            .iter()
            .map(|dir| PathBuf::from(dir).join(FILE_NAME)),
    );
    paths
}

fn insert(set: &mut HashSet<Vec<u8>>, rule: &str) {
    set.insert(rule.as_bytes().to_vec());
    if !rule.is_ascii()
        && let Some(ascii) = southstar_url::domain_to_ascii(rule)
    {
        set.insert(ascii.into_bytes());
    }
}

fn parse(text: &str) -> Rules {
    let mut rules = Rules::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let rule = line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if let Some(rest) = rule.strip_prefix('!') {
            insert(&mut rules.exception, rest);
        } else if let Some(rest) = rule.strip_prefix("*.") {
            insert(&mut rules.wildcard, rest);
        } else if !rule.contains('*') {
            insert(&mut rules.plain, &rule);
        }
    }
    rules
}

fn rules() -> Option<&'static Rules> {
    static RULES: OnceLock<Option<Rules>> = OnceLock::new();
    RULES
        .get_or_init(|| {
            candidates()
                .iter()
                .find_map(|path| std::fs::read_to_string(path).ok())
                .map(|text| parse(&text))
                .filter(|rules| !rules.plain.is_empty())
        })
        .as_ref()
}

fn parent(domain: &[u8]) -> Option<&[u8]> {
    let dot = domain.iter().position(|&b| b == b'.')?;
    Some(&domain[dot + 1..])
}

fn is_ip_literal(host: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(host) else {
        return false;
    };
    let bracketed = text
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'));
    text.parse::<Ipv4Addr>().is_ok()
        || text.parse::<Ipv6Addr>().is_ok()
        || bracketed.is_some_and(|inner| inner.parse::<Ipv6Addr>().is_ok())
}

fn normalized(domain: &[u8]) -> Option<&[u8]> {
    let domain = domain.strip_suffix(b".").unwrap_or(domain);
    let empty_label = domain.is_empty()
        || domain.starts_with(b".")
        || domain.windows(2).any(|pair| pair == b"..");
    (!empty_label).then_some(domain)
}

fn suffix_rule_matches(rules: &Rules, domain: &[u8]) -> bool {
    if rules.exception.contains(domain) {
        return false;
    }
    rules.plain.contains(domain)
        || parent(domain).is_some_and(|rest| rules.wildcard.contains(rest))
        || !domain.contains(&b'.')
}

pub fn is_public_suffix(domain: &[u8]) -> bool {
    let Some(domain) = normalized(domain) else {
        return false;
    };
    rules().is_none_or(|rules| suffix_rule_matches(rules, domain))
}

pub fn registrable_domain(host: &[u8]) -> Option<&[u8]> {
    let host = normalized(host)?;
    let rules = rules()?;
    if is_ip_literal(host) {
        return None;
    }
    let mut candidate = host;
    let mut registrable = None;
    loop {
        if rules.exception.contains(candidate) {
            return Some(candidate);
        }
        if suffix_rule_matches(rules, candidate) {
            return registrable;
        }
        registrable = Some(candidate);
        candidate = parent(candidate)?;
    }
}
