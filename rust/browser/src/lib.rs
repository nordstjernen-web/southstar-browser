//! Southstar — the embedding API's page lifecycle: building a page, relayout, image loading, settling, script callbacks and queries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod build;
mod callbacks;
mod ffi;
mod images;
mod open;
mod page;
mod query;
mod render;
mod scroll;
mod settle;
