//! Southstar — the handle a page owns its animation engine through and lends, as a raw pointer, to the crates that sample it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use core::ptr::NonNull;

use crate::engine::Anim;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AnimHandle(NonNull<Anim>);

impl AnimHandle {
    pub fn create() -> AnimHandle {
        AnimHandle(NonNull::from(Box::leak(Box::new(Anim::new()))))
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// # Safety
    ///
    /// No copy of this handle, and no pointer `raw()` returned, may be used afterwards.
    pub unsafe fn free(self) {
        // SAFETY: the engine was leaked from a Box by create(), and the caller guarantees nothing uses it again.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }

    fn with<R>(self, f: impl FnOnce(&mut Anim) -> R) -> R {
        // SAFETY: the engine stays allocated until free(), whose contract rules out later use, and no other reference to it is live while the page is on this thread calling in.
        f(unsafe { &mut *self.0.as_ptr() })
    }

    pub fn tick(self, now_us: i64) -> bool {
        self.with(|a| a.tick(now_us))
    }

    pub fn has_active(self) -> bool {
        self.with(|a| a.has_active())
    }

    pub fn needs_layout(self) -> bool {
        self.with(|a| a.needs_layout())
    }

    pub fn rebase(self, base_us: i64) {
        self.with(|a| a.rebase(base_us));
    }
}
