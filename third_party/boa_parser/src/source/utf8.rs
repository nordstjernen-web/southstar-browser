use super::ReadChar;
use std::io::{self, Bytes, Read};

/// Input for UTF-8 encoded sources.
#[derive(Debug)]
pub struct UTF8Input<R> {
    input: Bytes<R>,
    peeked: Option<u8>,
}

impl<R: Read> UTF8Input<R> {
    /// Creates a new `UTF8Input` from a UTF-8 encoded source.
    pub(crate) fn new(iter: R) -> Self {
        Self {
            #[allow(clippy::unbuffered_bytes)]
            input: iter.bytes(),
            peeked: None,
        }
    }
}

impl<R: Read> UTF8Input<R> {
    /// Retrieves the next byte
    fn next_byte(&mut self) -> io::Result<Option<u8>> {
        if let Some(byte) = self.peeked.take() {
            return Ok(Some(byte));
        }
        self.input.next().transpose()
    }
}

impl<R: Read> ReadChar for UTF8Input<R> {
    /// Retrieves the next char as a u32 code point, decoding like the
    /// WHATWG UTF-8 decoder: an invalid or truncated sequence yields U+FFFD
    /// and consumes only its own bytes, so it cannot swallow the quote,
    /// backslash or newline after it. Encoded surrogates (WTF-8) are kept.
    fn next_char(&mut self) -> io::Result<Option<u32>> {
        let x = match self.next_byte()? {
            Some(b) if b >= 128 => b,
            b => return Ok(b.map(u32::from)),
        };

        let (mut ch, needed, mut lower, mut upper) = match x {
            0xC2..=0xDF => (utf8_first_byte(x, 2), 1, 0x80, 0xBF),
            0xE0 => (utf8_first_byte(x, 3), 2, 0xA0, 0xBF),
            0xE1..=0xEF => (utf8_first_byte(x, 3), 2, 0x80, 0xBF),
            0xF0 => (utf8_first_byte(x, 4), 3, 0x90, 0xBF),
            0xF1..=0xF3 => (utf8_first_byte(x, 4), 3, 0x80, 0xBF),
            0xF4 => (utf8_first_byte(x, 4), 3, 0x80, 0x8F),
            _ => return Ok(Some(REPLACEMENT_CHARACTER)),
        };
        for _ in 0..needed {
            match self.next_byte()? {
                Some(byte) if (lower..=upper).contains(&byte) => {
                    ch = utf8_acc_cont_byte(ch, byte);
                    lower = 0x80;
                    upper = 0xBF;
                }
                Some(byte) => {
                    self.peeked = Some(byte);
                    return Ok(Some(REPLACEMENT_CHARACTER));
                }
                None => return Ok(Some(REPLACEMENT_CHARACTER)),
            }
        }

        Ok(Some(ch))
    }
}

/// U+FFFD, which replaces every invalid or truncated UTF-8 sequence.
const REPLACEMENT_CHARACTER: u32 = 0xFFFD;

/// Mask of the value bits of a continuation byte.
const CONT_MASK: u8 = 0b0011_1111;

/// Returns the initial codepoint accumulator for the first byte of a
/// sequence `width` bytes long: the bottom 5 bits for width 2, 4 bits for
/// width 3 and 3 bits for width 4.
fn utf8_first_byte(byte: u8, width: u32) -> u32 {
    u32::from(byte & (0x7F >> width))
}

/// Returns the value of `ch` updated with continuation byte `byte`.
fn utf8_acc_cont_byte(ch: u32, byte: u8) -> u32 {
    (ch << 6) | u32::from(byte & CONT_MASK)
}
