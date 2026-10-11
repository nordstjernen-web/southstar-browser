//! Southstar — box-tree queries ported from layout.c: find-in-page matches, CSS scroll snapping, paint bounds and subtree extents, inline element rectangles and the boxes, videos and frame viewports a laid-out page holds.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod collect;
mod extent;
mod ffi;
mod find;
mod snap;
