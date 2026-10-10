//! Southstar — performing one HTTP request: connecting with TLS and ALPN, HTTP/1.1 over its own connection, and HTTP/2 multiplexed over pooled connections that each run an I/O thread.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::doh::{self, Lookup};
use crate::ffi::socket::{self, ConnectError, PollFd, Wake};
use crate::ffi::tls::{self, Handshake, Io, Settings, Tls};
use crate::frame;
use crate::h1;
use crate::h2::{Connection, Event};
use crate::proxy::{self, Kind, Proxy};
use crate::transfer::{Handler, Transfer};

const MAX_CONCURRENT: usize = 64;
const MAX_REUSE: u32 = 1000;
const MAX_IDLE: Duration = Duration::from_secs(60);
const POOL_MAX_PER_ORIGIN: usize = 8;
const RESERVED: [&[u8]; 11] = [
    b"host",
    b"connection",
    b"keep-alive",
    b"proxy-connection",
    b"transfer-encoding",
    b"upgrade",
    b"accept-encoding",
    b"cookie",
    b"user-agent",
    b"referer",
    b"content-length",
];

pub struct Request<'a> {
    pub url: &'a [u8],
    pub https: bool,
    pub host: &'a str,
    pub port: u16,
    pub authority: &'a [u8],
    pub path: &'a [u8],
    pub method: &'a [u8],
    pub user_agent: Option<&'a [u8]>,
    pub referer: Option<&'a [u8]>,
    pub cookie: Option<Vec<u8>>,
    pub extra_headers: Vec<&'a [u8]>,
    pub body: &'a [u8],
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub allow_insecure: bool,
    pub tls: Settings,
    pub proxy: Option<Proxy>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Http11,
    Http2,
}

pub struct Outcome {
    pub status: i64,
    pub version: Version,
    pub connects: u32,
    pub remote_ip: Option<String>,
    pub namelookup_ms: f64,
    pub connect_ms: f64,
    pub appconnect_ms: f64,
    pub pretransfer_ms: f64,
    pub starttransfer_ms: f64,
    pub total_ms: f64,
    pub ok: bool,
    pub cancelled: bool,
    pub sink_full: bool,
    pub connect_failed: bool,
    pub tls_verify_failed: bool,
    pub tls_warning: Option<String>,
    pub error: Option<String>,
}

pub(crate) fn failed(error: String) -> Outcome {
    let mut out = Outcome::new();
    out.error = Some(error);
    out
}

impl Outcome {
    fn new() -> Outcome {
        Outcome {
            status: 0,
            version: Version::Http11,
            connects: 0,
            remote_ip: None,
            namelookup_ms: 0.0,
            connect_ms: 0.0,
            appconnect_ms: 0.0,
            pretransfer_ms: 0.0,
            starttransfer_ms: 0.0,
            total_ms: 0.0,
            ok: false,
            cancelled: false,
            sink_full: false,
            connect_failed: false,
            tls_verify_failed: false,
            tls_warning: None,
            error: None,
        }
    }
}

fn is_reserved(name: &[u8]) -> bool {
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name))
}

fn request_method<'a>(req: &Request<'a>) -> &'a [u8] {
    if req.method.is_empty() {
        return b"GET";
    }
    if req.method.iter().any(|&b| b == b'\r' || b == b'\n') {
        return if req.body.is_empty() { b"GET" } else { b"POST" };
    }
    req.method
}

fn common_fields<'a>(req: &'a Request<'a>) -> Vec<(&'a [u8], &'a [u8])> {
    let mut fields: Vec<(&[u8], &[u8])> = Vec::new();
    if let Some(ua) = req.user_agent {
        fields.push((b"user-agent", ua));
    }
    fields.push((b"accept-encoding", crate::decode::accept_encoding()));
    if let Some(r) = req.referer.filter(|r| !r.is_empty()) {
        fields.push((b"referer", r));
    }
    if let Some(c) = &req.cookie {
        fields.push((b"cookie", c));
    }
    fields
}

fn h2_fields(req: &Request) -> Vec<(Vec<u8>, Vec<u8>)> {
    let scheme: &[u8] = if req.https { b"https" } else { b"http" };
    let mut fields = vec![
        (b":method".to_vec(), request_method(req).to_vec()),
        (b":scheme".to_vec(), scheme.to_vec()),
        (b":authority".to_vec(), req.authority.to_vec()),
        (b":path".to_vec(), req.path.to_vec()),
    ];
    for (n, v) in common_fields(req) {
        fields.push((n.to_vec(), v.to_vec()));
    }
    for line in &req.extra_headers {
        let Some((name, value)) = h1::split_header(line) else {
            continue;
        };
        if is_reserved(name) {
            continue;
        }
        fields.push((name.to_ascii_lowercase(), value.to_vec()));
    }
    fields
}

