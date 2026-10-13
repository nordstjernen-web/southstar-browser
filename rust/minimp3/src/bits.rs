//! Southstar — the MSB-first bit reader the MP3 decoder parses side information and scalefactors with. Ported from minimp3 by lieff (CC0).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub struct BitReader<'a> {
    pub buf: &'a [u8],
    pub pos: i32,
    pub limit: i32,
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8], bytes: i32) -> BitReader<'a> {
        BitReader {
            buf,
            pos: 0,
            limit: bytes * 8,
        }
    }

    pub fn byte(&self, index: usize) -> u32 {
        self.buf.get(index).copied().unwrap_or(0) as u32
    }

    pub fn get_bits(&mut self, n: i32) -> u32 {
        let s = (self.pos & 7) as u32;
        let mut shl = n + s as i32;
        let mut p = (self.pos >> 3) as usize;
        self.pos += n;
        if self.pos > self.limit {
            return 0;
        }
        let mut cache = 0u32;
        let mut next = self.byte(p) & (255 >> s);
        p += 1;
        loop {
            shl -= 8;
            if shl <= 0 {
                break;
            }
            cache |= next.wrapping_shl(shl as u32);
            next = self.byte(p);
            p += 1;
        }
        cache | (next >> (-shl) as u32)
    }
}
