//! Southstar — appending a response body under the memory budget that stops a load before it exhausts the machine.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub trait Sink {
    fn append(&mut self, bytes: &[u8]);
}

pub struct Budgeted<'a, S: Sink> {
    sink: &'a mut S,
    total: u64,
    budget: u64,
}

impl<'a, S: Sink> Budgeted<'a, S> {
    pub fn new(sink: &'a mut S, budget: u64) -> Budgeted<'a, S> {
        Budgeted {
            sink,
            total: 0,
            budget,
        }
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn append(&mut self, data: &[u8]) -> bool {
        let len = data.len() as u64;
        if len == 0 {
            return true;
        }
        if len > u64::from(u32::MAX) || self.total > self.budget || len > self.budget - self.total {
            return false;
        }
        if self.total + len > u64::from(u32::MAX) {
            return false;
        }
        self.sink.append(data);
        self.total += len;
        true
    }
}

pub fn exhausted_error(stopped_at: u64) -> Vec<u8> {
    format!(
        "response would exhaust available memory (stopped at {} MiB)",
        stopped_at >> 20
    )
    .into_bytes()
}
