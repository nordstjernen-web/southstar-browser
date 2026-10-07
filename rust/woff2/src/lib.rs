//! Southstar — WOFF2 web font decoding: the Brotli stream and the glyf/loca and hmtx transforms.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

const MAX_OUTPUT: usize = 64 * 1024 * 1024;
const MAX_TABLES: u16 = 1024;

const fn tag(name: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*name)
}

const TTCF: u32 = tag(b"ttcf");
const OTTO: u32 = tag(b"OTTO");
const GLYF: u32 = tag(b"glyf");
const LOCA: u32 = tag(b"loca");
const HMTX: u32 = tag(b"hmtx");
const HEAD: u32 = tag(b"head");
const HHEA: u32 = tag(b"hhea");

const KNOWN_TAGS: [&[u8; 4]; 63] = [
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"cvt ", b"fpgm",
    b"glyf", b"loca", b"prep", b"CFF ", b"VORG", b"EBDT", b"EBLC", b"gasp", b"hdmx", b"kern",
    b"LTSH", b"PCLT", b"VDMX", b"vhea", b"vmtx", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"EBSC",
    b"JSTF", b"MATH", b"CBDT", b"CBLC", b"COLR", b"CPAL", b"SVG ", b"sbix", b"acnt", b"avar",
    b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar", b"gvar", b"hsty",
    b"just", b"lcar", b"mort", b"morx", b"opbd", b"prop", b"trak", b"Zapf", b"Silf", b"Glat",
    b"Gloc", b"Feat", b"Sill",
];

