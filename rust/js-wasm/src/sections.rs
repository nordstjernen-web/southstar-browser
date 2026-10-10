//! Southstar — reads the export names of a validated wasm binary in declaration order, which the JS API exposes and wasmi does not keep.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const EXPORT_SECTION: u8 = 7;
const HEADER_LEN: usize = 8;

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn leb(&mut self) -> Option<u32> {
        let mut result = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            result |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
        }
        None
    }

    fn name(&mut self) -> Option<String> {
        let len = self.leb()? as usize;
        let end = self.pos.checked_add(len)?;
        let text = core::str::from_utf8(self.bytes.get(self.pos..end)?).ok()?;
        self.pos = end;
        Some(text.to_owned())
    }

    fn exports(&mut self) -> Option<Vec<String>> {
        let count = self.leb()?;
        let mut names = Vec::new();
        for _ in 0..count {
            names.push(self.name()?);
            self.byte()?;
            self.leb()?;
        }
        Some(names)
    }
}

pub(crate) fn export_names(bytes: &[u8]) -> Vec<String> {
    let mut reader = Reader {
        bytes,
        pos: HEADER_LEN,
    };
    while reader.pos < bytes.len() {
        let (Some(id), Some(size)) = (reader.byte(), reader.leb()) else {
            break;
        };
        let Some(end) = reader.pos.checked_add(size as usize) else {
            break;
        };
        if id == EXPORT_SECTION {
            return reader.exports().unwrap_or_default();
        }
        reader.pos = end;
    }
    Vec::new()
}
