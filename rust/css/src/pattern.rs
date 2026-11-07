//! Southstar — matching a form control's value against its pattern attribute the way the HTML Standard compiles it: a whole-value JavaScript regular expression with the v flag, within a backtracking budget.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use regress::Regex;

const PATTERN_BACKTRACK_LIMIT: u64 = 100_000;

pub fn pattern_matches(pattern: &[u8], value: &[u8]) -> Option<bool> {
    let pattern = core::str::from_utf8(pattern).ok()?;
    let regex = Regex::with_flags(&format!("^(?:{pattern})$"), "v").ok()?;
    regex
        .find_with_backtrack_limit(&String::from_utf8_lossy(value), PATTERN_BACKTRACK_LIMIT)
        .ok()
        .map(|found| found.is_some())
}
