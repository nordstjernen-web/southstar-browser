//! Southstar — performance timeline entries and the resource timing a document may see of a fetch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) const ENTRY_CAP: usize = 256;

const TIMER_RESOLUTION_US: i64 = 100;

const NETWORK_HEADER_BYTES: i64 = 300;

#[derive(Clone, Copy, Default)]
pub(crate) struct Phases {
    pub fetch_start: f64,
    pub domain_lookup_start: f64,
    pub domain_lookup_end: f64,
    pub connect_start: f64,
    pub connect_end: f64,
    pub secure_connection_start: f64,
    pub request_start: f64,
    pub response_start: f64,
    pub response_end: f64,
}

impl Phases {
    pub fn named(&self) -> [(&'static str, f64); 9] {
        [
            ("fetchStart", self.fetch_start),
            ("domainLookupStart", self.domain_lookup_start),
            ("domainLookupEnd", self.domain_lookup_end),
            ("connectStart", self.connect_start),
            ("connectEnd", self.connect_end),
            ("secureConnectionStart", self.secure_connection_start),
            ("requestStart", self.request_start),
            ("responseStart", self.response_start),
            ("responseEnd", self.response_end),
        ]
    }
}

#[derive(Clone, Default)]
pub(crate) struct Entry {
    pub name: Vec<u8>,
    pub kind: Vec<u8>,
    pub initiator_type: Option<Vec<u8>>,
    pub start_time: f64,
    pub duration: f64,
    pub transfer_size: i64,
    pub encoded_size: i64,
    pub has_timing: bool,
    pub next_hop_protocol: Option<Vec<u8>>,
    pub response_status: i32,
    pub phases: Phases,
    pub render_blocking: bool,
    pub realm: usize,
}

impl Entry {
    pub fn simple(realm: usize, kind: &[u8], name: &[u8], start_time: f64, duration: f64) -> Entry {
        Entry {
            name: name.to_vec(),
            kind: kind.to_vec(),
            start_time,
            duration,
            realm,
            ..Entry::default()
        }
    }

    pub fn is(&self, kind: &[u8]) -> bool {
        self.kind == kind
    }
}

pub(crate) struct ResourceInfo<'a> {
    pub timeline: usize,
    pub document_url: Option<&'a [u8]>,
    pub render_blocking: bool,
    pub cors_mode: bool,
    pub next_hop_protocol: Option<&'a [u8]>,
    pub timing_allow_origin: Option<&'a [u8]>,
    pub status: i64,
    pub body_size: i64,
}

pub(crate) struct Response<'a> {
    pub status: i64,
    pub cors_allow_origin: Option<&'a [u8]>,
    pub raw_headers: Option<&'a [u8]>,
    pub body_len: Option<i64>,
    pub next_hop_protocol: Option<&'a [u8]>,
    pub request_start_us: i64,
    pub domain_lookup_ms: f64,
    pub connect_ms: f64,
    pub tls_ms: f64,
    pub pretransfer_ms: f64,
    pub response_start_ms: f64,
}

pub(crate) struct Fetch<'a> {
    pub url: &'a [u8],
    pub initiator: Option<&'a [u8]>,
    pub start_us: i64,
    pub end_us: i64,
}

fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

pub fn coarsen_us(us: i64) -> i64 {
    (us / TIMER_RESOLUTION_US) * TIMER_RESOLUTION_US
}

pub fn relative_ms(now_us: i64, origin_us: i64) -> f64 {
    let now_us = coarsen_us(now_us);
    let origin_us = coarsen_us(origin_us);
    if now_us < origin_us {
        return 0.0;
    }
    (now_us - origin_us) as f64 / 1000.0
}

pub fn coarsen_real_ms(real_us: i64) -> f64 {
    (real_us as f64 / 100.0).floor() / 10.0
}

pub fn untimed(url: &[u8]) -> bool {
    url.starts_with(b"data:") || url.starts_with(b"blob:") || url.starts_with(b"about:")
}

fn same_origin(a: Option<&[u8]>, b: &[u8]) -> bool {
    a.is_some_and(|a| southstar_net::same_origin(a, b))
}

fn origin_of(url: Option<&[u8]>) -> Option<Vec<u8>> {
    url.and_then(southstar_net::origin_from)
}

fn strip_ascii_space(s: &[u8]) -> &[u8] {
    let start = s
        .iter()
        .position(|c| !c.is_ascii_whitespace() && *c != 0x0b)
        .unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|c| !c.is_ascii_whitespace() && *c != 0x0b)
        .map_or(start, |i| i + 1);
    &s[start..end.max(start)]
}

fn timing_allowed(document_url: Option<&[u8]>, url: &[u8], tao: Option<&[u8]>) -> bool {
    if same_origin(document_url, url) {
        return true;
    }
    let (Some(tao), Some(_)) = (tao, document_url) else {
        return false;
    };
    let origin = origin_of(document_url);
    tao.split(|&c| c == b',').any(|part| {
        let value = strip_ascii_space(part);
        value == b"*"
            || origin
                .as_deref()
                .is_some_and(|origin| origin != b"null" && value == origin)
    })
}

