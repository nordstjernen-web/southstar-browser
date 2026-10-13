//! Southstar — the bit reader over an in-memory stream or a growing packet ring. Ported from pl_mpeg by Dominic Szablewski (MIT).
//! Copyright 2019 Dominic Szablewski
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: MIT

const RING_CAPACITY: usize = 128 * 1024;

pub struct Buffer {
    pub bytes: Vec<u8>,
    pub bit_index: usize,
    pub total_size: usize,
    pub discard_read_bytes: bool,
    pub has_ended: bool,
    fixed: bool,
}

#[derive(Clone, Copy)]
pub struct Vlc {
    pub index: i16,
    pub value: i16,
}

impl Buffer {
    pub fn with_memory(bytes: Vec<u8>) -> Buffer {
        let total_size = bytes.len();
        Buffer {
            bytes,
            bit_index: 0,
            total_size,
            discard_read_bytes: false,
            has_ended: false,
            fixed: true,
        }
    }

    pub fn ring() -> Buffer {
        Buffer {
            bytes: Vec::with_capacity(RING_CAPACITY),
            bit_index: 0,
            total_size: 0,
            discard_read_bytes: true,
            has_ended: false,
            fixed: false,
        }
    }

    pub fn size(&self) -> usize {
        self.bytes.len()
    }

    pub fn bits_left(&self) -> usize {
        (self.bytes.len() << 3).wrapping_sub(self.bit_index)
    }

    pub fn write(&mut self, data: &[u8]) {
        if self.fixed {
            return;
        }
        if self.discard_read_bytes {
            self.discard_read();
            self.total_size = 0;
        }
        self.bytes.extend_from_slice(data);
        self.has_ended = false;
    }

    pub fn signal_end(&mut self) {
        self.total_size = self.bytes.len();
    }

    pub fn rewind(&mut self) {
        self.seek(0);
    }

    pub fn seek(&mut self, pos: usize) {
        self.has_ended = false;
        if !self.fixed {
            if pos != 0 {
                return;
            }
            self.bit_index = 0;
            self.bytes.clear();
            self.total_size = 0;
        } else if pos < self.bytes.len() {
            self.bit_index = pos << 3;
        }
    }

    pub fn tell(&self) -> usize {
        self.bit_index >> 3
    }

    pub fn discard_read(&mut self) {
        let byte_pos = self.bit_index >> 3;
        if byte_pos == self.bytes.len() {
            self.bit_index = 0;
            self.bytes.clear();
        } else if byte_pos > 0 {
            self.bytes.drain(..byte_pos);
            self.bit_index -= byte_pos << 3;
        }
    }
}

pub trait Bits {
    fn buf(&mut self) -> &mut Buffer;

    fn load(&mut self) -> bool;

    fn has(&mut self, count: usize) -> bool {
        if self.buf().bits_left() >= count {
            return true;
        }
        if self.load() && self.buf().bits_left() >= count {
            return true;
        }
        let b = self.buf();
        if b.total_size != 0 && b.bytes.len() == b.total_size {
            b.has_ended = true;
        }
        false
    }

    fn read(&mut self, count: u32) -> i32 {
        if !self.has(count as usize) {
            return 0;
        }
        let b = self.buf();
        let mut count = count;
        let mut value: i32 = 0;
        while count != 0 {
            let current = b.bytes[b.bit_index >> 3] as i32;
            let remaining = 8 - (b.bit_index & 7) as u32;
            let read = remaining.min(count);
            let shift = remaining - read;
            let mask = 0xff >> (8 - read);
            value = (value << read) | ((current & (mask << shift)) >> shift);
            b.bit_index += read as usize;
            count -= read;
        }
        value
    }

    fn align(&mut self) {
        let b = self.buf();
        b.bit_index = ((b.bit_index + 7) >> 3) << 3;
    }

    fn skip(&mut self, count: usize) {
        if self.has(count) {
            self.buf().bit_index += count;
        }
    }

    fn skip_bytes(&mut self, v: u8) -> usize {
        self.align();
        let mut skipped = 0;
        while self.has(8) {
            let b = self.buf();
            if b.bytes[b.bit_index >> 3] != v {
                break;
            }
            b.bit_index += 8;
            skipped += 1;
        }
        skipped
    }

    fn next_start_code(&mut self) -> i32 {
        self.align();
        while self.has(5 << 3) {
            let b = self.buf();
            let i = b.bit_index >> 3;
            if b.bytes[i] == 0x00 && b.bytes[i + 1] == 0x00 && b.bytes[i + 2] == 0x01 {
                b.bit_index = (i + 4) << 3;
                return b.bytes[i + 3] as i32;
            }
            b.bit_index += 8;
        }
        -1
    }

    fn find_start_code(&mut self, code: i32) -> i32 {
        loop {
            let current = self.next_start_code();
            if current == code || current == -1 {
                return current;
            }
        }
    }

    fn has_start_code(&mut self, code: i32) -> i32 {
        let b = self.buf();
        let previous_bit_index = b.bit_index;
        let previous_discard = b.discard_read_bytes;
        b.discard_read_bytes = false;
        let current = self.find_start_code(code);
        let b = self.buf();
        b.bit_index = previous_bit_index;
        b.discard_read_bytes = previous_discard;
        current
    }

    fn peek_non_zero(&mut self, count: u32) -> bool {
        if !self.has(count as usize) {
            return false;
        }
        let val = self.read(count);
        self.buf().bit_index -= count as usize;
        val != 0
    }

    fn read_vlc(&mut self, table: &[Vlc]) -> i16 {
        let mut state = Vlc { index: 0, value: 0 };
        loop {
            state = table[(state.index as i32 + self.read(1)) as usize];
            if state.index <= 0 {
                return state.value;
            }
        }
    }
}

impl Bits for Buffer {
    fn buf(&mut self) -> &mut Buffer {
        self
    }

    fn load(&mut self) -> bool {
        false
    }
}
