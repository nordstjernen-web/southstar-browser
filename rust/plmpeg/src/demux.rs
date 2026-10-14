//! Southstar — the MPEG program stream demuxer: pack and system headers, PES packets, duration and seeking. Ported from pl_mpeg by Dominic Szablewski (MIT).
//! Copyright 2019 Dominic Szablewski
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: MIT

use crate::buffer::{Bits, Buffer};

pub const PACKET_PRIVATE: i32 = 0xBD;
pub const PACKET_AUDIO_1: i32 = 0xC0;
pub const PACKET_AUDIO_4: i32 = 0xC3;
pub const PACKET_VIDEO_1: i32 = 0xE0;
pub const INVALID_TS: f64 = -1.0;

const START_PACK: i32 = 0xBA;
const START_SYSTEM: i32 = 0xBB;

#[derive(Clone, Copy, Default)]
pub struct Packet {
    pub kind: i32,
    pub pts: f64,
    pub length: usize,
    pub start: usize,
}

pub struct Demux {
    pub buffer: Buffer,
    last_file_size: usize,
    last_decoded_pts: f64,
    start_time: f64,
    duration: f64,
    start_code: i32,
    has_pack_header: bool,
    has_system_header: bool,
    has_headers: bool,
    num_audio_streams: i32,
    num_video_streams: i32,
    current_packet: Packet,
    next_packet: Packet,
}

impl Demux {
    pub fn new(buffer: Buffer) -> Demux {
        let mut demux = Demux {
            buffer,
            last_file_size: 0,
            last_decoded_pts: 0.0,
            start_time: INVALID_TS,
            duration: INVALID_TS,
            start_code: -1,
            has_pack_header: false,
            has_system_header: false,
            has_headers: false,
            num_audio_streams: 0,
            num_video_streams: 0,
            current_packet: Packet::default(),
            next_packet: Packet::default(),
        };
        demux.has_headers();
        demux
    }

    pub fn data(&self, packet: &Packet) -> &[u8] {
        &self.buffer.bytes[packet.start..packet.start + packet.length]
    }

    pub fn has_headers(&mut self) -> bool {
        if self.has_headers {
            return true;
        }
        if !self.has_pack_header {
            if self.start_code != START_PACK && self.buffer.find_start_code(START_PACK) == -1 {
                return false;
            }
            self.start_code = START_PACK;
            if !self.buffer.has(64) {
                return false;
            }
            self.start_code = -1;
            if self.buffer.read(4) != 0x02 {
                return false;
            }
            self.decode_time();
            self.buffer.skip(1);
            self.buffer.skip(22);
            self.buffer.skip(1);
            self.has_pack_header = true;
        }
        if !self.has_system_header {
            if self.start_code != START_SYSTEM && self.buffer.find_start_code(START_SYSTEM) == -1 {
                return false;
            }
            self.start_code = START_SYSTEM;
            if !self.buffer.has(56) {
                return false;
            }
            self.start_code = -1;
            self.buffer.skip(16);
            self.buffer.skip(24);
            self.num_audio_streams = self.buffer.read(6);
            self.buffer.skip(5);
            self.num_video_streams = self.buffer.read(5);
            self.has_system_header = true;
        }
        self.has_headers = true;
        true
    }

    pub fn num_video_streams(&mut self) -> i32 {
        if self.has_headers() {
            self.num_video_streams
        } else {
            0
        }
    }

    pub fn num_audio_streams(&mut self) -> i32 {
        if self.has_headers() {
            self.num_audio_streams
        } else {
            0
        }
    }

    pub fn rewind(&mut self) {
        self.buffer.rewind();
        self.current_packet.length = 0;
        self.next_packet.length = 0;
        self.start_code = -1;
    }

    pub fn has_ended(&self) -> bool {
        self.buffer.has_ended
    }

    fn buffer_seek(&mut self, pos: usize) {
        self.buffer.seek(pos);
        self.current_packet.length = 0;
        self.next_packet.length = 0;
        self.start_code = -1;
    }

    pub fn start_time(&mut self, kind: i32) -> f64 {
        if self.start_time != INVALID_TS {
            return self.start_time;
        }
        let previous_pos = self.buffer.tell() as i32;
        let previous_start_code = self.start_code;
        self.rewind();
        while let Some(packet) = self.decode() {
            if packet.kind == kind {
                self.start_time = packet.pts;
            }
            if self.start_time != INVALID_TS {
                break;
            }
        }
        self.buffer_seek(previous_pos as usize);
        self.start_code = previous_start_code;
        self.start_time
    }

    pub fn duration(&mut self, kind: i32) -> f64 {
        let file_size = self.buffer.size();
        if self.duration != INVALID_TS && self.last_file_size == file_size {
            return self.duration;
        }
        let previous_pos = self.buffer.tell();
        let previous_start_code = self.start_code;
        let max_range: i64 = 4096 * 1024;
        let mut range: i64 = 64 * 1024;
        while range <= max_range {
            let mut seek_pos = file_size as i64 - range;
            if seek_pos < 0 {
                seek_pos = 0;
                range = max_range;
            }
            self.buffer_seek(seek_pos as usize);
            self.current_packet.length = 0;
            let mut last_pts = INVALID_TS;
            while let Some(packet) = self.decode() {
                if packet.pts != INVALID_TS && packet.kind == kind {
                    last_pts = packet.pts;
                }
            }
            if last_pts != INVALID_TS {
                self.duration = last_pts - self.start_time(kind);
                break;
            }
            range *= 2;
        }
        self.buffer_seek(previous_pos);
        self.start_code = previous_start_code;
        self.last_file_size = file_size;
        self.duration
    }