fn resource_timing_allowed(
    info: &ResourceInfo<'_>,
    resp: Option<&Response<'_>>,
    url: &[u8],
) -> bool {
    let tao = match resp {
        Some(resp) => resp
            .raw_headers
            .and_then(|raw| southstar_net::raw_header_values(raw, b"timing-allow-origin")),
        None => info.timing_allow_origin.map(<[u8]>::to_vec),
    };
    timing_allowed(info.document_url, url, tao.as_deref())
}

fn visible_status(
    info: &ResourceInfo<'_>,
    resp: Option<&Response<'_>>,
    url: &[u8],
    initiator: Option<&[u8]>,
) -> i32 {
    let status = resp.map_or(info.status, |resp| resp.status) as i32;
    if same_origin(info.document_url, url)
        || initiator == Some(b"iframe")
        || initiator == Some(b"frame")
    {
        return status;
    }
    let Some(acao) = resp
        .filter(|_| info.cors_mode)
        .and_then(|resp| resp.cors_allow_origin)
    else {
        return 0;
    };
    let document_origin = origin_of(info.document_url);
    let cors_ok = acao == b"*"
        || document_origin
            .as_deref()
            .is_some_and(|origin| acao.eq_ignore_ascii_case(origin));
    if cors_ok { status } else { 0 }
}

fn set_phases(
    entry: &mut Entry,
    resp: Option<&Response<'_>>,
    url: &[u8],
    origin_us: i64,
    end: f64,
) {
    let network = resp.filter(|resp| resp.request_start_us > 0 && resp.next_hop_protocol.is_some());
    let fetch = match network {
        Some(resp) => max(
            entry.start_time,
            relative_ms(resp.request_start_us, origin_us),
        ),
        None => entry.start_time,
    };
    let phase = |ms: f64| {
        min(
            end,
            fetch
                + if network.is_some() && ms > 0.0 {
                    ms
                } else {
                    0.0
                },
        )
    };
    let field = |f: fn(&Response<'_>) -> f64| resp.map_or(0.0, f);
    let p = &mut entry.phases;
    p.fetch_start = fetch;
    p.domain_lookup_start = fetch;
    p.domain_lookup_end = phase(field(|r| r.domain_lookup_ms));
    p.connect_start = p.domain_lookup_end;
    p.connect_end = max(
        p.connect_start,
        phase(field(|r| max(r.connect_ms, r.tls_ms))),
    );
    if url.starts_with(b"https:") {
        p.secure_connection_start = match network {
            Some(resp) if resp.tls_ms > 0.0 => max(p.connect_start, phase(resp.connect_ms)),
            _ => fetch,
        };
    }
    p.request_start = max(p.connect_end, phase(field(|r| r.pretransfer_ms)));
    p.response_start = max(
        p.request_start,
        match network {
            Some(resp) if resp.response_start_ms > 0.0 => phase(resp.response_start_ms),
            _ => end,
        },
    );
}

fn set_sizes(entry: &mut Entry, info: &ResourceInfo<'_>, resp: Option<&Response<'_>>) {
    entry.encoded_size = match resp {
        Some(resp) => resp.body_len.unwrap_or(0),
        None => info.body_size,
    };
    let protocol = match resp {
        Some(resp) => resp.next_hop_protocol,
        None => info.next_hop_protocol,
    };
    entry.next_hop_protocol = Some(protocol.unwrap_or_default().to_vec());
    entry.transfer_size = if protocol.is_some_and(|p| !p.is_empty()) {
        entry.encoded_size + NETWORK_HEADER_BYTES
    } else {
        0
    };
}

pub(crate) fn resource_entry(
    realm: usize,
    origin_us: i64,
    info: &ResourceInfo<'_>,
    fetch: &Fetch<'_>,
    resp: Option<&Response<'_>>,
) -> Entry {
    let start_time = relative_ms(fetch.start_us, origin_us);
    let end = relative_ms(fetch.end_us.max(fetch.start_us), origin_us);
    let mut entry = Entry {
        name: fetch.url.to_vec(),
        kind: b"resource".to_vec(),
        initiator_type: Some(fetch.initiator.unwrap_or(b"other").to_vec()),
        render_blocking: info.render_blocking,
        start_time,
        duration: end - start_time,
        response_status: visible_status(info, resp, fetch.url, fetch.initiator),
        has_timing: true,
        realm,
        ..Entry::default()
    };
    entry.phases.fetch_start = start_time;
    entry.phases.response_end = end;
    if resource_timing_allowed(info, resp, fetch.url) {
        set_sizes(&mut entry, info, resp);
        set_phases(&mut entry, resp, fetch.url, origin_us, end);
    } else {
        entry.next_hop_protocol = Some(Vec::new());
    }
    entry
}