pub struct Sfnt {
    pub bytes: Vec<u8>,
    pub cff: bool,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let at = self.data.get(self.pos..)?.get(..n)?;
        self.pos += n;
        Some(at)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.bytes(N)?.try_into().ok()
    }

    fn u8(&mut self) -> Option<u8> {
        self.array().map(u8::from_be_bytes)
    }

    fn u16(&mut self) -> Option<u16> {
        self.array().map(u16::from_be_bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_be_bytes)
    }

    fn base128(&mut self) -> Option<u32> {
        let mut value: u32 = 0;
        for i in 0..5 {
            let b = self.u8()?;
            if (i == 0 && b == 0x80) || value & 0xFE00_0000 != 0 {
                return None;
            }
            value = (value << 7) | u32::from(b & 0x7F);
            if b & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn u255_16(&mut self) -> Option<u16> {
        Some(match self.u8()? {
            253 => self.u16()?,
            255 => u16::from(self.u8()?) + 253,
            254 => u16::from(self.u8()?) + 506,
            code => u16::from(code),
        })
    }

    fn sub(&mut self, n: u32) -> Option<Reader<'a>> {
        self.bytes(n as usize).map(Reader::new)
    }
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn padded(len: usize) -> usize {
    (len + 3) & !3
}

fn pad4(out: &mut Vec<u8>) {
    out.resize(padded(out.len()), 0);
}

fn bit_set(bits: &[u8], index: usize) -> bool {
    bits[index >> 3] & (0x80 >> (index & 7)) != 0
}

struct Point {
    x: i32,
    y: i32,
    on_curve: bool,
}

fn with_sign(flag: i32, base: i32) -> i32 {
    if flag & 1 != 0 { base } else { -base }
}

fn triplet_delta(flag: i32, b: &[u8]) -> (i32, i32) {
    let byte = |i: usize| i32::from(b[i]);
    match flag {
        0..10 => (0, with_sign(flag, ((flag & 14) << 7) + byte(0))),
        10..20 => (with_sign(flag, (((flag - 10) & 14) << 7) + byte(0)), 0),
        20..84 => {
            let b0 = flag - 20;
            (
                with_sign(flag, 1 + (b0 & 0x30) + (byte(0) >> 4)),
                with_sign(flag >> 1, 1 + ((b0 & 0x0C) << 2) + (byte(0) & 0x0F)),
            )
        }
        84..120 => {
            let b0 = flag - 84;
            (
                with_sign(flag, 1 + ((b0 / 12) << 8) + byte(0)),
                with_sign(flag >> 1, 1 + (((b0 % 12) >> 2) << 8) + byte(1)),
            )
        }
        120..124 => (
            with_sign(flag, (byte(0) << 4) + (byte(1) >> 4)),
            with_sign(flag >> 1, ((byte(1) & 0x0F) << 8) + byte(2)),
        ),
        _ => (
            with_sign(flag, (byte(0) << 8) + byte(1)),
            with_sign(flag >> 1, (byte(2) << 8) + byte(3)),
        ),
    }
}

fn decode_triplets(glyphs: &mut Reader, flags: &[u8]) -> Option<Vec<Point>> {
    let coordinate = i32::from(i16::MIN)..=i32::from(i16::MAX);
    let mut points = Vec::with_capacity(flags.len());
    let (mut x, mut y) = (0, 0);
    for &raw in flags {
        let flag = i32::from(raw & 0x7F);
        let n = match flag {
            0..84 => 1,
            84..120 => 2,
            120..124 => 3,
            _ => 4,
        };
        let (dx, dy) = triplet_delta(flag, glyphs.bytes(n)?);
        x += dx;
        y += dy;
        if !coordinate.contains(&x) || !coordinate.contains(&y) {
            return None;
        }
        points.push(Point {
            x,
            y,
            on_curve: raw & 0x80 == 0,
        });
    }
    Some(points)
}

fn write_points_bbox(out: &mut Vec<u8>, points: &[Point]) -> i16 {
    let (mut x0, mut y0) = (points[0].x, points[0].y);
    let (mut x1, mut y1) = (x0, y0);
    for p in &points[1..] {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    for v in [x0, y0, x1, y1] {
        put_u16(out, v as u16);
    }
    x0 as i16
}

fn write_point_data(out: &mut Vec<u8>, points: &[Point], overlap: bool) {
    for (i, p) in points.iter().enumerate() {
        let on_curve = if p.on_curve { 0x01 } else { 0x00 };
        let overlap_flag = if i == 0 && overlap { 0x40 } else { 0x00 };
        out.push(on_curve | overlap_flag);
    }
    let mut prev = 0;
    for p in points {
        put_u16(out, (p.x - prev) as u16);
        prev = p.x;
    }
    prev = 0;
    for p in points {
        put_u16(out, (p.y - prev) as u16);
        prev = p.y;
    }
}

struct GlyfStreams<'a> {
    n_contours: Reader<'a>,
    n_points: Reader<'a>,
    flags: Reader<'a>,
    glyphs: Reader<'a>,
    composite: Reader<'a>,
    bbox: Reader<'a>,
    instructions: Reader<'a>,
    bbox_bitmap: &'a [u8],
    overlap_bitmap: Option<&'a [u8]>,
}

impl<'a> GlyfStreams<'a> {
    fn read(r: &mut Reader<'a>) -> Option<(Self, usize)> {
        r.u16()?;
        let options = r.u16()?;
        let num_glyphs = usize::from(r.u16()?);
        r.u16()?;
        let mut sizes = [0; 7];
        for size in &mut sizes {
            *size = r.u32()?;
        }
        let n_contours = r.sub(sizes[0])?;
        let n_points = r.sub(sizes[1])?;
        let flags = r.sub(sizes[2])?;
        let glyphs = r.sub(sizes[3])?;
        let composite = r.sub(sizes[4])?;
        let mut bbox = r.sub(sizes[5])?;
        let instructions = r.sub(sizes[6])?;
        let bbox_bitmap = bbox.bytes(num_glyphs.div_ceil(32) * 4)?;
        let overlap_bitmap = if options & 1 != 0 {
            Some(r.bytes(num_glyphs.div_ceil(8))?)
        } else {
            None
        };
        let streams = GlyfStreams {
            n_contours,
            n_points,
            flags,
            glyphs,
            composite,
            bbox,
            instructions,
            bbox_bitmap,
            overlap_bitmap,
        };
        Some((streams, num_glyphs))
    }

    fn copy_instructions(&mut self, out: &mut Vec<u8>) -> Option<()> {
        let n = self.glyphs.u255_16()?;
        let code = self.instructions.bytes(usize::from(n))?;
        put_u16(out, n);
        out.extend_from_slice(code);
        Some(())
    }

    fn copy_bbox(&mut self, out: &mut Vec<u8>) -> Option<i16> {
        let b = self.bbox.bytes(8)?;
        out.extend_from_slice(b);
        Some(i16::from_be_bytes([b[0], b[1]]))
    }

    fn write_composite_glyph(&mut self, out: &mut Vec<u8>) -> Option<i16> {
        put_u16(out, 0xFFFF);
        let x_min = self.copy_bbox(out)?;
        let start = self.composite.pos;
        let mut instructions = false;
        loop {
            let flags = self.composite.u16()?;
            let mut args = 2 + if flags & 0x0001 != 0 { 4 } else { 2 };
            if flags & 0x0008 != 0 {
                args += 2;
            } else if flags & 0x0040 != 0 {
                args += 4;
            } else if flags & 0x0080 != 0 {
                args += 8;
            }
            self.composite.bytes(args)?;
            instructions |= flags & 0x0100 != 0;
            if flags & 0x0020 == 0 {
                break;
            }
        }
        out.extend_from_slice(&self.composite.data[start..self.composite.pos]);
        if instructions {
            self.copy_instructions(out)?;
        }
        Some(x_min)
    }

    fn read_end_points(&mut self, n_contours: u16) -> Option<(Vec<u16>, usize)> {
        let mut end_points = Vec::with_capacity(usize::from(n_contours));
        let mut n_points: u32 = 0;
        for _ in 0..n_contours {
            n_points += u32::from(self.n_points.u255_16()?);
            if n_points == 0 || n_points > 0xFFFF {
                return None;
            }
            end_points.push((n_points - 1) as u16);
        }
        Some((end_points, n_points as usize))
    }

    fn write_simple_glyph(
        &mut self,
        glyph: usize,
        n_contours: u16,
        has_bbox: bool,
        out: &mut Vec<u8>,
    ) -> Option<i16> {
        let (end_points, n_points) = self.read_end_points(n_contours)?;
        let flags = self.flags.bytes(n_points)?;
        let points = decode_triplets(&mut self.glyphs, flags)?;
        put_u16(out, n_contours);
        let x_min = if has_bbox {
            self.copy_bbox(out)?
        } else {
            write_points_bbox(out, &points)
        };
        for end_point in end_points {
            put_u16(out, end_point);
        }
        self.copy_instructions(out)?;
        let overlap = self.overlap_bitmap.is_some_and(|bits| bit_set(bits, glyph));
        write_point_data(out, &points, overlap);
        Some(x_min)
    }

    fn write_glyph(&mut self, glyph: usize, out: &mut Vec<u8>) -> Option<i16> {
        let n_contours = self.n_contours.u16()? as i16;
        let has_bbox = bit_set(self.bbox_bitmap, glyph);
        match n_contours {
            0 if !has_bbox => Some(0),
            -1 if has_bbox => self.write_composite_glyph(out),
            1.. => self.write_simple_glyph(glyph, n_contours.unsigned_abs(), has_bbox, out),
            _ => None,
        }
    }
}

struct Outlines {
    glyf: Vec<u8>,
    loca: Vec<u8>,
    x_mins: Vec<i16>,
}

fn reconstruct_glyf(data: &[u8]) -> Option<Outlines> {
    let mut r = Reader::new(data);
    let (mut streams, num_glyphs) = GlyfStreams::read(&mut r)?;
    let mut glyf = Vec::with_capacity((data.len() * 2).min(MAX_OUTPUT));
    let mut offsets = Vec::with_capacity(num_glyphs + 1);
    let mut x_mins = Vec::with_capacity(num_glyphs);
    for glyph in 0..num_glyphs {
        offsets.push(glyf.len() as u32);
        x_mins.push(streams.write_glyph(glyph, &mut glyf)?);
        pad4(&mut glyf);
        if glyf.len() > MAX_OUTPUT {
            return None;
        }
    }
    offsets.push(glyf.len() as u32);
    let loca = offsets.iter().flat_map(|o| o.to_be_bytes()).collect();
    Some(Outlines { glyf, loca, x_mins })
}

fn reconstruct_hmtx(data: &[u8], num_hmetrics: u16, x_mins: &[i16]) -> Option<Vec<u8>> {
    let num_hmetrics = usize::from(num_hmetrics);
    let num_glyphs = x_mins.len();
    if num_hmetrics == 0 || num_hmetrics > num_glyphs {
        return None;
    }
    let mut r = Reader::new(data);
    let flags = r.u8()?;
    let advances = (0..num_hmetrics)
        .map(|_| r.u16())
        .collect::<Option<Vec<u16>>>()?;
    let mut out = Vec::with_capacity(num_hmetrics * 2 + num_glyphs * 2);
    for (i, &x_min) in x_mins.iter().enumerate() {
        let explicit = if i < num_hmetrics {
            flags & 1 == 0
        } else {
            flags & 2 == 0
        };
        let lsb = if explicit { r.u16()? as i16 } else { x_min };
        if let Some(&advance) = advances.get(i) {
            put_u16(&mut out, advance);
        }
        put_u16(&mut out, lsb as u16);
    }
    Some(out)
}

struct Table {
    tag: u32,
    transformed: bool,
    orig_len: u32,
    stream_off: usize,
    stream_len: usize,
    data: Option<Vec<u8>>,
}

impl Table {
    fn stream<'a>(&self, stream: &'a [u8]) -> &'a [u8] {
        &stream[self.stream_off..self.stream_off + self.stream_len]
    }
}

