//! Southstar — iframe browsing contexts and their realms: the realm cloner, frame realm and scope bootstraps, the frame table, sandbox flags, frame load queues and the helpers around a frame's scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod bootstrap;
mod cloner;
mod exposed;
mod ffi;
mod page;
mod queue;
mod realm;
mod restore;
mod sandbox;
