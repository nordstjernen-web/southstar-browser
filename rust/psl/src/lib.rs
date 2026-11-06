//! Southstar PSL — the Public Suffix List algorithm: whether a domain is a public suffix and the registrable domain (eTLD+1) of a host.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod table;

#[cfg(test)]
mod tests;

use std::net::{Ipv4Addr, Ipv6Addr};

fn listed(rules: &[&str], name: &[u8]) -> bool {
    rules
        .binary_search_by(|rule| rule.as_bytes().cmp(name))
        .is_ok()
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

fn suffix_rule_matches(domain: &[u8]) -> bool {
    if listed(&table::EXCEPTIONS, domain) {
        return false;
    }
    listed(&table::RULES, domain)
        || parent(domain).is_some_and(|rest| listed(&table::WILDCARDS, rest))
        || !domain.contains(&b'.')
}

pub fn is_public_suffix(domain: &[u8]) -> bool {
    normalized(domain).is_some_and(suffix_rule_matches)
}

pub fn registrable_domain(host: &[u8]) -> Option<&[u8]> {
    let host = normalized(host)?;
    if is_ip_literal(host) {
        return None;
    }
    let mut candidate = host;
    let mut registrable = None;
    loop {
        if listed(&table::EXCEPTIONS, candidate) {
            return Some(candidate);
        }
        if suffix_rule_matches(candidate) {
            return registrable;
        }
        registrable = Some(candidate);
        candidate = parent(candidate)?;
    }
}
