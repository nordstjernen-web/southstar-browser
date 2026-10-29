//! Southstar — character counting and the urlencoded query the form helpers append to.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_long};

pub struct Query {
    encoded: Vec<u8>,
    first: bool,
}

impl Query {
    pub fn new() -> Self {
        Self {
            encoded: Vec::new(),
            first: true,
        }
    }

    pub fn append(&mut self, name: &CStr, value: Option<&CStr>) {
        southstar_net::urlencoded_append_pair(
            &mut self.encoded,
            &mut self.first,
            name.to_bytes(),
            value.map(CStr::to_bytes),
        );
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.encoded
    }
}

pub fn utf8_strlen(s: &CStr) -> c_long {
    s.to_bytes()
        .iter()
        .filter(|byte| **byte & 0xC0 != 0x80)
        .count() as c_long
}