fn h1_head(req: &Request) -> Vec<u8> {
    let titled: Vec<(&[u8], &[u8])> = common_fields(req)
        .into_iter()
        .map(|(n, v)| {
            let name: &[u8] = match n {
                b"user-agent" => b"User-Agent",
                b"accept-encoding" => b"Accept-Encoding",
                b"referer" => b"Referer",
                _ => b"Cookie",
            };
            (name, v)
        })
        .collect();
    let mut extra: Vec<&[u8]> = req
        .extra_headers
        .iter()
        .copied()
        .filter(|line| h1::split_header(line).is_some_and(|(n, _)| !is_reserved(n)))
        .collect();
    let forward = forward_proxy(req);
    let auth = forward
        .and_then(Proxy::authorization)
        .map(|a| [&b"Proxy-Authorization: "[..], &a].concat());
    if let Some(a) = &auth {
        extra.push(a);
    }
    h1::request_head(
        request_method(req),
        if forward.is_some() { req.url } else { req.path },
        req.authority,
        &titled,
        &extra,
        req.body.len(),
        false,
    )
}

fn forward_proxy<'a>(req: &'a Request) -> Option<&'a Proxy> {
    req.proxy
        .as_ref()
        .filter(|p| p.kind == Kind::Http && !req.https)
}

fn origin_key(req: &Request) -> String {
    let scheme = if req.https { "https" } else { "http" };
    match &req.proxy {
        Some(p) => format!("{scheme}://{}:{} via {}", req.host, req.port, p.key()),
        None => format!("{scheme}://{}:{}", req.host, req.port),
    }
}

pub(crate) fn resolve_host(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
}

pub enum Stream {
    Plain(TcpStream),
    Tls(Tls, TcpStream),
}

impl Stream {
    fn raw(&self) -> socket::Raw {
        match self {
            Stream::Plain(s) | Stream::Tls(_, s) => socket::raw(s),
        }
    }

    pub(crate) fn read_some(&mut self, buf: &mut [u8], abort: &dyn Fn() -> bool) -> Option<usize> {
        loop {
            match self {
                Stream::Plain(s) => match s.read(buf) {
                    Ok(n) => return Some(n),
                    Err(e) if socket::retryable(e.raw_os_error().unwrap_or(0)) => {}
                    Err(_) => return None,
                },
                Stream::Tls(t, _) => match t.read(buf) {
                    Io::Done(n) => return Some(n),
                    Io::Closed => return Some(0),
                    Io::WouldBlock => {}
                    Io::Failed => return None,
                },
            }
            if abort() {
                return None;
            }
            socket::wait(self.raw(), socket::POLLIN, 250);
        }
    }

    pub(crate) fn write_all(&mut self, mut buf: &[u8], abort: &dyn Fn() -> bool) -> bool {
        while !buf.is_empty() {
            let n = match self {
                Stream::Plain(s) => match s.write(buf) {
                    Ok(0) => return false,
                    Ok(n) => n,
                    Err(e) if socket::retryable(e.raw_os_error().unwrap_or(0)) => 0,
                    Err(_) => return false,
                },
                Stream::Tls(t, _) => match t.write(buf) {
                    Io::Done(n) => n,
                    Io::WouldBlock => 0,
                    _ => return false,
                },
            };
            if n == 0 {
                if abort() {
                    return false;
                }
                socket::wait(self.raw(), socket::POLLOUT, 250);
            }
            buf = &buf[n..];
        }
        true
    }
}

struct Connected {
    stream: Stream,
    remote: SocketAddr,
    h2: bool,
}

fn tcp_connect(
    req: &Request,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> Result<(TcpStream, SocketAddr), String> {
    let (mut stream, addr) = match &req.proxy {
        Some(p) => tcp_connect_to(&p.host, p.port, deadline, cancelled)
            .map_err(|e| format!("proxy {}:{}: {e}", p.host, p.port))?,
        None => tcp_connect_to(req.host, req.port, deadline, cancelled)?,
    };
    let Some(p) = &req.proxy else {
        return Ok((stream, addr));
    };
    let abort = || cancelled() || Instant::now() > deadline;
    let host = resolve_host(req.host);
    let result = match p.kind {
        Kind::Http if req.https => proxy::connect_tunnel(p, &mut stream, host, req.port, &abort),
        Kind::Http => Ok(()),
        _ => proxy::socks_handshake(p, &mut stream, host, req.port, &abort),
    };
    match result {
        Ok(()) => Ok((stream, addr)),
        Err(proxy::Failure::Refused(message)) => Err(message),
        Err(proxy::Failure::Io) => Err(format!("proxy {}:{} handshake failed", p.host, p.port)),
    }
}

pub(crate) fn tcp_connect_to(
    host_name: &str,
    port: u16,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> Result<(TcpStream, SocketAddr), String> {
    if !socket::init() {
        return Err(String::from("socket initialization failed"));
    }
    let host = resolve_host(host_name);
    let addrs: Vec<SocketAddr> = match doh::resolve(host, cancelled) {
        Lookup::Found(ips) => ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, port))
            .collect(),
        Lookup::Failed => {
            return Err(format!(
                "could not resolve host {host_name} over DNS over HTTPS"
            ));
        }
        Lookup::System => match (host, port).to_socket_addrs() {
            Ok(a) => a.collect(),
            Err(_) => return Err(format!("could not resolve host {host_name}")),
        },
    };
    if addrs.is_empty() {
        return Err(format!("could not resolve host {host_name}"));
    }
    let remaining = || {
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as i64
    };
    for addr in &addrs {
        if cancelled() {
            break;
        }
        match socket::connect_addr(addr, &remaining, cancelled) {
            Ok(s) => return Ok((s, *addr)),
            Err(ConnectError::Cancelled | ConnectError::TimedOut) => break,
            Err(ConnectError::Failed) => {}
        }
    }
    Err(format!("could not connect to {host_name}:{port}"))
}

