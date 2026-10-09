//! Southstar — FTP downloads and directory listings: login, binary mode, a CWD per path segment, EPSV with a PASV fallback, and RETR or LIST over the data connection, directly or through a proxy.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::client::{self, Outcome, Stream};
use crate::proxy::{self, Kind, Proxy};
use crate::transfer::Handler;

pub struct Location {
    pub host: String,
    pub port: u16,
    pub user: Vec<u8>,
    pub password: Vec<u8>,
    pub dirs: Vec<Vec<u8>>,
    pub file: Option<Vec<u8>>,
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

pub fn parse(url: &[u8]) -> Option<Location> {
    let rest = url
        .get(..6)
        .filter(|s| s.eq_ignore_ascii_case(b"ftp://"))
        .map(|_| &url[6..])?;
    let rest = &rest[..rest
        .iter()
        .position(|&c| c == b'#' || c == b'?')
        .unwrap_or(rest.len())];
    let slash = rest.iter().position(|&c| c == b'/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(slash);
    let (userinfo, hostport) = match authority.iter().rposition(|&c| c == b'@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    let (user, password) = match userinfo {
        Some(info) => match info.iter().position(|&c| c == b':') {
            Some(c) => (percent_decode(&info[..c]), percent_decode(&info[c + 1..])),
            None => (percent_decode(info), Vec::new()),
        },
        None => (b"anonymous".to_vec(), b"ftp@example.com".to_vec()),
    };
    let (host, port) = if hostport.first() == Some(&b'[') {
        let close = hostport.iter().position(|&c| c == b']')?;
        let port = match hostport[close + 1..].strip_prefix(b":") {
            Some(p) if !p.is_empty() => core::str::from_utf8(p).ok()?.parse().ok()?,
            _ => 21,
        };
        (&hostport[..=close], port)
    } else {
        match hostport.iter().rposition(|&c| c == b':') {
            Some(c) if c + 1 < hostport.len() => (
                &hostport[..c],
                core::str::from_utf8(&hostport[c + 1..])
                    .ok()?
                    .parse()
                    .ok()?,
            ),
            Some(c) => (&hostport[..c], 21),
            None => (hostport, 21),
        }
    };
    if host.is_empty() {
        return None;
    }
    let path = path.strip_prefix(b"/").unwrap_or(path);
    let path = match path.iter().rposition(|&c| c == b';') {
        Some(semi) if path[semi..].starts_with(b";type=") => &path[..semi],
        _ => path,
    };
    let mut segments: Vec<Vec<u8>> = path.split(|&c| c == b'/').map(percent_decode).collect();
    let file = segments.pop().filter(|f| !f.is_empty());
    let dirs = segments.into_iter().filter(|d| !d.is_empty()).collect();
    Some(Location {
        host: String::from_utf8_lossy(host).to_ascii_lowercase(),
        port,
        user,
        password,
        dirs,
        file,
    })
}

struct Control<'a> {
    stream: Stream,
    buf: Vec<u8>,
    pos: usize,
    abort: &'a dyn Fn() -> bool,
}

impl Control<'_> {
    fn reply(&mut self) -> Option<(u32, Vec<u8>)> {
        let (start, end) =
            client::read_line(&mut self.stream, &mut self.buf, &mut self.pos, self.abort)?;
        let first = self.buf[start..end].to_vec();
        if first.len() < 3 || !first[..3].iter().all(u8::is_ascii_digit) {
            return None;
        }
        let code = core::str::from_utf8(&first[..3]).ok()?.parse().ok()?;
        let mut text = first.clone();
        if first.get(3) == Some(&b'-') {
            loop {
                let (s, e) =
                    client::read_line(&mut self.stream, &mut self.buf, &mut self.pos, self.abort)?;
                let line = self.buf[s..e].to_vec();
                text.extend_from_slice(b"\n");
                text.extend_from_slice(&line);
                if line.len() >= 4 && line[..3] == first[..3] && line[3] == b' ' {
                    break;
                }
            }
        }
        if self.pos > 64 * 1024 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        Some((code, text))
    }

