//! Southstar — parsing selectors into compounds, combinators, attribute and pseudo-class predicates, nested :is(), :where(), :not() and :has() groups, specificity and the ancestor hashes the Bloom filter checks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use southstar_dom::attrs::name_bloom_bit;

use crate::ffi;
use crate::lex::{read_ident, read_string};
use crate::scan::{
    is_ident_start, is_ws, scan_until, skip_comment, skip_ws_comments, strip, trim_range,
};

pub(crate) const COMB_NONE: u32 = 0;
pub(crate) const COMB_DESCENDANT: u32 = 1;
pub(crate) const COMB_CHILD: u32 = 2;
pub(crate) const COMB_ADJACENT: u32 = 3;
pub(crate) const COMB_SIBLING: u32 = 4;

pub(crate) const ATTR_PRESENT: u32 = 0;
pub(crate) const ATTR_EQ: u32 = 1;
const ATTR_PREFIX: u32 = 2;
const ATTR_SUFFIX: u32 = 3;
const ATTR_SUBSTR: u32 = 4;
const ATTR_WORD: u32 = 5;
const ATTR_HYPHEN: u32 = 6;

const PE_BEFORE: u32 = 1;
const PE_AFTER: u32 = 2;
const PE_FIRST_LETTER: u32 = 3;
const PE_FIRST_LINE: u32 = 4;
const PE_SELECTION: u32 = 5;
const PE_MARKER: u32 = 6;
const PE_BACKDROP: u32 = 7;
const PE_PLACEHOLDER: u32 = 8;
const PE_FILE_SELECTOR_BUTTON: u32 = 9;

const PC_NTH_CHILD: u32 = 19;
const PC_NTH_LAST_CHILD: u32 = 20;
const PC_NTH_OF_TYPE: u32 = 21;
const PC_NTH_LAST_OF_TYPE: u32 = 22;
const PC_HOVER: u32 = 26;
const PC_ACTIVE: u32 = 27;
const PC_LANG: u32 = 39;
const PC_DIR: u32 = 40;
const PC_HEADING: u32 = 45;

const MAX_SELECTOR_NESTING: i32 = 48;
const ANCESTOR_HASHES_MAX: usize = 4;

const PSEUDO_CLASSES: [(&[u8], u32); 49] = [
    (b"first-child", 0),
    (b"last-child", 1),
    (b"only-child", 2),
    (b"first-of-type", 4),
    (b"last-of-type", 5),
    (b"only-of-type", 3),
    (b"empty", 6),
    (b"root", 7),
    (b"checked", 8),
    (b"disabled", 9),
    (b"enabled", 10),
    (b"required", 11),
    (b"optional", 12),
    (b"valid", 13),
    (b"invalid", 14),
    (b"in-range", 15),
    (b"out-of-range", 16),
    (b"default", 17),
    (b"indeterminate", 18),
    (b"link", 23),
    (b"visited", 24),
    (b"any-link", 25),
    (b"hover", PC_HOVER),
    (b"active", PC_ACTIVE),
    (b"focus", 28),
    (b"focus-visible", 29),
    (b"focus-within", 30),
    (b"target", 31),
    (b"target-within", 32),
    (b"defined", 33),
    (b"scope", 34),
    (b"placeholder-shown", 35),
    (b"read-only", 36),
    (b"read-write", 37),
    (b"blank", 38),
    (b"open", 41),
    (b"popover-open", 42),
    (b"modal", 43),
    (b"fullscreen", 44),
    (b"user-valid", 46),
    (b"user-invalid", 47),
    (b"autofill", 48),
    (b"-webkit-autofill", 48),
    (b"playing", 49),
    (b"paused", 50),
    (b"muted", 51),
    (b"seeking", 52),
    (b"buffering", 53),
    (b"stalled", 54),
];

const STANDARD_PSEUDO_CLASSES: [&[u8]; 29] = [
    b"default",
    b"indeterminate",
    b"in-range",
    b"out-of-range",
    b"fullscreen",
    b"modal",
    b"autofill",
    b"blank",
    b"user-valid",
    b"user-invalid",
    b"target-within",
    b"focus-visible",
    b"local-link",
    b"current",
    b"past",
    b"future",
    b"playing",
    b"paused",
    b"muted",
    b"seeking",
    b"buffering",
    b"stalled",
    b"picture-in-picture",
    b"volume-locked",
    b"host",
    b"host-context",
    b"nth-col",
    b"nth-last-col",
    b"state",
];

