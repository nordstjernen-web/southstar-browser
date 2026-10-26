//! Southstar — tests for the per-page microphone grants that gate Web Audio's access to captured samples.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{grant, granted, revoke, revoke_all};

#[test]
fn a_page_without_a_grant_cannot_read_the_microphone() {
    let (page, other) = (0x1000, 0x2000);
    assert!(!granted(page));
    grant(page);
    assert!(granted(page));
    assert!(!granted(other));
    assert!(!revoke(other));
    assert!(revoke(page));
    assert!(!granted(page));
    assert!(!revoke(page));
}

#[test]
fn grants_count_and_page_teardown_clears_them() {
    let page = 0x3000;
    grant(page);
    grant(page);
    assert!(revoke(page));
    assert!(granted(page));
    grant(page);
    assert_eq!(revoke_all(page), 2);
    assert!(!granted(page));
    assert_eq!(revoke_all(page), 0);
}
