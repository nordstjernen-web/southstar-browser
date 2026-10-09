//! Southstar — HPACK header compression (RFC 7541): the decoder with its dynamic table and Huffman strings, and a literal-only encoder.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::VecDeque;
use std::sync::OnceLock;

use crate::hpack_tables::{HUFFMAN_CODES, STATIC_TABLE};

const ENTRY_OVERHEAD: usize = 32;
const EOS: u16 = 256;
const LEAF: u16 = 0x8000;

#[derive(Debug)]
pub struct Error;

pub type Field = (Vec<u8>, Vec<u8>);

pub struct Decoder {
    entries: VecDeque<(Vec<u8>, Vec<u8>)>,
    size: usize,
    max_size: usize,
    limit: usize,
}

struct Trie {
    nodes: Vec<[u16; 2]>,
}

fn trie() -> &'static Trie {
    static TRIE: OnceLock<Trie> = OnceLock::new();
    TRIE.get_or_init(|| {
        let mut nodes = vec![[0u16; 2]];
        for (symbol, &(code, len)) in HUFFMAN_CODES.iter().enumerate() {
            let mut node = 0usize;
            for bit in (0..len).rev() {
                let side = ((code >> bit) & 1) as usize;
                if bit == 0 {
                    nodes[node][side] = LEAF | symbol as u16;
                } else {
                    if nodes[node][side] == 0 {
                        nodes.push([0, 0]);
                        nodes[node][side] = (nodes.len() - 1) as u16;
                    }
                    node = nodes[node][side] as usize;
                }
            }
        }
        Trie { nodes }
    })
}

pub fn huffman_decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let trie = trie();
    let mut out = Vec::with_capacity(input.len() * 8 / 5);
    let mut node = 0usize;
    let mut depth = 0u32;
    let mut all_ones = true;
    for &byte in input {
        for bit in (0..8).rev() {
            let side = ((byte >> bit) & 1) as usize;
            all_ones &= side == 1;
            depth += 1;
            let next = trie.nodes[node][side];
            if next & LEAF != 0 {
                let symbol = next & !LEAF;
                if symbol == EOS {
                    return Err(Error);
                }
                out.push(symbol as u8);
                node = 0;
                depth = 0;
                all_ones = true;
            } else if next == 0 {
                return Err(Error);
            } else {
                node = next as usize;
            }
        }
    }
    if depth > 7 || !all_ones {
        return Err(Error);
    }
    Ok(out)
}

fn read_int(input: &[u8], pos: &mut usize, prefix_bits: u32) -> Result<usize, Error> {
    let first = *input.get(*pos).ok_or(Error)?;
    *pos += 1;
    let mask = (1usize << prefix_bits) - 1;
    let mut value = first as usize & mask;
    if value < mask {
        return Ok(value);
    }
    let mut shift = 0u32;
    loop {
        let byte = *input.get(*pos).ok_or(Error)?;
        *pos += 1;
        if shift > 28 {
            return Err(Error);
        }
        value = value
            .checked_add(((byte & 0x7f) as usize) << shift)
            .ok_or(Error)?;
        shift += 7;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
}

fn read_string(input: &[u8], pos: &mut usize) -> Result<Vec<u8>, Error> {
    let huffman = input.get(*pos).ok_or(Error)? & 0x80 != 0;
    let len = read_int(input, pos, 7)?;
    let end = pos
        .checked_add(len)
        .filter(|&e| e <= input.len())
        .ok_or(Error)?;
    let raw = &input[*pos..end];
    *pos = end;
    if huffman {
        huffman_decode(raw)
    } else {
        Ok(raw.to_vec())
    }
}

impl Decoder {
    pub fn new(max_size: usize) -> Decoder {
        Decoder {
            entries: VecDeque::new(),
            size: 0,
            max_size,
            limit: max_size,
        }
    }

    fn evict(&mut self) {
        while self.size > self.max_size {
            let Some((name, value)) = self.entries.pop_back() else {
                break;
            };
            self.size -= name.len() + value.len() + ENTRY_OVERHEAD;
        }
    }

    fn insert(&mut self, name: Vec<u8>, value: Vec<u8>) {
        let entry_size = name.len() + value.len() + ENTRY_OVERHEAD;
        if entry_size > self.max_size {
            self.entries.clear();
            self.size = 0;
            return;
        }
        self.size += entry_size;
        self.entries.push_front((name, value));
        self.evict();
    }

    fn lookup(&self, index: usize) -> Result<(&[u8], &[u8]), Error> {
        if index == 0 {
            return Err(Error);
        }
        if index <= STATIC_TABLE.len() {
            let (n, v) = STATIC_TABLE[index - 1];
            return Ok((n, v));
        }
        let (n, v) = self
            .entries
            .get(index - STATIC_TABLE.len() - 1)
            .ok_or(Error)?;
        Ok((n, v))
    }

    pub fn decode(&mut self, block: &[u8]) -> Result<Vec<Field>, Error> {
        let mut headers = Vec::new();
        let mut pos = 0;
        let mut size_updates_allowed = true;
        while pos < block.len() {
            let byte = block[pos];
            if byte & 0x80 != 0 {
                let index = read_int(block, &mut pos, 7)?;
                let (n, v) = self.lookup(index)?;
                headers.push((n.to_vec(), v.to_vec()));
                size_updates_allowed = false;
            } else if byte & 0xc0 == 0x40 {
                let index = read_int(block, &mut pos, 6)?;
                let name = if index == 0 {
                    read_string(block, &mut pos)?
                } else {
                    self.lookup(index)?.0.to_vec()
                };
                let value = read_string(block, &mut pos)?;
                self.insert(name.clone(), value.clone());
                headers.push((name, value));
                size_updates_allowed = false;
            } else if byte & 0xe0 == 0x20 {
                if !size_updates_allowed {
                    return Err(Error);
                }
                let size = read_int(block, &mut pos, 5)?;
                if size > self.limit {
                    return Err(Error);
                }
                self.max_size = size;
                self.evict();
            } else {
                let index = read_int(block, &mut pos, 4)?;
                let name = if index == 0 {
                    read_string(block, &mut pos)?
                } else {
                    self.lookup(index)?.0.to_vec()
                };
                let value = read_string(block, &mut pos)?;
                headers.push((name, value));
                size_updates_allowed = false;
            }
        }
        Ok(headers)
    }
}

fn write_int(out: &mut Vec<u8>, first: u8, prefix_bits: u32, mut value: usize) {
    let mask = (1usize << prefix_bits) - 1;
    if value < mask {
        out.push(first | value as u8);
        return;
    }
    out.push(first | mask as u8);
    value -= mask;
    while value >= 0x80 {
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn write_string(out: &mut Vec<u8>, s: &[u8]) {
    write_int(out, 0, 7, s.len());
    out.extend_from_slice(s);
}

pub fn encode(headers: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(name, value) in headers {
        if let Some(i) = STATIC_TABLE
            .iter()
            .position(|&(n, v)| n == name && v == value)
        {
            write_int(&mut out, 0x80, 7, i + 1);
            continue;
        }
        match STATIC_TABLE.iter().position(|&(n, _)| n == name) {
            Some(i) => write_int(&mut out, 0, 4, i + 1),
            None => {
                out.push(0);
                write_string(&mut out, name);
            }
        }
        write_string(&mut out, value);
    }
    out
}
