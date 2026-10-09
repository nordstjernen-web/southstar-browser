//! Southstar — the animation and transition properties: validating and canonicalizing their longhand lists, the animation-range values, parsing the animation and transition shorthands, building the per-element lists from computed longhands, and serializing them back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::content::{append_unescaped, ident_valid, wide_keyword_or_default};
use crate::ffi;
use crate::gradient::math_text_has_unit;
use crate::lex::{ident_serialize, read_ident};
use crate::math::math_canonical;
use crate::scan::{scan_until, split_ws_limit, split_ws_paren, starts_with_ci, strip, trim_range};
use crate::time;
use crate::timing::{self, Timing};
use crate::units::{self, NUMBER, number_text};

pub(crate) const ENTRIES_MAX: usize = 8;
const LIST_MAX: usize = ENTRIES_MAX * 4;

pub(crate) const TARGET_NONE: i32 = 0;
pub(crate) const TARGET_ALL: i32 = 1;
const TARGET_OPACITY: i32 = 2;
const TARGET_TRANSFORM: i32 = 3;
const TARGET_COLOR: i32 = 4;
const TARGET_BG_COLOR: i32 = 5;
const TARGET_OTHER: i32 = 6;

const DIR_NORMAL: i32 = 0;
const DIR_REVERSE: i32 = 1;
const DIR_ALTERNATE: i32 = 2;
const DIR_ALTERNATE_REVERSE: i32 = 3;

const FILL_NONE: i32 = 0;
const FILL_FORWARDS: i32 = 1;
const FILL_BACKWARDS: i32 = 2;
const FILL_BOTH: i32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Longhand {
    AnimationName,
    AnimationDuration,
    AnimationDelay,
    AnimationTimingFunction,
    AnimationIterationCount,
    AnimationDirection,
    AnimationFillMode,
    AnimationPlayState,
    AnimationComposition,
    AnimationTimeline,
    AnimationRangeStart,
    AnimationRangeEnd,
    TransitionProperty,
    TransitionDuration,
    TransitionDelay,
    TransitionTimingFunction,
    TransitionBehavior,
}

impl Longhand {
    pub(crate) const ALL: [Longhand; 17] = [
        Longhand::AnimationName,
        Longhand::AnimationDuration,
        Longhand::AnimationDelay,
        Longhand::AnimationTimingFunction,
        Longhand::AnimationIterationCount,
        Longhand::AnimationDirection,
        Longhand::AnimationFillMode,
        Longhand::AnimationPlayState,
        Longhand::AnimationComposition,
        Longhand::AnimationTimeline,
        Longhand::AnimationRangeStart,
        Longhand::AnimationRangeEnd,
        Longhand::TransitionProperty,
        Longhand::TransitionDuration,
        Longhand::TransitionDelay,
        Longhand::TransitionTimingFunction,
        Longhand::TransitionBehavior,
    ];

    pub(crate) fn name(self) -> &'static std::ffi::CStr {
        match self {
            Longhand::AnimationName => c"animation-name",
            Longhand::AnimationDuration => c"animation-duration",
            Longhand::AnimationDelay => c"animation-delay",
            Longhand::AnimationTimingFunction => c"animation-timing-function",
            Longhand::AnimationIterationCount => c"animation-iteration-count",
            Longhand::AnimationDirection => c"animation-direction",
            Longhand::AnimationFillMode => c"animation-fill-mode",
            Longhand::AnimationPlayState => c"animation-play-state",
            Longhand::AnimationComposition => c"animation-composition",
            Longhand::AnimationTimeline => c"animation-timeline",
            Longhand::AnimationRangeStart => c"animation-range-start",
            Longhand::AnimationRangeEnd => c"animation-range-end",
            Longhand::TransitionProperty => c"transition-property",
            Longhand::TransitionDuration => c"transition-duration",
            Longhand::TransitionDelay => c"transition-delay",
            Longhand::TransitionTimingFunction => c"transition-timing-function",
            Longhand::TransitionBehavior => c"transition-behavior",
        }
    }
}