const STANDARD_PSEUDO_ELEMENTS: [&[u8]; 15] = [
    b"part",
    b"slotted",
    b"cue",
    b"cue-region",
    b"highlight",
    b"target-text",
    b"spelling-error",
    b"grammar-error",
    b"file-selector-button",
    b"details-content",
    b"view-transition",
    b"view-transition-group",
    b"view-transition-image-pair",
    b"view-transition-old",
    b"view-transition-new",
];

const HTML_CI_ATTRS: [&[u8]; 46] = [
    b"accept",
    b"accept-charset",
    b"align",
    b"alink",
    b"axis",
    b"bgcolor",
    b"charset",
    b"checked",
    b"clear",
    b"codetype",
    b"color",
    b"compact",
    b"declare",
    b"defer",
    b"dir",
    b"direction",
    b"disabled",
    b"enctype",
    b"face",
    b"frame",
    b"hreflang",
    b"http-equiv",
    b"lang",
    b"language",
    b"link",
    b"media",
    b"method",
    b"multiple",
    b"nohref",
    b"noresize",
    b"noshade",
    b"nowrap",
    b"readonly",
    b"rel",
    b"rev",
    b"rules",
    b"scope",
    b"scrolling",
    b"selected",
    b"shape",
    b"target",
    b"text",
    b"type",
    b"valign",
    b"valuetype",
    b"vlink",
];

pub(crate) struct Flags {
    pub parse_error: AtomicBool,
    pub ns_prefix: AtomicBool,
    pub has_hover: AtomicBool,
    pub has_active: AtomicBool,
    pub strict: AtomicBool,
    pub has_depth: AtomicI32,
    pub attr_ancestor_hashes: AtomicBool,
}

pub(crate) static FLAGS: Flags = Flags {
    parse_error: AtomicBool::new(false),
    ns_prefix: AtomicBool::new(false),
    has_hover: AtomicBool::new(false),
    has_active: AtomicBool::new(false),
    strict: AtomicBool::new(false),
    has_depth: AtomicI32::new(0),
    attr_ancestor_hashes: AtomicBool::new(false),
};

pub(crate) fn get(flag: &AtomicBool) -> bool {
    flag.load(Ordering::Relaxed)
}

pub(crate) fn set(flag: &AtomicBool, value: bool) {
    flag.store(value, Ordering::Relaxed);
}

fn error() {
    set(&FLAGS.parse_error, true);
}

#[derive(Default)]
pub(crate) struct AttrPred {
    pub name: Vec<u8>,
    pub op: u32,
    pub value: Option<Vec<u8>>,
    pub case_insensitive: bool,
    pub case_sensitive: bool,
    pub html_ci: bool,
    pub name_bit: u64,
}

#[derive(Default)]
pub(crate) struct PseudoPred {
    pub kind: u32,
    pub a: i32,
    pub b: i32,
    pub arg: Option<Vec<u8>>,
    pub of_group: Option<Vec<Selector>>,
}

#[derive(Default)]
pub(crate) struct Compound {
    pub type_: Option<Vec<u8>>,
    pub id: Option<Vec<u8>>,
    pub classes: Vec<Vec<u8>>,
    pub attrs: Vec<AttrPred>,
    pub pseudos: Vec<PseudoPred>,
    pub matches_any: Option<Vec<Vec<Selector>>>,
    pub matches_none: Option<Vec<Vec<Selector>>>,
    pub has_groups: Option<Vec<Vec<Selector>>>,
    pub never_match: bool,
    pub ns_none: bool,
}

#[derive(Default)]
pub(crate) struct Selector {
    pub compounds: Vec<Compound>,
    pub combinators: Vec<u32>,
    pub pseudo_element: u32,
    pub spec: [i32; 3],
    pub ancestor_hashes: Vec<u32>,
    pub n_ancestor_attr_hashes: u32,
}

