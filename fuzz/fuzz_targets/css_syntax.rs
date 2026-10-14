//! Southstar — fuzz target: runs the CSS component-value parser and the statement scanners over arbitrary bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let (components, _) = southstar_css_syntax::parse(data);
    drop(components);
    let (end, _) = southstar_css_syntax::scan(data, b";}");
    assert!(end <= data.len());
    let _ = southstar_css_syntax::is_self_contained(data);
});