const ANIMATION_LIST_PROPS: [Longhand; 8] = [
    Longhand::AnimationName,
    Longhand::AnimationDuration,
    Longhand::AnimationDelay,
    Longhand::AnimationTimingFunction,
    Longhand::AnimationIterationCount,
    Longhand::AnimationDirection,
    Longhand::AnimationFillMode,
    Longhand::AnimationPlayState,
];

const TRANSITION_LIST_PROPS: [Longhand; 5] = [
    Longhand::TransitionProperty,
    Longhand::TransitionDuration,
    Longhand::TransitionDelay,
    Longhand::TransitionTimingFunction,
    Longhand::TransitionBehavior,
];

#[derive(Clone)]
pub(crate) struct Entry {
    pub target: i32,
    pub name: Option<Vec<u8>>,
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub timing: Timing,
    pub iter_count: i32,
    pub iterations: f64,
    pub direction: i32,
    pub fill: i32,
    pub paused: bool,
    pub duration_auto: bool,
    pub allow_discrete: bool,
}

impl Entry {
    fn initial(duration_auto: bool) -> Entry {
        Entry {
            target: TARGET_ALL,
            name: None,
            duration_ms: 0.0,
            delay_ms: 0.0,
            timing: Timing::of(timing::EASE),
            iter_count: 1,
            iterations: 1.0,
            direction: DIR_NORMAL,
            fill: FILL_NONE,
            paused: false,
            duration_auto,
            allow_discrete: false,
        }
    }
}

fn eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn one_of(a: &[u8], words: &[&[u8]]) -> bool {
    words.iter().any(|w| eq(a, w))
}

pub(crate) fn list_split(text: &[u8]) -> Vec<&[u8]> {
    let end = text.len();
    let mut out = Vec::new();
    let mut p = 0;
    while p <= end && out.len() < LIST_MAX {
        let (seg, term) = scan_until(text, p, end, b",");
        out.push(trim_range(text, p, seg));
        if term != b',' {
            break;
        }
        p = seg + 1;
    }
    out
}

fn string_decode(tok: &[u8]) -> Vec<u8> {
    let end = tok.len().saturating_sub(1);
    let mut out = Vec::new();
    let mut p = 1;
    while p < end {
        append_unescaped(&mut out, tok, &mut p);
    }
    out
}

pub(crate) fn ident_decode(tok: &[u8]) -> Option<Vec<u8>> {
    let mut p = 0;
    let out = read_ident(tok, &mut p, tok.len());
    (p == tok.len()).then_some(out)
}

fn starts_digit_like(tok: &[u8]) -> bool {
    let first = tok.first().copied().unwrap_or(0);
    let second = tok.get(1).copied().unwrap_or(0);
    first.is_ascii_digit() || (first == b'-' && second.is_ascii_digit())
}

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn item_valid(prop: Longhand, item: &[u8]) -> bool {
    match prop {
        Longhand::AnimationName => {
            if eq(item, b"none") {
                return true;
            }
            if item[0] == b'"' || item[0] == b'\'' {
                return item.len() >= 3 && item[item.len() - 1] == item[0];
            }
            ident_decode(item).is_some_and(|ident| {
                !ident.is_empty() && !wide_keyword_or_default(&ident) && !starts_digit_like(item)
            })
        }
        Longhand::AnimationTimingFunction | Longhand::TransitionTimingFunction => {
            timing::item_parse(item).is_some()
        }
        Longhand::AnimationIterationCount => {
            if eq(item, b"infinite") {
                return true;
            }
            let text = c_text(item);
            let (num, end) = ffi::strtod(&text, 0);
            end != 0 && end == text.to_bytes().len() && num >= 0.0
        }
        Longhand::AnimationDirection => one_of(
            item,
            &[b"normal", b"reverse", b"alternate", b"alternate-reverse"],
        ),
        Longhand::AnimationFillMode => one_of(item, &[b"none", b"forwards", b"backwards", b"both"]),
        Longhand::TransitionProperty => {
            one_of(item, &[b"none", b"all"])
                || (ident_valid(item) && !wide_keyword_or_default(item))
        }
        Longhand::TransitionBehavior => one_of(item, &[b"normal", b"allow-discrete"]),
        Longhand::AnimationPlayState => one_of(item, &[b"running", b"paused"]),
        Longhand::AnimationComposition => one_of(item, &[b"replace", b"add", b"accumulate"]),
        Longhand::AnimationTimeline => {
            if one_of(item, &[b"auto", b"none"]) {
                return true;
            }
            if item.starts_with(b"--") {
                return ident_valid(item);
            }
            (starts_with_ci(item, b"scroll(") || starts_with_ci(item, b"view("))
                && item[item.len() - 1] == b')'
        }
        Longhand::AnimationRangeStart | Longhand::AnimationRangeEnd => {
            range_item_canonical(item, prop == Longhand::AnimationRangeEnd).is_some()
        }
        _ => false,
    }
}