fn open(
    req: &Request,
    t: &Transfer,
    connect_deadline: Instant,
    out: &mut Outcome,
    allow_h2: bool,
) -> Option<Connected> {
    let cancelled = || t.handler.should_abort();
    let connected = tcp_connect(req, connect_deadline, &cancelled);
    out.namelookup_ms = t.ms_since_start();
    let (tcp, remote) = match connected {
        Ok(c) => c,
        Err(message) => {
            if cancelled() {
                out.cancelled = true;
            } else {
                out.connect_failed = true;
                out.error = Some(message);
            }
            return None;
        }
    };
    out.connect_ms = t.ms_since_start();
    out.remote_ip = Some(remote.ip().to_string());
    if !req.https {
        return Some(Connected {
            stream: Stream::Plain(tcp),
            remote,
            h2: false,
        });
    }
    let abort = || t.should_abort();
    let mut tcp = tcp;
    let mut verify = true;
    loop {
        match Tls::handshake(
            socket::raw(&tcp),
            req.host,
            req.port,
            verify,
            allow_h2,
            &req.tls,
            &abort,
        ) {
            Ok(session) => {
                out.appconnect_ms = t.ms_since_start();
                let h2 = session.alpn_h2();
                return Some(Connected {
                    stream: Stream::Tls(session, tcp),
                    remote,
                    h2,
                });
            }
            Err(Handshake::Cancelled) => {
                out.cancelled = true;
                return None;
            }
            Err(Handshake::Failed { verify_result }) => {
                if t.handler.should_abort() {
                    out.cancelled = true;
                    return None;
                }
                let verify_failed = verify_result != tls::X509_V_OK;
                if verify_failed && verify && req.allow_insecure {
                    out.tls_warning = Some(format!(
                        "Insecure: TLS certificate not trusted ({})",
                        tls::verify_error_text(verify_result)
                    ));
                    match tcp_connect(req, connect_deadline, &cancelled) {
                        Ok((s, _)) => tcp = s,
                        Err(_) => {
                            out.connect_failed = true;
                            out.error = Some(String::from("reconnect failed"));
                            return None;
                        }
                    }
                    verify = false;
                    continue;
                }
                out.tls_verify_failed = verify_failed;
                out.error = Some(if verify_failed {
                    format!(
                        "TLS certificate problem: {}",
                        tls::verify_error_text(verify_result)
                    )
                } else {
                    String::from("TLS handshake failed")
                });
                return None;
            }
        }
    }
}

pub(crate) fn read_line(
    stream: &mut Stream,
    buf: &mut Vec<u8>,
    pos: &mut usize,
    abort: &dyn Fn() -> bool,
) -> Option<(usize, usize)> {
    let mut search = *pos;
    loop {
        if let Some(i) = buf[search..].windows(2).position(|w| w == b"\r\n") {
            let start = *pos;
            let end = search + i;
            *pos = end + 2;
            return Some((start, end));
        }
        search = buf.len().saturating_sub(1).max(*pos);
        let mut tmp = [0u8; 16_384];
        match stream.read_some(&mut tmp, abort) {
            Some(n) if n > 0 => buf.extend_from_slice(&tmp[..n]),
            _ => return None,
        }
        if buf.len() > 1024 * 1024 {
            return None;
        }
    }
}

