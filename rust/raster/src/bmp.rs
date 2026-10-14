//! Southstar — BMP decoding: core, info and V2–V5 headers, 1/2/4/8-bit palettes, RLE4 and RLE8, 16- and 32-bit bit fields, 24- and 32-bit pixels, bottom-up and top-down rows.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::pixel::{Canvas, premul8, premul16};

const MAX_FIELD_DIM: u32 = 16_777_215;

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }

    fn u16(&mut self) -> Option<u32> {
        self.take(2)
            .map(|s| u32::from(u16::from_le_bytes([s[0], s[1]])))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    LowBit,
    Index8,
    Bitfields,
    Bgr,
    Bgrx,
    Bgra,
    Rle8,
    Rle4,
}

struct Masks {
    mask: [u32; 4],
    shift: [u32; 4],
    bits: [u32; 4],
}

impl Masks {
    fn process(mask: [u32; 4]) -> Option<Masks> {
        let mut out = Masks {
            mask,
            shift: [0; 4],
            bits: [0; 4],
        };
        for (i, &channel_mask) in mask.iter().enumerate() {
            let mut m = channel_mask;
            if m == 0 {
                if i != 3 {
                    return None;
                }
                continue;
            }
            let mut n = 0;
            while m & 1 == 0 {
                n += 1;
                m >>= 1;
            }
            out.shift[i] = n & 31;
            n = 0;
            while m & 1 == 1 {
                n += 1;
                m >>= 1;
            }
            if m != 0 || n > 32 {
                return None;
            }
            out.bits[i] = n;
        }
        Some(out)
    }

    fn channel(&self, v: u32, i: usize) -> u32 {
        if self.bits[i] == 0 {
            return 0xFFFF;
        }
        let mut c = (v & self.mask[i]) >> self.shift[i];
        let mut n = self.bits[i];
        while n < 16 {
            c |= c << n;
            n *= 2;
        }
        c >> (n - 16)
    }
}

struct Header {
    width: u32,
    height: u32,
    top_down: bool,
    bpp: u32,
    layout: Layout,
    masks: Option<Masks>,
    palette: [[u8; 4]; 256],
    pixels_at: usize,
}

