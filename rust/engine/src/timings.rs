//! Southstar — the resource timings recorded for style sheet fetches until the page's script layer collects them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;
use std::sync::Mutex;

use southstar_net::Response;

use crate::fetch::CssFetch;

const TIMING_CAP: usize = 256;

pub struct ResourceTiming {
    top_url: CString,
    pub url: CString,
    pub initiator: &'static CStr,
    pub start_us: i64,
    pub end_us: i64,
    pub response: Response,
    pub render_blocking: bool,
    pub in_frame: bool,
}

static TIMINGS: Mutex<Vec<ResourceTiming>> = Mutex::new(Vec::new());

pub(crate) fn record_timing(f: &CssFetch, start_us: i64, end_us: i64, response: Response) {
    let Some(top_url) = f.top_url else {
        return;
    };
    let timing = ResourceTiming {
        top_url: top_url.to_owned(),
        url: f.url.to_owned(),
        initiator: f.initiator,
        start_us,
        end_us,
        response,
        render_blocking: f.render_blocking,
        in_frame: f.in_frame,
    };
    let Ok(mut timings) = TIMINGS.lock() else {
        return;
    };
    if timings.len() >= TIMING_CAP {
        timings.remove(0);
    }
    timings.push(timing);
}

pub fn take_resource_timings(top_url: Option<&CStr>) -> Vec<ResourceTiming> {
    let Some(top_url) = top_url else {
        return Vec::new();
    };
    let Ok(mut timings) = TIMINGS.lock() else {
        return Vec::new();
    };
    let (taken, kept) = timings
        .drain(..)
        .partition(|timing| timing.top_url.as_c_str() == top_url);
    *timings = kept;
    taken
}