fn range_is_name(tok: &[u8]) -> bool {
    one_of(
        tok,
        &[
            b"cover",
            b"contain",
            b"entry",
            b"exit",
            b"entry-crossing",
            b"exit-crossing",
        ],
    )
}

fn range_lp_canonical(tok: &[u8]) -> Option<Vec<u8>> {
    if tok.is_empty() {
        return None;
    }
    let text = c_text(tok);
    if time::starts_math_fn(tok) {
        let Some(m) = math_canonical(&text) else {
            let len = tok.len();
            let contains = |needle: &[u8]| tok.windows(needle.len()).any(|w| w == needle);
            if len < 3 || tok[len - 1] != b')' || contains(b"s)") || contains(b"deg") {
                return None;
            }
            return Some(tok.to_vec());
        };
        if math_text_has_unit(&m, &[b"s", b"ms", b"deg", b"rad", b"turn", b"hz"]) {
            return None;
        }
        return Some(m);
    }
    let (v, unit) = units::parse_length(&text)?;
    if unit == NUMBER {
        return (v == 0.0).then(|| b"0px".to_vec());
    }
    let (_, unit_start) = ffi::strtod(&text, 0);
    let mut out = number_text(v);
    out.extend_from_slice(&text.to_bytes()[unit_start..].to_ascii_lowercase());
    Some(out)
}

pub(crate) fn range_item_canonical(item: &[u8], is_end: bool) -> Option<Vec<u8>> {
    let toks = split_ws_limit(item, 4);
    match toks.as_slice() {
        [only] if eq(only, b"normal") => Some(b"normal".to_vec()),
        [only] if range_is_name(only) => Some(only.to_ascii_lowercase()),
        [only] => range_lp_canonical(only),
        [name, lp] if range_is_name(name) => {
            let lp = range_lp_canonical(lp)?;
            let default: &[u8] = if is_end { b"100%" } else { b"0%" };
            let mut out = name.to_ascii_lowercase();
            if lp != default {
                out.push(b' ');
                out.extend_from_slice(&lp);
            }
            Some(out)
        }
        _ => None,
    }
}

pub(crate) fn duration_canonical(t: &[u8]) -> Option<Vec<u8>> {
    let items = list_split(t);
    let mut canon = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            canon.extend_from_slice(b", ");
        }
        if eq(item, b"auto") {
            canon.extend_from_slice(b"auto");
        } else if let Some(spec) = time::list_serialize(item, false) {
            canon.extend_from_slice(&spec);
        } else if time::is_time(item) {
            canon.extend_from_slice(item);
        } else {
            return None;
        }
    }
    Some(canon)
}

