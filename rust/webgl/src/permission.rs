//! Southstar — the per-origin WebGL decisions and the origin waiting for the user's answer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

use crate::ffi::host;

#[derive(Default)]
struct Decisions {
    allowed: HashMap<Vec<u8>, bool>,
    pending: Option<Vec<u8>>,
}

static DECISIONS: LazyLock<Mutex<Decisions>> = LazyLock::new(Mutex::default);

fn decisions() -> MutexGuard<'static, Decisions> {
    DECISIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn take_pending_origin() -> Option<Vec<u8>> {
    decisions().pending.take()
}

pub(crate) fn set_decision(origin: &[u8], allow: bool) {
    if origin.is_empty() {
        return;
    }
    decisions().allowed.insert(origin.to_vec(), allow);
    if allow {
        southstar_config::enable_webgl();
    }
    let mut state = decisions();
    if state.pending.as_deref() == Some(origin) {
        state.pending = None;
    }
}

pub(crate) fn permitted(js: usize) -> bool {
    let (url, origin) = host::page_url_and_origin(js);
    let origin = origin
        .filter(|origin| !origin.is_empty())
        .unwrap_or_else(|| match url {
            Some(url) if !url.is_empty() => url,
            _ => b"this page".to_vec(),
        });
    if !southstar_config::webgl_enabled() {
        return false;
    }
    let mut state = decisions();
    if let Some(&allowed) = state.allowed.get(&origin) {
        return allowed;
    }
    state.allowed.insert(origin.clone(), true);
    state.pending = Some(origin);
    true
}