fn find(tables: &[Table], tag: u32) -> Option<usize> {
    tables.iter().position(|t| t.tag == tag)
}

fn is_transformed(tables: &[Table], index: Option<usize>) -> bool {
    index.is_some_and(|i| tables[i].transformed)
}

fn transform(tag: u32, version: u8) -> Option<bool> {
    if tag == GLYF || tag == LOCA {
        matches!(version, 0 | 3).then_some(version == 0)
    } else {
        let transformed = tag == HMTX && version == 1;
        (version == 0 || transformed).then_some(transformed)
    }
}

fn read_table_tag(r: &mut Reader, index: u8) -> Option<u32> {
    match KNOWN_TAGS.get(usize::from(index)) {
        Some(known) => Some(tag(known)),
        None => r.u32(),
    }
}

fn read_directory(r: &mut Reader, n: u16) -> Option<(Vec<Table>, usize)> {
    let mut tables = Vec::with_capacity(usize::from(n));
    let mut total = 0;
    for _ in 0..n {
        let flags = r.u8()?;
        let tag = read_table_tag(r, flags & 0x3F)?;
        let orig_len = r.base128()?;
        let transformed = transform(tag, flags >> 6)?;
        let stream_len = if transformed { r.base128()? } else { orig_len };
        let stream_len = stream_len as usize;
        if stream_len > MAX_OUTPUT {
            return None;
        }
        tables.push(Table {
            tag,
            transformed,
            orig_len,
            stream_off: total,
            stream_len,
            data: None,
        });
        total += stream_len;
        if total > MAX_OUTPUT {
            return None;
        }
    }
    Some((tables, total))
}

