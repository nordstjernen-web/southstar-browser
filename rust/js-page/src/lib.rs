//! Southstar — installing a parsed document into the page's realm and running its scripts, and the page's JavaScript statistics.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod install;
mod interfaces;
mod run;
mod stats;

use southstar_dom::Node;

pub(crate) type Element = Node<'static>;
