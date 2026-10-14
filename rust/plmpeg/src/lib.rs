//! Southstar — MPEG-1 playback from memory: the program stream demuxer feeding the video and MP2 decoders. Ported from pl_mpeg by Dominic Szablewski (MIT).
//! Copyright 2019 Dominic Szablewski
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: MIT

#![forbid(unsafe_code)]

mod audio;
mod audio_tables;
mod buffer;
mod demux;
mod video;
mod video_tables;

use audio::Audio;
use buffer::{Bits, Buffer};
use demux::{Demux, PACKET_AUDIO_1, PACKET_VIDEO_1};
use video::Video;

pub use audio::{SAMPLES_PER_FRAME, Samples};
pub use video::{Frame, Plane};

struct Feed {
    demux: Demux,
    video_type: i32,
    audio_type: i32,
    video_buf: Buffer,
    audio_buf: Buffer,
}

impl Feed {
    fn read_packets(&mut self, requested_type: i32) {
        while let Some(packet) = self.demux.decode() {
            if packet.kind == self.video_type {
                self.video_buf.write(self.demux.data(&packet));
            } else if packet.kind == self.audio_type {
                self.audio_buf.write(self.demux.data(&packet));
            }
            if packet.kind == requested_type {
                return;
            }
        }
        if self.demux.has_ended() {
            self.video_buf.signal_end();
            self.audio_buf.signal_end();
        }
    }
}

struct Track<'a> {
    feed: &'a mut Feed,
    audio: bool,
}

impl Bits for Track<'_> {
    fn buf(&mut self) -> &mut Buffer {
        if self.audio {
            &mut self.feed.audio_buf
        } else {
            &mut self.feed.video_buf
        }
    }

    fn load(&mut self) -> bool {
        let requested = if self.audio {
            self.feed.audio_type
        } else {
            self.feed.video_type
        };
        self.feed.read_packets(requested);
        true
    }
}

pub struct Plm {
    feed: Feed,
    video: Option<Box<Video>>,
    audio: Option<Box<Audio>>,
    time: f64,
    has_ended: bool,
    looping: bool,
    has_decoders: bool,
    video_enabled: bool,
    audio_enabled: bool,
    audio_stream_index: i32,
}

impl Plm {
    pub fn new(bytes: Vec<u8>) -> Plm {
        let mut plm = Plm {
            feed: Feed {
                demux: Demux::new(Buffer::with_memory(bytes)),
                video_type: 0,
                audio_type: 0,
                video_buf: Buffer::ring(),
                audio_buf: Buffer::ring(),
            },
            video: None,
            audio: None,
            time: 0.0,
            has_ended: false,
            looping: false,
            has_decoders: false,
            video_enabled: true,
            audio_enabled: true,
            audio_stream_index: 0,
        };
        plm.init_decoders();
        plm
    }

