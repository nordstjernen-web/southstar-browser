//! Southstar — fuzz target: parses a URL against a base, checks that serializing is idempotent, and runs the URL API setters.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;
use southstar_url::{Setter, Url, domain_to_ascii};

const SETTERS: [Setter; 9] = [
    Setter::Protocol,
    Setter::Username,
    Setter::Password,
    Setter::Host,
    Setter::Hostname,
    Setter::Port,
    Setter::Pathname,
    Setter::Search,
    Setter::Hash,
];

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let mut parts = text.splitn(3, '\n');
    let input = parts.next().unwrap_or_default();
    let base = parts.next().and_then(|base| Url::parse(base, None));
    let value = parts.next().unwrap_or_default();
    let _ = domain_to_ascii(input);
    let Some(mut url) = Url::parse(input, base.as_ref()) else {
        return;
    };
    let href = url.href();
    let reparsed = Url::parse(&href, None).expect("a serialized URL parses again");
    assert_eq!(
        reparsed.href(),
        href,
        "serializing a parsed URL is idempotent"
    );
    let _ = (
        url.host_string(),
        url.path_string(),
        url.serialize(true),
        url.port(),
    );
    let setter = SETTERS[data.len() % SETTERS.len()];
    url.set(setter, value);
    let _ = Url::parse(&url.href(), None);
});
