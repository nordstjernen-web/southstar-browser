//! Southstar text — tab stop arrays: positions, alignments and decimal points of a paragraph's tab stops.
//! Copyright 2000 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::types::TabAlign;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tab {
    location: i32,
    alignment: TabAlign,
    decimal_point: u32,
}

const DEFAULT_TAB: Tab = Tab {
    location: 0,
    alignment: TabAlign::Left,
    decimal_point: 0,
};

#[derive(Debug)]
pub struct TabArray {
    size: i32,
    tabs: Vec<Tab>,
    positions_in_pixels: bool,
}

impl Clone for TabArray {
    fn clone(&self) -> TabArray {
        let mut copy = TabArray::new(self.len(), self.positions_in_pixels);
        let size = self.len();
        copy.tabs[..size].copy_from_slice(&self.tabs[..size]);
        copy
    }
}

impl TabArray {
    pub fn new(initial_size: usize, positions_in_pixels: bool) -> TabArray {
        TabArray {
            size: initial_size as i32,
            tabs: vec![DEFAULT_TAB; initial_size],
            positions_in_pixels,
        }
    }

    fn len(&self) -> usize {
        self.size.max(0) as usize
    }

    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn resize(&mut self, new_size: i32) {
        let allocated = self.tabs.len() as i32;
        if new_size > allocated {
            let mut grown = if allocated == 0 { 2 } else { allocated };
            while new_size > grown {
                grown *= 2;
            }
            self.tabs.resize(grown as usize, DEFAULT_TAB);
        }
        self.size = new_size;
    }

    pub fn set_tab(&mut self, tab_index: i32, alignment: TabAlign, location: i32) {
        if tab_index < 0 || location < 0 {
            return;
        }
        if tab_index >= self.size {
            self.resize(tab_index + 1);
        }
        let tab = &mut self.tabs[tab_index as usize];
        tab.alignment = alignment;
        tab.location = location;
    }

    pub fn tab(&self, tab_index: i32) -> Option<(TabAlign, i32)> {
        if tab_index >= self.size || tab_index < 0 {
            return None;
        }
        let tab = &self.tabs[tab_index as usize];
        Some((tab.alignment, tab.location))
    }

    pub fn positions_in_pixels(&self) -> bool {
        self.positions_in_pixels
    }

    pub fn decimal_point(&self, tab_index: i32) -> u32 {
        if tab_index >= self.size || tab_index < 0 {
            return 0;
        }
        self.tabs[tab_index as usize].decimal_point
    }

    pub fn sort(&mut self) {
        let size = self.len();
        self.tabs[..size].sort_by_key(|tab| tab.location);
    }
}
