//! Southstar — unit tests for cookie parsing, storage and matching, the HSTS list and policy, HTTP dates and strtoll.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_url::bytes::parts;

use crate::cookies::{self, Context, Cookie, SameSite};
use crate::hsts::{self, Policy, Scope};
use crate::{http_date, strtoll};

const NOW: i64 = 1_760_000_000;

fn no_public_suffix(_: &[u8]) -> bool {
    false
}

fn set(target: &str, cookie: &str) -> Option<Cookie> {
    cookies::parse(target.as_bytes(), cookie.as_bytes(), NOW, |d: &[u8]| {
        d == b"com" || d == b"co.uk"
    })
}

fn store_all(target: &str, set_cookies: &[&str]) -> Vec<u8> {
    let mut jar = Vec::new();
    for header in set_cookies {
        if let Some(cookie) = set(target, header) {
            jar = cookies::store(Some(&jar), &cookie, true);
        }
    }
    jar
}

fn header(jar: &[u8], url: &str, include_httponly: bool) -> Option<String> {
    let url = parts(url.as_bytes()).expect("url parses");
    cookies::header(&url, [jar], include_httponly, Context::SameSite, NOW)
        .map(|h| String::from_utf8(h).expect("utf-8"))
}

#[test]
fn strtoll_follows_g_ascii_strtoll() {
    assert_eq!(strtoll(b"  42abc"), (42, 4));
    assert_eq!(strtoll(b"-7"), (-7, 2));
    assert_eq!(strtoll(b"+"), (0, 0));
    assert_eq!(strtoll(b"x1"), (0, 0));
    assert_eq!(strtoll(b"99999999999999999999"), (i64::MAX, 20));
    assert_eq!(strtoll(b"-99999999999999999999"), (i64::MIN, 21));
    assert_eq!(strtoll(b"-9223372036854775808"), (i64::MIN, 20));
    assert_eq!(strtoll(b"12\x0034"), (12, 2));
}

