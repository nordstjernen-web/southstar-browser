//! Southstar — iframe browsing contexts and their realms: the realm cloner, frame realm and scope bootstraps, the frame table, sandbox flags, the frame owner element bindings, frame sources and load scheduling, and the helpers around a frame's scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod api;
mod bootstrap;
mod cloner;
mod exposed;
mod ffi;
mod globals;
pub mod links;
mod load;
mod origin;
pub mod owner;
mod page;
mod pristine;
mod queue;
mod realm;
mod restore;
mod sandbox;
mod schedule;
mod scripts;
mod submit;

pub use api::*;
pub use ffi::install_runtime_hooks;
