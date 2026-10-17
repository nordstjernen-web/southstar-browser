//! Southstar — CSS transitions and @keyframes animations: observing style changes, running and sampling animations, their events and the Web Animations hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod engine;
mod ffi;
mod handle;
mod props;
mod timing;

pub use engine::ScriptTiming as AnimTiming;
pub use handle::{AnimEvent, AnimHandle, AnimInfo, AnimKeyframe, AnimatedColor};