    fn command(&mut self, line: &[u8]) -> Option<(u32, Vec<u8>)> {
        let mut out = line.to_vec();
        out.extend_from_slice(b"\r\n");
        if !self.stream.write_all(&out, self.abort) {
            return None;
        }
        self.reply()
    }
}

fn connect(
    host: &str,
    port: u16,
    proxy: Option<&Proxy>,
    deadline: Instant,
    abort: &dyn Fn() -> bool,
) -> Result<TcpStream, String> {
    let Some(p) = proxy else {
        return client::tcp_connect_to(host, port, deadline, abort).map(|(s, _)| s);
    };
    let (mut stream, _) = client::tcp_connect_to(&p.host, p.port, deadline, abort)
        .map_err(|e| format!("proxy {}:{}: {e}", p.host, p.port))?;
    let target = client::resolve_host(host);
    let result = match p.kind {
        Kind::Http => proxy::connect_tunnel(p, &mut stream, target, port, abort),
        _ => proxy::socks_handshake(p, &mut stream, target, port, abort),
    };
    match result {
        Ok(()) => Ok(stream),
        Err(proxy::Failure::Refused(m)) => Err(m),
        Err(proxy::Failure::Io) => Err(format!("proxy {}:{} handshake failed", p.host, p.port)),
    }
}

