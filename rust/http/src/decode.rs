//! Southstar — the Content-Encoding of a response and the decoder that turns its body back into bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::codec::{Emit, Failure, Inflate};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Identity,
    Gzip,
    Deflate,
    Brotli,
    Zstd,
}

pub fn classify(value: &[u8]) -> Option<Encoding> {
    let v = value.trim_ascii();
    if v.eq_ignore_ascii_case(b"gzip") || v.eq_ignore_ascii_case(b"x-gzip") {
        Some(Encoding::Gzip)
    } else if v.eq_ignore_ascii_case(b"deflate") {
        Some(Encoding::Deflate)
    } else if v.eq_ignore_ascii_case(b"br") {
        Some(Encoding::Brotli)
    } else if v.eq_ignore_ascii_case(b"zstd") {
        Some(Encoding::Zstd)
    } else if v.eq_ignore_ascii_case(b"identity") {
        Some(Encoding::Identity)
    } else {
        None
    }
}

pub fn accept_encoding() -> &'static [u8] {
    match (cfg!(feature = "brotli"), cfg!(feature = "zstd")) {
        (true, true) => b"gzip, deflate, br, zstd",
        (true, false) => b"gzip, deflate, br",
        (false, true) => b"gzip, deflate, zstd",
        (false, false) => b"gzip, deflate",
    }
}

enum State {
    Identity,
    Sniffing(Vec<u8>),
    Inflate(Inflate),
    #[cfg(feature = "brotli")]
    Brotli(crate::ffi::codec::Brotli),
    #[cfg(feature = "zstd")]
    Zstd(crate::ffi::codec::Zstd),
}

pub struct Decoder {
    state: State,
}

pub enum DecodeError {
    Corrupt,
    Stopped,
}

impl From<Failure> for DecodeError {
    fn from(f: Failure) -> DecodeError {
        match f {
            Failure::Corrupt => DecodeError::Corrupt,
            Failure::Stopped => DecodeError::Stopped,
        }
    }
}

fn zlib_wrapped(head: &[u8]) -> bool {
    head.len() >= 2
        && head[0] & 0x0f == 8
        && ((u16::from(head[0]) << 8) | u16::from(head[1])) % 31 == 0
}

impl Decoder {
    pub fn new(encoding: Encoding) -> Option<Decoder> {
        let state = match encoding {
            Encoding::Identity => State::Identity,
            Encoding::Gzip => State::Inflate(Inflate::new(15 + 32)?),
            Encoding::Deflate => State::Sniffing(Vec::new()),
            #[cfg(feature = "brotli")]
            Encoding::Brotli => State::Brotli(crate::ffi::codec::Brotli::new()?),
            #[cfg(feature = "zstd")]
            Encoding::Zstd => State::Zstd(crate::ffi::codec::Zstd::new()?),
            #[allow(unreachable_patterns)]
            _ => State::Identity,
        };
        Some(Decoder { state })
    }

    pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), DecodeError> {
        match &mut self.state {
            State::Identity => {
                if !input.is_empty() && !emit(input) {
                    return Err(DecodeError::Stopped);
                }
                Ok(())
            }
            State::Sniffing(head) => {
                head.extend_from_slice(input);
                if head.len() < 2 {
                    return Ok(());
                }
                let head = std::mem::take(head);
                let bits = if zlib_wrapped(&head) { 15 + 32 } else { -15 };
                let inflate = Inflate::new(bits).ok_or(DecodeError::Corrupt)?;
                self.state = State::Inflate(inflate);
                self.feed(&head, emit)
            }
            State::Inflate(z) => Ok(z.feed(input, emit)?),
            #[cfg(feature = "brotli")]
            State::Brotli(b) => Ok(b.feed(input, emit)?),
            #[cfg(feature = "zstd")]
            State::Zstd(z) => Ok(z.feed(input, emit)?),
        }
    }

    pub fn finish(&mut self, emit: &mut Emit) -> Result<(), DecodeError> {
        if let State::Sniffing(head) = &mut self.state {
            let head = std::mem::take(head);
            if head.is_empty() {
                return Ok(());
            }
            let inflate = Inflate::new(-15).ok_or(DecodeError::Corrupt)?;
            self.state = State::Inflate(inflate);
            return self.feed(&head, emit);
        }
        Ok(())
    }
}
