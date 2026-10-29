//! Southstar — matching a form control's value against its pattern attribute the way the HTML Standard compiles it: a whole-value JavaScript regular expression with the v flag.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use regress::Regex;

pub fn pattern_matches(pattern: &[u8], value: &[u8]) -> Option<bool> {
    let pattern = core::str::from_utf8(pattern).ok()?;
    let regex = Regex::with_flags(&format!("^(?:{pattern})$"), "v").ok()?;
    Some(regex.find(&String::from_utf8_lossy(value)).is_some())
}
