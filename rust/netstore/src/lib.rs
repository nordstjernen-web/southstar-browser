//! Southstar — the rules behind the network stack's stored state, over bytes and without I/O: cookie jars, the HSTS host list and HTTP dates.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

pub mod cookies;
pub mod hsts;
pub mod http_date;
#[cfg(test)]
mod tests;
mod text;

pub use text::strtoll;
