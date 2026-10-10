//! Southstar — hit testing ported from layout.c: the box, element, form control, link, scroll container and scrollbar under a point of the laid-out page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod geometry;
mod stack;
mod tree;
mod walks;
