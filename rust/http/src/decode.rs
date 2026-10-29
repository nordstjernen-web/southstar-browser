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

fn classify(value: &[u8]) -> Option<Encoding> {
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

const MAX_CODINGS: usize = 5;
const GZIP_MAGIC: u8 = 0x1f;

pub fn codings(value: &[u8], into: &mut Vec<Encoding>) {
    into.extend(
        value
            .split(|&b| b == b',')
            .filter_map(classify)
            .filter(|&e| e != Encoding::Identity),
    );
}

enum State {
    Gzip(Inflate),
    Sniffing(Vec<u8>),
    Inflate(Inflate),
    #[cfg(feature = "brotli")]
    Brotli(crate::ffi::codec::Brotli),
    #[cfg(feature = "zstd")]
    Zstd(crate::ffi::codec::Zstd),
}

pub struct Decoder {
    stages: Vec<State>,
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

impl State {
    fn new(encoding: Encoding) -> Option<Option<State>> {
        Some(match encoding {
            Encoding::Gzip => Some(State::Gzip(Inflate::new(15 + 32)?)),
            Encoding::Deflate => Some(State::Sniffing(Vec::new())),
            #[cfg(feature = "brotli")]
            Encoding::Brotli => Some(State::Brotli(crate::ffi::codec::Brotli::new()?)),
            #[cfg(feature = "zstd")]
            Encoding::Zstd => Some(State::Zstd(crate::ffi::codec::Zstd::new()?)),
            #[allow(unreachable_patterns)]
            _ => None,
        })
    }

    fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), DecodeError> {
        match self {
            State::Gzip(z) => {
                let mut rest = input;
                loop {
                    let trailing = z.feed(rest, emit)?;
                    if trailing == 0 {
                        return Ok(());
                    }
                    rest = &rest[rest.len() - trailing..];
                    if rest[0] != GZIP_MAGIC {
                        return Ok(());
                    }
                    *z = Inflate::new(15 + 32).ok_or(DecodeError::Corrupt)?;
                }
            }
            State::Sniffing(head) => {
                head.extend_from_slice(input);
                if head.len() < 2 {
                    return Ok(());
                }
                let head = std::mem::take(head);
                let bits = if zlib_wrapped(&head) { 15 + 32 } else { -15 };
                let inflate = Inflate::new(bits).ok_or(DecodeError::Corrupt)?;
                *self = State::Inflate(inflate);
                self.feed(&head, emit)
            }
            State::Inflate(z) => z.feed(input, emit).map(|_| ()).map_err(Into::into),
            #[cfg(feature = "brotli")]
            State::Brotli(b) => Ok(b.feed(input, emit)?),
            #[cfg(feature = "zstd")]
            State::Zstd(z) => Ok(z.feed(input, emit)?),
        }
    }

    fn finish(&mut self, emit: &mut Emit) -> Result<(), DecodeError> {
        if let State::Sniffing(head) = self {
            let head = std::mem::take(head);
            if head.is_empty() {
                return Ok(());
            }
            let inflate = Inflate::new(-15).ok_or(DecodeError::Corrupt)?;
            *self = State::Inflate(inflate);
            return self.feed(&head, emit);
        }
        Ok(())
    }
}

fn feed_stages(stages: &mut [State], input: &[u8], emit: &mut Emit) -> Result<(), DecodeError> {
    let Some((first, rest)) = stages.split_first_mut() else {
        return if input.is_empty() || emit(input) {
            Ok(())
        } else {
            Err(DecodeError::Stopped)
        };
    };
    let mut inner = Ok(());
    let outcome = first.feed(
        input,
        &mut |bytes: &[u8]| match feed_stages(rest, bytes, emit) {
            Ok(()) => true,
            Err(e) => {
                inner = Err(e);
                false
            }
        },
    );
    inner?;
    outcome
}

fn finish_stages(stages: &mut [State], emit: &mut Emit) -> Result<(), DecodeError> {
    let Some((first, rest)) = stages.split_first_mut() else {
        return Ok(());
    };
    let mut inner = Ok(());
    let outcome = first.finish(&mut |bytes: &[u8]| match feed_stages(rest, bytes, emit) {
        Ok(()) => true,
        Err(e) => {
            inner = Err(e);
            false
        }
    });
    inner?;
    outcome?;
    finish_stages(rest, emit)
}

impl Decoder {
    pub fn new(encodings: &[Encoding]) -> Option<Decoder> {
        if encodings.len() > MAX_CODINGS {
            return None;
        }
        let mut stages = Vec::with_capacity(encodings.len());
        for &encoding in encodings.iter().rev() {
            stages.extend(State::new(encoding)?);
        }
        Some(Decoder { stages })
    }

    pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), DecodeError> {
        feed_stages(&mut self.stages, input, emit)
    }

    pub fn finish(&mut self, emit: &mut Emit) -> Result<(), DecodeError> {
        finish_stages(&mut self.stages, emit)
    }
}
