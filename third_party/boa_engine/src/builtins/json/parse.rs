//! A JSON text parser for `JSON.parse` and `JSON.rawJSON`.
//!
//! The parser reads the code units of the string directly and builds the resulting values as it
//! goes, without recursion, so nesting depth is limited only by memory. When a reviver needs the
//! source text of primitive values, it also builds a [`JsonRecord`] tree (the JSON Parse Records of
//! the `JSON.parse` source text access proposal).

use crate::{
    Context, JsResult, JsString, JsValue,
    builtins::Array,
    error::JsNativeError,
    object::JsObject,
    string::{JsStr, JsStrVariant},
};

/// The parse record of a value: the value as parsed, and for primitives the source text that
/// produced it, for arrays the records of the elements and for objects those of the entries.
#[derive(Debug)]
pub(super) struct JsonRecord {
    pub(super) value: JsValue,
    pub(super) source: Option<JsString>,
    pub(super) elements: Vec<JsonRecord>,
    pub(super) entries: Vec<(JsString, JsonRecord)>,
}

impl JsonRecord {
    fn leaf(value: JsValue, source: Option<JsString>) -> Self {
        Self {
            value,
            source,
            elements: Vec::new(),
            entries: Vec::new(),
        }
    }
}

/// Parses `text` as a JSON text (ECMA-404), throwing a `SyntaxError` if it is not one.
///
/// Returns the value and, if `track` is set, its parse record.
pub(super) fn parse_json(
    text: &JsString,
    track: bool,
    context: &mut Context,
) -> JsResult<(JsValue, Option<JsonRecord>)> {
    match text.variant() {
        JsStrVariant::Latin1(units) => Parser::new(units, track).parse(context),
        JsStrVariant::Utf16(units) => Parser::new(units, track).parse(context),
    }
}

trait Unit: Copy {
    fn get(self) -> u32;
}

impl Unit for u8 {
    #[inline]
    fn get(self) -> u32 {
        u32::from(self)
    }
}

impl Unit for u16 {
    #[inline]
    fn get(self) -> u32 {
        u32::from(self)
    }
}

/// Creates a string from code units, stored as Latin1 when every unit fits.
fn string_from_units<T: Unit>(units: &[T]) -> JsString {
    if units.iter().all(|u| u.get() <= 0xFF) {
        let bytes: Vec<u8> = units.iter().map(|u| u.get() as u8).collect();
        JsString::from(JsStr::latin1(&bytes))
    } else {
        let wide: Vec<u16> = units.iter().map(|u| u.get() as u16).collect();
        JsString::from(JsStr::utf16(&wide))
    }
}

fn string_from_wide(units: &[u16]) -> JsString {
    if units.iter().all(|&u| u <= 0xFF) {
        let bytes: Vec<u8> = units.iter().map(|&u| u as u8).collect();
        JsString::from(JsStr::latin1(&bytes))
    } else {
        JsString::from(JsStr::utf16(units))
    }
}

enum Frame {
    Array {
        values: Vec<JsValue>,
        records: Vec<JsonRecord>,
    },
    Object {
        object: JsObject,
        key: JsString,
        records: Vec<(JsString, JsonRecord)>,
    },
}

struct Parser<'a, T> {
    text: &'a [T],
    pos: usize,
    track: bool,
    scratch: Vec<u16>,
}

impl<'a, T: Unit> Parser<'a, T> {
    fn new(text: &'a [T], track: bool) -> Self {
        Self {
            text,
            pos: 0,
            track,
            scratch: Vec::new(),
        }
    }

    #[inline]
    fn peek(&self) -> Option<u32> {
        self.text.get(self.pos).map(|u| u.get())
    }

    #[inline]
    fn skip_whitespace(&mut self) {
        while let Some(0x09 | 0x0A | 0x0D | 0x20) = self.peek() {
            self.pos += 1;
        }
    }

    fn error(&self, expected: &str) -> crate::JsError {
        let message = match self.peek() {
            None => format!("unexpected end of JSON input, expected {expected}"),
            Some(unit) => {
                let found = char::from_u32(unit)
                    .map_or_else(|| format!("U+{unit:04X}"), |c| format!("'{c}'"));
                format!(
                    "unexpected character {found} at position {} in JSON, expected {expected}",
                    self.pos
                )
            }
        };
        JsNativeError::syntax().with_message(message).into()
    }

