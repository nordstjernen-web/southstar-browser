//! Southstar — which pages count as browser chrome when they ask for about: and view-source: documents.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub fn request_from_chrome(top_url: Option<&[u8]>) -> bool {
    let Some(top_url) = top_url.filter(|t| !t.is_empty()) else {
        return true;
    };
    let Some(page) = top_url.strip_prefix(b"about:") else {
        return false;
    };
    let page = &page[..page
        .iter()
        .position(|&c| c == b'?' || c == b'#')
        .unwrap_or(page.len())];
    !page.eq_ignore_ascii_case(b"blank") && !page.eq_ignore_ascii_case(b"srcdoc")
}
