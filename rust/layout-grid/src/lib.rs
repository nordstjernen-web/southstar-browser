//! Southstar — CSS grid layout ported from layout.c: track sizing, line resolution and auto-placement, alignment, subgrids, intrinsic widths and grid-positioned absolute boxes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod abs;
mod ffi;
mod intrinsic;
mod layout;
mod lines;
mod place;
mod text;
mod tracks;
