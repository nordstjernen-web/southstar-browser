//! Southstar — box-tree construction ported from layout.c: inline runs collected from the DOM, with their text, links, attributes and atomic inline boxes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod atomic;
mod collect;
mod controls;
mod ffi;
mod run;