fn reconstruct_transformed(stream: &[u8], tables: &mut [Table]) -> Option<()> {
    let glyf = find(tables, GLYF);
    let loca = find(tables, LOCA);
    let hmtx = find(tables, HMTX);
    let head = find(tables, HEAD);
    let hhea = find(tables, HHEA);
    if is_transformed(tables, glyf) != is_transformed(tables, loca) {
        return None;
    }
    let mut x_mins = None;
    if let Some(glyf) = glyf.filter(|&i| tables[i].transformed) {
        let loca = loca?;
        let head = head?;
        if tables[loca].stream_len != 0 || tables[head].orig_len < 54 {
            return None;
        }
        let outlines = reconstruct_glyf(tables[glyf].stream(stream))?;
        tables[glyf].data = Some(outlines.glyf);
        tables[loca].data = Some(outlines.loca);
        x_mins = Some(outlines.x_mins);
    }
    if let Some(hmtx) = hmtx.filter(|&i| tables[i].transformed) {
        let x_mins = x_mins?;
        let hhea = hhea.filter(|&i| tables[i].orig_len >= 36)?;
        let metrics = tables[hhea].stream(stream);
        let num_hmetrics = u16::from_be_bytes([metrics[34], metrics[35]]);
        tables[hmtx].data = Some(reconstruct_hmtx(
            tables[hmtx].stream(stream),
            num_hmetrics,
            &x_mins,
        )?);
    }
    Some(())
}

