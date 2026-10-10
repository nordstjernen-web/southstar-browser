//! Southstar — proxies: parsing a proxy URL as curl accepts it, matching the no-proxy list, and the HTTP CONNECT and SOCKS 4, 4a, 5 and 5h handshakes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};

use crate::ffi::socket;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Http,
    Socks4,
    Socks4a,
    Socks5,
    Socks5h,
}

#[derive(Clone, Debug)]
pub struct Proxy {
    pub kind: Kind,
    pub host: String,
    pub port: u16,
    pub user: Option<Vec<u8>>,
    pub password: Option<Vec<u8>>,
}

fn percent_decode(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && i + 2 < s.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(s[i + 1]), hex(s[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

pub fn parse(spec: &[u8]) -> Option<Proxy> {
    let spec = spec.trim_ascii();
    if spec.is_empty() {
        return None;
    }
    let (kind, rest, default_port) = match spec.windows(3).position(|w| w == b"://") {
        Some(i) => {
            let scheme = spec[..i].to_ascii_lowercase();
            let kind = match scheme.as_slice() {
                b"http" => Kind::Http,
                b"socks4" => Kind::Socks4,
                b"socks4a" => Kind::Socks4a,
                b"socks5" => Kind::Socks5,
                b"socks5h" | b"socks" => Kind::Socks5h,
                _ => return None,
            };
            (
                kind,
                &spec[i + 3..],
                if kind == Kind::Http { 80 } else { 1080 },
            )
        }
        None => (Kind::Http, spec, 1080),
    };
    let rest = &rest[..rest.iter().position(|&c| c == b'/').unwrap_or(rest.len())];
    let (userinfo, hostport) = match rest.iter().rposition(|&c| c == b'@') {
        Some(at) => (Some(&rest[..at]), &rest[at + 1..]),
        None => (None, rest),
    };
    let (user, password) = match userinfo {
        Some(info) => match info.iter().position(|&c| c == b':') {
            Some(colon) => (
                Some(percent_decode(&info[..colon])),
                Some(percent_decode(&info[colon + 1..])),
            ),
            None => (Some(percent_decode(info)), None),
        },
        None => (None, None),
    };
    let (host, port) = if let Some(stripped) = hostport.strip_prefix(b"[") {
        let close = stripped.iter().position(|&c| c == b']')?;
        let after = &stripped[close + 1..];
        let port = match after.strip_prefix(b":") {
            Some(p) => core::str::from_utf8(p).ok()?.parse().ok()?,
            None => default_port,
        };
        (&stripped[..close], port)
    } else {
        match hostport.iter().rposition(|&c| c == b':') {
            Some(colon) => (
                &hostport[..colon],
                core::str::from_utf8(&hostport[colon + 1..])
                    .ok()?
                    .parse()
                    .ok()?,
            ),
            None => (hostport, default_port),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some(Proxy {
        kind,
        host: String::from_utf8_lossy(host).into_owned(),
        port,
        user,
        password,
    })
}

pub fn bypassed(no_proxy: &[u8], host: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    for entry in no_proxy.split(|&c| c == b',' || c == b' ') {
        let entry = entry.trim_ascii();
        if entry.is_empty() {
            continue;
        }
        if entry == b"*" {
            return true;
        }
        let entry = String::from_utf8_lossy(entry).to_ascii_lowercase();
        let entry = entry.trim_start_matches('.').trim_end_matches('.');
        let entry = match entry.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') && p.bytes().all(|b| b.is_ascii_digit()) => h,
            _ => entry,
        };
        let entry = entry.trim_start_matches('[').trim_end_matches(']');
        if let Some((net, bits)) = entry.split_once('/') {
            if let (Ok(net), Ok(bits), Ok(ip)) = (
                net.parse::<IpAddr>(),
                bits.parse::<u32>(),
                host.parse::<IpAddr>(),
            ) && in_cidr(ip, net, bits)
            {
                return true;
            }
            continue;
        }
        let lower = host.to_ascii_lowercase();
        if lower == entry
            || (lower.len() > entry.len()
                && lower.ends_with(entry)
                && lower.as_bytes()[lower.len() - entry.len() - 1] == b'.')
        {
            return true;
        }
    }
    false
}

fn in_cidr(ip: IpAddr, net: IpAddr, bits: u32) -> bool {
    match (ip, net) {
        (IpAddr::V4(a), IpAddr::V4(n)) if bits <= 32 => {
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            u32::from(a) & mask == u32::from(n) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(n)) if bits <= 128 => {
            let mask = if bits == 0 {
                0
            } else {
                u128::MAX << (128 - bits)
            };
            u128::from(a) & mask == u128::from(n) & mask
        }
        _ => false,
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63]);
        out.push(B64[(n >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63]
        } else {
            b'='
        });
    }
    out
}

impl Proxy {
    pub fn authorization(&self) -> Option<Vec<u8>> {
        let user = self.user.as_ref()?;
        let creds = [
            user.as_slice(),
            b":",
            self.password.as_deref().unwrap_or_default(),
        ]
        .concat();
        Some([&b"Basic "[..], &base64(&creds)].concat())
    }

    pub fn key(&self) -> String {
        format!("{:?}://{}:{}", self.kind, self.host, self.port)
    }
}

pub enum Failure {
    Io,
    Refused(String),
}

fn read_exact(
    stream: &mut TcpStream,
    buf: &mut [u8],
    abort: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    let mut got = 0;
    while got < buf.len() {
        match stream.read(&mut buf[got..]) {
            Ok(0) => return Err(Failure::Io),
            Ok(n) => got += n,
            Err(e) if socket::retryable(e.raw_os_error().unwrap_or(0)) => {
                if abort() {
                    return Err(Failure::Io);
                }
                socket::wait(socket::raw(stream), socket::POLLIN, 250);
            }
            Err(_) => return Err(Failure::Io),
        }
    }
    Ok(())
}

fn write_all(
    stream: &mut TcpStream,
    mut buf: &[u8],
    abort: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    while !buf.is_empty() {
        match stream.write(buf) {
            Ok(0) => return Err(Failure::Io),
            Ok(n) => buf = &buf[n..],
            Err(e) if socket::retryable(e.raw_os_error().unwrap_or(0)) => {
                if abort() {
                    return Err(Failure::Io);
                }
                socket::wait(socket::raw(stream), socket::POLLOUT, 250);
            }
            Err(_) => return Err(Failure::Io),
        }
    }
    Ok(())
}

pub fn connect_tunnel(
    proxy: &Proxy,
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    abort: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    let authority = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut head = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n").into_bytes();
    if let Some(auth) = proxy.authorization() {
        head.extend_from_slice(b"Proxy-Authorization: ");
        head.extend_from_slice(&auth);
        head.extend_from_slice(b"\r\n");
    }
    head.extend_from_slice(b"Proxy-Connection: Keep-Alive\r\n\r\n");
    write_all(stream, &head, abort)?;
    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    while !response.ends_with(b"\r\n\r\n") {
        read_exact(stream, &mut byte, abort)?;
        response.push(byte[0]);
        if response.len() > 64 * 1024 {
            return Err(Failure::Io);
        }
    }
    let line_end = response
        .iter()
        .position(|&c| c == b'\r')
        .unwrap_or(response.len());
    let status = crate::h1::status_code(&response[..line_end]).unwrap_or(0);
    if status / 100 != 2 {
        return Err(Failure::Refused(format!(
            "proxy CONNECT refused with status {status}"
        )));
    }
    Ok(())
}

fn resolve_v4(host: &str, port: u16) -> Option<[u8; 4]> {
    (host, port)
        .to_socket_addrs()
        .ok()?
        .find_map(|a| match a.ip() {
            IpAddr::V4(v4) => Some(v4.octets()),
            IpAddr::V6(_) => None,
        })
}

pub fn socks_handshake(
    proxy: &Proxy,
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    abort: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match proxy.kind {
        Kind::Socks4 | Kind::Socks4a => {
            let mut msg = vec![4, 1];
            msg.extend_from_slice(&port.to_be_bytes());
            let remote_name = proxy.kind == Kind::Socks4a && host.parse::<IpAddr>().is_err();
            if remote_name {
                msg.extend_from_slice(&[0, 0, 0, 1]);
            } else {
                let ip = resolve_v4(host, port)
                    .ok_or_else(|| Failure::Refused(format!("could not resolve host {host}")))?;
                msg.extend_from_slice(&ip);
            }
            msg.extend_from_slice(proxy.user.as_deref().unwrap_or_default());
            msg.push(0);
            if remote_name {
                msg.extend_from_slice(host.as_bytes());
                msg.push(0);
            }
            write_all(stream, &msg, abort)?;
            let mut reply = [0u8; 8];
            read_exact(stream, &mut reply, abort)?;
            if reply[1] != 0x5a {
                return Err(Failure::Refused(format!(
                    "SOCKS4 proxy refused the connection ({})",
                    reply[1]
                )));
            }
            Ok(())
        }
        Kind::Socks5 | Kind::Socks5h => {
            let creds = proxy.user.is_some();
            let greeting: &[u8] = if creds { &[5, 2, 0, 2] } else { &[5, 1, 0] };
            write_all(stream, greeting, abort)?;
            let mut choice = [0u8; 2];
            read_exact(stream, &mut choice, abort)?;
            match choice[1] {
                0 => {}
                2 if creds => {
                    let user = proxy.user.as_deref().unwrap_or_default();
                    let pass = proxy.password.as_deref().unwrap_or_default();
                    if user.len() > 255 || pass.len() > 255 {
                        return Err(Failure::Refused(String::from(
                            "SOCKS5 credentials too long",
                        )));
                    }
                    let mut msg = vec![1, user.len() as u8];
                    msg.extend_from_slice(user);
                    msg.push(pass.len() as u8);
                    msg.extend_from_slice(pass);
                    write_all(stream, &msg, abort)?;
                    let mut status = [0u8; 2];
                    read_exact(stream, &mut status, abort)?;
                    if status[1] != 0 {
                        return Err(Failure::Refused(String::from(
                            "SOCKS5 proxy rejected the credentials",
                        )));
                    }
                }
                _ => {
                    return Err(Failure::Refused(String::from(
                        "SOCKS5 proxy offered no usable authentication",
                    )));
                }
            }
            let mut msg = vec![5, 1, 0];
            match host.parse::<IpAddr>() {
                Ok(IpAddr::V4(v4)) => {
                    msg.push(1);
                    msg.extend_from_slice(&v4.octets());
                }
                Ok(IpAddr::V6(v6)) => {
                    msg.push(4);
                    msg.extend_from_slice(&v6.octets());
                }
                Err(_) if proxy.kind == Kind::Socks5h => {
                    if host.len() > 255 {
                        return Err(Failure::Refused(String::from(
                            "host name too long for SOCKS5",
                        )));
                    }
                    msg.push(3);
                    msg.push(host.len() as u8);
                    msg.extend_from_slice(host.as_bytes());
                }
                Err(_) => {
                    let addr = (host, port)
                        .to_socket_addrs()
                        .ok()
                        .and_then(|mut a| a.next())
                        .ok_or_else(|| {
                            Failure::Refused(format!("could not resolve host {host}"))
                        })?;
                    match addr.ip() {
                        IpAddr::V4(v4) => {
                            msg.push(1);
                            msg.extend_from_slice(&v4.octets());
                        }
                        IpAddr::V6(v6) => {
                            msg.push(4);
                            msg.extend_from_slice(&v6.octets());
                        }
                    }
                }
            }
            msg.extend_from_slice(&port.to_be_bytes());
            write_all(stream, &msg, abort)?;
            let mut head = [0u8; 4];
            read_exact(stream, &mut head, abort)?;
            if head[1] != 0 {
                return Err(Failure::Refused(format!(
                    "SOCKS5 proxy refused the connection ({})",
                    head[1]
                )));
            }
            let rest = match head[3] {
                1 => 4 + 2,
                4 => 16 + 2,
                3 => {
                    let mut len = [0u8; 1];
                    read_exact(stream, &mut len, abort)?;
                    len[0] as usize + 2
                }
                _ => return Err(Failure::Io),
            };
            let mut skip = vec![0u8; rest];
            read_exact(stream, &mut skip, abort)
        }
        Kind::Http => Ok(()),
    }
}
