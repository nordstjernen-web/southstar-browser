//! Southstar — the engine-independent core of Intl: locale identifiers, CLDR locale data, collation and special casing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

pub mod casing;
mod cldr;
pub mod cldr_index;
pub mod collation;
pub mod tag;
#[cfg(test)]
mod tests;
