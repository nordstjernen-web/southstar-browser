//! Southstar — the Media Foundation decoders on systems without Media Foundation: none can be opened.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{Pcm, Picture, Received, Sent};

pub fn h264_available() -> bool {
    false
}

pub fn aac_available() -> bool {
    false
}

pub enum VideoDecoder {}

impl VideoDecoder {
    pub fn h264(_config: &[u8], _width: u32, _height: u32) -> Option<VideoDecoder> {
        None
    }

    pub fn send(&mut self, _packet: &[u8], _time: Option<i64>) -> Sent {
        match *self {}
    }

    pub fn receive<R>(&mut self, _use: impl FnOnce(&Picture<'_>) -> R) -> Received<R> {
        match *self {}
    }

    pub fn drain(&mut self) {
        match *self {}
    }

    pub fn flush(&mut self) {
        match *self {}
    }
}

pub enum AudioDecoder {}

impl AudioDecoder {
    pub fn aac(_config: &[u8], _rate: u32, _channels: u32) -> Option<AudioDecoder> {
        None
    }

    pub fn rate(&self) -> u32 {
        match *self {}
    }

    pub fn channels(&self) -> u32 {
        match *self {}
    }

    pub fn send(&mut self, _packet: &[u8], _time: Option<i64>) -> Sent {
        match *self {}
    }

    pub fn receive<R>(&mut self, _use: impl FnOnce(&Pcm<'_>) -> R) -> Received<R> {
        match *self {}
    }

    pub fn drain(&mut self) {
        match *self {}
    }

    pub fn flush(&mut self) {
        match *self {}
    }
}