fn parse(data: &[u8]) -> Option<Header> {
    let mut c = Cursor { data, pos: 0 };
    if c.take(2)? != b"BM" {
        return None;
    }
    c.skip(8)?;
    let mut padding = c.u32()?.checked_sub(14)?;
    let mut info_len = c.u32()?;
    padding = padding.checked_sub(info_len)?;
    let (width, height, bpp, mut compression);
    let mut top_down = false;
    let mut mask = [0u32; 4];
    let mut masks = None;
    match info_len {
        12 => {
            width = c.u16()?;
            height = c.u16()?;
            if c.u16()? != 1 {
                return None;
            }
            bpp = c.u16()?;
            compression = 0;
        }
        16 => {
            width = c.u32()?;
            if width > MAX_FIELD_DIM {
                return None;
            }
            height = c.u32()?;
            if height > MAX_FIELD_DIM {
                return None;
            }
            if c.u16()? != 1 {
                return None;
            }
            bpp = c.u16()?;
            compression = 0;
        }
        _ => {
            width = c.u32()?;
            if width > MAX_FIELD_DIM {
                return None;
            }
            let raw_h = c.u32()?;
            if raw_h == 0x8000_0000 {
                return None;
            }
            if raw_h > 0x8000_0000 {
                height = raw_h.wrapping_neg();
                top_down = true;
            } else {
                height = raw_h;
            }
            if height > MAX_FIELD_DIM {
                return None;
            }
            if c.u16()? != 1 {
                return None;
            }
            bpp = c.u16()?;
            compression = c.u32()?;
            if bpp == 0 {
                return None;
            }
            c.skip(20)?;
            if info_len == 40 {
                if bpp >= 16 {
                    if padding >= 16 {
                        info_len = 56;
                        padding -= 16;
                    } else if padding >= 12 {
                        info_len = 52;
                        padding -= 12;
                    }
                }
            } else if !matches!(info_len, 52 | 56 | 64 | 108 | 124) {
                return None;
            }
            if compression == 6 {
                compression = 3;
            }
            if compression == 3 {
                if info_len >= 52 {
                    mask[2] = c.u32()?;
                    mask[1] = c.u32()?;
                    mask[0] = c.u32()?;
                    if info_len >= 56 {
                        mask[3] = c.u32()?;
                        c.skip((info_len - 56) as usize)?;
                    }
                    if mask[0] == 0xFF
                        && mask[1] == 0xFF00
                        && mask[2] == 0x00FF_0000
                        && (bpp == 24 || (bpp == 32 && (mask[3] == 0 || mask[3] == 0xFF00_0000)))
                    {
                        compression = 0;
                    }
                    masks = Some(Masks::process(mask)?);
                }
            } else {
                c.skip((info_len - 40) as usize)?;
            }
        }
    }
    let mut palette = [[0u8, 0, 0, 0xFF]; 256];
    if compression != 3 && bpp < 16 {
        let entry = if info_len == 12 { 3 } else { 4 };
        let mut i = 0;
        while i < 256 && padding >= entry {
            padding -= entry;
            let e = c.take(entry as usize)?;
            palette[i] = [e[0], e[1], e[2], 0xFF];
            i += 1;
        }
    }
    let layout = match compression {
        0 => match bpp {
            1 | 2 | 4 => Layout::LowBit,
            8 => Layout::Index8,
            16 => {
                masks = Some(Masks::process([31, 992, 31744, 0])?);
                Layout::Bitfields
            }
            24 => Layout::Bgr,
            32 if mask[3] == 0 => Layout::Bgrx,
            32 => Layout::Bgra,
            _ => return None,
        },
        1 if bpp == 8 => Layout::Rle8,
        2 if bpp == 4 => Layout::Rle4,
        3 if bpp == 16 || bpp == 32 => Layout::Bitfields,
        _ => return None,
    };
    if layout == Layout::Bitfields && masks.is_none() {
        masks = Some(Masks {
            mask: [0; 4],
            shift: [0; 4],
            bits: [0; 4],
        });
    }
    if (info_len < 40 || info_len == 64) && !matches!(bpp, 1 | 4 | 8 | 24) {
        return None;
    }
    let pixels_at = c.pos.checked_add(padding as usize)?;
    Some(Header {
        width,
        height,
        top_down,
        bpp,
        layout,
        masks,
        palette,
        pixels_at,
    })
}

struct Rows {
    y: u32,
    inc: u32,
    height: u32,
}

impl Rows {
    fn new(h: &Header) -> Rows {
        Rows {
            y: if h.top_down { 0 } else { h.height - 1 },
            inc: if h.top_down { 1 } else { u32::MAX },
            height: h.height,
        }
    }

    fn advance(&mut self) -> bool {
        self.y = self.y.wrapping_add(self.inc);
        self.y < self.height
    }
}

fn put(canvas: &mut Canvas, x: u32, y: u32, px: [u8; 4]) {
    if x < canvas.width && y < canvas.height {
        let o = (y as usize * canvas.width as usize + x as usize) * 4;
        canvas.pixels[o..o + 4].copy_from_slice(&px);
    }
}

fn decode_plain(h: &Header, data: &[u8], canvas: &mut Canvas) -> Option<()> {
    let mut c = Cursor {
        data,
        pos: h.pixels_at,
    };
    let w = h.width;
    let pad = match h.bpp {
        8 => (4 - (w & 3)) & 3,
        16 => (w & 1) * 2,
        24 => w & 3,
        _ => 0,
    } as usize;
    let mut rows = Rows::new(h);
    loop {
        let y = rows.y;
        for x in 0..w {
            let px = match h.layout {
                Layout::Index8 => h.palette[usize::from(c.u8()?)],
                Layout::Bgr => {
                    let s = c.take(3)?;
                    [s[0], s[1], s[2], 0xFF]
                }
                Layout::Bgrx => {
                    let s = c.take(4)?;
                    [s[0], s[1], s[2], 0xFF]
                }
                Layout::Bgra => {
                    let s = c.take(4)?;
                    premul8(
                        u32::from(s[2]),
                        u32::from(s[1]),
                        u32::from(s[0]),
                        u32::from(s[3]),
                    )
                }
                _ => {
                    let m = h.masks.as_ref()?;
                    let v = if h.bpp == 16 { c.u16()? } else { c.u32()? };
                    premul16(
                        m.channel(v, 2),
                        m.channel(v, 1),
                        m.channel(v, 0),
                        m.channel(v, 3),
                    )
                }
            };
            put(canvas, x, y, px);
        }
        c.skip(pad)?;
        if !rows.advance() {
            return Some(());
        }
    }
}