fn run_http1(req: &Request, stream: &mut Stream, t: &mut Transfer) -> bool {
    let head = h1_head(req);
    let deadline = t.deadline;
    let ok = {
        let h: &dyn Handler = &*t.handler;
        let abort = || h.should_abort() || Instant::now() > deadline;
        stream.write_all(&head, &abort)
            && (req.body.is_empty() || stream.write_all(req.body, &abort))
    };
    if !ok {
        return false;
    }
    let mut buf = Vec::new();
    let mut pos = 0usize;
    let mut status_parsed = false;
    let mut headers_complete = false;
    let mut chunked = false;
    let mut content_length: Option<i64> = None;
    loop {
        let line = {
            let h: &dyn Handler = &*t.handler;
            let abort = || h.should_abort() || Instant::now() > deadline;
            read_line(stream, &mut buf, &mut pos, &abort)
        };
        let Some((start, end)) = line else {
            if !status_parsed {
                return false;
            }
            break;
        };
        let line = buf[start..end].to_vec();
        if line.is_empty() {
            if t.informational {
                t.informational = false;
                status_parsed = false;
                t.status = 0;
                continue;
            }
            headers_complete = true;
            t.handler.headers_done();
            break;
        }
        if !status_parsed {
            t.h1_status_line(&line);
            status_parsed = true;
        } else if !t.informational {
            if let Some((name, value)) = h1::split_header(h1::until_nul(&line)) {
                if name.eq_ignore_ascii_case(b"transfer-encoding") {
                    if value.len() >= 7 && value[..7].eq_ignore_ascii_case(b"chunked") {
                        chunked = true;
                    }
                } else if name.eq_ignore_ascii_case(b"content-length") {
                    let v = h1::strtoll(value, 10);
                    content_length = (v >= 0).then_some(v);
                }
            }
            t.h1_header_line(&line);
        }
    }
    let first_byte = t.ms_since_start();
    t.first_byte_ms = Some(first_byte);
    if request_method(req).eq_ignore_ascii_case(b"HEAD") || t.status == 204 || t.status == 304 {
        return status_parsed;
    }
    let mut decoder = chunked.then(h1::Chunked::new);
    let mut received: i64 = 0;
    let mut pending = if headers_complete && pos < buf.len() {
        buf[pos..].to_vec()
    } else {
        Vec::new()
    };
    drop(buf);
    loop {
        if !pending.is_empty() {
            let mut data = std::mem::take(&mut pending);
            if let Some(chunks) = decoder.as_mut() {
                let mut body = Vec::new();
                if chunks.feed(&data, &mut body).is_err() {
                    break;
                }
                data = body;
            } else if let Some(limit) = content_length {
                let room = (limit - received).max(0) as usize;
                data.truncate(room);
            }
            received += data.len() as i64;
            t.body(&data);
            if t.stopped() {
                break;
            }
        }
        if decoder.as_ref().is_some_and(h1::Chunked::done) {
            break;
        }
        if decoder.is_none() && content_length.is_some_and(|l| received >= l) {
            break;
        }
        if t.should_abort() {
            break;
        }
        let mut tmp = vec![0u8; 65_536];
        let n = {
            let h: &dyn Handler = &*t.handler;
            let abort = || h.should_abort() || Instant::now() > deadline;
            stream.read_some(&mut tmp, &abort)
        };
        match n {
            Some(n) if n > 0 => pending.extend_from_slice(&tmp[..n]),
            _ => break,
        }
    }
    t.finish();
    status_parsed
}

#[derive(Default)]
struct Slot {
    events: VecDeque<Event>,
    submitted: bool,
    stream: u32,
    done: bool,
    ok: bool,
    refused: bool,
}

struct Pending {
    token: u64,
    fields: Vec<(Vec<u8>, Vec<u8>)>,
    body: Vec<u8>,
}

struct Shared {
    conn: Connection,
    pending: VecDeque<Pending>,
    slots: HashMap<u64, Slot>,
    streams: HashMap<u32, u64>,
    next_token: u64,
    io_failed: bool,
    stopping: bool,
    reuse_count: u32,
    last_used: Instant,
}

impl Shared {
    fn active(&self) -> usize {
        self.conn.active() + self.pending.len()
    }

    fn idle(&self) -> bool {
        self.conn.active() == 0 && self.pending.is_empty()
    }

    fn dead(&self) -> bool {
        self.io_failed || self.stopping || self.conn.goaway()
    }

    fn fail_all(&mut self) {
        self.pending.clear();
        for slot in self.slots.values_mut() {
            if !slot.done {
                slot.done = true;
                slot.ok = false;
            }
        }
        self.streams.clear();
    }

    fn dispatch(&mut self) {
        while let Some(event) = self.conn.next_event() {
            let stream = match &event {
                Event::Headers { stream, .. }
                | Event::Data { stream, .. }
                | Event::Closed { stream, .. } => *stream,
            };
            let Some(&token) = self.streams.get(&stream) else {
                continue;
            };
            let Some(slot) = self.slots.get_mut(&token) else {
                continue;
            };
            if let Event::Closed { code, .. } = event {
                slot.done = true;
                slot.ok = code == frame::NO_ERROR;
                slot.refused = code == frame::REFUSED_STREAM;
                self.streams.remove(&stream);
                self.last_used = Instant::now();
            } else {
                slot.events.push_back(event);
            }
        }
    }

    fn submit_pending(&mut self) {
        while self.conn.can_open() {
            let Some(p) = self.pending.pop_front() else {
                break;
            };
            let fields: Vec<(&[u8], &[u8])> = p
                .fields
                .iter()
                .map(|(n, v)| (n.as_slice(), v.as_slice()))
                .collect();
            match self.conn.submit(&fields, p.body) {
                Some(id) => {
                    if let Some(slot) = self.slots.get_mut(&p.token) {
                        slot.submitted = true;
                        slot.stream = id;
                        self.streams.insert(id, p.token);
                    } else {
                        self.conn.reset(id, frame::CANCEL);
                    }
                }
                None => {
                    if let Some(slot) = self.slots.get_mut(&p.token) {
                        slot.done = true;
                    }
                }
            }
        }
        if self.conn.goaway() {
            for p in std::mem::take(&mut self.pending) {
                if let Some(slot) = self.slots.get_mut(&p.token) {
                    slot.done = true;
                }
            }
        }
    }
}

