//! Southstar — unit tests for the request shaping rules: CORS redirects and the tainted origin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::request::{
    CORS_MODE_LINE, cors_redirect_needs_preflight, cors_safelisted, redirect_drops_body,
    redirect_taints_origin, serialized_origin, taint_origin,
};

fn lines(items: &[&[u8]]) -> Vec<Vec<u8>> {
    items.iter().map(|l| l.to_vec()).collect()
}

const PAGE: &[u8] = b"http://attacker.localtest.me:8765/page";
const SAME: &[u8] = b"http://attacker.localtest.me:8765/r";
const OTHER: &[u8] = b"http://127.0.0.1:8766/victim";

#[test]
fn safelisted_requests() {
    let internal = lines(&[b"X-ND-Credentials: include", b"X-ND-Document: http://a/"]);
    assert!(cors_safelisted(b"GET", None, &internal));
    assert!(cors_safelisted(
        b"post",
        Some(b"text/plain;charset=UTF-8"),
        &internal
    ));
    assert!(cors_safelisted(
        b"GET",
        None,
        &lines(&[
            b"Accept: */*",
            b"Content-Type: multipart/form-data; boundary=x"
        ])
    ));
    assert!(!cors_safelisted(b"DELETE", None, &internal));
    assert!(!cors_safelisted(
        b"POST",
        Some(b"application/json"),
        &internal
    ));
    assert!(!cors_safelisted(b"GET", None, &lines(&[b"X-Custom: 1"])));
    assert!(!cors_safelisted(
        b"GET",
        None,
        &lines(&[b"Content-Type: application/json"])
    ));
}

#[test]
fn cross_origin_redirect_of_unsafe_cors_request_is_refused() {
    let cors = lines(&[CORS_MODE_LINE, b"X-Custom: 1"]);
    assert!(cors_redirect_needs_preflight(
        &cors, b"DELETE", None, SAME, OTHER
    ));
    assert!(cors_redirect_needs_preflight(
        &cors, b"GET", None, SAME, OTHER
    ));
    assert!(cors_redirect_needs_preflight(
        &cors, b"GET", None, OTHER, SAME
    ));
    let json = lines(&[CORS_MODE_LINE]);
    assert!(cors_redirect_needs_preflight(
        &json,
        b"PUT",
        Some(b"application/json"),
        SAME,
        OTHER
    ));
}

#[test]
fn same_origin_and_simple_redirects_follow() {
    let cors = lines(&[CORS_MODE_LINE, b"X-Custom: 1"]);
    assert!(!cors_redirect_needs_preflight(
        &cors,
        b"DELETE",
        None,
        SAME,
        b"http://attacker.localtest.me:8765/elsewhere"
    ));
    let simple = lines(&[CORS_MODE_LINE, b"Accept: */*"]);
    assert!(!cors_redirect_needs_preflight(
        &simple, b"GET", None, SAME, OTHER
    ));
    assert!(!cors_redirect_needs_preflight(
        &simple,
        b"POST",
        Some(b"text/plain"),
        SAME,
        OTHER
    ));
    let no_cors = lines(&[b"X-Custom: 1"]);
    assert!(!cors_redirect_needs_preflight(
        &no_cors, b"DELETE", None, SAME, OTHER
    ));
}

#[test]
fn see_other_rewrite_drops_unsafe_method_before_the_check() {
    assert!(redirect_drops_body(303, b"PUT"));
    assert!(redirect_drops_body(302, b"POST"));
    assert!(!redirect_drops_body(307, b"POST"));
    let cors = lines(&[CORS_MODE_LINE]);
    assert!(!cors_redirect_needs_preflight(
        &cors, b"GET", None, SAME, OTHER
    ));
}

#[test]
fn origin_is_tainted_after_leaving_a_cross_origin_url() {
    assert!(!redirect_taints_origin(Some(PAGE), SAME, OTHER));
    assert!(redirect_taints_origin(
        Some(PAGE),
        OTHER,
        b"http://example.localtest.me:9000/"
    ));
    assert!(redirect_taints_origin(Some(PAGE), OTHER, SAME));
    assert!(!redirect_taints_origin(
        Some(PAGE),
        OTHER,
        b"http://127.0.0.1:8766/again"
    ));
}

#[test]
fn tainted_origin_serializes_as_null() {
    let mut headers = lines(&[CORS_MODE_LINE]);
    assert_eq!(
        serialized_origin(b"http://attacker.localtest.me:8765", &headers),
        b"http://attacker.localtest.me:8765"
    );
    taint_origin(&mut headers);
    taint_origin(&mut headers);
    assert_eq!(headers.len(), 2);
    assert_eq!(
        serialized_origin(b"http://attacker.localtest.me:8765", &headers),
        b"null"
    );
    assert_eq!(serialized_origin(b"", &headers), b"null");
    let mut no_cors = Vec::new();
    taint_origin(&mut no_cors);
    assert_eq!(serialized_origin(b"", &no_cors), b"");
}