    fn video_track(feed: &mut Feed) -> Track<'_> {
        Track { feed, audio: false }
    }

    fn audio_track(feed: &mut Feed) -> Track<'_> {
        Track { feed, audio: true }
    }

    fn init_decoders(&mut self) -> bool {
        if self.has_decoders {
            return true;
        }
        if !self.feed.demux.has_headers() {
            return false;
        }
        if self.feed.demux.num_video_streams() > 0 {
            if self.video_enabled {
                self.feed.video_type = PACKET_VIDEO_1;
            }
            if self.video.is_none() {
                self.feed.video_buf = Buffer::ring();
                let video = Video::new(&mut Self::video_track(&mut self.feed));
                self.video = Some(Box::new(video));
            }
        }
        if self.feed.demux.num_audio_streams() > 0 {
            if self.audio_enabled {
                self.feed.audio_type = PACKET_AUDIO_1 + self.audio_stream_index;
            }
            if self.audio.is_none() {
                self.feed.audio_buf = Buffer::ring();
                self.audio = Some(Audio::new(&mut Self::audio_track(&mut self.feed)));
            }
        }
        self.has_decoders = true;
        true
    }

    pub fn has_headers(&mut self) -> bool {
        if !self.feed.demux.has_headers() || !self.init_decoders() {
            return false;
        }
        if let Some(video) = self.video.as_mut()
            && !video.has_header(&mut Self::video_track(&mut self.feed))
        {
            return false;
        }
        if let Some(audio) = self.audio.as_mut()
            && !audio.has_header(&mut Self::audio_track(&mut self.feed))
        {
            return false;
        }
        true
    }

    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.audio_enabled = enabled;
        if !enabled {
            self.feed.audio_type = 0;
            return;
        }
        self.feed.audio_type = if self.init_decoders() && self.audio.is_some() {
            PACKET_AUDIO_1 + self.audio_stream_index
        } else {
            0
        };
    }

    pub fn set_audio_stream(&mut self, stream_index: i32) {
        if !(0..=3).contains(&stream_index) {
            return;
        }
        self.audio_stream_index = stream_index;
        self.set_audio_enabled(self.audio_enabled);
    }

    pub fn set_video_enabled(&mut self, enabled: bool) {
        self.video_enabled = enabled;
        if !enabled {
            self.feed.video_type = 0;
            return;
        }
        self.feed.video_type = if self.init_decoders() && self.video.is_some() {
            PACKET_VIDEO_1
        } else {
            0
        };
    }

    pub fn num_video_streams(&mut self) -> i32 {
        self.feed.demux.num_video_streams()
    }

    pub fn num_audio_streams(&mut self) -> i32 {
        self.feed.demux.num_audio_streams()
    }

    fn video_header(&mut self) -> Option<&Video> {
        if !self.init_decoders() {
            return None;
        }
        let video = self.video.as_mut()?;
        if video.has_header(&mut Self::video_track(&mut self.feed)) {
            Some(video)
        } else {
            None
        }
    }

    pub fn width(&mut self) -> i32 {
        self.video_header().map_or(0, Video::width)
    }

    pub fn height(&mut self) -> i32 {
        self.video_header().map_or(0, Video::height)
    }

    pub fn framerate(&mut self) -> f64 {
        self.video_header().map_or(0.0, Video::framerate)
    }

    pub fn pixel_aspect_ratio(&mut self) -> f64 {
        self.video_header().map_or(0.0, Video::pixel_aspect_ratio)
    }

    pub fn samplerate(&mut self) -> i32 {
        if !self.init_decoders() {
            return 0;
        }
        let Some(audio) = self.audio.as_mut() else {
            return 0;
        };
        if audio.has_header(&mut Self::audio_track(&mut self.feed)) {
            audio.samplerate()
        } else {
            0
        }
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn duration(&mut self) -> f64 {
        self.feed.demux.duration(PACKET_VIDEO_1)
    }

    pub fn rewind(&mut self) {
        if let Some(video) = self.video.as_mut() {
            video.rewind();
            self.feed.video_buf.rewind();
        }
        if let Some(audio) = self.audio.as_mut() {
            audio.rewind();
            self.feed.audio_buf.rewind();
        }
        self.feed.demux.rewind();
        self.time = 0.0;
        self.has_ended = false;
    }

    pub fn looping(&self) -> bool {
        self.looping
    }

    pub fn set_loop(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub fn has_ended(&self) -> bool {
        self.has_ended
    }

    fn handle_end(&mut self) {
        if self.looping {
            self.rewind();
        } else {
            self.has_ended = true;
        }
    }

    pub fn decode_video(&mut self) -> Option<Frame<'_>> {
        if !self.init_decoders() || self.feed.video_type == 0 {
            return None;
        }
        let video = self.video.as_mut()?;
        match video.decode(&mut Self::video_track(&mut self.feed)) {
            Some((base, time)) => {
                self.time = time;
                self.video.as_deref().map(|video| video.frame(base, time))
            }
            None => {
                if self.feed.demux.has_ended() {
                    self.handle_end();
                }
                None
            }
        }
    }

    pub fn decode_audio(&mut self) -> Option<&Samples> {
        if !self.init_decoders() || self.feed.audio_type == 0 {
            return None;
        }
        let audio = self.audio.as_mut()?;
        if !audio.decode(&mut Self::audio_track(&mut self.feed)) {
            if self.feed.demux.has_ended() {
                self.handle_end();
            }
            return None;
        }
        let samples = self.audio.as_deref()?.samples();
        self.time = samples.time;
        Some(samples)
    }

    pub fn seek_frame(&mut self, time: f64, seek_exact: bool) -> Option<Frame<'_>> {
        if !self.init_decoders() || self.feed.video_type == 0 {
            return None;
        }
        let kind = self.feed.video_type;
        let start_time = self.feed.demux.start_time(kind);
        let duration = self.feed.demux.duration(kind);
        let time = if time < 0.0 {
            0.0
        } else if time > duration {
            duration
        } else {
            time
        };
        let packet = self.feed.demux.seek(time, kind, true)?;
        let previous_audio_type = self.feed.audio_type;
        self.feed.audio_type = 0;
        let video = self.video.as_mut()?;
        video.rewind();
        self.feed.video_buf.rewind();
        video.set_time(packet.pts - start_time);
        let Feed {
            demux, video_buf, ..
        } = &mut self.feed;
        video_buf.write(demux.data(&packet));
        let mut frame = video.decode(&mut Self::video_track(&mut self.feed));
        if seek_exact {
            while let Some((_, frame_time)) = frame
                && frame_time < time
            {
                frame = video.decode(&mut Self::video_track(&mut self.feed));
            }
        }
        self.feed.audio_type = previous_audio_type;
        if let Some((_, frame_time)) = frame {
            self.time = frame_time;
        }
        self.has_ended = false;
        let (base, frame_time) = frame?;
        self.video
            .as_deref()
            .map(|video| video.frame(base, frame_time))
    }
}
