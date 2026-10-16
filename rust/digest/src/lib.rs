//! Southstar — SHA digests over ring and the hex and base64 text forms of bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use ring::digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Algorithm {
    fn ring(self) -> &'static digest::Algorithm {
        match self {
            Algorithm::Sha1 => &digest::SHA1_FOR_LEGACY_USE_ONLY,
            Algorithm::Sha256 => &digest::SHA256,
            Algorithm::Sha384 => &digest::SHA384,
            Algorithm::Sha512 => &digest::SHA512,
        }
    }
}

pub struct Hasher(digest::Context);

impl Hasher {
    pub fn new(algorithm: Algorithm) -> Self {
        Hasher(digest::Context::new(algorithm.ring()))
    }

    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    pub fn finish(self) -> Vec<u8> {
        self.0.finish().as_ref().to_vec()
    }
}

pub fn digest(algorithm: Algorithm, data: &[u8]) -> Vec<u8> {
    digest::digest(algorithm.ring(), data).as_ref().to_vec()
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    digest(Algorithm::Sha256, data)
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&sha256(data))
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    out
}

pub fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        let sextet = |shift: u32| char::from(ALPHABET[(n >> shift) as usize & 63]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests;
