//! Southstar — the depth of the nested main loops blocking fetches are pumping, which script and image work consult before re-entering the page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::atomic::{AtomicI32, Ordering};

static DEPTH: AtomicI32 = AtomicI32::new(0);

pub fn in_blocking_fetch() -> bool {
    DEPTH.load(Ordering::Relaxed) > 0
}

pub struct BlockingFetch(());

impl BlockingFetch {
    pub fn enter() -> BlockingFetch {
        DEPTH.fetch_add(1, Ordering::Relaxed);
        BlockingFetch(())
    }
}

impl Drop for BlockingFetch {
    fn drop(&mut self) {
        DEPTH.fetch_sub(1, Ordering::Relaxed);
    }
}
