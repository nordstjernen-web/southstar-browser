//! Southstar — the embedder's resource policy: whether this process may reach the network at all, which hosts, private addresses and local files.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::RwLock;

pub const REFUSED: &str = "blocked by the embedder's resource policy";

const PRIVATE_NAME_SUFFIXES: [&str; 11] = [
    ".localhost",
    ".local",
    ".localdomain",
    ".internal",
    ".home.arpa",
    ".lan",
    ".intranet",
    ".corp",
    ".home",
    ".private",
    ".onion",
];

#[derive(Clone, Default)]
pub struct Policy {
    pub offline: bool,
    pub block_files: bool,
    pub block_private: bool,
    pub allow_hosts: Option<Vec<String>>,
    pub deny_hosts: Vec<String>,
}

static POLICY: RwLock<Policy> = RwLock::new(Policy {
    offline: false,
    block_files: false,
    block_private: false,
    allow_hosts: None,
    deny_hosts: Vec::new(),
});

pub fn host_list(text: &[u8]) -> Vec<String> {
    text.split(|&b| matches!(b, b',' | b' ' | b'\t' | b'\r' | b'\n'))
        .filter(|entry| !entry.is_empty())
        .map(|entry| String::from_utf8_lossy(entry).to_ascii_lowercase())
        .collect()
}

pub fn set(policy: Policy) {
    if let Ok(mut current) = POLICY.write() {
        *current = policy;
    }
}

fn current() -> Policy {
    POLICY.read().map(|p| p.clone()).unwrap_or_default()
}

fn bare_host(host: &str) -> String {
    let host = host.trim_end_matches('.');
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let host = host.split('%').next().unwrap_or(host);
    host.to_ascii_lowercase()
}

fn matches(patterns: &[String], host: &str) -> bool {
    patterns.iter().any(|pattern| {
        pattern == "*"
            || pattern == host
            || pattern
                .strip_prefix('*')
                .filter(|suffix| suffix.starts_with('.'))
                .is_some_and(|suffix| host.len() > suffix.len() && host.ends_with(suffix))
    })
}

fn v4_is_public(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && ip.octets()[2] == 0)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 240)
}

fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let joined = |high: u16, low: u16| Ipv4Addr::from((u32::from(high) << 16) | u32::from(low));
    match ip.segments() {
        [0x64, 0xff9b, 0, 0, 0, 0, high, low] | [0x64, 0xff9b, 1, _, _, _, high, low] => {
            Some(joined(high, low))
        }
        [0x2002, high, low, ..] => Some(joined(high, low)),
        [0x2001, 0, _, _, _, _, high, low] => Some(joined(!high, !low)),
        _ => None,
    }
}

fn v6_is_public(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_is_public(v4);
    }
    if let Some(v4) = embedded_v4(ip) {
        return v4_is_public(v4);
    }
    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first & 0xffc0) == 0xfec0
        || ip.segments()[..6] == [0, 0, 0, 0, 0, 0])
}

pub fn address_is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_is_public(v4),
        IpAddr::V6(v6) => v6_is_public(v6),
    }
}

fn host_is_private(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return !address_is_public(ip);
    }
    host == "localhost"
        || !host.contains('.')
        || PRIVATE_NAME_SUFFIXES
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

pub fn allows_host(host: &str) -> bool {
    let policy = current();
    if policy.offline {
        return false;
    }
    let host = bare_host(host);
    if host.is_empty() {
        return policy.allow_hosts.is_none();
    }
    if matches(&policy.deny_hosts, &host) {
        return false;
    }
    if let Some(allowed) = &policy.allow_hosts
        && !matches(allowed, &host)
    {
        return false;
    }
    !policy.block_private || !host_is_private(&host)
}

pub fn allows_address(ip: IpAddr) -> bool {
    !current().block_private || address_is_public(ip)
}

pub fn allows_files() -> bool {
    !current().block_files
}
