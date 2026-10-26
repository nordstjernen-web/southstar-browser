//! Southstar — the tab's media helpers: which X-Audio commands reach the audio and video helpers, the video overlay's rectangle and state, and which queued frame to show.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const HELPER_LINE_MAX: usize = 4096;
const TOKEN_MAX: usize = 63;
pub const RING_SLOTS: u32 = 8;
pub const CLOCK_USED: u32 = 1;
pub const CLOCK_PLAYING: u32 = 2;

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

pub fn scan_token(s: &[u8]) -> Option<(&[u8], &[u8])> {
    let start = s.iter().position(|&b| !is_space(b))?;
    let s = &s[start..];
    let len = s
        .iter()
        .take(TOKEN_MAX)
        .position(|&b| is_space(b))
        .unwrap_or(s.len().min(TOKEN_MAX));
    Some((&s[..len], &s[len..]))
}

fn scan_int(s: &[u8]) -> Option<(i32, &[u8])> {
    let start = s.iter().position(|&b| !is_space(b))?;
    let s = &s[start..];
    let mut i = 0;
    let negative = match s.first() {
        Some(b'-') => {
            i = 1;
            true
        }
        Some(b'+') => {
            i = 1;
            false
        }
        _ => false,
    };
    let digits = s[i..].iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let mut value: i64 = 0;
    for &b in &s[i..i + digits] {
        value = value.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    if negative {
        value = -value;
    }
    Some((value as i32, &s[i + digits..]))
}

pub fn line_ok(cmd: &[u8]) -> bool {
    cmd.len() < HELPER_LINE_MAX - 1 && !cmd.iter().any(|&b| b == b'\r' || b == b'\n')
}

fn stream_file_name_ok(name: &[u8]) -> bool {
    let Some(stem) = name.strip_suffix(b".dat") else {
        return false;
    };
    let Some((&kind, rest)) = stem.split_first() else {
        return false;
    };
    let rest = rest.strip_prefix(b"-").unwrap_or(rest);
    let Some((pid, token)) = rest
        .iter()
        .position(|&b| b == b'-')
        .map(|i| rest.split_at(i))
    else {
        return false;
    };
    let digits = |s: &[u8]| (1..=10).contains(&s.len()) && s.iter().all(u8::is_ascii_digit);
    matches!(kind, b'a' | b'v') && digits(pid) && digits(&token[1..])
}

pub fn stream_file_parts(url: &[u8]) -> Option<(&[u8], &[u8])> {
    let rest = url.strip_prefix(b"file://")?;
    let path = rest.strip_prefix(b"localhost").unwrap_or(rest);
    let split = path.iter().rposition(|&b| b == b'/' || b == b'\\')?;
    let (dir, name) = (&path[..split], &path[split + 1..]);
    (!dir.is_empty() && stream_file_name_ok(name)).then_some((dir, name))
}

pub fn open_url(cmd: &[u8]) -> Option<&[u8]> {
    let mut s = cmd;
    while s.first().is_some_and(|&b| is_blank(b)) {
        s = &s[1..];
    }
    let op_len = s.iter().position(|&b| is_blank(b)).unwrap_or(s.len());
    let op = &s[..op_len];
    if op != b"open" && op != b"reload" {
        return None;
    }
    s = &s[op_len..];
    while s.first().is_some_and(|&b| is_blank(b)) {
        s = &s[1..];
    }
    let token_len = s.iter().position(|&b| is_blank(b)).unwrap_or(s.len());
    s = &s[token_len..];
    if !s.is_empty() {
        s = &s[1..];
    }
    while s.first() == Some(&b' ') {
        s = &s[1..];
    }
    Some(s)
}

#[derive(Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub clip_x: f64,
    pub clip_y: f64,
    pub clip_w: f64,
    pub clip_h: f64,
    pub fit: i32,
    pub valid: bool,
    pub page: bool,
}

pub struct RectCommand<'a> {
    pub token: &'a [u8],
    pub fields: usize,
    pub values: [i32; 10],
}

pub fn parse_rect(args: &[u8]) -> Option<RectCommand<'_>> {
    let (token, mut rest) = scan_token(args)?;
    let mut values = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0];
    let mut fields = 1;
    for value in &mut values {
        let Some((parsed, tail)) = scan_int(rest) else {
            break;
        };
        *value = parsed;
        rest = tail;
        fields += 1;
    }
    Some(RectCommand {
        token,
        fields,
        values,
    })
}

impl Rect {
    pub fn from_command(cmd: &RectCommand<'_>) -> Rect {
        let [x, y, w, h, _, cx, cy, cw, ch, page] = cmd.values.map(f64::from);
        let clipped = cmd.fields >= 10;
        Rect {
            x,
            y,
            w,
            h,
            fit: cmd.values[4],
            clip_x: if clipped { cx } else { x },
            clip_y: if clipped { cy } else { y },
            clip_w: if clipped { cw } else { w },
            clip_h: if clipped { ch } else { h },
            valid: cmd.values[2] > 0 && cmd.values[3] > 0,
            page: cmd.fields >= 11 && page != 0.0,
        }
    }

