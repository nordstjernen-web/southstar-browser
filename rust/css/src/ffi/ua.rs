//! Southstar — the user-agent style sheet: the default styles of HTML elements, plus the quirks-mode additions, each parsed once and shared by every style pass.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use super::sheet::{OwnedSheet, SheetData};

const UA: &str = include_str!("../ua.css");
const UA_QUIRKS: &str = include_str!("../ua-quirks.css");

struct Sheet(*const SheetData);

unsafe impl Send for Sheet {}
unsafe impl Sync for Sheet {}

fn parse(text: &str) -> Sheet {
    Sheet(OwnedSheet::parse(text.as_bytes()).into_raw())
}

pub(super) fn ua_sheet(quirks: bool) -> &'static SheetData {
    static STANDARD: OnceLock<Sheet> = OnceLock::new();
    static QUIRKS: OnceLock<Sheet> = OnceLock::new();
    let sheet = if quirks {
        QUIRKS.get_or_init(|| parse(&[UA, UA_QUIRKS].concat()))
    } else {
        STANDARD.get_or_init(|| parse(UA))
    };
    // SAFETY: the user-agent sheets are parsed once and never freed.
    unsafe { &*sheet.0 }
}