pub(crate) fn longhand_canonical(prop: Longhand, t: &[u8]) -> Option<Vec<u8>> {
    let items = list_split(t);
    let n = items.len();
    let lower = prop != Longhand::AnimationName && prop != Longhand::AnimationTimeline;
    let range = prop == Longhand::AnimationRangeStart || prop == Longhand::AnimationRangeEnd;
    let mut canon = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if item.is_empty() || !item_valid(prop, item) {
            return None;
        }
        if n > 1 && prop == Longhand::TransitionProperty && eq(item, b"none") {
            return None;
        }
        if i > 0 {
            canon.extend_from_slice(b", ");
        }
        if range {
            let c = range_item_canonical(item, prop == Longhand::AnimationRangeEnd);
            canon.extend_from_slice(c.as_deref().unwrap_or(item));
        } else if prop == Longhand::AnimationName {
            if eq(item, b"none") {
                canon.extend_from_slice(b"none");
            } else if item[0] == b'"' || item[0] == b'\'' {
                let inner = string_decode(item);
                if wide_keyword_or_default(&inner) || eq(&inner, b"none") {
                    canon.push(b'"');
                    canon.extend_from_slice(&inner);
                    canon.push(b'"');
                } else {
                    canon.extend_from_slice(&ident_serialize(&inner));
                }
            } else {
                let ident = ident_decode(item);
                canon.extend_from_slice(&ident_serialize(ident.as_deref().unwrap_or(item)));
            }
        } else if lower {
            canon.extend_from_slice(&item.to_ascii_lowercase());
        } else {
            canon.extend_from_slice(item);
        }
    }
    Some(canon)
}

fn set_target(e: &mut Entry, tok: &[u8], is_animation: bool) {
    e.name = None;
    if is_animation {
        e.target = TARGET_ALL;
        if !eq(tok, b"none") {
            e.name = Some(
                if matches!(tok.first(), Some(b'"' | b'\'')) && tok.len() >= 2 {
                    string_decode(tok)
                } else {
                    ident_decode(tok).unwrap_or_else(|| tok.to_vec())
                },
            );
        }
        return;
    }
    e.target = if eq(tok, b"none") {
        TARGET_NONE
    } else if eq(tok, b"all") {
        TARGET_ALL
    } else if eq(tok, b"opacity") {
        TARGET_OPACITY
    } else if eq(tok, b"transform") {
        TARGET_TRANSFORM
    } else if eq(tok, b"color") {
        TARGET_COLOR
    } else if eq(tok, b"background-color") || eq(tok, b"background") {
        TARGET_BG_COLOR
    } else {
        e.name = Some(tok.to_ascii_lowercase());
        TARGET_OTHER
    };
}

fn apply_longhand(out: &mut [Entry], value: Option<&[u8]>, prop: Longhand) {
    let Some(value) = value else {
        return;
    };
    if out.is_empty() {
        return;
    }
    let items = list_split(value);
    let n = items.len();
    for (i, e) in out.iter_mut().enumerate() {
        let item = items[i % n];
        match prop {
            Longhand::AnimationDuration | Longhand::TransitionDuration => {
                if eq(item, b"auto") {
                    e.duration_ms = 0.0;
                    e.duration_auto = true;
                } else if let Some(sec) = time::seconds(item).filter(|s| s.is_finite()) {
                    e.duration_ms = (if sec > 0.0 { sec } else { 0.0 }) * 1000.0;
                    e.duration_auto = false;
                }
            }
            Longhand::AnimationDelay | Longhand::TransitionDelay => {
                if let Some(sec) = time::seconds(item).filter(|s| s.is_finite()) {
                    e.delay_ms = sec * 1000.0;
                }
            }
            Longhand::AnimationTimingFunction | Longhand::TransitionTimingFunction => {
                if let Some(t) = timing::item_parse(item) {
                    e.timing = t;
                }
            }
            Longhand::AnimationIterationCount => {
                if eq(item, b"infinite") {
                    e.iter_count = -1;
                    e.iterations = f64::INFINITY;
                } else {
                    let (num, _) = ffi::strtod(&c_text(item), 0);
                    e.iterations = if num < 0.0 { 0.0 } else { num };
                    e.iter_count = if num <= 0.0 { 0 } else { num as i32 };
                }
            }
            Longhand::AnimationDirection => {
                e.direction = if eq(item, b"reverse") {
                    DIR_REVERSE
                } else if eq(item, b"alternate") {
                    DIR_ALTERNATE
                } else if eq(item, b"alternate-reverse") {
                    DIR_ALTERNATE_REVERSE
                } else {
                    DIR_NORMAL
                };
            }
            Longhand::AnimationFillMode => {
                e.fill = if eq(item, b"forwards") {
                    FILL_FORWARDS
                } else if eq(item, b"backwards") {
                    FILL_BACKWARDS
                } else if eq(item, b"both") {
                    FILL_BOTH
                } else {
                    FILL_NONE
                };
            }
            Longhand::AnimationPlayState => e.paused = eq(item, b"paused"),
            Longhand::TransitionBehavior => e.allow_discrete = eq(item, b"allow-discrete"),
            _ => {}
        }
    }
}