fn reconstruct_tables(stream: &[u8], tables: &mut [Table]) -> Option<()> {
    reconstruct_transformed(stream, tables)?;
    for table in tables.iter_mut() {
        if table.data.is_some() {
            continue;
        }
        if table.transformed {
            return None;
        }
        table.data = Some(table.stream(stream).to_vec());
    }
    let glyf_transformed = is_transformed(tables, find(tables, GLYF));
    let head = find(tables, HEAD).and_then(|i| tables[i].data.as_mut());
    if let Some(head) = head.filter(|data| data.len() >= 54) {
        head[8..12].fill(0);
        if glyf_transformed {
            head[50] = 0;
            head[51] = 1;
        }
    }
    Some(())
}

fn table_data(table: &Table) -> &[u8] {
    table.data.as_deref().unwrap_or_default()
}

fn table_checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn sfnt_size(tables: &[Table]) -> Option<usize> {
    let mut total = 12 + tables.len() * 16;
    for table in tables {
        total += padded(table_data(table).len());
        if total > MAX_OUTPUT {
            return None;
        }
    }
    Some(total)
}

fn assemble_sfnt(flavor: u32, tables: &[Table], size: usize) -> Vec<u8> {
    let mut order: Vec<&Table> = tables.iter().collect();
    order.sort_by_key(|t| t.tag);
    let n = order.len() as u16;
    let mut selector: u16 = 0;
    while (1u32 << (selector + 1)) <= u32::from(n) {
        selector += 1;
    }
    let range = (1u16 << selector) * 16;

    let mut out = Vec::with_capacity(size);
    put_u32(&mut out, flavor);
    put_u16(&mut out, n);
    put_u16(&mut out, range);
    put_u16(&mut out, selector);
    put_u16(&mut out, n * 16 - range);
    let mut offset = 12 + order.len() * 16;
    for table in &order {
        let data = table_data(table);
        put_u32(&mut out, table.tag);
        put_u32(&mut out, table_checksum(data));
        put_u32(&mut out, offset as u32);
        put_u32(&mut out, data.len() as u32);
        offset += padded(data.len());
    }
    for table in &order {
        out.extend_from_slice(table_data(table));
        pad4(&mut out);
    }
    out
}

fn decompress(compressed: &[u8], size: usize) -> Option<Vec<u8>> {
    let mut stream = Vec::new();
    stream.try_reserve_exact(size.max(1)).ok()?;
    stream.resize(size, 0);
    (ffi::brotli_decompress(compressed, &mut stream)? == size).then_some(stream)
}

pub fn is_woff2(data: &[u8]) -> bool {
    data.starts_with(b"wOF2")
}

pub fn to_sfnt(data: &[u8]) -> Option<Sfnt> {
    if !is_woff2(data) || data.len() < 48 {
        return None;
    }
    let mut r = Reader { data, pos: 4 };
    let flavor = r.u32()?;
    r.u32()?;
    let num_tables = r.u16()?;
    r.u16()?;
    r.u32()?;
    let compressed_len = r.u32()?;
    r.pos = 48;
    if flavor == TTCF || num_tables == 0 || num_tables > MAX_TABLES {
        return None;
    }
    let (mut tables, stream_total) = read_directory(&mut r, num_tables)?;
    let compressed = r.bytes(compressed_len as usize)?;
    let stream = decompress(compressed, stream_total)?;
    reconstruct_tables(&stream, &mut tables)?;
    let size = sfnt_size(&tables)?;
    Some(Sfnt {
        bytes: assemble_sfnt(flavor, &tables, size),
        cff: flavor == OTTO,
    })
}
