//! Southstar — unit tests for the sandbox crate's pure helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::uuid_v4;

#[test]
fn uuid_v4_has_version_and_variant() {
    let uuid = uuid_v4().expect("csprng");
    let bytes = uuid.as_bytes();
    assert_eq!(bytes.len(), 36);
    for i in [8, 13, 18, 23] {
        assert_eq!(bytes[i], b'-');
    }
    assert_eq!(bytes[14], b'4');
    assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'));
    assert!(
        uuid.bytes()
            .all(|b| b == b'-' || b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_ne!(uuid_v4(), uuid_v4());
}