fn longhand_count(value: Option<&[u8]>) -> usize {
    value.map_or(0, |v| list_split(v).len())
}

fn list_props(is_animation: bool) -> &'static [Longhand] {
    if is_animation {
        &ANIMATION_LIST_PROPS
    } else {
        &TRANSITION_LIST_PROPS
    }
}

fn build_list<'a>(
    value_of: &impl Fn(Longhand) -> Option<&'a [u8]>,
    is_animation: bool,
    n: usize,
) -> Vec<Entry> {
    let n = n.min(ENTRIES_MAX);
    let mut out = vec![Entry::initial(false); n];
    if n == 0 {
        return out;
    }
    let names = value_of(if is_animation {
        Longhand::AnimationName
    } else {
        Longhand::TransitionProperty
    });
    if let Some(names) = names {
        let items = list_split(names);
        for (i, e) in out.iter_mut().enumerate() {
            set_target(e, items[i % items.len()], is_animation);
        }
    }
    for &prop in &list_props(is_animation)[1..] {
        apply_longhand(&mut out, value_of(prop), prop);
    }
    out
}

pub(crate) fn may_animate(value_present: impl Fn(Longhand) -> bool) -> bool {
    value_present(Longhand::AnimationName)
        || TRANSITION_LIST_PROPS.iter().any(|&p| value_present(p))
}

pub(crate) fn effective<'a>(
    value_of: impl Fn(Longhand) -> Option<&'a [u8]>,
    present: impl Fn(Longhand) -> bool,
    is_animation: bool,
) -> Vec<Entry> {
    let n = if is_animation {
        longhand_count(value_of(Longhand::AnimationName))
    } else if present(Longhand::TransitionProperty) {
        longhand_count(value_of(Longhand::TransitionProperty))
    } else {
        TRANSITION_LIST_PROPS
            .iter()
            .map(|&p| longhand_count(value_of(p)))
            .max()
            .unwrap_or(0)
    };
    build_list(&value_of, is_animation, n)
}

pub(crate) fn lists<'a>(
    value_of: impl Fn(Longhand) -> Option<&'a [u8]>,
    is_animation: bool,
) -> (Vec<Entry>, bool) {
    let counts: Vec<usize> = list_props(is_animation)
        .iter()
        .map(|&p| longhand_count(value_of(p)))
        .collect();
    let n = counts.iter().copied().max().unwrap_or(0);
    let mismatch = counts.iter().any(|&c| c > 0 && c != n);
    (build_list(&value_of, is_animation, n), mismatch)
}

fn target_text(e: &Entry) -> Vec<u8> {
    match e.target {
        TARGET_NONE => b"none".to_vec(),
        TARGET_OPACITY => b"opacity".to_vec(),
        TARGET_TRANSFORM => b"transform".to_vec(),
        TARGET_COLOR => b"color".to_vec(),
        TARGET_BG_COLOR => b"background-color".to_vec(),
        TARGET_OTHER => e.name.clone().unwrap_or_else(|| b"all".to_vec()),
        _ => b"all".to_vec(),
    }
}

