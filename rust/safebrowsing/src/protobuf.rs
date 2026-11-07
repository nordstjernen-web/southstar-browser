//! Southstar — the protobuf wire reader and Rice-Golomb delta decoding behind Google Safe Browsing's binary responses.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) const MAX_ENTRIES: u32 = 1 << 25;

pub(crate) enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

pub(crate) struct Field<'a> {
    pub number: u32,
    pub value: Value<'a>,
}

pub(crate) struct Reader<'a> {
    rest: &'a [u8],
    pub bad: bool,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader {
            rest: data,
            bad: false,
        }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let (&byte, rest) = self.rest.split_first()?;
            self.rest = rest;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.rest.len() {
            return None;
        }
        let (head, rest) = self.rest.split_at(n);
        self.rest = rest;
        Some(head)
    }

    fn field(&mut self) -> Option<Field<'a>> {
        let key = self.varint()?;
        let number = u32::try_from(key >> 3).ok()?;
        let value = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => {
                self.take(8)?;
                Value::Fixed
            }
            5 => {
                self.take(4)?;
                Value::Fixed
            }
            2 => {
                let len = usize::try_from(self.varint()?).ok()?;
                Value::Bytes(self.take(len)?)
            }
            _ => return None,
        };
        Some(Field { number, value })
    }
}

impl<'a> Iterator for Reader<'a> {
    type Item = Field<'a>;

    fn next(&mut self) -> Option<Field<'a>> {
        if self.bad || self.rest.is_empty() {
            return None;
        }
        let field = self.field();
        self.bad = field.is_none();
        field
    }
}

pub(crate) fn duration_seconds(data: &[u8]) -> Option<u64> {
    let mut seconds = None;
    for field in Reader::new(data) {
        if let (1, Value::Varint(n)) = (field.number, field.value) {
            seconds = Some(n);
        }
    }
    seconds
}

#[derive(Default)]
pub(crate) struct Rice<'a> {
    present: bool,
    first: u32,
    parameter: u32,
    count: u32,
    data: &'a [u8],
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn next(&mut self) -> Option<bool> {
        let byte = *self.data.get(self.pos >> 3)?;
        let bit = (byte >> (self.pos & 7)) & 1 == 1;
        self.pos += 1;
        Some(bit)
    }
}

impl<'a> Rice<'a> {
    fn parse(data: &'a [u8]) -> Option<Rice<'a>> {
        let mut rice = Rice {
            present: true,
            ..Rice::default()
        };
        let mut reader = Reader::new(data);
        for field in reader.by_ref() {
            match (field.number, field.value) {
                (1, Value::Varint(n)) => rice.first = n as u32,
                (2, Value::Varint(n)) => rice.parameter = n as u32,
                (3, Value::Varint(n)) => rice.count = n as u32,
                (4, Value::Bytes(bytes)) => rice.data = bytes,
                _ => {}
            }
        }
        (!reader.bad).then_some(rice)
    }

    pub fn decode(&self) -> Option<Vec<u32>> {
        if !self.present {
            return Some(Vec::new());
        }
        if self.count >= MAX_ENTRIES || (self.count > 0 && !(1..=31).contains(&self.parameter)) {
            return None;
        }
        let capacity = (self.count as usize + 1).min(self.data.len() * 8 + 1);
        let mut values = Vec::with_capacity(capacity);
        values.push(self.first);
        let mut current = u64::from(self.first);
        let mut bits = Bits {
            data: self.data,
            pos: 0,
        };
        for _ in 0..self.count {
            let mut quotient = 0u64;
            while bits.next()? {
                quotient += 1;
            }
            let mut remainder = 0u64;
            for shift in 0..self.parameter {
                remainder |= u64::from(bits.next()?) << shift;
            }
            if quotient > u64::from(u32::MAX >> self.parameter) {
                return None;
            }
            current += (quotient << self.parameter) + remainder;
            values.push(u32::try_from(current).ok()?);
        }
        Some(values)
    }
}

pub(crate) struct HashList<'a> {
    pub version: Vec<u8>,
    pub partial: bool,
    pub additions: Rice<'a>,
    pub removals: Rice<'a>,
    pub min_wait_s: u64,
    pub checksum: Option<&'a [u8]>,
}

pub(crate) fn hash_list(data: &[u8]) -> Option<HashList<'_>> {
    let mut list = HashList {
        version: Vec::new(),
        partial: false,
        additions: Rice::default(),
        removals: Rice::default(),
        min_wait_s: 0,
        checksum: None,
    };
    let mut reader = Reader::new(data);
    for field in reader.by_ref() {
        match (field.number, field.value) {
            (2, Value::Bytes(bytes)) => list.version = bytes.to_vec(),
            (3, Value::Varint(n)) => list.partial = n != 0,
            (4, Value::Bytes(bytes)) => list.additions = Rice::parse(bytes)?,
            (5, Value::Bytes(bytes)) => list.removals = Rice::parse(bytes)?,
            (6, Value::Bytes(bytes)) => {
                list.min_wait_s = duration_seconds(bytes).unwrap_or(list.min_wait_s);
            }
            (7, Value::Bytes(bytes)) => list.checksum = Some(bytes),
            (9..=11, Value::Bytes(_)) => return None,
            _ => {}
        }
    }
    (!reader.bad).then_some(list)
}
