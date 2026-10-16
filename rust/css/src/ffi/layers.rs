//! Southstar — cascade layer order: the rank table the gather loop looks each rule's layer up in, built from a style pass's sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use std::collections::HashMap;

use super::selector_view::SheetRef;
use crate::layers;

pub(super) type LayerRanks = HashMap<Vec<u8>, c_int>;

pub(super) fn layer_ranks_build(sheets: &[SheetRef<'_>]) -> LayerRanks {
    layers::ranks(sheets.iter().flat_map(|sheet| sheet.layer_names()))
        .into_iter()
        .map(|(name, rank)| (name.to_vec(), rank as c_int))
        .collect()
}
