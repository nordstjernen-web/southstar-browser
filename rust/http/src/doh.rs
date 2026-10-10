//! Southstar — DNS over HTTPS (RFC 8484): A and AAAA queries in DNS wire format sent to the configured resolver over this client, answers cached for their TTL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::client::{Request, perform};
use crate::fetch::parse_target;
use crate::ffi::tls::Settings;
use crate::transfer::Handler;

const TYPE_A: u16 = 1;
const TYPE_AAAA: u16 = 28;
const MIN_TTL: u32 = 30;
const MAX_TTL: u32 = 3600;
const MAX_ANSWER: usize = 64 * 1024;

struct Resolver {
    url: Vec<u8>,
    tls: Settings,
    cache: HashMap<String, (Vec<IpAddr>, Instant)>,
}

static RESOLVER: Mutex<Option<Resolver>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Resolver>> {
    RESOLVER.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn configure(url: Option<&[u8]>, tls: Settings) {
    let url = url.filter(|u| u.len() > 8 && u[..8].eq_ignore_ascii_case(b"https://"));
    let mut slot = lock();
    match url {
        Some(url) if slot.as_ref().is_none_or(|r| r.url != url) => {
            *slot = Some(Resolver {
                url: url.to_vec(),
                tls,
                cache: HashMap::new(),
            });
        }
        Some(_) => {}
        None => *slot = None,
    }
}

pub fn query(name: &str, kind: u16) -> Option<Vec<u8>> {
    let mut out = vec![0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    Some(out)
}

fn skip_name(msg: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = *msg.get(pos)?;
        if len & 0xc0 == 0xc0 {
            return Some(pos + 2);
        }
        if len == 0 {
            return Some(pos + 1);
        }
        pos += 1 + len as usize;
    }
}

fn read_u16(msg: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*msg.get(pos)?, *msg.get(pos + 1)?]))
}

pub fn answers(msg: &[u8]) -> Option<(Vec<IpAddr>, u32)> {
    if msg.len() < 12 || msg[3] & 0x0f != 0 {
        return None;
    }
    let questions = read_u16(msg, 4)?;
    let count = read_u16(msg, 6)?;
    let mut pos = 12;
    for _ in 0..questions {
        pos = skip_name(msg, pos)? + 4;
    }
    let mut addrs = Vec::new();
    let mut ttl = MAX_TTL;
    for _ in 0..count {
        pos = skip_name(msg, pos)?;
        let kind = read_u16(msg, pos)?;
        let record_ttl = u32::from_be_bytes(msg.get(pos + 4..pos + 8)?.try_into().ok()?);
        let len = read_u16(msg, pos + 8)? as usize;
        let data = msg.get(pos + 10..pos + 10 + len)?;
        pos += 10 + len;
        let addr = match (kind, len) {
            (TYPE_A, 4) => IpAddr::V4(Ipv4Addr::new(data[0], data[1], data[2], data[3])),
            (TYPE_AAAA, 16) => {
                let octets: [u8; 16] = data.try_into().ok()?;
                IpAddr::V6(Ipv6Addr::from(octets))
            }
            _ => continue,
        };
        ttl = ttl.min(record_ttl);
        addrs.push(addr);
    }
    Some((addrs, ttl.clamp(MIN_TTL, MAX_TTL)))
}

fn base64url(input: &[u8]) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63]);
        out.push(ALPHABET[(n >> 12) as usize & 63]);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63]);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63]);
        }
    }
    out
}

struct Collect<'a> {
    body: Vec<u8>,
    abort: &'a dyn Fn() -> bool,
}

impl Handler for Collect<'_> {
    fn should_abort(&self) -> bool {
        (self.abort)()
    }

    fn status_line(&mut self, _line: &[u8]) {}

    fn header(&mut self, _line: &[u8], _name: &[u8], _value: &[u8]) {}

    fn body(&mut self, data: &[u8]) -> bool {
        if self.body.len() + data.len() > MAX_ANSWER {
            return false;
        }
        self.body.extend_from_slice(data);
        true
    }
}

fn ask(
    server: &[u8],
    tls: &Settings,
    name: &str,
    kind: u16,
    abort: &dyn Fn() -> bool,
) -> Option<(Vec<IpAddr>, u32)> {
    let message = query(name, kind)?;
    let separator: &[u8] = if server.contains(&b'?') { b"&" } else { b"?" };
    let url = [server, separator, b"dns=", &base64url(&message)].concat();
    let target = parse_target(&url)?;
    let request = Request {
        url: &url,
        https: target.https,
        host: &target.host,
        port: target.port,
        authority: &target.authority,
        path: &target.path,
        method: b"GET",
        user_agent: None,
        referer: None,
        cookie: None,
        extra_headers: vec![b"Accept: application/dns-message"],
        body: b"",
        timeout: Duration::from_secs(10),
        connect_timeout: Duration::from_secs(10),
        allow_insecure: false,
        tls: Settings {
            ca_bundle: tls.ca_bundle.clone(),
            curves: tls.curves.clone(),
        },
        proxy: None,
    };
    let mut collect = Collect {
        body: Vec::new(),
        abort,
    };
    let outcome = perform(&request, &mut collect);
    if !outcome.ok || outcome.status != 200 {
        return None;
    }
    answers(&collect.body)
}

pub enum Lookup {
    System,
    Found(Vec<IpAddr>),
    Failed,
}

pub fn resolve(host: &str, abort: &dyn Fn() -> bool) -> Lookup {
    if host.parse::<IpAddr>().is_ok() || host.eq_ignore_ascii_case("localhost") {
        return Lookup::System;
    }
    let (server, tls) = {
        let mut slot = lock();
        let Some(resolver) = slot.as_mut() else {
            return Lookup::System;
        };
        let server_host = parse_target(&resolver.url).map(|t| t.host);
        if server_host
            .as_deref()
            .is_some_and(|h| h.eq_ignore_ascii_case(host))
        {
            return Lookup::System;
        }
        let key = host.to_ascii_lowercase();
        if let Some((addrs, expires)) = resolver.cache.get(&key) {
            if *expires > Instant::now() {
                return Lookup::Found(addrs.clone());
            }
        }
        (
            resolver.url.clone(),
            Settings {
                ca_bundle: resolver.tls.ca_bundle.clone(),
                curves: resolver.tls.curves.clone(),
            },
        )
    };
    let mut addrs = Vec::new();
    let mut ttl = MAX_TTL;
    for kind in [TYPE_A, TYPE_AAAA] {
        if let Some((found, t)) = ask(&server, &tls, host, kind, abort) {
            addrs.extend(found);
            ttl = ttl.min(t);
        }
    }
    if addrs.is_empty() {
        return Lookup::Failed;
    }
    if let Some(resolver) = lock().as_mut().filter(|r| r.url == server) {
        resolver.cache.insert(
            host.to_ascii_lowercase(),
            (
                addrs.clone(),
                Instant::now() + Duration::from_secs(u64::from(ttl)),
            ),
        );
    }
    Lookup::Found(addrs)
}