fn passive_port(control: &mut Control) -> Option<u16> {
    if let Some((229, text)) = control.command(b"EPSV") {
        let open = text.iter().position(|&c| c == b'(')?;
        let inner = &text[open + 1..];
        let delim = *inner.first()?;
        let fields: Vec<&[u8]> = inner.split(|&c| c == delim).collect();
        return core::str::from_utf8(fields.get(3)?).ok()?.parse().ok();
    }
    let (code, text) = control.command(b"PASV")?;
    if code != 227 {
        return None;
    }
    let start = text.iter().position(|c| c.is_ascii_digit())?;
    let numbers: Vec<u32> = text[start..]
        .split(|c| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .take(6)
        .filter_map(|s| core::str::from_utf8(s).ok()?.parse().ok())
        .collect();
    if numbers.len() < 6 {
        return None;
    }
    Some((numbers[4] * 256 + numbers[5]) as u16)
}

fn fail(out: &mut Outcome, code: u32, message: String) {
    out.status = i64::from(code);
    out.error = Some(message);
}

pub fn get(
    url: &[u8],
    proxy: Option<&Proxy>,
    connect_timeout: Duration,
    timeout: Duration,
    handler: &mut dyn Handler,
) -> Outcome {
    let handler = RefCell::new(handler);
    let should_abort = || handler.borrow().should_abort();
    let start = Instant::now();
    let mut out = client::failed(String::new());
    out.error = None;
    let Some(loc) = parse(url) else {
        out.connect_failed = true;
        out.error = Some(String::from("invalid FTP URL"));
        return out;
    };
    let deadline = start
        .checked_add(timeout)
        .unwrap_or_else(|| start + Duration::from_secs(3600));
    let connect_deadline = (start + connect_timeout).min(deadline);
    let ms = || start.elapsed().as_secs_f64() * 1000.0;
    let stream = {
        let abort = || should_abort() || Instant::now() > deadline;
        connect(&loc.host, loc.port, proxy, connect_deadline, &abort)
    };
    out.namelookup_ms = ms();
    let stream = match stream {
        Ok(s) => s,
        Err(message) => {
            if should_abort() {
                out.cancelled = true;
            } else {
                out.connect_failed = true;
                out.error = Some(message);
            }
            return out;
        }
    };
    out.connect_ms = ms();
    out.connects = 1;
    if proxy.is_none() {
        out.remote_ip = stream.peer_addr().ok().map(|a| a.ip().to_string());
    }
    let peer = stream.peer_addr().ok();
    let abort = || should_abort() || Instant::now() > deadline;
    let mut control = Control {
        stream: Stream::Plain(stream),
        buf: Vec::new(),
        pos: 0,
        abort: &abort,
    };
    let data = (|| -> Result<(TcpStream, u32), (u32, String)> {
        let lost = || (0, String::from("FTP control connection lost"));
        match control.reply().ok_or_else(lost)? {
            (code, _) if code / 100 == 2 => {}
            (code, text) => return Err((code, String::from_utf8_lossy(&text).into_owned())),
        }
        let (mut code, mut text) = control
            .command(&[&b"USER "[..], &loc.user].concat())
            .ok_or_else(lost)?;
        if code == 331 {
            (code, text) = control
                .command(&[&b"PASS "[..], &loc.password].concat())
                .ok_or_else(lost)?;
        }
        if code / 100 != 2 {
            return Err((
                code,
                format!("FTP login denied: {}", String::from_utf8_lossy(&text)),
            ));
        }
        let (code, text) = control.command(b"TYPE I").ok_or_else(lost)?;
        if code / 100 != 2 {
            return Err((code, String::from_utf8_lossy(&text).into_owned()));
        }
        for dir in &loc.dirs {
            let (code, text) = control
                .command(&[&b"CWD "[..], dir].concat())
                .ok_or_else(lost)?;
            if code / 100 != 2 {
                return Err((
                    code,
                    format!(
                        "FTP directory not found: {}",
                        String::from_utf8_lossy(&text)
                    ),
                ));
            }
        }
        let port = passive_port(&mut control)
            .ok_or_else(|| (0, String::from("FTP server refused passive mode")))?;
        let data_host = match (proxy, peer) {
            (None, Some(addr)) => addr.ip().to_string(),
            _ => loc.host.clone(),
        };
        let data = connect(
            &data_host,
            port,
            proxy,
            connect_deadline.max(Instant::now() + connect_timeout),
            &abort,
        )
        .map_err(|e| (0, e))?;
        let command = match &loc.file {
            Some(file) => [&b"RETR "[..], file].concat(),
            None => b"LIST".to_vec(),
        };
        let (code, text) = control.command(&command).ok_or_else(lost)?;
        if code != 125 && code != 150 {
            let what = if loc.file.is_some() {
                "file"
            } else {
                "directory"
            };
            return Err((
                code,
                format!(
                    "FTP {what} not available: {}",
                    String::from_utf8_lossy(&text)
                ),
            ));
        }
        Ok((data, code))
    })();
    let (data, _) = match data {
        Ok(d) => d,
        Err((code, message)) => {
            if should_abort() {
                out.cancelled = true;
            } else {
                fail(&mut out, code, message);
            }
            return out;
        }
    };
    out.pretransfer_ms = ms();
    let mut data = Stream::Plain(data);
    let mut buf = vec![0u8; 65_536];
    let mut full = false;
    loop {
        let n = {
            let abort = || should_abort() || Instant::now() > deadline;
            data.read_some(&mut buf, &abort)
        };
        match n {
            Some(0) => break,
            Some(n) => {
                if out.starttransfer_ms == 0.0 {
                    out.starttransfer_ms = ms();
                }
                if !handler.borrow_mut().body(&buf[..n]) {
                    full = true;
                    break;
                }
            }
            None => {
                if should_abort() {
                    out.cancelled = true;
                    return out;
                }
                fail(&mut out, 0, String::from("FTP data connection failed"));
                return out;
            }
        }
    }
    drop(data);
    out.total_ms = ms();
    if full {
        out.sink_full = true;
        return out;
    }
    match control.reply() {
        Some((code, _)) if code / 100 == 2 => {
            out.status = i64::from(code);
            out.ok = true;
            let _ = control.command(b"QUIT");
        }
        Some((code, text)) => fail(&mut out, code, String::from_utf8_lossy(&text).into_owned()),
        None => fail(&mut out, 0, String::from("FTP transfer did not complete")),
    }
    out.total_ms = ms();
    out
}
