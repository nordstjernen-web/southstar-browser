//! Southstar — exact decimal values for Intl number formatting: parsing, rounding modes and increments, and raw digit strings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Decimal {
    digits: Vec<u8>,
    exponent: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Numeric {
    NotANumber,
    Infinite { negative: bool },
    Finite { negative: bool, magnitude: Decimal },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnsignedRounding {
    Zero,
    Infinity,
    HalfZero,
    HalfInfinity,
    HalfEven,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Remainder {
    Exact,
    BelowHalf,
    Half,
    AboveHalf,
}

pub(crate) struct RawFormat {
    pub text: String,
    pub rounded: Decimal,
    pub integer_digits: usize,
    pub rounding_magnitude: i64,
}

impl Decimal {
    pub(crate) fn zero() -> Decimal {
        Decimal {
            digits: Vec::new(),
            exponent: 0,
        }
    }

    pub(crate) fn from_digits(mut digits: Vec<u8>, mut exponent: i64) -> Decimal {
        let leading = digits.iter().take_while(|&&d| d == 0).count();
        digits.drain(..leading);
        while digits.last() == Some(&0) {
            digits.pop();
            exponent += 1;
        }
        if digits.is_empty() {
            return Decimal::zero();
        }
        Decimal { digits, exponent }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    pub(crate) fn magnitude(&self) -> i64 {
        self.digits.len() as i64 - 1 + self.exponent
    }

    pub(crate) fn scaled(&self, power: i64) -> Decimal {
        if self.is_zero() {
            return Decimal::zero();
        }
        Decimal {
            digits: self.digits.clone(),
            exponent: self.exponent + power,
        }
    }

    pub(crate) fn is_integer(&self) -> bool {
        self.exponent >= 0
    }

    pub(crate) fn to_f64(&self) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let digits: String = self.digits.iter().map(|d| char::from(b'0' + d)).collect();
        format!("{digits}e{}", self.exponent)
            .parse()
            .unwrap_or(f64::INFINITY)
    }

    fn integer_text(&self, shift: i64) -> String {
        if self.is_zero() {
            return "0".to_owned();
        }
        let zeros = (self.exponent + shift).max(0) as usize;
        let mut text: String = self.digits.iter().map(|d| char::from(b'0' + d)).collect();
        text.extend(std::iter::repeat_n('0', zeros));
        text
    }

    fn split_at_position(&self, position: i64) -> (Vec<u8>, Remainder) {
        let shift = position - self.exponent;
        if shift <= 0 {
            let mut quotient = self.digits.clone();
            quotient.extend(std::iter::repeat_n(0, (-shift) as usize));
            return (quotient, Remainder::Exact);
        }
        let shift = shift as usize;
        let length = self.digits.len();
        let (quotient, rest) = if shift >= length {
            (Vec::new(), {
                let mut rest = vec![0; shift - length];
                rest.extend_from_slice(&self.digits);
                rest
            })
        } else {
            (
                self.digits[..length - shift].to_vec(),
                self.digits[length - shift..].to_vec(),
            )
        };
        let remainder = match rest.first() {
            None => Remainder::Exact,
            Some(&first) if first < 5 => Remainder::BelowHalf,
            Some(&5) if rest.len() == 1 => Remainder::Half,
            Some(_) => Remainder::AboveHalf,
        };
        (quotient, remainder)
    }

    pub(crate) fn round_at(
        &self,
        position: i64,
        increment: u32,
        mode: UnsignedRounding,
    ) -> Decimal {
        let (mut quotient, fraction) = self.split_at_position(position);
        let increment = increment.max(1);
        let below = remainder_mod(&quotient, increment);
        let exact = below == 0 && fraction == Remainder::Exact;
        subtract_small(&mut quotient, below);
        if exact {
            return Decimal::from_digits(quotient, position);
        }
        let half = half_comparison(below, increment, fraction);
        let round_up = match mode {
            UnsignedRounding::Zero => false,
            UnsignedRounding::Infinity => true,
            UnsignedRounding::HalfZero => half == Remainder::AboveHalf,
            UnsignedRounding::HalfInfinity => half != Remainder::BelowHalf,
            UnsignedRounding::HalfEven => match half {
                Remainder::BelowHalf => false,
                Remainder::AboveHalf => true,
                _ => quotient_parity_is_odd(&quotient, increment),
            },
        };
        if round_up {
            add_small(&mut quotient, increment);
        }
        Decimal::from_digits(quotient, position)
    }

    pub(crate) fn digits_text(&self) -> String {
        self.digits.iter().map(|d| char::from(b'0' + d)).collect()
    }
}

fn half_comparison(below: u32, increment: u32, fraction: Remainder) -> Remainder {
    if increment.is_multiple_of(2) {
        let half = increment / 2;
        return if below < half {
            Remainder::BelowHalf
        } else if below == half && fraction == Remainder::Exact {
            Remainder::Half
        } else {
            Remainder::AboveHalf
        };
    }
    let half = (increment - 1) / 2;
    if below < half {
        Remainder::BelowHalf
    } else if below > half {
        Remainder::AboveHalf
    } else {
        match fraction {
            Remainder::Exact | Remainder::BelowHalf => Remainder::BelowHalf,
            other => other,
        }
    }
}

fn remainder_mod(digits: &[u8], modulus: u32) -> u32 {
    digits
        .iter()
        .fold(0u32, |acc, &d| (acc * 10 + u32::from(d)) % modulus)
}

fn quotient_parity_is_odd(digits: &[u8], increment: u32) -> bool {
    remainder_mod(digits, increment * 2) >= increment
}

fn subtract_small(digits: &mut [u8], amount: u32) {
    let mut borrow = amount;
    for digit in digits.iter_mut().rev() {
        if borrow == 0 {
            break;
        }
        let take = borrow % 10;
        borrow /= 10;
        if u32::from(*digit) >= take {
            *digit -= take as u8;
        } else {
            *digit = (u32::from(*digit) + 10 - take) as u8;
            borrow += 1;
        }
    }
}

fn add_small(digits: &mut Vec<u8>, amount: u32) {
    let mut carry = amount;
    for digit in digits.iter_mut().rev() {
        if carry == 0 {
            return;
        }
        let sum = u32::from(*digit) + carry;
        *digit = (sum % 10) as u8;
        carry = sum / 10;
    }
    while carry > 0 {
        digits.insert(0, (carry % 10) as u8);
        carry /= 10;
    }
}

pub(crate) fn to_raw_fixed(
    x: &Decimal,
    minimum_fraction: u32,
    maximum_fraction: u32,
    increment: u32,
    mode: UnsignedRounding,
) -> RawFormat {
    let fraction = i64::from(maximum_fraction);
    let rounded = x.round_at(-fraction, increment, mode);
    let mut text = rounded.integer_text(fraction);
    let integer_digits = if fraction != 0 {
        let fraction = fraction as usize;
        if text.len() <= fraction {
            let zeros = fraction + 1 - text.len();
            text.insert_str(0, &"0".repeat(zeros));
        }
        let integer_length = text.len() - fraction;
        text.insert(integer_length, '.');
        integer_length
    } else {
        text.len()
    };
    trim_optional_zeros(&mut text, maximum_fraction - minimum_fraction);
    RawFormat {
        text,
        rounded,
        integer_digits,
        rounding_magnitude: -fraction,
    }
}

pub(crate) fn to_raw_precision(
    x: &Decimal,
    minimum_precision: u32,
    maximum_precision: u32,
    mode: UnsignedRounding,
) -> RawFormat {
    let precision = i64::from(maximum_precision);
    let (mut digits, exponent, rounded) = if x.is_zero() {
        ("0".repeat(precision as usize), 0, Decimal::zero())
    } else {
        let rounded = x.round_at(x.magnitude() - precision + 1, 1, mode);
        let exponent = rounded.magnitude();
        let mut digits = rounded.digits_text();
        digits.extend(std::iter::repeat_n(
            '0',
            (precision as usize).saturating_sub(digits.len()),
        ));
        (digits, exponent, rounded)
    };
    let integer_digits = if exponent >= precision - 1 {
        digits.extend(std::iter::repeat_n(
            '0',
            (exponent - precision + 1) as usize,
        ));
        (exponent + 1) as usize
    } else if exponent >= 0 {
        digits.insert((exponent + 1) as usize, '.');
        (exponent + 1) as usize
    } else {
        let zeros = "0".repeat((-(exponent + 1)) as usize);
        digits = format!("0.{zeros}{digits}");
        1
    };
    if digits.contains('.') && maximum_precision > minimum_precision {
        trim_optional_zeros(&mut digits, maximum_precision - minimum_precision);
    }
    RawFormat {
        text: digits,
        rounded,
        integer_digits,
        rounding_magnitude: exponent - precision + 1,
    }
}

fn trim_optional_zeros(text: &mut String, mut cut: u32) {
    if !text.contains('.') {
        return;
    }
    while cut > 0 && text.ends_with('0') {
        text.pop();
        cut -= 1;
    }
    if text.ends_with('.') {
        text.pop();
    }
}

impl Numeric {
    pub(crate) fn from_f64(value: f64) -> Numeric {
        if value.is_nan() {
            return Numeric::NotANumber;
        }
        let negative = value.is_sign_negative();
        if value.is_infinite() {
            return Numeric::Infinite { negative };
        }
        let text = format!("{:e}", value.abs());
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let exponent: i64 = exponent.parse().unwrap_or(0);
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let digits: Vec<u8> = whole
            .bytes()
            .chain(fraction.bytes())
            .map(|c| c - b'0')
            .collect();
        let magnitude = Decimal::from_digits(digits, exponent - fraction.len() as i64);
        Numeric::Finite {
            negative,
            magnitude,
        }
    }

    pub(crate) fn from_decimal_text(text: &str) -> Numeric {
        let trimmed = text.trim_matches(is_js_whitespace);
        if trimmed.is_empty() {
            return Numeric::Finite {
                negative: false,
                magnitude: Decimal::zero(),
            };
        }
        if let Some(value) = parse_radix_literal(trimmed) {
            return value;
        }
        let (negative, unsigned) = match trimmed.as_bytes()[0] {
            b'-' => (true, &trimmed[1..]),
            b'+' => (false, &trimmed[1..]),
            _ => (false, trimmed),
        };
        if unsigned == "Infinity" {
            return Numeric::Infinite { negative };
        }
        match parse_unsigned_decimal(unsigned) {
            Some(magnitude) => clamp_to_double_range(negative, magnitude),
            None => Numeric::NotANumber,
        }
    }

    pub(crate) fn from_bigint_text(text: &str) -> Numeric {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let digits = digits.bytes().map(|c| c.wrapping_sub(b'0')).collect();
        let magnitude = Decimal::from_digits(digits, 0);
        Numeric::Finite {
            negative: negative && !magnitude.is_zero(),
            magnitude,
        }
    }

    pub(crate) fn is_nan(&self) -> bool {
        matches!(self, Numeric::NotANumber)
    }
}

fn clamp_to_double_range(negative: bool, magnitude: Decimal) -> Numeric {
    let approximation = magnitude.to_f64();
    if approximation.is_infinite() {
        return Numeric::Infinite { negative };
    }
    if approximation == 0.0 {
        return Numeric::Finite {
            negative,
            magnitude: Decimal::zero(),
        };
    }
    Numeric::Finite {
        negative,
        magnitude,
    }
}

fn is_js_whitespace(c: char) -> bool {
    c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}')
}

fn parse_radix_literal(text: &str) -> Option<Numeric> {
    let bytes = text.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'0' {
        return None;
    }
    let radix = match bytes[1] {
        b'x' | b'X' => 16,
        b'o' | b'O' => 8,
        b'b' | b'B' => 2,
        _ => return None,
    };
    let mut decimal_digits: Vec<u8> = Vec::new();
    for c in text[2..].chars() {
        let digit = c.to_digit(radix)?;
        multiply_add(&mut decimal_digits, radix, digit);
    }
    Some(clamp_to_double_range(
        false,
        Decimal::from_digits(decimal_digits, 0),
    ))
}

fn multiply_add(digits: &mut Vec<u8>, factor: u32, addend: u32) {
    let mut carry = addend;
    for digit in digits.iter_mut().rev() {
        let value = u32::from(*digit) * factor + carry;
        *digit = (value % 10) as u8;
        carry = value / 10;
    }
    while carry > 0 {
        digits.insert(0, (carry % 10) as u8);
        carry /= 10;
    }
}

fn parse_unsigned_decimal(text: &str) -> Option<Decimal> {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.bytes().all(|c| c.is_ascii_digit()) || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let exponent = match exponent {
        None => 0i64,
        Some(exponent) => {
            let (sign, digits) = match exponent.as_bytes().first() {
                Some(b'-') => (-1i64, &exponent[1..]),
                Some(b'+') => (1, &exponent[1..]),
                _ => (1, exponent),
            };
            if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let trimmed = digits.trim_start_matches('0');
            if trimmed.len() > 12 {
                if sign > 0 {
                    1_000_000_000_000
                } else {
                    -1_000_000_000_000
                }
            } else {
                sign * trimmed.parse::<i64>().unwrap_or(0)
            }
        }
    };
    let digits: Vec<u8> = whole
        .bytes()
        .chain(fraction.bytes())
        .map(|c| c - b'0')
        .collect();
    Some(Decimal::from_digits(
        digits,
        exponent - fraction.len() as i64,
    ))
}
