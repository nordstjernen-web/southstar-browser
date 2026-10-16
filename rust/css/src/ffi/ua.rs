//! Southstar — the user-agent style sheet: the default styles of HTML elements, plus the quirks-mode additions, each parsed once and shared by every style pass.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use super::sheet::{RawSheet, ns_css_stylesheet_parse};

const UA: &str = include_str!("../ua.css");
const UA_QUIRKS: &str = include_str!("../ua-quirks.css");

struct Sheet(*const RawSheet);

unsafe impl Send for Sheet {}
unsafe impl Sync for Sheet {}

fn parse(text: &str) -> Sheet {
    Sheet(unsafe { ns_css_stylesheet_parse(text.as_ptr().cast(), text.len() as isize) }.cast())
}

pub(super) fn ua_sheet(quirks: bool) -> *const RawSheet {
    static STANDARD: OnceLock<Sheet> = OnceLock::new();
    static QUIRKS: OnceLock<Sheet> = OnceLock::new();
    if quirks {
        QUIRKS.get_or_init(|| parse(&[UA, UA_QUIRKS].concat())).0
    } else {
        STANDARD.get_or_init(|| parse(UA)).0
    }
}