pub(crate) fn entry_longhand_text(e: &Entry, lh: Longhand) -> Option<Vec<u8>> {
    let text = match lh {
        Longhand::AnimationName => e
            .name
            .as_deref()
            .map_or_else(|| b"none".to_vec(), ident_serialize),
        Longhand::AnimationDuration if e.duration_auto => b"auto".to_vec(),
        Longhand::AnimationDuration | Longhand::TransitionDuration => {
            time::time_text(e.duration_ms)
        }
        Longhand::AnimationDelay | Longhand::TransitionDelay => time::time_text(e.delay_ms),
        Longhand::AnimationTimingFunction | Longhand::TransitionTimingFunction => {
            timing::serialize(Some(&e.timing))
        }
        Longhand::AnimationIterationCount if e.iterations.is_finite() => number_text(e.iterations),
        Longhand::AnimationIterationCount => b"infinite".to_vec(),
        Longhand::AnimationDirection => match e.direction {
            DIR_REVERSE => b"reverse".to_vec(),
            DIR_ALTERNATE => b"alternate".to_vec(),
            DIR_ALTERNATE_REVERSE => b"alternate-reverse".to_vec(),
            _ => b"normal".to_vec(),
        },
        Longhand::AnimationFillMode => match e.fill {
            FILL_FORWARDS => b"forwards".to_vec(),
            FILL_BACKWARDS => b"backwards".to_vec(),
            FILL_BOTH => b"both".to_vec(),
            _ => b"none".to_vec(),
        },
        Longhand::AnimationPlayState if e.paused => b"paused".to_vec(),
        Longhand::AnimationPlayState => b"running".to_vec(),
        Longhand::TransitionProperty => target_text(e),
        Longhand::TransitionBehavior if e.allow_discrete => b"allow-discrete".to_vec(),
        Longhand::TransitionBehavior => b"normal".to_vec(),
        Longhand::AnimationTimeline => b"auto".to_vec(),
        Longhand::AnimationRangeStart | Longhand::AnimationRangeEnd => b"normal".to_vec(),
        Longhand::AnimationComposition => return None,
    };
    Some(text)
}

fn append_part(out: &mut Vec<u8>, part: &[u8]) {
    if out.last().is_some_and(|&c| c != b' ' && c != b',') {
        out.push(b' ');
    }
    out.extend_from_slice(part);
}

fn longhand_part(e: &Entry, lh: Longhand) -> Vec<u8> {
    entry_longhand_text(e, lh).unwrap_or_default()
}

pub(crate) fn shorthand_serialize(list: &[Entry], is_animation: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, e) in list.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        let mut entry = Vec::new();
        if is_animation {
            if !e.duration_auto || e.delay_ms != 0.0 {
                append_part(&mut entry, &longhand_part(e, Longhand::AnimationDuration));
            }
            if e.timing.kind != timing::EASE {
                append_part(&mut entry, &timing::serialize(Some(&e.timing)));
            }
            if e.delay_ms != 0.0 {
                append_part(&mut entry, &time::time_text(e.delay_ms));
            }
            if e.iterations != 1.0 {
                append_part(
                    &mut entry,
                    &longhand_part(e, Longhand::AnimationIterationCount),
                );
            }
            if e.direction != DIR_NORMAL {
                append_part(&mut entry, &longhand_part(e, Longhand::AnimationDirection));
            }
            if e.fill != FILL_NONE {
                append_part(&mut entry, &longhand_part(e, Longhand::AnimationFillMode));
            }
            if e.paused {
                append_part(&mut entry, b"paused");
            }
            if let Some(name) = &e.name {
                append_part(&mut entry, &ident_serialize(name));
            }
            if entry.is_empty() {
                entry.extend_from_slice(b"none");
            }
        } else {
            if e.target != TARGET_ALL {
                append_part(&mut entry, &target_text(e));
            }
            if e.duration_ms != 0.0 || e.delay_ms != 0.0 {
                append_part(&mut entry, &time::time_text(e.duration_ms));
            }
            if e.timing.kind != timing::EASE {
                append_part(&mut entry, &timing::serialize(Some(&e.timing)));
            }
            if e.delay_ms != 0.0 {
                append_part(&mut entry, &time::time_text(e.delay_ms));
            }
            if e.allow_discrete {
                append_part(&mut entry, b"allow-discrete");
            }
            if entry.is_empty() {
                entry.extend_from_slice(b"all");
            }
        }
        out.extend_from_slice(&entry);
    }
    out
}

