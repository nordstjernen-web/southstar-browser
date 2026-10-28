//! Southstar — unit tests for the URL parser, serializer, host parser and setters against the URL Standard's examples.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{Host, Setter, Url, domain_to_ascii};

fn href(input: &str) -> Option<String> {
    Url::parse(input, None).map(|url| url.href())
}

fn resolve(input: &str, base: &str) -> Option<String> {
    let base = Url::parse(base, None).expect("base parses");
    Url::parse(input, Some(&base)).map(|url| url.href())
}

#[test]
fn normalizes_special_urls() {
    assert_eq!(
        href("HTTPS://Example.COM/a/./b/../c").as_deref(),
        Some("https://example.com/a/c")
    );
    assert_eq!(
        href("https://example.com").as_deref(),
        Some("https://example.com/")
    );
    assert_eq!(
        href("https://example.com:443/").as_deref(),
        Some("https://example.com/")
    );
    assert_eq!(
        href("http://example.com:8080/").as_deref(),
        Some("http://example.com:8080/")
    );
    assert_eq!(
        href("http:\\\\example.com\\a").as_deref(),
        Some("http://example.com/a")
    );
    assert_eq!(
        href("  https://example.com/\t\n ").as_deref(),
        Some("https://example.com/")
    );
}

#[test]
fn percent_encodes_by_component() {
    assert_eq!(
        href("http://example.com/a b").as_deref(),
        Some("http://example.com/a%20b")
    );
    assert_eq!(
        href("http://example.com/?a b").as_deref(),
        Some("http://example.com/?a%20b")
    );
    assert_eq!(
        href("http://example.com/#a b").as_deref(),
        Some("http://example.com/#a%20b")
    );
    assert_eq!(
        href("http://example.com/ü").as_deref(),
        Some("http://example.com/%C3%BC")
    );
    assert_eq!(
        href("http://example.com/?'").as_deref(),
        Some("http://example.com/?%27")
    );
    assert_eq!(
        href("foo://example.com/?'").as_deref(),
        Some("foo://example.com/?'")
    );
}

#[test]
fn resolves_relative_references() {
    let base = "http://a/b/c/d;p?q";
    for (input, expected) in [
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g/"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../../../g", "http://a/g"),
        ("g;x=1/../y", "http://a/b/c/y"),
    ] {
        assert_eq!(resolve(input, base).as_deref(), Some(expected), "{input}");
    }
}

#[test]
fn parses_hosts() {
    let url = Url::parse("http://0x7f.1/", None).expect("IPv4 with hex and short form");
    assert_eq!(url.host(), Some(&Host::Ipv4(0x7f00_0001)));
    assert_eq!(url.host_string(), "127.0.0.1");
    assert!(Url::parse("http://192.168.0.257/", None).is_none());
    let url = Url::parse("http://[0:0::1]:8080/", None).expect("IPv6 with port");
    assert_eq!(url.host_string(), "[::1]");
    assert_eq!(url.port(), Some(8080));
    assert_eq!(
        href("https://bücher.de/").as_deref(),
        Some("https://xn--bcher-kva.de/")
    );
    assert_eq!(
        domain_to_ascii("EXAMPLE.com").as_deref(),
        Some("example.com")
    );
    assert!(Url::parse("http://exa mple.com/", None).is_none());
    assert!(Url::parse("https://", None).is_none());
    assert!(Url::parse("http://example.com:65536/", None).is_none());
}

#[test]
fn keeps_credentials_and_non_special_schemes() {
    let url = Url::parse("https://user:pa%20ss@example.com/", None).expect("credentials");
    assert_eq!(url.username(), "user");
    assert_eq!(url.password(), "pa%20ss");
    assert_eq!(url.href(), "https://user:pa%20ss@example.com/");
    let url = Url::parse("mailto:someone@example.com", None).expect("opaque path");
    assert!(url.has_opaque_path());
    assert!(url.host().is_none());
    assert_eq!(url.href(), "mailto:someone@example.com");
    assert_eq!(
        href("file:///C:/dir/file").as_deref(),
        Some("file:///C:/dir/file")
    );
    assert_eq!(href("file://localhost/etc").as_deref(), Some("file:///etc"));
}

#[test]
fn rejects_relative_input_without_a_base() {
    assert!(Url::parse("/path", None).is_none());
    assert!(Url::parse("example.com", None).is_none());
    let opaque = Url::parse("data:text/plain,hi", None).expect("opaque base");
    assert!(Url::parse("x", Some(&opaque)).is_none());
    assert_eq!(
        Url::parse("#frag", Some(&opaque))
            .map(|url| url.href())
            .as_deref(),
        Some("data:text/plain,hi#frag")
    );
}

#[test]
fn setters_follow_the_url_api() {
    let mut url = Url::parse("https://example.com/a?b#c", None).expect("parses");
    url.set(Setter::Port, "8443");
    url.set(Setter::Pathname, "/x y");
    url.set(Setter::Search, "q=1");
    url.set(Setter::Hash, "");
    assert_eq!(url.href(), "https://example.com:8443/x%20y?q=1");
    url.set(Setter::Port, "443");
    assert_eq!(url.port(), None);
    url.set(Setter::Protocol, "foo");
    assert_eq!(url.scheme(), "https");
    url.set(Setter::Protocol, "http");
    assert_eq!(url.href(), "http://example.com/x%20y?q=1");
    url.set(Setter::Hostname, "EXAMPLE.org");
    assert_eq!(url.host_string(), "example.org");
}

#[test]
fn serialization_reparses_to_itself() {
    for input in [
        "http://example.com/a/b?c#d",
        "web+demo:/.//not-a-host/",
        "non-spec:/.//p",
        "https://[2001:db8::1]/",
        "blob:https://example.com/uuid",
    ] {
        let first = href(input).expect(input);
        assert_eq!(href(&first).as_deref(), Some(first.as_str()), "{input}");
    }
}

#[test]
fn blob_urls_are_reachable_from_their_own_origin_only() {
    use crate::bytes::blob_url_reachable;
    let blob = b"blob:https://a.example/9b2c";
    assert!(blob_url_reachable(blob, b"https://a.example/page"));
    assert!(blob_url_reachable(blob, b"https://A.example:443/other"));
    assert!(blob_url_reachable(blob, b"blob:https://a.example/other"));
    assert!(!blob_url_reachable(blob, b"https://b.example/page"));
    assert!(!blob_url_reachable(blob, b"http://a.example/page"));
    assert!(!blob_url_reachable(blob, b"https://sub.a.example/page"));
    assert!(!blob_url_reachable(blob, b""));
    assert!(blob_url_reachable(
        b"blob:null/9b2c",
        b"file:///tmp/page.html"
    ));
    assert!(!blob_url_reachable(
        b"blob:null/9b2c",
        b"https://a.example/page"
    ));
    assert!(!blob_url_reachable(
        b"https://a.example/9b2c",
        b"https://a.example/page"
    ));
}
