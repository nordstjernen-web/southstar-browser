//! Southstar PSL — unit tests: the publicsuffix.org test cases (tests/test_psl.txt), IP literals and the public-suffix check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::*;

const PUBLICSUFFIX_ORG_CASES: [(&str, Option<&str>); 77] = [
    ("COM", None),
    ("example.COM", Some("example.com")),
    ("WwW.example.COM", Some("example.com")),
    (".com", None),
    (".example", None),
    (".example.com", None),
    (".example.example", None),
    ("example", None),
    ("example.example", Some("example.example")),
    ("b.example.example", Some("example.example")),
    ("a.b.example.example", Some("example.example")),
    ("biz", None),
    ("domain.biz", Some("domain.biz")),
    ("b.domain.biz", Some("domain.biz")),
    ("a.b.domain.biz", Some("domain.biz")),
    ("com", None),
    ("example.com", Some("example.com")),
    ("b.example.com", Some("example.com")),
    ("a.b.example.com", Some("example.com")),
    ("uk.com", None),
    ("example.uk.com", Some("example.uk.com")),
    ("b.example.uk.com", Some("example.uk.com")),
    ("a.b.example.uk.com", Some("example.uk.com")),
    ("test.ac", Some("test.ac")),
    ("mm", None),
    ("c.mm", None),
    ("b.c.mm", Some("b.c.mm")),
    ("a.b.c.mm", Some("b.c.mm")),
    ("jp", None),
    ("test.jp", Some("test.jp")),
    ("www.test.jp", Some("test.jp")),
    ("ac.jp", None),
    ("test.ac.jp", Some("test.ac.jp")),
    ("www.test.ac.jp", Some("test.ac.jp")),
    ("kyoto.jp", None),
    ("test.kyoto.jp", Some("test.kyoto.jp")),
    ("ide.kyoto.jp", None),
    ("b.ide.kyoto.jp", Some("b.ide.kyoto.jp")),
    ("a.b.ide.kyoto.jp", Some("b.ide.kyoto.jp")),
    ("c.kobe.jp", None),
    ("b.c.kobe.jp", Some("b.c.kobe.jp")),
    ("a.b.c.kobe.jp", Some("b.c.kobe.jp")),
    ("city.kobe.jp", Some("city.kobe.jp")),
    ("www.city.kobe.jp", Some("city.kobe.jp")),
    ("ck", None),
    ("test.ck", None),
    ("b.test.ck", Some("b.test.ck")),
    ("a.b.test.ck", Some("b.test.ck")),
    ("www.ck", Some("www.ck")),
    ("www.www.ck", Some("www.ck")),
    ("us", None),
    ("test.us", Some("test.us")),
    ("www.test.us", Some("test.us")),
    ("ak.us", None),
    ("test.ak.us", Some("test.ak.us")),
    ("www.test.ak.us", Some("test.ak.us")),
    ("k12.ak.us", None),
    ("test.k12.ak.us", Some("test.k12.ak.us")),
    ("www.test.k12.ak.us", Some("test.k12.ak.us")),
    ("食狮.com.cn", Some("食狮.com.cn")),
    ("食狮.公司.cn", Some("食狮.公司.cn")),
    ("www.食狮.公司.cn", Some("食狮.公司.cn")),
    ("shishi.公司.cn", Some("shishi.公司.cn")),
    ("公司.cn", None),
    ("食狮.中国", Some("食狮.中国")),
    ("www.食狮.中国", Some("食狮.中国")),
    ("shishi.中国", Some("shishi.中国")),
    ("中国", None),
    ("xn--85x722f.com.cn", Some("xn--85x722f.com.cn")),
    (
        "xn--85x722f.xn--55qx5d.cn",
        Some("xn--85x722f.xn--55qx5d.cn"),
    ),
    (
        "www.xn--85x722f.xn--55qx5d.cn",
        Some("xn--85x722f.xn--55qx5d.cn"),
    ),
    ("shishi.xn--55qx5d.cn", Some("shishi.xn--55qx5d.cn")),
    ("xn--55qx5d.cn", None),
    ("xn--85x722f.xn--fiqs8s", Some("xn--85x722f.xn--fiqs8s")),
    ("www.xn--85x722f.xn--fiqs8s", Some("xn--85x722f.xn--fiqs8s")),
    ("shishi.xn--fiqs8s", Some("shishi.xn--fiqs8s")),
    ("xn--fiqs8s", None),
];

#[test]
fn publicsuffix_org_test_cases() {
    for (input, expected) in PUBLICSUFFIX_ORG_CASES {
        let lower = input.to_lowercase();
        let found =
            registrable_domain(lower.as_bytes()).map(|d| String::from_utf8_lossy(d).into_owned());
        assert_eq!(found.as_deref(), expected, "registrable domain of {input}");
    }
}

#[test]
fn punycode_and_unicode_rules_both_match() {
    assert_eq!(
        registrable_domain("食狮.公司.cn".as_bytes()),
        Some("食狮.公司.cn".as_bytes())
    );
    assert_eq!(
        registrable_domain(b"xn--85x722f.xn--55qx5d.cn"),
        Some(&b"xn--85x722f.xn--55qx5d.cn"[..])
    );
    assert!(is_public_suffix(b"xn--55qx5d.cn"));
}

#[test]
fn ip_literals_have_no_registrable_domain() {
    assert_eq!(registrable_domain(b"192.168.1.10"), None);
    assert_eq!(registrable_domain(b"[::1]"), None);
    assert_eq!(registrable_domain(b"::1"), None);
}

#[test]
fn public_suffix_check_follows_rules_wildcards_and_exceptions() {
    assert!(is_public_suffix(b"com"));
    assert!(is_public_suffix(b"co.uk"));
    assert!(is_public_suffix(b"github.io"));
    assert!(is_public_suffix(b"unlisted-tld"));
    assert!(!is_public_suffix(b"example.com"));
    assert!(is_public_suffix(b"anything.ck"));
    assert!(!is_public_suffix(b"www.ck"));
    assert!(!is_public_suffix(b""));
    assert!(is_public_suffix(b"com."));
    assert_eq!(
        registrable_domain(b"www.example.com."),
        Some(&b"example.com"[..])
    );
}