impl Selector {
    fn add_spec(&mut self, spec: [i32; 3]) {
        for (mine, add) in self.spec.iter_mut().zip(spec) {
            *mine += add;
        }
    }

    fn add_ancestor_hash(&mut self, hash: u32) {
        if self.ancestor_hashes.len() < ANCESTOR_HASHES_MAX {
            self.ancestor_hashes.push(hash);
        }
    }
}

fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn max_specificity(group: &[Selector]) -> [i32; 3] {
    let mut best = [0; 3];
    for sub in group {
        let [a, b, c] = sub.spec;
        if a > best[0]
            || (a == best[0] && b > best[1])
            || (a == best[0] && b == best[1] && c > best[2])
        {
            best = sub.spec;
        }
    }
    best
}

pub(crate) fn identifier_hash(kind: u8, name: &[u8]) -> u32 {
    let mut h: u32 = 2_166_136_261;
    h = (h ^ u32::from(kind)).wrapping_mul(16_777_619);
    for &c in name {
        h = (h ^ u32::from(c.to_ascii_lowercase())).wrapping_mul(16_777_619);
    }
    h
}

pub(crate) fn attr_value_hash(name: &[u8], value: &[u8]) -> u32 {
    let mut h = identifier_hash(b'[', name);
    h = (h ^ u32::from(b'=')).wrapping_mul(16_777_619);
    for &c in value {
        h = (h ^ u32::from(c)).wrapping_mul(16_777_619);
    }
    h
}

fn attr_filterable(a: &AttrPred) -> bool {
    a.op == ATTR_EQ
        && a.value.is_some()
        && !a.case_insensitive
        && !a.html_ci
        && !a.name.contains(&b'|')
}

fn collect_ancestor_hashes(sel: &mut Selector) {
    let n = sel.compounds.len();
    for k in (0..n.saturating_sub(1)).rev() {
        let right = sel.combinators[k + 1];
        if right != COMB_DESCENDANT && right != COMB_CHILD {
            continue;
        }
        let hashes: Vec<u32> = sel.compounds[k]
            .attrs
            .iter()
            .filter(|a| attr_filterable(a))
            .map(|a| attr_value_hash(&a.name, a.value.as_deref().unwrap_or_default()))
            .collect();
        for hash in hashes {
            sel.add_ancestor_hash(hash);
            sel.n_ancestor_attr_hashes = sel.ancestor_hashes.len() as u32;
            set(&FLAGS.attr_ancestor_hashes, true);
        }
    }
    for k in (0..n.saturating_sub(1)).rev() {
        let right = sel.combinators[k + 1];
        if right != COMB_DESCENDANT && right != COMB_CHILD {
            continue;
        }
        let c = &sel.compounds[k];
        let mut hashes = Vec::new();
        if let Some(id) = &c.id {
            hashes.push(identifier_hash(b'#', id));
        }
        for class in &c.classes {
            hashes.push(identifier_hash(b'.', class));
        }
        if let Some(t) = c.type_.as_deref().filter(|t| *t != b"*") {
            hashes.push(identifier_hash(b'%', t));
        }
        for hash in hashes {
            sel.add_ancestor_hash(hash);
        }
    }
}

pub(crate) fn html_ci_attr(name: &[u8]) -> bool {
    HTML_CI_ATTRS.iter().any(|known| eq_ci(name, known))
}

fn find_nth_of(s: &[u8]) -> Option<usize> {
    let end = s.len();
    let mut quote = 0u8;
    let (mut paren, mut bracket) = (0u32, 0u32);
    let mut p = 0;
    while p < end {
        let c = s[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < end {
                p += 2;
            } else {
                if c == quote {
                    quote = 0;
                }
                p += 1;
            }
            continue;
        }
        if c == b'/' && p + 1 < end && s[p + 1] == b'*' {
            p = skip_comment(s, p, end);
            continue;
        }
        if c == b'\\' && p + 1 < end {
            p += 2;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
            p += 1;
            continue;
        }
        match c {
            b'[' => bracket += 1,
            b']' if bracket > 0 => bracket -= 1,
            b'(' => paren += 1,
            b')' if paren > 0 => paren -= 1,
            _ => {}
        }
        if paren == 0
            && bracket == 0
            && p + 2 <= end
            && eq_ci(&s[p..p + 2], b"of")
            && (p == 0 || is_ws(s[p - 1]))
            && (p + 2 == end || is_ws(s[p + 2]))
        {
            return Some(p);
        }
        p += 1;
    }
    None
}