    fn expect(&mut self, unit: u8, expected: &str) -> JsResult<()> {
        if self.peek() == Some(u32::from(unit)) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(expected))
        }
    }

    fn source(&self, start: usize) -> Option<JsString> {
        self.track
            .then(|| string_from_units(&self.text[start..self.pos]))
    }

    fn parse(mut self, context: &mut Context) -> JsResult<(JsValue, Option<JsonRecord>)> {
        let mut stack: Vec<Frame> = Vec::new();
        loop {
            self.skip_whitespace();
            let start = self.pos;
            let (mut value, mut record) = match self.peek() {
                Some(0x7B) => {
                    self.pos += 1;
                    let object = JsObject::with_object_proto(context.intrinsics());
                    self.skip_whitespace();
                    if self.peek() == Some(0x7D) {
                        self.pos += 1;
                        let value = JsValue::from(object);
                        let record = self.track.then(|| JsonRecord::leaf(value.clone(), None));
                        (value, record)
                    } else {
                        let key = self.parse_key()?;
                        stack.push(Frame::Object {
                            object,
                            key,
                            records: Vec::new(),
                        });
                        continue;
                    }
                }
                Some(0x5B) => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(0x5D) {
                        self.pos += 1;
                        let value = JsValue::from(Array::create_array_from_list([], context));
                        let record = self.track.then(|| JsonRecord::leaf(value.clone(), None));
                        (value, record)
                    } else {
                        stack.push(Frame::Array {
                            values: Vec::new(),
                            records: Vec::new(),
                        });
                        continue;
                    }
                }
                Some(0x22) => {
                    let value = JsValue::from(self.parse_string()?);
                    let record = self
                        .track
                        .then(|| JsonRecord::leaf(value.clone(), self.source(start)));
                    (value, record)
                }
                Some(0x74) => self.parse_literal(b"true", JsValue::from(true))?,
                Some(0x66) => self.parse_literal(b"false", JsValue::from(false))?,
                Some(0x6E) => self.parse_literal(b"null", JsValue::null())?,
                Some(0x2D | 0x30..=0x39) => {
                    let value = self.parse_number()?;
                    let record = self
                        .track
                        .then(|| JsonRecord::leaf(value.clone(), self.source(start)));
                    (value, record)
                }
                _ => return Err(self.error("a JSON value")),
            };

            loop {
                match stack.last_mut() {
                    None => {
                        self.skip_whitespace();
                        if self.pos != self.text.len() {
                            return Err(self.error("the end of the JSON input"));
                        }
                        return Ok((value, record));
                    }
                    Some(Frame::Array { values, records }) => {
                        values.push(value);
                        if let Some(record) = record {
                            records.push(record);
                        }
                        self.skip_whitespace();
                        match self.peek() {
                            Some(0x2C) => {
                                self.pos += 1;
                                break;
                            }
                            Some(0x5D) => {
                                self.pos += 1;
                                let Some(Frame::Array { values, records }) = stack.pop() else {
                                    unreachable!("the innermost frame is an array");
                                };
                                value =
                                    JsValue::from(Array::create_array_from_list(values, context));
                                record = self.track.then(|| JsonRecord {
                                    value: value.clone(),
                                    source: None,
                                    elements: records,
                                    entries: Vec::new(),
                                });
                            }
                            _ => return Err(self.error("',' or ']'")),
                        }
                    }
                    Some(Frame::Object {
                        object,
                        key,
                        records,
                    }) => {
                        object.create_data_property_or_throw(key.clone(), value, context)?;
                        if let Some(record) = record {
                            records.push((key.clone(), record));
                        }
                        self.skip_whitespace();
                        match self.peek() {
                            Some(0x2C) => {
                                self.pos += 1;
                                self.skip_whitespace();
                                *key = self.parse_key()?;
                                break;
                            }
                            Some(0x7D) => {
                                self.pos += 1;
                                let Some(Frame::Object {
                                    object, records, ..
                                }) = stack.pop()
                                else {
                                    unreachable!("the innermost frame is an object");
                                };
                                value = JsValue::from(object);
                                record = self.track.then(|| JsonRecord {
                                    value: value.clone(),
                                    source: None,
                                    elements: Vec::new(),
                                    entries: records,
                                });
                            }
                            _ => return Err(self.error("',' or '}'")),
                        }
                    }
                }
            }
        }
    }

    fn parse_key(&mut self) -> JsResult<JsString> {
        if self.peek() != Some(0x22) {
            return Err(self.error("a string property name"));
        }
        let key = self.parse_string()?;
        self.skip_whitespace();
        self.expect(b':', "':'")?;
        Ok(key)
    }

    fn parse_literal(
        &mut self,
        word: &[u8],
        value: JsValue,
    ) -> JsResult<(JsValue, Option<JsonRecord>)> {
        let start = self.pos;
        for &byte in word {
            self.expect(byte, "a JSON value")?;
        }
        let record = self
            .track
            .then(|| JsonRecord::leaf(value.clone(), self.source(start)));
        Ok((value, record))
    }

    fn parse_string(&mut self) -> JsResult<JsString> {
        self.pos += 1;
        let start = self.pos;
        while let Some(unit) = self.peek() {
            match unit {
                0x22 => {
                    let string = string_from_units(&self.text[start..self.pos]);
                    self.pos += 1;
                    return Ok(string);
                }
                0x5C => break,
                0x00..=0x1F => return Err(self.error("a string character")),
                _ => self.pos += 1,
            }
        }
        let mut units = std::mem::take(&mut self.scratch);
        units.clear();
        units.extend(self.text[start..self.pos].iter().map(|u| u.get() as u16));
        let result = self.parse_escaped_string(&mut units);
        self.scratch = units;
        result
    }

    fn parse_escaped_string(&mut self, units: &mut Vec<u16>) -> JsResult<JsString> {
        loop {
            let Some(unit) = self.peek() else {
                return Err(self.error("'\"'"));
            };
            match unit {
                0x22 => {
                    self.pos += 1;
                    return Ok(string_from_wide(units));
                }
                0x5C => {
                    self.pos += 1;
                    let escaped = match self.peek() {
                        Some(0x22) => 0x22,
                        Some(0x5C) => 0x5C,
                        Some(0x2F) => 0x2F,
                        Some(0x62) => 0x08,
                        Some(0x66) => 0x0C,
                        Some(0x6E) => 0x0A,
                        Some(0x72) => 0x0D,
                        Some(0x74) => 0x09,
                        Some(0x75) => {
                            let mut code = 0u16;
                            for _ in 0..4 {
                                self.pos += 1;
                                let digit = self
                                    .peek()
                                    .and_then(|u| char::from_u32(u))
                                    .and_then(|c| c.to_digit(16))
                                    .ok_or_else(|| self.error("a hexadecimal digit"))?;
                                code = code * 16 + digit as u16;
                            }
                            code
                        }
                        _ => return Err(self.error("an escape sequence")),
                    };
                    self.pos += 1;
                    units.push(escaped);
                }
                0x00..=0x1F => return Err(self.error("a string character")),
                _ => {
                    units.push(unit as u16);
                    self.pos += 1;
                }
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while let Some(0x30..=0x39) = self.peek() {
            self.pos += 1;
        }
        self.pos - start
    }

    fn parse_number(&mut self) -> JsResult<JsValue> {
        let start = self.pos;
        let negative = self.peek() == Some(0x2D);
        if negative {
            self.pos += 1;
        }
        let integer_start = self.pos;
        match self.peek() {
            Some(0x30) => self.pos += 1,
            Some(0x31..=0x39) => {
                self.digits();
            }
            _ => return Err(self.error("a digit")),
        }
        let integer_end = self.pos;
        let mut integral = true;
        if self.peek() == Some(0x2E) {
            integral = false;
            self.pos += 1;
            if self.digits() == 0 {
                return Err(self.error("a digit"));
            }
        }
        if let Some(0x45 | 0x65) = self.peek() {
            integral = false;
            self.pos += 1;
            if let Some(0x2B | 0x2D) = self.peek() {
                self.pos += 1;
            }
            if self.digits() == 0 {
                return Err(self.error("a digit"));
            }
        }
        if integral && integer_end - integer_start <= 15 {
            let magnitude = self.text[integer_start..integer_end]
                .iter()
                .fold(0i64, |n, u| n * 10 + i64::from(u.get() - 0x30));
            return Ok(if negative {
                if magnitude == 0 {
                    JsValue::from(-0.0)
                } else {
                    JsValue::from(-magnitude)
                }
            } else {
                JsValue::from(magnitude)
            });
        }
        let literal: String = self.text[start..self.pos]
            .iter()
            .map(|u| u.get() as u8 as char)
            .collect();
        let number = literal.parse::<f64>().map_err(|_| self.error("a number"))?;
        Ok(JsValue::from(number))
    }
}
