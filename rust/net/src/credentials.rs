//! Southstar — the cookies a connection outside the fetch path (a WebSocket handshake) sends and stores, under the same policy, partition and SameSite context as a subresource fetch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_netstore::cookies::Context;

use crate::{cookies, request, storage, url};

const COOKIE_FIRST_PARTY: c_int = 1;
const COOKIE_NEVER: c_int = 2;

pub(crate) fn cookies_allowed(target: &[u8], top_url: Option<&[u8]>, partition: &[u8]) -> bool {
    let policy = southstar_config::get().map_or(COOKIE_FIRST_PARTY, |c| c.cookie_policy);
    if policy == COOKIE_NEVER || partition.is_empty() {
        return false;
    }
    let third_party = top_url.is_some_and(|top| !url::is_same_site(target, top));
    !(policy == COOKIE_FIRST_PARTY && third_party)
}

pub struct Credentials {
    target: Vec<u8>,
    jar: Vec<u8>,
    js_jar: Option<Vec<u8>>,
    context: Context,
}

pub fn for_subresource(target: &[u8], top_url: Option<&[u8]>) -> Option<Credentials> {
    let effective_top = top_url.unwrap_or(target);
    let partition = request::partition_of(
        url::site_from(effective_top).as_deref(),
        url::origin_from(effective_top).as_deref(),
    );
    if !cookies_allowed(target, top_url, &partition) {
        return None;
    }
    Some(Credentials {
        target: target.to_vec(),
        jar: storage::cookie_jar_path(Some(&partition), false)?,
        js_jar: storage::cookie_jar_path(Some(&partition), true),
        context: request::cookie_context(false, None, top_url, target),
    })
}

impl Credentials {
    pub fn cookie_header(&self) -> Option<Vec<u8>> {
        let jars = [Some(self.jar.clone()), self.js_jar.clone()];
        cookies::collect_in(&self.target, &jars, self.context)
    }

    pub fn store(&self, set_cookie: &[u8]) {
        cookies::store_in(&self.target, set_cookie, &self.jar, self.context);
    }
}