#[test]
fn parses_http_dates() {
    assert_eq!(http_date::parse(b"Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    assert_eq!(
        http_date::parse(b"Sun, 06 Nov 1994 08:49:37 GMT"),
        Some(784_111_777)
    );
    assert_eq!(
        http_date::parse(b"Sunday, 06-Nov-94 08:49:37 GMT"),
        Some(784_111_777)
    );
    assert_eq!(
        http_date::parse(b"Sun Nov  6 08:49:37 1994"),
        Some(784_111_777)
    );
    assert_eq!(http_date::parse(b"not a date"), None);
}

#[test]
fn stores_and_sends_cookies() {
    let jar = store_all(
        "https://www.example.com/docs/page",
        &[
            "a=1",
            "b=2; Path=/; Secure",
            "c=3; HttpOnly",
            "d=4; Domain=example.com; Path=/",
        ],
    );
    assert_eq!(
        header(&jar, "https://www.example.com/docs/x", true).as_deref(),
        Some("a=1; b=2; c=3; d=4")
    );
    assert_eq!(
        header(&jar, "https://www.example.com/docs/x", false).as_deref(),
        Some("a=1; b=2; d=4")
    );
    assert_eq!(
        header(&jar, "http://www.example.com/docs/x", true).as_deref(),
        Some("a=1; c=3; d=4")
    );
    assert_eq!(
        header(&jar, "https://www.example.com/other", true).as_deref(),
        Some("b=2; d=4")
    );
    assert_eq!(
        header(&jar, "https://api.example.com/", true).as_deref(),
        Some("d=4")
    );
    assert_eq!(header(&jar, "https://example.org/", true), None);
}

#[test]
fn replaces_and_expires_cookies() {
    let target = "https://example.com/";
    let jar = store_all(target, &["id=old", "id=new", "gone=1", "gone=; Max-Age=0"]);
    assert_eq!(header(&jar, target, true).as_deref(), Some("id=new"));
    let past = store_all(target, &["x=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT"]);
    assert_eq!(header(&past, target, true), None);
    let later = set(target, "y=1; Max-Age=60").expect("valid cookie");
    assert_eq!(later.attributes.expiry, NOW + 60);
    let stale = format!("example.com\tFALSE\t/\tFALSE\t{}\tz\t1\n", NOW - 1);
    assert_eq!(header(stale.as_bytes(), target, true), None);
}

#[test]
fn rejects_unsafe_cookies() {
    assert!(set("http://example.com/", "s=1; Secure").is_none());
    assert!(set("https://example.com/", "__Secure-s=1").is_none());
    assert!(set("https://example.com/", "__Secure-s=1; Secure").is_some());
    assert!(set("https://example.com/", "__Host-h=1; Secure; Path=/").is_some());
    assert!(
        set(
            "https://example.com/",
            "__Host-h=1; Secure; Domain=example.com"
        )
        .is_none()
    );
    assert!(set("https://example.com/", "__Host-h=1; Secure; Path=/x").is_none());
    assert!(set("https://example.com/", "d=1; Domain=com").is_none());
    assert!(set("https://example.com/", "d=1; Domain=other.com").is_none());
    assert!(set("https://example.com/", "=nameless").is_none());
    assert!(set("https://example.com/", "ctl=a\x01b").is_none());
    assert!(set("ftp://example.com/", "a=1").is_none());
    assert!(
        cookies::parse(
            b"https://a.example.com/",
            b"d=1; Domain=example.com",
            NOW,
            no_public_suffix
        )
        .is_some()
    );
}

#[test]
fn script_cookies_cannot_overwrite_httponly_ones() {
    let target = "https://example.com/";
    let jar = store_all(target, &["session=secret; HttpOnly"]);
    let from_script = set(target, "session=evil").expect("valid cookie");
    assert!(cookies::has_live_httponly(&jar, &from_script));
    let jar = cookies::store(Some(&jar), &from_script, false);
    assert_eq!(
        header(&jar, target, true).as_deref(),
        Some("session=secret")
    );
}

#[test]
fn defaults_the_cookie_path_to_the_request_directory() {
    assert_eq!(
        set("https://example.com/a/b/c", "x=1").expect("valid").path,
        b"/a/b"
    );
    assert_eq!(
        set("https://example.com/a", "x=1").expect("valid").path,
        b"/"
    );
    assert_eq!(
        set("https://example.com/a/b", "x=1; Path=rel")
            .expect("valid")
            .path,
        b"/a"
    );
}

#[test]
fn hsts_list_lookup() {
    let hosts = hsts::parse_list(b"# comment\n.example.com \"20300101 00:00:00\"\nexact.test \"20300101 00:00:00\"\nbroken\n");
    assert_eq!(
        hosts.get(&b"example.com"[..]).map(|e| e.scope),
        Some(Scope::WithSubdomains)
    );
    assert!(hsts::lookup(&hosts, b"example.com", NOW));
    assert!(hsts::lookup(&hosts, b"deep.www.example.com", NOW));
    assert!(hsts::lookup(&hosts, b"exact.test", NOW));
    assert!(!hsts::lookup(&hosts, b"sub.exact.test", NOW));
    assert!(!hsts::lookup(&hosts, b"notexample.com", NOW));
    assert_eq!(hsts::normalize_host(b"Example.COM.."), b"example.com");
}

#[test]
fn hsts_entries_expire() {
    let hosts = hsts::parse_list(
        b".old.test \"20200101 00:00:00\"\nforever.test unlimited\nbad.test \"2030-01-01\"\n.new.test \"20300101 00:00:00\"\n",
    );
    assert!(!hsts::lookup(&hosts, b"old.test", NOW));
    assert!(!hsts::lookup(&hosts, b"www.old.test", NOW));
    assert!(hsts::lookup(&hosts, b"old.test", 1_500_000_000));
    assert!(hsts::lookup(&hosts, b"forever.test", i64::MAX));
    assert!(!hsts::lookup(&hosts, b"bad.test", NOW));
    let new_expiry = hsts::parse_expiry(b"\"20300101 00:00:00\"");
    assert_eq!(new_expiry, Some(1_893_456_000));
    assert!(hsts::lookup(&hosts, b"www.new.test", 1_893_455_999));
    assert!(!hsts::lookup(&hosts, b"www.new.test", 1_893_456_000));
    for at in [0, 951_782_400, NOW, 4_102_444_799] {
        assert_eq!(hsts::parse_expiry(&hsts::expiry_text(at)), Some(at));
    }
    assert_eq!(hsts::parse_expiry(b"\"20301301 00:00:00\""), None);
    assert_eq!(hsts::parse_expiry(b"20300101 00:00:00"), None);
    let pruned = hsts::record(
        b"old.test \"20200101 00:00:00\"\nkeep.test unlimited\n",
        b"example.com",
        &Policy {
            max_age: Some(60),
            subdomains: false,
        },
        60,
        NOW,
    );
    assert!(!pruned.windows(8).any(|w| w == b"old.test"));
    assert!(pruned.starts_with(b"keep.test unlimited\n"));
    let forever = hsts::record(
        b"",
        b"example.com",
        &Policy {
            max_age: Some(i64::MAX),
            subdomains: false,
        },
        i64::MAX,
        NOW,
    );
    assert_eq!(forever, b"example.com \"99991231 23:59:59\"\n");
    assert!(hsts::lookup(
        &hsts::parse_list(&forever),
        b"example.com",
        NOW
    ));
}

#[test]
fn hsts_policy_parsing() {
    let policy = |v: &str| hsts::parse_policy(v.as_bytes());
    assert_eq!(
        policy("max-age=31536000; includeSubDomains"),
        Policy {
            max_age: Some(31_536_000),
            subdomains: true
        }
    );
    assert_eq!(
        policy("MAX-AGE=\"60\""),
        Policy {
            max_age: Some(60),
            subdomains: false
        }
    );
    assert_eq!(policy("max-age=-1").max_age, None);
    assert_eq!(policy("includesubdomains").max_age, None);
    assert_eq!(
        policy("max-age=99999999999999999999999").max_age,
        Some(i64::MAX)
    );
    assert!(hsts::is_ip_literal(b"192.168.0.1"));
    assert!(hsts::is_ip_literal(b"[::1]"));
    assert!(!hsts::is_ip_literal(b"example.com"));
}

#[test]
fn hsts_record_rewrites_the_list() {
    let subdomains = Policy {
        max_age: Some(86_400),
        subdomains: true,
    };
    let list = hsts::record(
        b"other.test \"20300101 00:00:00\"\n",
        b"example.com",
        &subdomains,
        86_400,
        0,
    );
    assert_eq!(
        list,
        b"other.test \"20300101 00:00:00\"\n.example.com \"19700102 00:00:00\"\n"
    );
    let removed = hsts::record(
        &list,
        b"example.com",
        &Policy {
            max_age: Some(0),
            subdomains: false,
        },
        0,
        NOW,
    );
    assert_eq!(removed, b"other.test \"20300101 00:00:00\"\n");
    assert_eq!(hsts::expiry_text(951_782_400), b"\"20000229 00:00:00\"");
}

fn header_in(jar: &[u8], url: &str, context: Context) -> Option<String> {
    let url = parts(url.as_bytes()).expect("url parses");
    cookies::header(&url, [jar], true, context, NOW).map(|h| String::from_utf8(h).expect("utf-8"))
}

#[test]
fn same_site_limits_cross_site_requests() {
    let target = "https://example.com/";
    let jar = store_all(
        target,
        &[
            "plain=1",
            "lax=2; SameSite=Lax",
            "strict=3; SameSite=strict",
            "none=4; SameSite=None; Secure",
            "odd=5; SameSite=bogus",
        ],
    );
    assert_eq!(
        header_in(&jar, target, Context::SameSite).as_deref(),
        Some("plain=1; lax=2; strict=3; none=4; odd=5")
    );
    assert_eq!(
        header_in(&jar, target, Context::TopLevelSafeNavigation).as_deref(),
        Some("plain=1; lax=2; none=4; odd=5")
    );
    assert_eq!(
        header_in(&jar, target, Context::CrossSite).as_deref(),
        Some("none=4")
    );
    let legacy = b"example.com\tFALSE\t/\tFALSE\t0\told\t1\n";
    assert_eq!(header_in(legacy, target, Context::CrossSite), None);
    assert_eq!(
        header_in(legacy, target, Context::TopLevelSafeNavigation).as_deref(),
        Some("old=1")
    );
}

#[test]
fn same_site_none_needs_secure_and_cross_site_responses_set_only_none() {
    assert!(set("https://example.com/", "a=1; SameSite=None").is_none());
    let none = set("https://example.com/", "a=1; SameSite=None; Secure").expect("valid");
    assert_eq!(none.attributes.same_site, SameSite::None);
    let lax = set("https://example.com/", "a=1").expect("valid");
    assert_eq!(lax.attributes.same_site, SameSite::Lax);
    assert!(cookies::settable_in(&none, Context::CrossSite));
    assert!(!cookies::settable_in(&lax, Context::CrossSite));
    assert!(cookies::settable_in(&lax, Context::TopLevelSafeNavigation));
}
