//! Southstar — unit tests for the extension host's path helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::{basename, build_filename};

#[cfg(not(windows))]
#[test]
fn build_filename_matches_glib() {
    assert_eq!(build_filename(&[b"/a", b"b"]), b"/a/b");
    assert_eq!(build_filename(&[b"/a/", b"/b/"]), b"/a/b/");
    assert_eq!(
        build_filename(&[b"a", b"", b"/etc/passwd"]),
        b"a/etc/passwd"
    );
    assert_eq!(build_filename(&[b"//a//", b"//", b"b"]), b"//a/b");
    assert_eq!(build_filename(&[b"/", b"x"]), b"/x");
    assert_eq!(build_filename(&[b"///"]), b"///");
    assert_eq!(build_filename(&[b"", b""]), b"");
    assert_eq!(build_filename(&[b"a", b"b//"]), b"a/b//");
}

#[cfg(windows)]
#[test]
fn build_filename_matches_glib() {
    assert_eq!(build_filename(&[b"C:\\a", b"b"]), b"C:\\a\\b");
    assert_eq!(build_filename(&[b"C:/a/", b"b"]), b"C:/a/b");
    assert_eq!(build_filename(&[b"a", b"/etc/passwd"]), b"a/etc/passwd");
    assert_eq!(build_filename(&[b"a\\", b"b/"]), b"a/b/");
}

#[test]
fn basename_matches_glib() {
    assert_eq!(basename(b""), b".");
    assert_eq!(basename(b"/a/b/"), b"b");
    assert_eq!(basename(b"abc"), b"abc");
    assert_eq!(basename(b"/a/bc"), b"bc");
    #[cfg(not(windows))]
    assert_eq!(basename(b"///"), b"/");
    #[cfg(windows)]
    {
        assert_eq!(basename(b"///"), b"\\");
        assert_eq!(basename(b"C:\\"), b"\\");
        assert_eq!(basename(b"C:x"), b"x");
        assert_eq!(basename(b"C:\\dir\\x"), b"x");
    }
}