struct Conn {
    shared: Mutex<Shared>,
    cond: Condvar,
    wake: Wake,
    remote_ip: String,
    thread: Mutex<Option<JoinHandle<()>>>,
}

fn lock(conn: &Conn) -> MutexGuard<'_, Shared> {
    conn.shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl Conn {
    fn enqueue(&self, shared: &mut Shared, fields: Vec<(Vec<u8>, Vec<u8>)>, body: Vec<u8>) -> u64 {
        let token = shared.next_token;
        shared.next_token += 1;
        shared.slots.insert(token, Slot::default());
        shared.pending.push_back(Pending {
            token,
            fields,
            body,
        });
        self.wake.signal();
        token
    }
}

fn io_thread(conn: Arc<Conn>, mut stream: Stream) {
    let mut outbuf: Vec<u8> = Vec::new();
    let mut rbuf = vec![0u8; 65_536];
    loop {
        let stop = {
            let mut s = lock(&conn);
            s.submit_pending();
            outbuf.extend_from_slice(&s.conn.take_output());
            let idle = s.idle();
            if idle && s.last_used.elapsed() > MAX_IDLE {
                s.stopping = true;
            }
            s.io_failed
                || (s.stopping && idle)
                || (s.conn.goaway() && s.conn.active() == 0 && outbuf.is_empty())
        };
        if stop {
            break;
        }
        let mut write_failed = false;
        while !outbuf.is_empty() {
            let r = match &mut stream {
                Stream::Tls(t, _) => t.write(&outbuf),
                Stream::Plain(_) => Io::Failed,
            };
            match r {
                Io::Done(n) => {
                    outbuf.drain(..n);
                }
                Io::WouldBlock => break,
                _ => {
                    write_failed = true;
                    break;
                }
            }
        }
        if write_failed {
            lock(&conn).io_failed = true;
            continue;
        }
        let pending_tls = matches!(&stream, Stream::Tls(t, _) if t.pending());
        let mut fds = [
            PollFd {
                fd: conn.wake.raw(),
                events: socket::POLLIN,
                revents: 0,
            },
            PollFd {
                fd: stream.raw(),
                events: socket::POLLIN
                    | if outbuf.is_empty() {
                        0
                    } else {
                        socket::POLLOUT
                    },
                revents: 0,
            },
        ];
        let timeout = if pending_tls { 0 } else { 250 };
        match socket::poll_fds(&mut fds, timeout) {
            Err(e) if !socket::interrupted(e) => {
                lock(&conn).io_failed = true;
                continue;
            }
            _ => {}
        }
        if fds[0].revents & socket::POLLIN != 0 {
            conn.wake.drain();
        }
        let readable = fds[1].revents & (socket::POLLIN | socket::POLLHUP | socket::POLLERR) != 0;
        if !(readable || pending_tls) {
            continue;
        }
        loop {
            let r = match &mut stream {
                Stream::Tls(t, _) => t.read(&mut rbuf),
                Stream::Plain(_) => Io::Failed,
            };
            match r {
                Io::Done(n) => {
                    let mut s = lock(&conn);
                    if s.conn.receive(&rbuf[..n]).is_err() {
                        s.io_failed = true;
                    }
                    s.dispatch();
                    let failed = s.io_failed;
                    drop(s);
                    conn.cond.notify_all();
                    if failed {
                        break;
                    }
                }
                Io::WouldBlock => break,
                Io::Closed | Io::Failed => {
                    lock(&conn).io_failed = true;
                    break;
                }
            }
        }
    }
    let mut s = lock(&conn);
    s.dispatch();
    s.fail_all();
    drop(s);
    conn.cond.notify_all();
    drop(stream);
}

struct Entry {
    conns: Vec<Arc<Conn>>,
    connecting: bool,
}

struct Pool {
    entries: HashMap<String, Entry>,
}

static POOL: Mutex<Option<Pool>> = Mutex::new(None);
static POOL_COND: Condvar = Condvar::new();

fn pool_lock() -> MutexGuard<'static, Option<Pool>> {
    POOL.lock().unwrap_or_else(|e| e.into_inner())
}

fn sweep(pool: &mut Pool) {
    for entry in pool.entries.values_mut() {
        let mut kept = 0;
        entry.conns.retain(|conn| {
            let mut s = lock(conn);
            let idle = s.idle();
            let expired = idle && s.last_used.elapsed() > MAX_IDLE;
            let retire = s.dead() || expired || (kept >= POOL_MAX_PER_ORIGIN && idle);
            if retire {
                s.stopping = true;
                drop(s);
                conn.wake.signal();
                false
            } else {
                kept += 1;
                true
            }
        });
    }
}

enum Attach {
    Reused(Arc<Conn>, u64),
    Connector,
    GaveUp,
}

