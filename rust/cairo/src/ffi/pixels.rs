//! Southstar — the per-thread record of which surfaces' pixels are lent out as slices, so a mutable slice never aliases another.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;

use super::RawSurface;

thread_local! {
    static LENT: RefCell<Vec<(usize, bool)>> = const { RefCell::new(Vec::new()) };
}

pub(super) struct PixelBorrow {
    surface: usize,
    exclusive: bool,
}

impl PixelBorrow {
    fn take(surface: *mut RawSurface, exclusive: bool) -> Option<PixelBorrow> {
        let surface = surface.addr();
        LENT.with_borrow_mut(|lent| {
            let busy = lent
                .iter()
                .any(|&(s, held_exclusive)| s == surface && (exclusive || held_exclusive));
            if busy {
                return None;
            }
            lent.push((surface, exclusive));
            Some(PixelBorrow { surface, exclusive })
        })
    }

    pub(super) fn shared(surface: *mut RawSurface) -> Option<PixelBorrow> {
        PixelBorrow::take(surface, false)
    }

    pub(super) fn exclusive(surface: *mut RawSurface) -> Option<PixelBorrow> {
        PixelBorrow::take(surface, true)
    }
}

impl Drop for PixelBorrow {
    fn drop(&mut self) {
        LENT.with_borrow_mut(|lent| {
            if let Some(i) = lent
                .iter()
                .rposition(|&entry| entry == (self.surface, self.exclusive))
            {
                lent.remove(i);
            }
        });
    }
}
