//! Southstar — unit tests for Content-Security-Policy parsing, source matching, fallbacks, nonces, hashes and strict-dynamic.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::{BASE_URI, Csp, FRAME_ANCESTORS, IMG, SCRIPT, STYLE, WORKER};

const PAGE: &str = "https://example.com/app/index.html";

fn csp(header: &str) -> Csp {
    Csp::parse(header.as_bytes()).expect("policy parses")
}

fn allows(policy: &Csp, kind: usize, url: &str) -> bool {
    let url = CString::new(url).expect("no NUL");
    let page = CString::new(PAGE).expect("no NUL");
    policy.allows(kind, &url, Some(&page), None, true)
}

#[test]
fn self_matches_the_document_origin_only() {
    let policy = csp("default-src 'self'");
    assert!(allows(&policy, SCRIPT, "https://example.com/a.js"));
    assert!(!allows(&policy, SCRIPT, "https://cdn.example.com/a.js"));
    assert!(!allows(&policy, IMG, "http://example.com/a.png"));
}

#[test]
fn falls_back_to_default_src() {
    let policy = csp("default-src 'none'; img-src https:; script-src 'self'");
    assert!(allows(&policy, IMG, "https://images.test/x.png"));
    assert!(!allows(&policy, IMG, "http://images.test/x.png"));
    assert!(!allows(&policy, STYLE, "https://example.com/a.css"));
    assert!(allows(&policy, WORKER, "https://example.com/w.js"));
    assert!(allows(&policy, BASE_URI, "https://anywhere.test/"));
}

#[test]
fn matches_hosts_wildcards_ports_and_paths() {
    let policy =
        csp("script-src *.cdn.test https://static.test:8443/lib/ https://exact.test/one.js");
    assert!(allows(&policy, SCRIPT, "https://a.cdn.test/x.js"));
    assert!(!allows(&policy, SCRIPT, "https://cdn.test/x.js"));
    assert!(allows(&policy, SCRIPT, "https://static.test:8443/lib/x.js"));
    assert!(!allows(&policy, SCRIPT, "https://static.test/lib/x.js"));
    assert!(!allows(
        &policy,
        SCRIPT,
        "https://static.test:8443/other/x.js"
    ));
    assert!(allows(&policy, SCRIPT, "https://exact.test/one.js"));
    assert!(!allows(&policy, SCRIPT, "https://exact.test/two.js"));
}

#[test]
fn upgrades_a_schemeless_source_from_http_to_https() {
    let policy = csp("img-src images.test");
    let url = CString::new("https://images.test/x.png").expect("no NUL");
    let http_page = CString::new("http://example.com/").expect("no NUL");
    assert!(policy.allows(IMG, &url, Some(&http_page), None, true));
    let insecure = CString::new("http://images.test/x.png").expect("no NUL");
    let https_page = CString::new(PAGE).expect("no NUL");
    assert!(!policy.allows(IMG, &insecure, Some(&https_page), None, true));
}

#[test]
fn every_policy_in_a_list_must_allow() {
    let policy = csp("script-src https:, script-src 'self'");
    assert!(allows(&policy, SCRIPT, "https://example.com/a.js"));
    assert!(!allows(&policy, SCRIPT, "https://other.test/a.js"));
    let mut merged = csp("img-src *");
    merged.merge(&mut csp("img-src 'none'"));
    assert!(!allows(&merged, IMG, "https://example.com/x.png"));
}

#[test]
fn nonces_hashes_and_unsafe_inline() {
    let policy =
        csp("script-src 'nonce-abc123' 'sha256-bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI='");
    assert!(policy.inline_script_allowed(b"anything", Some(b"abc123")));
    assert!(!policy.inline_script_allowed(b"anything", Some(b"wrong")));
    assert!(policy.inline_script_allowed(b"alert(1)", None));
    assert!(!policy.inline_script_allowed(b"alert(2)", None));
    assert!(!policy.inline_event_handler_allowed());

    let inline = csp("script-src 'unsafe-inline'");
    assert!(inline.inline_script_allowed(b"x()", None));
    assert!(inline.inline_event_handler_allowed());
    let ignored = csp("script-src 'unsafe-inline' 'nonce-n'");
    assert!(!ignored.inline_script_allowed(b"x()", None));
    assert!(csp("img-src 'none'").inline_script_allowed(b"x()", None));
}

#[test]
fn strict_dynamic_trusts_only_non_parser_inserted_scripts() {
    let policy = csp("script-src 'strict-dynamic' 'nonce-n' https://example.com");
    let url = CString::new("https://example.com/a.js").expect("no NUL");
    let page = CString::new(PAGE).expect("no NUL");
    assert!(!policy.allows(SCRIPT, &url, Some(&page), None, true));
    assert!(policy.allows(SCRIPT, &url, Some(&page), None, false));
    assert!(policy.allows(SCRIPT, &url, Some(&page), Some(b"n"), true));
}

#[test]
fn frame_ancestors_and_empty_headers() {
    let policy = csp("frame-ancestors 'self' https://partner.test");
    assert!(allows(
        &policy,
        FRAME_ANCESTORS,
        "https://partner.test/embed"
    ));
    assert!(!allows(&policy, FRAME_ANCESTORS, "https://evil.test/"));
    assert!(Csp::parse(b"  ,  ").is_none());
}

#[test]
fn keywords_match_without_regard_to_case() {
    let policy = csp("script-src 'SELF'; img-src 'NONE'");
    assert!(allows(&policy, SCRIPT, "https://example.com/a.js"));
    assert!(!allows(&policy, IMG, "https://example.com/a.png"));
    let inline = csp("script-src 'Unsafe-Inline'");
    assert!(inline.inline_script_allowed(b"x()", None));
    let nonce = csp("script-src 'NONCE-AbC'");
    assert!(nonce.inline_script_allowed(b"x()", Some(b"AbC")));
    assert!(!nonce.inline_script_allowed(b"x()", Some(b"abc")));
}

#[test]
fn a_source_with_an_empty_host_matches_nothing() {
    let policy = csp("img-src https://");
    assert!(!allows(&policy, IMG, "https://example.com/a.png"));
}

#[test]
fn eval_needs_unsafe_eval() {
    assert!(csp("img-src 'self'").eval_allowed());
    assert!(!csp("default-src 'self'").eval_allowed());
    assert!(!csp("script-src 'self' 'unsafe-inline'").eval_allowed());
    assert!(csp("script-src 'self' 'UNSAFE-EVAL'").eval_allowed());
    assert!(csp("default-src 'none'; script-src 'unsafe-eval'").eval_allowed());
}
