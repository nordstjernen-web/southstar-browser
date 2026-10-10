//! Southstar — painting the layout box tree with cairo and ns-pango, ported from paint.c section by section: so far page text setup, fonts, line heights and list markers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod ffi;
mod marker;
mod text;
mod util;
