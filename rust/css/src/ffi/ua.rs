//! Southstar — the user-agent style sheet: the default styles of HTML elements, plus the quirks-mode additions, each parsed once per styling thread and shared by every style pass on it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::sheet::Sheet;

const UA: &str = include_str!("../ua.css");
const UA_QUIRKS: &str = include_str!("../ua-quirks.css");

fn parse(text: &str) -> &'static Sheet {
    Box::leak(Sheet::parse(text.as_bytes()))
}

thread_local! {
    static STANDARD: &'static Sheet = parse(UA);
    static QUIRKS: &'static Sheet = parse(&[UA, UA_QUIRKS].concat());
}

pub(super) fn ua_sheet(quirks: bool) -> &'static Sheet {
    if quirks {
        QUIRKS.with(|sheet| *sheet)
    } else {
        STANDARD.with(|sheet| *sheet)
    }
}