#[derive(Default)]
struct Seen {
    dur: bool,
    delay: bool,
    timing: bool,
    iter: bool,
    dir: bool,
    fill: bool,
    play: bool,
    name: bool,
    behavior: bool,
}

fn shorthand_token(e: &mut Entry, got: &mut Seen, tok: &[u8], is_animation: bool) -> bool {
    if let Some(t) = timing::item_parse(tok).filter(|_| !got.timing) {
        e.timing = t;
        got.timing = true;
        return true;
    }
    if starts_with_ci(tok, b"steps(")
        || starts_with_ci(tok, b"cubic-bezier(")
        || starts_with_ci(tok, b"linear(")
        || (timing::keyword_matches(tok) && got.timing)
    {
        return false;
    }
    if let Some(ms) = time::parse_ms(tok) {
        if !got.dur {
            if ms < 0.0 {
                return false;
            }
            e.duration_ms = ms;
            got.dur = true;
            e.duration_auto = false;
        } else if !got.delay {
            e.delay_ms = ms;
            got.delay = true;
        } else {
            return false;
        }
        return true;
    }
    let text = c_text(tok);
    let (num, end) = ffi::strtod(&text, 0);
    if end != 0 && end == text.to_bytes().len() {
        if !is_animation || got.iter || num < 0.0 {
            return false;
        }
        e.iter_count = if num <= 0.0 { 0 } else { num as i32 };
        e.iterations = num;
        got.iter = true;
        return true;
    }
    if is_animation && eq(tok, b"infinite") && !got.iter {
        e.iter_count = -1;
        e.iterations = f64::INFINITY;
        got.iter = true;
        return true;
    }
    if is_animation && eq(tok, b"auto") && !got.dur {
        got.dur = true;
        e.duration_auto = true;
        return true;
    }
    if !is_animation && !got.behavior && one_of(tok, &[b"allow-discrete", b"normal"]) {
        e.allow_discrete = eq(tok, b"allow-discrete");
        got.behavior = true;
        return true;
    }
    if is_animation {
        let keywords: [(&[u8], u8, i32); 10] = [
            (b"paused", 0, 1),
            (b"running", 0, 0),
            (b"normal", 1, DIR_NORMAL),
            (b"reverse", 1, DIR_REVERSE),
            (b"alternate", 1, DIR_ALTERNATE),
            (b"alternate-reverse", 1, DIR_ALTERNATE_REVERSE),
            (b"forwards", 2, FILL_FORWARDS),
            (b"backwards", 2, FILL_BACKWARDS),
            (b"both", 2, FILL_BOTH),
            (b"none", 2, FILL_NONE),
        ];
        for (word, group, value) in keywords {
            let seen = match group {
                0 => &mut got.play,
                1 => &mut got.dir,
                _ => &mut got.fill,
            };
            if eq(tok, word) && !*seen {
                *seen = true;
                match group {
                    0 => e.paused = value != 0,
                    1 => e.direction = value,
                    _ => e.fill = value,
                }
                return true;
            }
        }
        if got.name {
            return false;
        }
        if tok[0] == b'"' || tok[0] == b'\'' {
            if tok.len() < 2 || tok[tok.len() - 1] != tok[0] {
                return false;
            }
            let name = string_decode(tok);
            if name.is_empty() {
                return false;
            }
            e.name = Some(name);
        } else {
            let Some(ident) = ident_decode(tok) else {
                return false;
            };
            if wide_keyword_or_default(&ident) || starts_digit_like(tok) {
                return false;
            }
            e.name = (!eq(&ident, b"none")).then_some(ident);
        }
        got.name = true;
        return true;
    }
    if got.name {
        return false;
    }
    let Some(ident) = ident_decode(tok) else {
        return false;
    };
    if wide_keyword_or_default(&ident) {
        return false;
    }
    set_target(e, &ident, false);
    got.name = true;
    true
}

