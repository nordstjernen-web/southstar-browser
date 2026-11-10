//! Southstar — which vertex array holds which WebGL buffer, so a deleted buffer's GL name is not handed out again while another vertex array still draws from it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use crate::ffi::gl;

pub(crate) const ELEMENT_SLOT: u32 = u32::MAX;

#[derive(Default)]
pub(crate) struct Buffers {
    live: HashSet<u32>,
    vertex_arrays: HashSet<u32>,
    current: u32,
    slots: HashMap<u32, HashMap<u32, u32>>,
    holders: HashMap<u32, u32>,
    retired: HashSet<u32>,
    held: HashSet<u32>,
}

impl Buffers {
    pub(crate) fn create(&mut self) -> u32 {
        loop {
            let name = gl::gen_buffer();
            if name == 0 {
                return 0;
            }
            if !self.retired.contains(&name) {
                self.live.insert(name);
                return name;
            }
            self.held.insert(name);
        }
    }

    pub(crate) fn is_live(&self, name: u32) -> bool {
        self.live.contains(&name)
    }

    pub(crate) fn create_vertex_array(&mut self) -> u32 {
        let name = gl::gen_vertex_array();
        if name != 0 {
            self.vertex_arrays.insert(name);
        }
        name
    }

    pub(crate) fn bind_vertex_array(&mut self, name: u32) -> bool {
        if name != 0 && !self.vertex_arrays.contains(&name) {
            return false;
        }
        self.current = name;
        true
    }

    fn release(&mut self, buffer: u32) -> Option<u32> {
        let count = self.holders.get_mut(&buffer)?;
        *count = count.saturating_sub(1);
        if *count > 0 {
            return None;
        }
        self.holders.remove(&buffer);
        if !self.retired.remove(&buffer) {
            return None;
        }
        if self.held.remove(&buffer) {
            gl::delete_buffer(buffer);
        }
        Some(buffer)
    }

    pub(crate) fn attach(&mut self, slot: u32, buffer: u32) -> Option<u32> {
        let slots = self.slots.entry(self.current).or_default();
        let old = if buffer == 0 {
            slots.remove(&slot)
        } else {
            slots.insert(slot, buffer)
        };
        if buffer != 0 {
            *self.holders.entry(buffer).or_default() += 1;
        }
        old.and_then(|old| self.release(old))
    }

    pub(crate) fn delete_vertex_array(&mut self, name: u32) -> Vec<u32> {
        if name == 0 || !self.vertex_arrays.remove(&name) {
            return Vec::new();
        }
        if self.current == name {
            self.current = 0;
        }
        let slots = self.slots.remove(&name).unwrap_or_default();
        slots
            .into_values()
            .filter_map(|buffer| self.release(buffer))
            .collect()
    }

    pub(crate) fn delete(&mut self, buffer: u32) -> bool {
        self.live.remove(&buffer);
        let detached = self.slots.get_mut(&self.current).map_or(0, |slots| {
            let before = slots.len();
            slots.retain(|_, held| *held != buffer);
            before - slots.len()
        });
        for _ in 0..detached {
            self.release(buffer);
        }
        if self.holders.contains_key(&buffer) {
            self.retired.insert(buffer);
            return false;
        }
        true
    }
}