fn decode_low_bit(h: &Header, data: &[u8], canvas: &mut Canvas) -> Option<()> {
    let mut c = Cursor {
        data,
        pos: h.pixels_at,
    };
    let per_chunk = 32 / h.bpp;
    let chunks = h.width.div_ceil(per_chunk);
    let mask = (1u32 << h.bpp) - 1;
    let mut rows = Rows::new(h);
    loop {
        let y = rows.y;
        let mut x = 0;
        for _ in 0..chunks {
            let bits = u32::from_be_bytes(c.take(4)?.try_into().ok()?);
            for k in 0..per_chunk {
                if x < h.width {
                    let shift = 32 - h.bpp * (k + 1);
                    let idx = (bits >> shift) & mask;
                    put(canvas, x, y, h.palette[idx as usize]);
                }
                x += 1;
            }
        }
        if !rows.advance() {
            return Some(());
        }
    }
}

fn decode_rle(h: &Header, data: &[u8], canvas: &mut Canvas) -> Option<()> {
    let mut c = Cursor {
        data,
        pos: h.pixels_at,
    };
    let eight = h.layout == Layout::Rle8;
    let mut rows = Rows::new(h);
    let mut x = 0u32;
    loop {
        let code = c.u8()?;
        if code != 0 {
            let value = c.u8()?;
            for k in 0..u32::from(code) {
                let idx = if eight {
                    value
                } else if k % 2 == 0 {
                    value >> 4
                } else {
                    value & 15
                };
                put(
                    canvas,
                    x.saturating_add(k),
                    rows.y,
                    h.palette[usize::from(idx)],
                );
            }
            x = x.saturating_add(u32::from(code));
            continue;
        }
        match c.u8()? {
            0 => {
                if rows.y >= h.height {
                    return None;
                }
                x = 0;
                rows.y = rows.y.wrapping_add(rows.inc);
            }
            1 => return Some(()),
            2 => {
                let dx = c.u8()?;
                let dy = c.u8()?;
                if dx > 0 {
                    x = x.saturating_add(u32::from(dx));
                    if x > h.width {
                        return None;
                    }
                }
                for _ in 0..dy {
                    rows.y = rows.y.wrapping_add(rows.inc);
                    if rows.y >= h.height {
                        return None;
                    }
                }
            }
            n => {
                let n = u32::from(n);
                if eight {
                    let room = if rows.y < h.height {
                        h.width.saturating_sub(x)
                    } else {
                        0
                    };
                    if n > room {
                        return None;
                    }
                    let run = c.take(n as usize)?;
                    for (k, &idx) in run.iter().enumerate() {
                        put(
                            canvas,
                            x.saturating_add(k as u32),
                            rows.y,
                            h.palette[usize::from(idx)],
                        );
                    }
                    if n & 1 != 0 {
                        c.skip(1)?;
                    }
                } else {
                    let bytes = n.div_ceil(4) as usize * 2;
                    let run = c.take(bytes)?;
                    for k in 0..n {
                        let b = run[(k / 2) as usize];
                        let idx = if k % 2 == 0 { b >> 4 } else { b & 15 };
                        put(
                            canvas,
                            x.saturating_add(k),
                            rows.y,
                            h.palette[usize::from(idx)],
                        );
                    }
                }
                x = x.saturating_add(n);
            }
        }
    }
}

pub fn decode(data: &[u8]) -> Option<Canvas> {
    let h = parse(data)?;
    let mut canvas = Canvas::new(h.width, h.height)?;
    match h.layout {
        Layout::LowBit => decode_low_bit(&h, data, &mut canvas)?,
        Layout::Rle8 | Layout::Rle4 => decode_rle(&h, data, &mut canvas)?,
        _ => decode_plain(&h, data, &mut canvas)?,
    }
    Some(canvas)
}
