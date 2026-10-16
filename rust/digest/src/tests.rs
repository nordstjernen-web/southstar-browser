//! Southstar — tests for the SHA digests and the hex and base64 encoders.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::*;

#[test]
fn sha256_of_known_inputs() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn incremental_hash_matches_one_shot() {
    let mut hasher = Hasher::new(Algorithm::Sha256);
    hasher.update(b"a");
    hasher.update(b"bc");
    assert_eq!(hasher.finish(), sha256(b"abc"));
}

#[test]
fn digest_lengths() {
    assert_eq!(digest(Algorithm::Sha1, b"abc").len(), 20);
    assert_eq!(digest(Algorithm::Sha384, b"abc").len(), 48);
    assert_eq!(digest(Algorithm::Sha512, b"abc").len(), 64);
    assert_eq!(
        hex(&digest(Algorithm::Sha1, b"abc")),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
}

#[test]
fn base64_pads_to_four() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
}

#[test]
fn sri_style_digest() {
    assert_eq!(
        base64(&digest(Algorithm::Sha256, b"alert('Hello, world.');")),
        "qznLcsROx4GACP2dm0UCKCzCG+HiZ1guq6ZZDob/Tng="
    );
}