    pub fn seek(&mut self, seek_time: f64, kind: i32, force_intra: bool) -> Option<Packet> {
        if !self.has_headers() {
            return None;
        }
        let duration = self.duration(kind);
        let file_size = self.buffer.size() as i64;
        let mut byterate = (file_size as f64 / duration) as i64;
        let mut cur_time = self.last_decoded_pts;
        let mut scan_span = 1.0;
        let mut seek_time = seek_time;
        if seek_time > duration {
            seek_time = duration;
        } else if seek_time < 0.0 {
            seek_time = 0.0;
        }
        seek_time += self.start_time;

        for _ in 0..32 {
            let mut found_packet_with_pts = false;
            let mut found_packet_in_range = false;
            let mut last_valid_packet_start: i64 = -1;
            let mut first_packet_time = INVALID_TS;
            let cur_pos = self.buffer.tell() as i64;
            let offset = ((seek_time - cur_time - scan_span) * byterate as f64) as i64;
            let seek_pos = cur_pos
                .saturating_add(offset)
                .clamp(0, (file_size - 256).max(0));
            self.buffer_seek(seek_pos as usize);

            while self.buffer.find_start_code(kind) != -1 {
                let packet_start = self.buffer.tell() as i64;
                let packet = match self.decode_packet(kind) {
                    Some(packet) if packet.pts != INVALID_TS => packet,
                    _ => continue,
                };
                if packet.pts > seek_time || packet.pts < seek_time - scan_span {
                    found_packet_with_pts = true;
                    byterate = ((seek_pos - cur_pos) as f64 / (packet.pts - cur_time)) as i64;
                    cur_time = packet.pts;
                    break;
                }
                if !found_packet_in_range {
                    found_packet_in_range = true;
                    first_packet_time = packet.pts;
                }
                if force_intra {
                    let data = self.data(&packet);
                    for i in 0..data.len().saturating_sub(6) {
                        if data[i] == 0x00
                            && data[i + 1] == 0x00
                            && data[i + 2] == 0x01
                            && data[i + 3] == 0x00
                        {
                            if (data[i + 5] & 0x38) == 8 {
                                last_valid_packet_start = packet_start;
                            }
                            break;
                        }
                    }
                } else {
                    last_valid_packet_start = packet_start;
                }
            }

            if last_valid_packet_start != -1 {
                self.buffer_seek(last_valid_packet_start as usize);
                return self.decode_packet(kind);
            } else if found_packet_in_range {
                scan_span *= 2.0;
                seek_time = first_packet_time;
            } else if !found_packet_with_pts {
                byterate = ((seek_pos - cur_pos) as f64 / (duration - cur_time)) as i64;
                cur_time = duration;
            }
        }
        None
    }

    pub fn decode(&mut self) -> Option<Packet> {
        if !self.has_headers() {
            return None;
        }
        if self.current_packet.length != 0 {
            let bits_till_next_packet = self.current_packet.length << 3;
            if !self.buffer.has(bits_till_next_packet) {
                return None;
            }
            self.buffer.skip(bits_till_next_packet);
            self.current_packet.length = 0;
        }
        if self.next_packet.length != 0 {
            return self.get_packet();
        }
        if self.start_code != -1 {
            return self.decode_packet(self.start_code);
        }
        loop {
            self.start_code = self.buffer.next_start_code();
            if self.start_code == PACKET_VIDEO_1
                || self.start_code == PACKET_PRIVATE
                || (PACKET_AUDIO_1..=PACKET_AUDIO_4).contains(&self.start_code)
            {
                return self.decode_packet(self.start_code);
            }
            if self.start_code == -1 {
                return None;
            }
        }
    }

    fn decode_time(&mut self) -> f64 {
        let mut clock = (self.buffer.read(3) << 30) as i64;
        self.buffer.skip(1);
        clock |= (self.buffer.read(15) << 15) as i64;
        self.buffer.skip(1);
        clock |= self.buffer.read(15) as i64;
        self.buffer.skip(1);
        clock as f64 / 90000.0
    }

    fn decode_packet(&mut self, kind: i32) -> Option<Packet> {
        if !self.buffer.has(16 << 3) {
            return None;
        }
        self.start_code = -1;
        self.next_packet.kind = kind;
        self.next_packet.length = self.buffer.read(16) as usize;
        self.next_packet.length = self
            .next_packet
            .length
            .wrapping_sub(self.buffer.skip_bytes(0xff));
        if self.buffer.read(2) == 0x01 {
            self.buffer.skip(16);
            self.next_packet.length = self.next_packet.length.wrapping_sub(2);
        }
        match self.buffer.read(2) {
            0x03 => {
                self.next_packet.pts = self.decode_time();
                self.last_decoded_pts = self.next_packet.pts;
                self.buffer.skip(40);
                self.next_packet.length = self.next_packet.length.wrapping_sub(10);
            }
            0x02 => {
                self.next_packet.pts = self.decode_time();
                self.last_decoded_pts = self.next_packet.pts;
                self.next_packet.length = self.next_packet.length.wrapping_sub(5);
            }
            0x00 => {
                self.next_packet.pts = INVALID_TS;
                self.buffer.skip(4);
                self.next_packet.length = self.next_packet.length.wrapping_sub(1);
            }
            _ => return None,
        }
        self.get_packet()
    }

    fn get_packet(&mut self) -> Option<Packet> {
        if !self.buffer.has(self.next_packet.length.wrapping_shl(3)) {
            return None;
        }
        self.current_packet = Packet {
            kind: self.next_packet.kind,
            pts: self.next_packet.pts,
            length: self.next_packet.length,
            start: self.buffer.bit_index >> 3,
        };
        self.next_packet.length = 0;
        Some(self.current_packet)
    }
}