fn attach(origin: &str, fields: &[(Vec<u8>, Vec<u8>)], body: &[u8], t: &Transfer) -> Attach {
    let mut guard = pool_lock();
    let pool = guard.get_or_insert_with(|| Pool {
        entries: HashMap::new(),
    });
    sweep(pool);
    loop {
        let pool = guard.get_or_insert_with(|| Pool {
            entries: HashMap::new(),
        });
        let entry = pool
            .entries
            .entry(origin.to_string())
            .or_insert_with(|| Entry {
                conns: Vec::new(),
                connecting: false,
            });
        let mut found = None;
        entry.conns.retain(|conn| {
            if found.is_some() {
                return true;
            }
            let mut s = lock(conn);
            if s.reuse_count >= MAX_REUSE {
                s.stopping = true;
            }
            if s.dead() {
                return false;
            }
            if s.active() < MAX_CONCURRENT {
                let token = conn.enqueue(&mut s, fields.to_vec(), body.to_vec());
                s.last_used = Instant::now();
                s.reuse_count += 1;
                found = Some((conn.clone(), token));
            }
            true
        });
        if let Some((conn, token)) = found {
            return Attach::Reused(conn, token);
        }
        if !entry.connecting {
            entry.connecting = true;
            return Attach::Connector;
        }
        let (g, timeout) = POOL_COND
            .wait_timeout(guard, Duration::from_millis(100))
            .unwrap_or_else(|e| e.into_inner());
        guard = g;
        if timeout.timed_out() && t.should_abort() {
            return Attach::GaveUp;
        }
    }
}

fn connect_done(origin: &str, conn: Option<Arc<Conn>>) {
    let mut guard = pool_lock();
    let pool = guard.get_or_insert_with(|| Pool {
        entries: HashMap::new(),
    });
    let entry = pool
        .entries
        .entry(origin.to_string())
        .or_insert_with(|| Entry {
            conns: Vec::new(),
            connecting: false,
        });
    entry.connecting = false;
    if let Some(conn) = conn {
        entry.conns.insert(0, conn);
    }
    drop(guard);
    POOL_COND.notify_all();
}

pub fn shutdown() {
    let pool = pool_lock().take();
    if let Some(pool) = pool {
        for entry in pool.entries.into_values() {
            for conn in entry.conns {
                lock(&conn).stopping = true;
                conn.wake.signal();
                let handle = conn.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
                if let Some(h) = handle {
                    let _ = h.join();
                }
            }
        }
    }
    tls::shutdown();
}

fn start_h2(stream: Stream, remote: SocketAddr) -> Option<Arc<Conn>> {
    let wake = Wake::new()?;
    if let Stream::Tls(_, tcp) = &stream {
        tcp.set_nonblocking(true).ok()?;
    }
    let conn = Arc::new(Conn {
        shared: Mutex::new(Shared {
            conn: Connection::new(),
            pending: VecDeque::new(),
            slots: HashMap::new(),
            streams: HashMap::new(),
            next_token: 1,
            io_failed: false,
            stopping: false,
            reuse_count: 0,
            last_used: Instant::now(),
        }),
        cond: Condvar::new(),
        wake,
        remote_ip: remote.ip().to_string(),
        thread: Mutex::new(None),
    });
    let worker = conn.clone();
    let handle = std::thread::Builder::new()
        .name(String::from("ns-h2-io"))
        .spawn(move || io_thread(worker, stream))
        .ok()?;
    *conn.thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
    Some(conn)
}

struct StreamResult {
    ok: bool,
    submitted: bool,
    refused: bool,
    rst: bool,
}

fn wait_stream(conn: &Conn, token: u64, t: &mut Transfer) -> StreamResult {
    let mut first_headers = true;
    loop {
        let mut s = lock(conn);
        let abort = t.should_abort();
        let (events, done, slot_ok, submitted, refused, stream) = {
            let Some(slot) = s.slots.get_mut(&token) else {
                return StreamResult {
                    ok: false,
                    submitted: false,
                    refused: false,
                    rst: false,
                };
            };
            (
                std::mem::take(&mut slot.events),
                slot.done,
                slot.ok,
                slot.submitted,
                slot.refused,
                slot.stream,
            )
        };
        if abort && !done {
            if submitted {
                s.conn.reset(stream, frame::CANCEL);
                s.streams.remove(&stream);
            } else {
                s.pending.retain(|p| p.token != token);
            }
            s.slots.remove(&token);
            drop(s);
            conn.wake.signal();
            return StreamResult {
                ok: false,
                submitted,
                refused: false,
                rst: true,
            };
        }
        if events.is_empty() && !done {
            let _ = conn
                .cond
                .wait_timeout(s, Duration::from_millis(200))
                .unwrap_or_else(|e| e.into_inner());
            continue;
        }
        drop(s);
        for event in events {
            match event {
                Event::Headers { fields, .. } => {
                    if !first_headers && !t.informational {
                        continue;
                    }
                    first_headers = false;
                    for (name, value) in &fields {
                        t.h2_header(name, value, b"HTTP/2");
                    }
                    if !t.informational && t.status_line_fed && !t.proto_error {
                        t.handler.headers_done();
                    }
                }
                Event::Data { bytes, .. } => t.body(&bytes),
                Event::Closed { .. } => {}
            }
            if t.proto_error || t.stopped() {
                break;
            }
        }
        if (t.proto_error || t.stopped()) && !done {
            let mut s = lock(conn);
            if s.streams.remove(&stream).is_some() {
                s.conn.reset(
                    stream,
                    if t.proto_error {
                        frame::INTERNAL_ERROR
                    } else {
                        frame::CANCEL
                    },
                );
            }
            s.slots.remove(&token);
            drop(s);
            conn.wake.signal();
            return StreamResult {
                ok: false,
                submitted,
                refused: false,
                rst: false,
            };
        }
        if done {
            lock(conn).slots.remove(&token);
            return StreamResult {
                ok: slot_ok && !t.proto_error,
                submitted,
                refused,
                rst: false,
            };
        }
    }
}