pub(crate) fn anb_int_strict(text: &[u8]) -> Option<i32> {
    let digits = match text.first() {
        Some(b'+' | b'-') => &text[1..],
        _ => text,
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(ffi::parse_int(text, 0, -1_000_000, 1_000_000))
}

fn parse_anb(arg: &[u8]) -> Option<(i32, i32)> {
    let s: Vec<u8> = arg.iter().copied().filter(|&c| !is_ws(c)).collect();
    if eq_ci(&s, b"odd") {
        return Some((2, 1));
    }
    if eq_ci(&s, b"even") {
        return Some((2, 0));
    }
    let n_pos = s
        .iter()
        .position(|&c| c == b'n')
        .or_else(|| s.iter().position(|&c| c == b'N'));
    let Some(n_pos) = n_pos else {
        return Some((0, anb_int_strict(&s)?));
    };
    let a_str = &s[..n_pos];
    let a = match a_str {
        b"" | b"+" => Some(1),
        b"-" => Some(-1),
        _ => anb_int_strict(a_str),
    };
    let b_str = &s[n_pos + 1..];
    let b = if b_str.is_empty() {
        Some(0)
    } else if b_str[0] != b'+' && b_str[0] != b'-' {
        None
    } else {
        a.and_then(|_| anb_int_strict(b_str))
    };
    Some((a?, b?))
}

fn pseudo_keyword(name: &[u8], arg: Option<&[u8]>, depth: i32) -> Option<PseudoPred> {
    if let Some(&(_, kind)) = PSEUDO_CLASSES.iter().find(|(k, _)| eq_ci(name, k)) {
        return Some(PseudoPred {
            kind,
            ..PseudoPred::default()
        });
    }
    if eq_ci(name, b"heading") {
        let Some(arg) = arg else {
            return Some(PseudoPred {
                kind: PC_HEADING,
                ..PseudoPred::default()
            });
        };
        if arg.is_empty()
            || arg
                .split(|&c| c == b',')
                .any(|item| anb_int_strict(strip(item)).is_none())
        {
            return None;
        }
        return Some(PseudoPred {
            kind: PC_HEADING,
            arg: Some(arg.to_vec()),
            ..PseudoPred::default()
        });
    }
    let nth_kind = match name.to_ascii_lowercase().as_slice() {
        b"nth-child" => Some(PC_NTH_CHILD),
        b"nth-last-child" => Some(PC_NTH_LAST_CHILD),
        b"nth-of-type" => Some(PC_NTH_OF_TYPE),
        b"nth-last-of-type" => Some(PC_NTH_LAST_OF_TYPE),
        _ => None,
    };
    if let (Some(kind), Some(arg)) = (nth_kind, arg) {
        let of = if kind == PC_NTH_CHILD || kind == PC_NTH_LAST_CHILD {
            find_nth_of(arg)
        } else {
            None
        };
        let (a, b) = parse_anb(&arg[..of.unwrap_or(arg.len())])?;
        let of_group = match of {
            Some(of) => {
                let from = skip_ws_comments(arg, of + 2, arg.len());
                let group = parse_group(&arg[from..], depth + 1, false);
                if group.is_empty() {
                    return None;
                }
                Some(group)
            }
            None => None,
        };
        return Some(PseudoPred {
            kind,
            a,
            b,
            arg: None,
            of_group,
        });
    }
    match (arg, name.len()) {
        (Some(arg), 4) if eq_ci(name, b"lang") => {
            let lang = trim_range(arg, 0, arg.len());
            (!lang.is_empty()).then(|| PseudoPred {
                kind: PC_LANG,
                arg: Some(lang.to_vec()),
                ..PseudoPred::default()
            })
        }
        (Some(arg), 3) if eq_ci(name, b"dir") => {
            let dir = trim_range(arg, 0, arg.len()).to_ascii_lowercase();
            (dir == b"ltr" || dir == b"rtl").then(|| PseudoPred {
                kind: PC_DIR,
                arg: Some(dir),
                ..PseudoPred::default()
            })
        }
        _ => None,
    }
}

pub(crate) fn parse_group(arg: &[u8], depth: i32, relative: bool) -> Vec<Selector> {
    let mut group = Vec::new();
    if depth > MAX_SELECTOR_NESTING {
        return group;
    }
    let end = arg.len();
    let mut p = 0;
    while p < end {
        let loop_start = p;
        p = skip_ws_comments(arg, p, end);
        if p >= end {
            break;
        }
        match parse_one(arg, &mut p, depth, relative) {
            Some(sub) => group.push(sub),
            None if get(&FLAGS.strict) => error(),
            None => {}
        }
        p = skip_ws_comments(arg, p, end);
        if p < end && arg[p] == b',' {
            p += 1;
            continue;
        }
        if p == loop_start {
            p += 1;
        }
    }
    group
}

fn not_equals_follows(s: &[u8], p: usize) -> bool {
    p + 1 < s.len() && s[p + 1] == b'='
}

fn pseudo_element(name: &[u8]) -> Option<u32> {
    let known: [(&[u8], u32); 12] = [
        (b"before", PE_BEFORE),
        (b"after", PE_AFTER),
        (b"first-letter", PE_FIRST_LETTER),
        (b"first-line", PE_FIRST_LINE),
        (b"selection", PE_SELECTION),
        (b"marker", PE_MARKER),
        (b"backdrop", PE_BACKDROP),
        (b"file-selector-button", PE_FILE_SELECTOR_BUTTON),
        (b"placeholder", PE_PLACEHOLDER),
        (b"-webkit-input-placeholder", PE_PLACEHOLDER),
        (b"-ms-input-placeholder", PE_PLACEHOLDER),
        (b"-moz-placeholder", PE_PLACEHOLDER),
    ];
    known
        .iter()
        .find(|(known, _)| eq_ci(name, known))
        .map(|&(_, pe)| pe)
}

fn parse_pseudo(s: &[u8], p: &mut usize, sel: &mut Selector, cmp: &mut Compound, depth: i32) {
    let end = s.len();
    *p += 1;
    let is_element = *p < end && s[*p] == b':';
    if is_element {
        *p += 1;
    }
    let name = read_ident(s, p, end);
    if name.is_empty() {
        error();
        cmp.never_match = true;
        return;
    }
    let mut arg = None;
    if *p < end && s[*p] == b'(' {
        *p += 1;
        let start = *p;
        let (arg_end, term) = scan_until(s, *p, end, b")");
        arg = Some(&s[start..arg_end]);
        *p = if term == b')' { arg_end + 1 } else { arg_end };
    }
    let legacy_element = [&b"before"[..], b"after", b"first-line", b"first-letter"]
        .iter()
        .any(|known| eq_ci(&name, known));
    if is_element || legacy_element {
        match pseudo_element(&name) {
            Some(pe) => {
                sel.pseudo_element = pe;
                sel.spec[2] += 1;
            }
            None => {
                cmp.never_match = true;
                if name[0] != b'-' && !STANDARD_PSEUDO_ELEMENTS.iter().any(|k| eq_ci(&name, k)) {
                    error();
                }
            }
        }
        return;
    }
    let is_has = eq_ci(&name, b"has");
    match arg {
        Some(_) if is_has && FLAGS.has_depth.load(Ordering::Relaxed) > 0 => {
            cmp.never_match = true;
            error();
        }
        Some(arg) if is_has => {
            FLAGS.has_depth.fetch_add(1, Ordering::Relaxed);
            let group = parse_group(arg, depth + 1, true);
            FLAGS.has_depth.fetch_sub(1, Ordering::Relaxed);
            if group.is_empty() {
                cmp.never_match = true;
            } else {
                sel.add_spec(max_specificity(&group));
                cmp.has_groups.get_or_insert_with(Vec::new).push(group);
            }
        }
        Some(arg) if eq_ci(&name, b"is") || eq_ci(&name, b"where") => {
            let is_where = name.len() == 5;
            let saved_error = get(&FLAGS.parse_error);
            let saved_ns = get(&FLAGS.ns_prefix);
            let group = parse_group(arg, depth + 1, false);
            if !get(&FLAGS.strict) {
                set(&FLAGS.parse_error, saved_error);
            }
            set(&FLAGS.ns_prefix, saved_ns);
            if group.is_empty() {
                cmp.never_match = true;
            } else {
                if !is_where {
                    sel.add_spec(max_specificity(&group));
                }
                cmp.matches_any.get_or_insert_with(Vec::new).push(group);
            }
        }
        Some(arg) if eq_ci(&name, b"not") => {
            let group = parse_group(arg, depth + 1, false);
            if !group.is_empty() {
                sel.add_spec(max_specificity(&group));
                cmp.matches_none.get_or_insert_with(Vec::new).push(group);
            }
        }
        _ => match pseudo_keyword(&name, arg, depth) {
            Some(pc) => {
                if pc.kind == PC_HOVER {
                    set(&FLAGS.has_hover, true);
                }
                if pc.kind == PC_ACTIVE {
                    set(&FLAGS.has_active, true);
                }
                sel.spec[1] += 1;
                if let Some(group) = &pc.of_group {
                    sel.add_spec(max_specificity(group));
                }
                cmp.pseudos.push(pc);
            }
            None => {
                cmp.never_match = true;
                if !STANDARD_PSEUDO_CLASSES.iter().any(|k| eq_ci(&name, k)) {
                    error();
                }
            }
        },
    }
}

fn parse_attr(s: &[u8], p: &mut usize, sel: &mut Selector, cmp: &mut Compound) -> bool {
    let end = s.len();
    *p = skip_ws_comments(s, *p + 1, end);
    if *p + 1 < end && s[*p] == b'*' && s[*p + 1] == b'|' {
        *p += 2;
    } else if *p < end && s[*p] == b'|' && !not_equals_follows(s, *p) {
        *p += 1;
    }
    let mut name = read_ident(s, p, end);
    if !name.is_empty() && *p < end && s[*p] == b'|' && !not_equals_follows(s, *p) {
        set(&FLAGS.ns_prefix, true);
        *p += 1;
        name = read_ident(s, p, end);
        cmp.never_match = true;
    }
    if name.is_empty() {
        let (close, term) = scan_until(s, *p, end, b"]");
        *p = if term == b']' { close + 1 } else { close };
        return false;
    }
    let name = name.to_ascii_lowercase();
    let mut pred = AttrPred {
        name_bit: name_bloom_bit(&name),
        html_ci: html_ci_attr(&name),
        name,
        op: ATTR_PRESENT,
        ..AttrPred::default()
    };
    *p = skip_ws_comments(s, *p, end);
    if *p < end && matches!(s[*p], b'=' | b'^' | b'$' | b'*' | b'~' | b'|') {
        let op_c = s[*p];
        if op_c == b'=' {
            pred.op = ATTR_EQ;
        } else {
            *p += 1;
            if *p < end && s[*p] == b'=' {
                pred.op = match op_c {
                    b'^' => ATTR_PREFIX,
                    b'$' => ATTR_SUFFIX,
                    b'*' => ATTR_SUBSTR,
                    b'~' => ATTR_WORD,
                    _ => ATTR_HYPHEN,
                };
            }
        }
        if *p < end && s[*p] == b'=' {
            *p += 1;
        }
        *p = skip_ws_comments(s, *p, end);
        let quoted = *p < end && (s[*p] == b'"' || s[*p] == b'\'');
        pred.value = Some(if quoted {
            read_string(s, p, end)
        } else {
            read_ident(s, p, end)
        });
    }
    *p = skip_ws_comments(s, *p, end);
    if *p < end && s[*p] != b']' {
        let flag_start = *p;
        let flag = read_ident(s, p, end);
        if eq_ci(&flag, b"i") {
            if pred.op == ATTR_PRESENT {
                error();
            }
            pred.case_insensitive = true;
        } else if eq_ci(&flag, b"s") {
            if pred.op == ATTR_PRESENT {
                error();
            }
            pred.case_sensitive = true;
        } else {
            *p = flag_start;
            error();
        }
    }
    *p = skip_ws_comments(s, *p, end);
    if *p < end && s[*p] != b']' {
        error();
    }
    let (close, term) = scan_until(s, *p, end, b"]");
    *p = if term == b']' { close + 1 } else { close };
    cmp.attrs.push(pred);
    sel.spec[1] += 1;
    true
}

fn parse_compound(s: &[u8], p: &mut usize, sel: &mut Selector, depth: i32) -> Option<Compound> {
    let end = s.len();
    let mut cmp = Compound::default();
    let mut any = false;
    while *p < end {
        let tok_start = *p;
        let cc = s[*p];
        if cc == b'*' || (cc == b'|' && !not_equals_follows(s, *p)) {
            if any {
                error();
                cmp.never_match = true;
            }
            if cc == b'*' {
                *p += 1;
            }
            if *p < end && s[*p] == b'|' && !not_equals_follows(s, *p) {
                if cc == b'|' {
                    cmp.ns_none = true;
                }
                *p += 1;
                if *p < end && s[*p] == b'*' {
                    *p += 1;
                    cmp.type_ = Some(b"*".to_vec());
                } else {
                    let name = read_ident(s, p, end);
                    if name.is_empty() {
                        error();
                    } else if cmp.type_.is_none() {
                        cmp.type_ = Some(name.to_ascii_lowercase());
                        sel.spec[2] += 1;
                    }
                }
            } else {
                if cmp.type_.is_some() {
                    error();
                    cmp.never_match = true;
                }
                cmp.type_ = Some(b"*".to_vec());
            }
            any = true;
        } else if cc == b'#' {
            *p += 1;
            let id = read_ident(s, p, end);
            if id.is_empty() {
                error();
                cmp.never_match = true;
            } else {
                cmp.id = Some(id);
                sel.spec[0] += 1;
            }
            any = true;
        } else if cc == b'.' {
            *p += 1;
            let bad_start = *p < end
                && (s[*p].is_ascii_digit()
                    || (s[*p] == b'-' && *p + 1 < end && s[*p + 1].is_ascii_digit()));
            let class = read_ident(s, p, end);
            if !bad_start && !class.is_empty() {
                cmp.classes.push(class);
                sel.spec[1] += 1;
            } else {
                error();
                cmp.never_match = true;
            }
            any = true;
        } else if is_ident_start(cc) || cc == b'\\' {
            if any {
                error();
                cmp.never_match = true;
            }
            let name = read_ident(s, p, end);
            if *p < end && s[*p] == b'|' && !not_equals_follows(s, *p) {
                set(&FLAGS.ns_prefix, true);
                cmp.never_match = true;
                *p += 1;
                if *p < end && s[*p] == b'*' {
                    *p += 1;
                } else {
                    read_ident(s, p, end);
                }
            } else if cmp.type_.is_none() {
                cmp.type_ = Some(name.to_ascii_lowercase());
                sel.spec[2] += 1;
            } else {
                error();
                cmp.never_match = true;
            }
            any = true;
        } else if cc == b':' {
            parse_pseudo(s, p, sel, &mut cmp, depth);
            any = true;
            continue;
        } else if cc == b'[' {
            if parse_attr(s, p, sel, &mut cmp) {
                any = true;
            } else {
                continue;
            }
        } else {
            break;
        }
        if *p == tok_start {
            break;
        }
    }
    any.then_some(cmp)
}

pub(crate) fn parse_one(s: &[u8], p: &mut usize, depth: i32, relative: bool) -> Option<Selector> {
    let end = s.len();
    let mut sel = Selector::default();
    let mut pending = COMB_NONE;
    let mut expect_compound = true;
    let mut leading_comb_used = false;
    while *p < end {
        let before_ws = *p;
        *p = skip_ws_comments(s, *p, end);
        let had_ws = *p > before_ws;
        if *p >= end {
            break;
        }
        let c = s[*p];
        if c == b',' || c == b'{' {
            break;
        }
        if c == b'>' || c == b'+' || c == b'~' {
            if relative && sel.compounds.is_empty() && !leading_comb_used {
                leading_comb_used = true;
            } else if expect_compound || sel.compounds.is_empty() {
                error();
            }
            pending = match c {
                b'>' => COMB_CHILD,
                b'+' => COMB_ADJACENT,
                _ => COMB_SIBLING,
            };
            expect_compound = true;
            *p += 1;
            continue;
        }
        if had_ws && !expect_compound {
            pending = COMB_DESCENDANT;
        }
        let Some(cmp) = parse_compound(s, p, &mut sel, depth) else {
            break;
        };
        sel.compounds.push(cmp);
        sel.combinators.push(pending);
        pending = COMB_NONE;
        expect_compound = false;
    }
    if pending != COMB_NONE {
        error();
    }
    if sel.compounds.is_empty() {
        return None;
    }
    if !relative {
        collect_ancestor_hashes(&mut sel);
    }
    Some(sel)
}

pub(crate) fn parse_list(text: &[u8]) -> Vec<Selector> {
    let end = text.len();
    let mut out = Vec::new();
    let mut p = 0;
    let mut expect_selector = true;
    while p < end {
        while p < end && is_ws(text[p]) {
            p += 1;
        }
        if p >= end {
            break;
        }
        if text[p] == b',' {
            error();
            p += 1;
            expect_selector = true;
            continue;
        }
        let iter_start = p;
        if let Some(sel) = parse_one(text, &mut p, 0, false) {
            out.push(sel);
            expect_selector = false;
        }
        while p < end && is_ws(text[p]) {
            p += 1;
        }
        if p < end && text[p] == b',' {
            p += 1;
            expect_selector = true;
        } else if p == iter_start {
            break;
        }
    }
    if expect_selector {
        error();
    }
    out
}

pub(crate) fn parse_list_checked(text: Option<&[u8]>) -> (Vec<Selector>, bool) {
    set(&FLAGS.parse_error, false);
    set(&FLAGS.ns_prefix, false);
    let list = text.map(parse_list).unwrap_or_default();
    let valid = !get(&FLAGS.parse_error) && !get(&FLAGS.ns_prefix) && !list.is_empty();
    set(&FLAGS.parse_error, false);
    set(&FLAGS.ns_prefix, false);
    (list, valid)
}

pub(crate) struct RuleSelectors {
    pub selectors: Vec<Selector>,
    pub ok: bool,
    pub has_hover: bool,
    pub has_active: bool,
}

pub(crate) fn parse_rule_selectors(text: &[u8]) -> RuleSelectors {
    set(&FLAGS.has_hover, false);
    set(&FLAGS.has_active, false);
    set(&FLAGS.parse_error, false);
    let end = text.len();
    let mut p = 0;
    let mut selectors = Vec::new();
    let mut ok = false;
    while p < end {
        match parse_one(text, &mut p, 0, false) {
            Some(sel) => {
                selectors.push(sel);
                ok = true;
            }
            None => error(),
        }
        while p < end && is_ws(text[p]) {
            p += 1;
        }
        if p < end && text[p] == b',' {
            p += 1;
            while p < end && is_ws(text[p]) {
                p += 1;
            }
            if p >= end {
                error();
            }
            continue;
        }
        if p < end {
            error();
        }
        break;
    }
    RuleSelectors {
        selectors,
        ok: ok && !get(&FLAGS.parse_error),
        has_hover: get(&FLAGS.has_hover),
        has_active: get(&FLAGS.has_active),
    }
}

fn selector_supported(sel: &Selector) -> bool {
    !sel.compounds.is_empty()
        && sel.compounds.iter().all(|c| {
            !c.never_match
                && [&c.matches_any, &c.matches_none, &c.has_groups]
                    .into_iter()
                    .flatten()
                    .flatten()
                    .flatten()
                    .all(selector_supported)
        })
}

pub(crate) fn supports_selector(text: &[u8]) -> bool {
    let saved_strict = get(&FLAGS.strict);
    set(&FLAGS.strict, true);
    let (list, valid) = parse_list_checked(Some(text));
    set(&FLAGS.strict, saved_strict);
    valid && list.len() == 1 && list.iter().all(selector_supported)
}
