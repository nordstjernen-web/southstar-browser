//! Southstar — counter-reset, counter-increment and counter-set lists, list-style-type, and the list-style shorthand's canonical text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::animation::ident_decode;
use crate::calc::{self, Parsed};
use crate::content::{ident_valid, symbols_canonical, wide_keyword_or_default};
use crate::lex::ident_serialize;
use crate::scan::{split_ws_paren, starts_with_ci, strip, strtol10};
use crate::time::starts_math_fn;
use crate::units::NUMBER;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CounterProp {
    Increment,
    Reset,
    Set,
}

fn integer_text(tok: &[u8]) -> Option<Vec<u8>> {
    if starts_math_fn(tok) {
        let text = CString::new(tok).ok()?;
        return matches!(calc::parse_calc(&text), Some(Parsed::Length(_, NUMBER)))
            .then(|| tok.to_vec());
    }
    let (v, end) = strtol10(tok);
    (end != 0 && end == tok.len()).then(|| v.to_string().into_bytes())
}

fn name_rejected(decoded: &[u8]) -> bool {
    let first = decoded.first().copied().unwrap_or(0);
    let second = decoded.get(1).copied().unwrap_or(0);
    wide_keyword_or_default(decoded)
        || decoded.eq_ignore_ascii_case(b"none")
        || first.is_ascii_digit()
        || (first == b'-' && (second.is_ascii_digit() || second == 0))
}

pub(crate) fn list_canonical(text: &[u8], prop: CounterProp) -> Option<Vec<u8>> {
    let toks = split_ws_paren(text, 64);
    let n = toks.len();
    if n == 0 {
        return None;
    }
    if n == 1 && toks[0].eq_ignore_ascii_case(b"none") {
        return Some(b"none".to_vec());
    }
    let default: &[u8] = if prop == CounterProp::Increment {
        b"1"
    } else {
        b"0"
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let tok = toks[i];
        let reversed = starts_with_ci(tok, b"reversed(") && tok.last() == Some(&b')');
        let name = if reversed {
            if prop != CounterProp::Reset {
                return None;
            }
            strip(&tok[9..tok.len() - 1])
        } else {
            tok
        };
        let decoded = ident_decode(name)?;
        let decoded = &decoded[..decoded
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(decoded.len())];
        if name_rejected(decoded) {
            return None;
        }
        let name = ident_serialize(decoded);
        let num = toks.get(i + 1).and_then(|next| integer_text(next));
        if num.is_some() {
            i += 1;
        }
        if !out.is_empty() {
            out.push(b' ');
        }
        if reversed {
            out.extend_from_slice(b"reversed(");
            out.extend_from_slice(&name);
            out.push(b')');
        } else {
            out.extend_from_slice(&name);
        }
        if let Some(num) = num {
            out.push(b' ');
            out.extend_from_slice(&num);
        } else if !reversed {
            out.push(b' ');
            out.extend_from_slice(default);
        }
        i += 1;
    }
    Some(out)
}

pub(crate) fn list_style_type_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let toks = split_ws_paren(text, 4);
    let [tok] = toks.as_slice() else {
        return None;
    };
    let len = tok.len();
    if starts_with_ci(tok, b"symbols(") && tok[len - 1] == b')' {
        let canon = symbols_canonical(&tok[8..len - 1])?;
        let contains = |needle: &[u8]| canon.windows(needle.len()).any(|w| w == needle);
        if contains(b"url(") || contains(b"image(") || contains(b"gradient(") {
            return None;
        }
        let mut out = b"symbols(".to_vec();
        out.extend_from_slice(&canon);
        out.push(b')');
        return Some(out);
    }
    if (tok[0] == b'"' || tok[0] == b'\'') && len >= 2 && tok[len - 1] == tok[0] {
        return Some(tok.to_vec());
    }
    if tok.eq_ignore_ascii_case(b"none") {
        return Some(b"none".to_vec());
    }
    (ident_valid(tok) && !wide_keyword_or_default(tok)).then(|| tok.to_vec())
}

pub(crate) fn list_style_serialize(
    type_: Option<&[u8]>,
    position: Option<&[u8]>,
    image: Option<&[u8]>,
) -> Vec<u8> {
    let mut out = Vec::new();
    let type_none = type_.is_none_or(|t| t == b"none");
    let image_none = image.is_none_or(|i| i == b"none");
    let type_is_position_word = type_.is_some_and(|t| t == b"inside" || t == b"outside");
    let push = |out: &mut Vec<u8>, part: &[u8]| {
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(part);
    };
    if let Some(position) = position.filter(|p| *p != b"outside" || type_is_position_word) {
        push(&mut out, position);
    }
    if let Some(image) = image.filter(|_| !image_none) {
        push(&mut out, image);
    }
    if let Some(type_) = type_.filter(|t| *t != b"disc") {
        push(&mut out, type_);
    }
    if out.is_empty() {
        out.extend_from_slice(if type_none && image_none && type_.is_some() {
            b"none"
        } else {
            b"outside"
        });
    }
    out
}