pub fn perform(req: &Request, handler: &mut dyn Handler) -> Outcome {
    let mut out = Outcome::new();
    let start = Instant::now();
    let deadline = start
        .checked_add(req.timeout)
        .unwrap_or_else(|| start + Duration::from_secs(10 * 365 * 86_400));
    let connect_deadline = (start + req.connect_timeout).min(deadline);
    let mut t = Transfer::new(handler, start, deadline);
    let origin = origin_key(req);
    let fields = h2_fields(req);

    let mut ok = false;
    let mut reused = false;
    let mut via_h2 = false;
    let mut rst = false;
    let mut connects = 1;
    for attempt in 0..2 {
        let (conn, token) = match attach(&origin, &fields, req.body, &t) {
            Attach::Reused(conn, token) => {
                reused = true;
                via_h2 = true;
                if out.remote_ip.is_none() {
                    out.remote_ip = Some(conn.remote_ip.clone());
                }
                (conn, token)
            }
            Attach::Connector => {
                let Some(connected) = open(req, &t, connect_deadline, &mut out, true) else {
                    connect_done(&origin, None);
                    out.total_ms = t.ms_since_start();
                    return out;
                };
                connects = 1;
                out.pretransfer_ms = t.ms_since_start();
                let Connected {
                    mut stream,
                    remote,
                    h2,
                    ..
                } = connected;
                if !h2 {
                    connect_done(&origin, None);
                    ok = run_http1(req, &mut stream, &mut t);
                    break;
                }
                let Some(conn) = start_h2(stream, remote) else {
                    connect_done(&origin, None);
                    out.error = Some(String::from("HTTP/2 session init failed"));
                    out.total_ms = t.ms_since_start();
                    return out;
                };
                via_h2 = true;
                let token = {
                    let mut s = lock(&conn);
                    conn.enqueue(&mut s, fields.clone(), req.body.to_vec())
                };
                connect_done(&origin, Some(conn.clone()));
                (conn, token)
            }
            Attach::GaveUp => {
                out.cancelled = t.handler.should_abort();
                if !out.cancelled {
                    out.error = Some(String::from("request timed out"));
                }
                out.total_ms = t.ms_since_start();
                return out;
            }
        };
        let result = wait_stream(&conn, token, &mut t);
        ok = result.ok;
        rst = result.rst;
        if reused && attempt == 0 && !t.got_first_byte && (!result.submitted || result.refused) {
            reused = false;
            via_h2 = false;
            t.reset();
            continue;
        }
        break;
    }
    if via_h2 && !t.stopped() && !rst {
        t.finish();
    }

    out.starttransfer_ms = t.first_byte_ms.unwrap_or(0.0);
    out.total_ms = t.ms_since_start();
    out.version = if via_h2 {
        Version::Http2
    } else {
        Version::Http11
    };
    out.connects = if reused { 0 } else { connects };
    if t.handler.should_abort() {
        out.cancelled = true;
        return out;
    }
    out.status = t.status;
    if t.sink_full {
        out.sink_full = true;
    } else if t.corrupt {
        out.error = Some(String::from("content decoding failed"));
    } else if ok && t.status > 0 {
        out.ok = true;
    } else if out.error.is_none() {
        out.error = Some(String::from(if rst {
            "request timed out"
        } else if via_h2 {
            "HTTP/2 transfer failed"
        } else {
            "HTTP transfer failed"
        }));
    }
    out
}

struct Abort<'a>(&'a dyn Fn() -> bool);

impl Handler for Abort<'_> {
    fn should_abort(&self) -> bool {
        (self.0)()
    }

    fn status_line(&mut self, _line: &[u8]) {}

    fn header(&mut self, _line: &[u8], _name: &[u8], _value: &[u8]) {}

    fn body(&mut self, _data: &[u8]) -> bool {
        false
    }
}

pub enum Received {
    Data(usize),
    Idle,
    Closed,
    Failed,
}

pub struct Upgraded {
    pub status: i64,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub tls_warning: Option<String>,
    stream: Stream,
    buffered: Vec<u8>,
    poll_ms: i32,
}