pub(crate) fn shorthand_parse(text: &[u8], is_animation: bool) -> Option<Vec<Entry>> {
    let end = text.len();
    let mut entries = Vec::new();
    let mut p = 0;
    while p < end && entries.len() < ENTRIES_MAX {
        let (seg, term) = scan_until(text, p, end, b",");
        let item = trim_range(text, p, seg);
        if item.is_empty() {
            return None;
        }
        let mut e = Entry::initial(is_animation);
        let mut got = Seen::default();
        for tok in split_ws_paren(item, 16) {
            let tok = strip(tok);
            if tok.is_empty() {
                continue;
            }
            if !shorthand_token(&mut e, &mut got, tok, is_animation) {
                return None;
            }
        }
        if !is_animation
            && e.target == TARGET_NONE
            && (got.dur || got.delay || got.timing || got.behavior)
        {
            return None;
        }
        entries.push(e);
        p = if term == b',' { seg + 1 } else { seg };
    }
    if entries.is_empty() || p < end {
        return None;
    }
    if !is_animation && entries.len() > 1 && entries.iter().any(|e| e.target == TARGET_NONE) {
        return None;
    }
    Some(entries)
}

pub(crate) fn shorthand_canonical(text: &[u8], is_animation: bool) -> Option<Vec<u8>> {
    shorthand_parse(text, is_animation).map(|list| shorthand_serialize(&list, is_animation))
}

fn range_shorthand_split(item: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let toks = split_ws_limit(item, 6);
    let n = toks.len();
    if n == 0 {
        return None;
    }
    let (start, used) = if eq(toks[0], b"normal") {
        (Some(b"normal".to_vec()), 1)
    } else if range_is_name(toks[0]) {
        let lp = if n >= 2 {
            range_lp_canonical(toks[1])
        } else {
            None
        };
        let mut joined = toks[0].to_vec();
        if lp.is_some() {
            joined.push(b' ');
            joined.extend_from_slice(toks[1]);
        }
        (
            range_item_canonical(&joined, false),
            if lp.is_some() { 2 } else { 1 },
        )
    } else {
        (range_item_canonical(toks[0], false), 1)
    };
    let start = start?;
    let end = match n - used {
        0 => {
            let name = &start[..start.iter().position(|&c| c == b' ').unwrap_or(start.len())];
            if range_is_name(name) {
                name.to_vec()
            } else {
                b"normal".to_vec()
            }
        }
        1 => range_item_canonical(toks[used], true)?,
        2 if range_is_name(toks[used]) => {
            let mut joined = toks[used].to_vec();
            joined.push(b' ');
            joined.extend_from_slice(toks[used + 1]);
            range_item_canonical(&joined, true)?
        }
        _ => return None,
    };
    Some((start, end))
}

pub(crate) fn range_shorthand_expand(text: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let items = list_split(text);
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let (st, en) = range_shorthand_split(item)?;
        if i > 0 {
            starts.extend_from_slice(b", ");
            ends.extend_from_slice(b", ");
        }
        starts.extend_from_slice(&st);
        ends.extend_from_slice(&en);
    }
    Some((starts, ends))
}

pub(crate) fn range_serialize(start_list: &[u8], end_list: &[u8]) -> Vec<u8> {
    let starts = list_split(start_list);
    let ends = list_split(end_list);
    let mut out = Vec::new();
    if starts.len() != ends.len() {
        return out;
    }
    for (i, (start, end)) in starts.iter().zip(&ends).enumerate() {
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(start);
        let start_name = &start[..start.iter().position(|&c| c == b' ').unwrap_or(start.len())];
        let named = range_is_name(start_name);
        let omit = if *end == b"normal" {
            !named
        } else if named {
            *end == start_name
        } else {
            *end == b"100%"
        };
        if !omit {
            out.push(b' ');
            out.extend_from_slice(end);
        }
    }
    out
}