    pub fn draw_box(&self, frame_w: i32, frame_h: i32) -> (f64, f64, f64, f64) {
        let (fw, fh) = (f64::from(frame_w), f64::from(frame_h));
        if self.fit == 0 {
            return (self.x, self.y, self.w, self.h);
        }
        let scale_x = self.w / fw;
        let scale_y = self.h / fh;
        let scale = match self.fit {
            2 => scale_x.max(scale_y),
            3 => 1.0,
            4 => scale_x.min(scale_y).min(1.0),
            _ => scale_x.min(scale_y),
        };
        let draw_w = fw * scale;
        let draw_h = fh * scale;
        (
            self.x + (self.w - draw_w) * 0.5,
            self.y + (self.h - draw_h) * 0.5,
            draw_w,
            draw_h,
        )
    }
}

pub enum VideoCommand<'a> {
    Rect(RectCommand<'a>),
    Open(Option<&'a [u8]>),
    Play(Option<&'a [u8]>),
    Pause(Option<&'a [u8]>),
    Other,
}

fn first_token(rest: &[u8]) -> Option<&[u8]> {
    scan_token(rest).map(|(token, _)| token)
}

pub fn video_command(cmd: &[u8]) -> VideoCommand<'_> {
    if let Some(args) = cmd.strip_prefix(b"rect ") {
        return parse_rect(args).map_or(VideoCommand::Other, VideoCommand::Rect);
    }
    if let Some(args) = cmd.strip_prefix(b"open ") {
        return VideoCommand::Open(first_token(args));
    }
    if let Some(args) = cmd.strip_prefix(b"play ") {
        return VideoCommand::Play(first_token(args));
    }
    if let Some(args) = cmd
        .strip_prefix(b"pause ")
        .or_else(|| cmd.strip_prefix(b"stop "))
    {
        return VideoCommand::Pause(first_token(args));
    }
    VideoCommand::Other
}

pub enum HelperLine<'a> {
    Shm { token: &'a [u8], name: &'a [u8] },
    Closed(&'a [u8]),
    Playing(&'a [u8]),
    Paused(&'a [u8]),
    Waiting { token: &'a [u8], event: &'a [u8] },
    Ignored,
}

pub fn chomp(line: &[u8]) -> &[u8] {
    let end = line
        .iter()
        .rposition(|&b| !is_space(b))
        .map_or(0, |i| i + 1);
    &line[..end]
}

pub fn helper_line(clean: &[u8]) -> HelperLine<'_> {
    let tokens: Vec<&[u8]> = if clean.is_empty() {
        Vec::new()
    } else {
        clean.splitn(8, |&b| b == b' ').collect()
    };
    let n = tokens.len();
    match tokens.first().copied() {
        Some(b"shm") if n >= 6 => HelperLine::Shm {
            token: tokens[1],
            name: tokens[2],
        },
        Some(b"closed") if n >= 2 => HelperLine::Closed(tokens[1]),
        Some(b"playing") if n >= 2 => HelperLine::Playing(tokens[1]),
        Some(b"paused") if n >= 2 => HelperLine::Paused(tokens[1]),
        Some(event @ (b"ended" | b"stalled")) if n >= 2 => HelperLine::Waiting {
            token: tokens[1],
            event,
        },
        _ => HelperLine::Ignored,
    }
}

pub fn truncated(token: &[u8]) -> Vec<u8> {
    token[..token.len().min(TOKEN_MAX)].to_vec()
}

pub struct SlotView {
    pub sequence: u32,
    pub generation: u32,
    pub pts: f64,
}

pub trait FrameRing {
    fn slot(&self, index: u32) -> SlotView;
}

pub struct Pick {
    pub slot: u32,
    pub sequence: u32,
    pub pts: f64,
}

pub fn pick_frame(
    ring: &impl FrameRing,
    generation: u32,
    published: u32,
    position: f64,
    current: Option<(u32, u32)>,
    playing: bool,
) -> Option<Pick> {
    let mut selected: Option<Pick> = None;
    if let Some((sequence, slot)) = current {
        let meta = ring.slot(slot);
        if meta.sequence == sequence && meta.generation == generation {
            selected = Some(Pick {
                slot,
                sequence,
                pts: meta.pts,
            });
        }
    }
    let mut age = 0;
    while age < RING_SLOTS && age < published {
        let sequence = published - age;
        let slot = (sequence - 1) % RING_SLOTS;
        let meta = ring.slot(slot);
        age += 1;
        if meta.sequence != sequence || meta.generation != generation {
            continue;
        }
        let selected_sequence = selected.as_ref().map_or(0, |p| p.sequence);
        if meta.pts <= position + 0.010 && sequence >= selected_sequence {
            selected = Some(Pick {
                slot,
                sequence,
                pts: meta.pts,
            });
            break;
        }
    }
    if selected.is_none() && !playing {
        for age in (1..=RING_SLOTS).rev() {
            if age > published {
                continue;
            }
            let sequence = published - age + 1;
            let slot = (sequence - 1) % RING_SLOTS;
            let meta = ring.slot(slot);
            if meta.sequence == sequence && meta.generation == generation {
                selected = Some(Pick {
                    slot,
                    sequence,
                    pts: meta.pts,
                });
                break;
            }
        }
    }
    selected.filter(|pick| pick.sequence != 0)
}

pub fn clock_position(
    flags: u32,
    sample_us: i64,
    position: f64,
    now_us: i64,
    horizon_us: i64,
) -> Option<f64> {
    if flags & CLOCK_USED == 0 {
        return None;
    }
    let mut position = position;
    if flags & CLOCK_PLAYING != 0 {
        let elapsed = now_us - sample_us;
        if elapsed > 0 && elapsed < horizon_us {
            position += elapsed as f64 / 1e6;
        }
    }
    Some(position)
}