impl Upgraded {
    pub fn set_poll_interval(&mut self, interval: Duration) {
        self.poll_ms = interval.as_millis().min(i32::MAX as u128) as i32;
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Received {
        match self.read_now(buf) {
            Received::Idle if socket::wait(self.stream.raw(), socket::POLLIN, self.poll_ms) => {
                self.read_now(buf)
            }
            other => other,
        }
    }

    fn read_now(&mut self, buf: &mut [u8]) -> Received {
        if !self.buffered.is_empty() {
            let n = buf.len().min(self.buffered.len());
            buf[..n].copy_from_slice(&self.buffered[..n]);
            self.buffered.drain(..n);
            return Received::Data(n);
        }
        match &mut self.stream {
            Stream::Plain(s) => match s.read(buf) {
                Ok(0) => Received::Closed,
                Ok(n) => Received::Data(n),
                Err(e) if socket::retryable(e.raw_os_error().unwrap_or(0)) => Received::Idle,
                Err(_) => Received::Failed,
            },
            Stream::Tls(t, _) => match t.read(buf) {
                Io::Done(n) => Received::Data(n),
                Io::Closed => Received::Closed,
                Io::WouldBlock => Received::Idle,
                Io::Failed => Received::Failed,
            },
        }
    }

    pub fn write_all(&mut self, data: &[u8], abort: &dyn Fn() -> bool) -> bool {
        self.stream.write_all(data, abort)
    }
}

pub fn upgrade(req: &Request, abort: &dyn Fn() -> bool) -> Result<Upgraded, String> {
    let start = Instant::now();
    let deadline = start
        .checked_add(req.connect_timeout)
        .unwrap_or_else(|| start + Duration::from_secs(3600));
    let mut quiet = Abort(abort);
    let t = Transfer::new(&mut quiet, start, deadline);
    let mut out = Outcome::new();
    let Some(connected) = open(req, &t, deadline, &mut out, false) else {
        return Err(out.error.unwrap_or_else(|| {
            String::from(if out.cancelled {
                "aborted"
            } else {
                "connect failed"
            })
        }));
    };
    let mut stream = connected.stream;
    let give_up = || abort() || Instant::now() > deadline;
    if !stream.write_all(&h1_head_upgrade(req), &give_up) {
        return Err(String::from("failed to send the upgrade request"));
    }
    let mut buf = Vec::new();
    let mut pos = 0usize;
    let mut status = 0;
    let mut headers = Vec::new();
    loop {
        let Some((start, end)) = read_line(&mut stream, &mut buf, &mut pos, &give_up) else {
            return Err(String::from("connection closed during the upgrade"));
        };
        let line = h1::until_nul(&buf[start..end]).to_vec();
        if line.is_empty() {
            if status / 100 == 1 && status != 101 {
                status = 0;
                headers.clear();
                continue;
            }
            break;
        }
        if status == 0 {
            status = h1::status_code(&line).unwrap_or(0);
        } else if let Some((name, value)) = h1::split_header(&line) {
            headers.push((name.to_vec(), value.trim_ascii_end().to_vec()));
        }
    }
    let buffered = buf[pos..].to_vec();
    Ok(Upgraded {
        status,
        headers,
        tls_warning: out.tls_warning,
        stream,
        buffered,
        poll_ms: 10,
    })
}

fn h1_head_upgrade(req: &Request) -> Vec<u8> {
    let mut fields: Vec<(&[u8], &[u8])> = Vec::new();
    if let Some(ua) = req.user_agent {
        fields.push((b"User-Agent", ua));
    }
    let forward = forward_proxy(req);
    let auth = forward
        .and_then(Proxy::authorization)
        .map(|a| [&b"Proxy-Authorization: "[..], &a].concat());
    let mut extra: Vec<&[u8]> = req.extra_headers.clone();
    if let Some(a) = &auth {
        extra.push(a);
    }
    let mut head = h1::request_head(
        b"GET",
        if forward.is_some() { req.url } else { req.path },
        req.authority,
        &fields,
        &extra,
        0,
        true,
    );
    let tail = b"Connection: keep-alive\r\n\r\n";
    head.truncate(head.len() - tail.len());
    head.extend_from_slice(b"Connection: Upgrade\r\n\r\n");
    head
}

pub fn preconnect(req: &Request, abort: &dyn Fn() -> bool) {
    let origin = origin_key(req);
    {
        let mut guard = pool_lock();
        let pool = guard.get_or_insert_with(|| Pool {
            entries: HashMap::new(),
        });
        let entry = pool.entries.entry(origin.clone()).or_insert_with(|| Entry {
            conns: Vec::new(),
            connecting: false,
        });
        let live = entry.conns.iter().any(|c| !lock(c).dead());
        if live || entry.connecting {
            return;
        }
        entry.connecting = true;
    }
    let start = Instant::now();
    let connect_deadline = start
        .checked_add(req.connect_timeout)
        .unwrap_or_else(|| start + Duration::from_secs(60));
    let mut quiet = Abort(abort);
    let t = Transfer::new(&mut quiet, start, connect_deadline);
    let mut out = Outcome::new();
    let conn = open(req, &t, connect_deadline, &mut out, true)
        .filter(|c| c.h2)
        .and_then(|c| start_h2(c.stream, c.remote));
    connect_done(&origin, conn);
}
