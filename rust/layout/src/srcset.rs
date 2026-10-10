//! Southstar — choosing an image candidate from srcset and sizes: the candidates the HTML srcset grammar gives, each one's density from its width descriptor and the source size, and the one that best fits the device pixel ratio.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib as glib;

struct Candidate {
    url: Vec<u8>,
    density: f64,
    width: f64,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn valid_integer(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(u8::is_ascii_digit)
}

fn digits(s: &[u8], from: usize) -> usize {
    s[from..].iter().take_while(|c| c.is_ascii_digit()).count()
}

fn valid_float(s: &[u8]) -> bool {
    let mut i = usize::from(s.first() == Some(&b'-'));
    let mut mantissa = digits(s, i);
    i += mantissa;
    if s.get(i) == Some(&b'.') {
        i += 1;
        let fraction = digits(s, i);
        if fraction == 0 {
            return false;
        }
        i += fraction;
        mantissa += fraction;
    }
    if mantissa == 0 {
        return false;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(s.get(i), Some(b'-' | b'+')) {
            i += 1;
        }
        let exponent = digits(s, i);
        if exponent == 0 {
            return false;
        }
        i += exponent;
    }
    i == s.len()
}

fn descriptors_size(descriptors: &[Vec<u8>]) -> Option<(f64, f64)> {
    let (mut width, mut density) = (-1.0, -1.0);
    let (mut has_w, mut has_x, mut has_h) = (false, false, false);
    for d in descriptors {
        let (&last, number) = d.split_last()?;
        match last {
            b'w' => {
                if has_w || has_x || !valid_integer(number) {
                    return None;
                }
                width = glib::ascii_strtod(number);
                if width <= 0.0 {
                    return None;
                }
                has_w = true;
            }
            b'x' => {
                if has_w || has_x || has_h || !valid_float(number) {
                    return None;
                }
                density = glib::ascii_strtod(number);
                if density < 0.0 {
                    return None;
                }
                has_x = true;
            }
            b'h' => {
                if has_h || has_x || !valid_integer(number) || glib::ascii_strtod(number) <= 0.0 {
                    return None;
                }
                has_h = true;
            }
            _ => return None,
        }
    }
    (!has_h || has_w).then_some((width, density))
}

#[derive(PartialEq)]
enum State {
    Descriptor,
    Parens,
    AfterDescriptor,
}

fn tokenize_descriptors(s: &[u8], mut p: usize, out: &mut Vec<Vec<u8>>) -> usize {
    while p < s.len() && is_space(s[p]) {
        p += 1;
    }
    let mut current = Vec::new();
    let mut state = State::Descriptor;
    while let Some(&c) = s.get(p) {
        match state {
            State::Descriptor => {
                if is_space(c) {
                    if !current.is_empty() {
                        out.push(core::mem::take(&mut current));
                        state = State::AfterDescriptor;
                    }
                } else if c == b',' {
                    p += 1;
                    break;
                } else {
                    current.push(c);
                    if c == b'(' {
                        state = State::Parens;
                    }
                }
            }
            State::Parens => {
                current.push(c);
                if c == b')' {
                    state = State::Descriptor;
                }
            }
            State::AfterDescriptor => {
                if !is_space(c) {
                    state = State::Descriptor;
                    continue;
                }
            }
        }
        p += 1;
    }
    if !current.is_empty() {
        out.push(current);
    }
    p
}

fn parse(s: &[u8]) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut p = 0;
    loop {
        while p < s.len() && (is_space(s[p]) || s[p] == b',') {
            p += 1;
        }
        if p == s.len() {
            break;
        }
        let start = p;
        while p < s.len() && !is_space(s[p]) {
            p += 1;
        }
        let mut url = &s[start..p];
        let mut descriptors = Vec::new();
        if url.last() == Some(&b',') {
            while let Some(stripped) = url.strip_suffix(b",") {
                url = stripped;
            }
        } else {
            p = tokenize_descriptors(s, p, &mut descriptors);
        }
        if url.is_empty() {
            continue;
        }
        if let Some((width, density)) = descriptors_size(&descriptors) {
            out.push(Candidate {
                url: url.to_vec(),
                density,
                width,
            });
        }
    }
    out
}

pub fn has_width_descriptor(srcset: Option<&[u8]>) -> bool {
    parse(srcset.unwrap_or_default())
        .iter()
        .any(|c| c.width > 0.0)
}

pub fn select(
    srcset: Option<&[u8]>,
    sizes: Option<&[u8]>,
    src: Option<&[u8]>,
    dpr: f64,
    resolve_sizes: &dyn Fn(Option<&[u8]>) -> f64,
) -> Option<(Vec<u8>, f64)> {
    let mut candidates = parse(srcset.unwrap_or_default());
    let mut any_width = false;
    let mut any_unit_density = false;
    let mut source_size = -1.0;
    for c in &mut candidates {
        if c.width > 0.0 {
            any_width = true;
            if source_size < 0.0 {
                source_size = resolve_sizes(sizes);
                if !source_size.is_finite() {
                    source_size = resolve_sizes(None);
                }
            }
            c.density = if source_size > 0.0 {
                c.width / source_size
            } else {
                1.0
            };
        } else if c.density < 0.0 {
            c.density = 1.0;
        }
        if c.width <= 0.0 && c.density == 1.0 {
            any_unit_density = true;
        }
    }
    if let Some(src) = src.filter(|s| !s.is_empty())
        && !any_width
        && !any_unit_density
    {
        candidates.push(Candidate {
            url: src.to_vec(),
            density: 1.0,
            width: -1.0,
        });
    }
    let mut best: Option<usize> = None;
    let mut largest: Option<usize> = None;
    for (i, c) in candidates.iter().enumerate() {
        if candidates[..i]
            .iter()
            .any(|earlier| earlier.density == c.density)
        {
            continue;
        }
        if c.density >= dpr && best.is_none_or(|b| c.density < candidates[b].density) {
            best = Some(i);
        }
        if largest.is_none_or(|l| c.density > candidates[l].density) {
            largest = Some(i);
        }
    }
    let chosen = best.or(largest)?;
    let density = candidates[chosen].density;
    Some((candidates.swap_remove(chosen).url, density))
}
