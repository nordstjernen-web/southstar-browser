//! Southstar — the CSS properties defined by one table row each, giving the name's property, its value kind, whether it inherits, its initial value and its value grammar, and the parser that reads their values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::sync::OnceLock;

use crate::calc::Parsed;
use crate::prop::Prop;
use crate::property::{Body, Value, calc_value, color_prop, length};
use crate::scan::{split_ws_paren, strip};
use crate::units::{NUMBER, PERCENT, PX};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Keywords,
    Length,
    Color,
    Number,
}

pub(crate) struct Def {
    pub(crate) prop: Prop,
    pub(crate) kind: Kind,
    pub(crate) inherited: bool,
    pub(crate) initial: &'static CStr,
    pub(crate) grammar: &'static str,
}

pub(crate) const DEFS: [Def; 61] = [
    Def {
        prop: Prop::TextUnderlineOffset,
        kind: Kind::Length,
        inherited: true,
        initial: c"auto",
        grammar: "auto | <length-percentage>",
    },
    Def {
        prop: Prop::TextDecorationThickness,
        kind: Kind::Length,
        inherited: false,
        initial: c"auto",
        grammar: "auto | from-font | <length-percentage>",
    },
    Def {
        prop: Prop::TextUnderlinePosition,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | [from-font under] [left right]",
    },
    Def {
        prop: Prop::TextDecorationSkipInk,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none | all",
    },
    Def {
        prop: Prop::TextDecorationSkipSpaces,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"start end",
        grammar: "none | all | [start] [end]",
    },
    Def {
        prop: Prop::TextEmphasisStyle,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"none",
        grammar: "none | <string> | [filled* open] [dot circle double-circle triangle sesame]",
    },
    Def {
        prop: Prop::TextEmphasisColor,
        kind: Kind::Color,
        inherited: true,
        initial: c"currentcolor",
        grammar: "currentcolor | <color>",
    },
    Def {
        prop: Prop::TextEmphasisPosition,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"over",
        grammar: "auto | ![over under] [right* left]",
    },
    Def {
        prop: Prop::ScrollBehavior,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"auto",
        grammar: "auto | smooth",
    },
    Def {
        prop: Prop::TouchAction,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"auto",
        grammar: "auto | none | manipulation | [pan-x pan-left pan-right] [pan-y pan-up pan-down] [pinch-zoom]",
    },
    Def {
        prop: Prop::ColorScheme,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"normal",
        grammar: "normal",
    },
    Def {
        prop: Prop::FontVariantAlternates,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"normal",
        grammar: "normal",
    },
    Def {
        prop: Prop::FontSynthesisWeight,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none",
    },
    Def {
        prop: Prop::FontSynthesisStyle,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none | oblique-only",
    },
    Def {
        prop: Prop::FontSynthesisSmallCaps,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none",
    },
    Def {
        prop: Prop::FontSynthesisPosition,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none",
    },
    Def {
        prop: Prop::TextRendering,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | optimizespeed | optimizelegibility | geometricprecision",
    },
    Def {
        prop: Prop::TextSizeAdjust,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none | <percentage [0,inf]>",
    },
    Def {
        prop: Prop::Zoom,
        kind: Kind::Number,
        inherited: false,
        initial: c"1",
        grammar: "normal | <number [0,inf]>",
    },
    Def {
        prop: Prop::Resize,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"none",
        grammar: "none | both | horizontal | vertical | block | inline",
    },
    Def {
        prop: Prop::EmptyCells,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"show",
        grammar: "show | hide",
    },
    Def {
        prop: Prop::LineBreak,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | loose | normal | strict | anywhere",
    },
    Def {
        prop: Prop::HyphenateCharacter,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | <string>",
    },
    Def {
        prop: Prop::HyphenateLimitChars,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "[auto <integer [1,inf]>]{1,3}",
    },
    Def {
        prop: Prop::TextCombineUpright,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"none",
        grammar: "none | all | ![digits] [<integer [2,4]>]",
    },
    Def {
        prop: Prop::RubyPosition,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"alternate",
        grammar: "inter-character | [alternate] [over under]",
    },
    Def {
        prop: Prop::RubyAlign,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"space-around",
        grammar: "start | center | space-between | space-around",
    },
    Def {
        prop: Prop::WhiteSpaceCollapse,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"collapse",
        grammar: "collapse | discard | preserve | preserve-breaks | preserve-spaces | break-spaces",
    },
    Def {
        prop: Prop::TextWrapMode,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"wrap",
        grammar: "wrap | nowrap",
    },
    Def {
        prop: Prop::TextWrapStyle,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | balance | stable | pretty | avoid-orphans",
    },
    Def {
        prop: Prop::PrintColorAdjust,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"economy",
        grammar: "economy | exact",
    },
    Def {
        prop: Prop::ForcedColorAdjust,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none | preserve-parent-color",
    },
    Def {
        prop: Prop::ImageOrientation,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"from-image",
        grammar: "from-image | none",
    },
    Def {
        prop: Prop::ScrollbarGutter,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"auto",
        grammar: "auto | ![stable] [both-edges]",
    },
    Def {
        prop: Prop::OverflowAnchor,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"auto",
        grammar: "auto | none",
    },
    Def {
        prop: Prop::BoxDecorationBreak,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"slice",
        grammar: "slice | clone",
    },
    Def {
        prop: Prop::ColorInterpolation,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"srgb",
        grammar: "auto | srgb | linearrgb",
    },
    Def {
        prop: Prop::ColorInterpolationFilters,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"linearrgb",
        grammar: "auto | srgb | linearrgb",
    },
    Def {
        prop: Prop::ColorRendering,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | optimizespeed | optimizequality",
    },
    Def {
        prop: Prop::FloodColor,
        kind: Kind::Color,
        inherited: false,
        initial: c"rgb(0, 0, 0)",
        grammar: "currentcolor | <color>",
    },
    Def {
        prop: Prop::FloodOpacity,
        kind: Kind::Number,
        inherited: false,
        initial: c"1",
        grammar: "<number>",
    },
    Def {
        prop: Prop::LightingColor,
        kind: Kind::Color,
        inherited: false,
        initial: c"rgb(255, 255, 255)",
        grammar: "currentcolor | <color>",
    },
    Def {
        prop: Prop::MarkerStart,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"none",
        grammar: "none | <url>",
    },
    Def {
        prop: Prop::MarkerMid,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"none",
        grammar: "none | <url>",
    },
    Def {
        prop: Prop::MarkerEnd,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"none",
        grammar: "none | <url>",
    },
    Def {
        prop: Prop::BaselineSource,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"auto",
        grammar: "auto | first | last",
    },
    Def {
        prop: Prop::InitialLetter,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"normal",
        grammar: "normal | ![<number [1,inf]>] [<integer [1,inf]> drop raise]",
    },
    Def {
        prop: Prop::MarginTrim,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"none",
        grammar: "none | block | [block-start] [block-end]",
    },
    Def {
        prop: Prop::FieldSizing,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"fixed",
        grammar: "fixed | content",
    },
    Def {
        prop: Prop::InterpolateSize,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"numeric-only",
        grammar: "numeric-only | allow-keywords",
    },
    Def {
        prop: Prop::ViewTransitionName,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"none",
        grammar: "none | auto | match-element | <custom-ident>",
    },
    Def {
        prop: Prop::ViewTransitionClass,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"none",
        grammar: "none | <custom-ident>+",
    },
    Def {
        prop: Prop::WebkitFontSmoothing,
        kind: Kind::Keywords,
        inherited: true,
        initial: c"auto",
        grammar: "auto | none | antialiased | subpixel-antialiased",
    },
    Def {
        prop: Prop::WebkitTapHighlightColor,
        kind: Kind::Color,
        inherited: true,
        initial: c"rgba(0, 0, 0, 0.18)",
        grammar: "currentcolor | <color>",
    },
    Def {
        prop: Prop::WebkitTextStrokeWidth,
        kind: Kind::Length,
        inherited: true,
        initial: c"0px",
        grammar: "thin | medium | thick | <length [0,inf]>",
    },
    Def {
        prop: Prop::WebkitTextStrokeColor,
        kind: Kind::Color,
        inherited: true,
        initial: c"currentcolor",
        grammar: "currentcolor | <color>",
    },
    Def {
        prop: Prop::OffsetPath,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"none",
        grammar: "none",
    },
    Def {
        prop: Prop::OffsetDistance,
        kind: Kind::Length,
        inherited: false,
        initial: c"0px",
        grammar: "<length-percentage>",
    },
    Def {
        prop: Prop::OffsetRotate,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"auto",
        grammar: "auto",
    },
    Def {
        prop: Prop::OffsetAnchor,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"auto",
        grammar: "auto",
    },
    Def {
        prop: Prop::OffsetPosition,
        kind: Kind::Keywords,
        inherited: false,
        initial: c"normal",
        grammar: "normal",
    },
];

pub(crate) fn of(prop: Prop) -> Option<&'static Def> {
    static INDEX: OnceLock<Vec<Option<usize>>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        let mut index = vec![None; Prop::ALL.len()];
        for (i, def) in DEFS.iter().enumerate() {
            index[def.prop.id()] = Some(i);
        }
        index
    });
    index.get(prop.id()).copied().flatten().map(|i| &DEFS[i])
}

pub(crate) fn initial(name: &[u8]) -> Option<&'static CStr> {
    Prop::from_name(name).and_then(of).map(|def| def.initial)
}

pub(crate) fn keywords_only(prop: Prop) -> bool {
    of(prop).is_some_and(|def| def.kind == Kind::Keywords)
}

fn number_of(text: &[u8]) -> Option<f64> {
    match calc_value(text)? {
        Parsed::Length(v, NUMBER) => Some(v),
        _ => None,
    }
}

pub(crate) fn grammar_canonical(grammar: &str, text: &[u8]) -> Option<String> {
    southstar_css_grammar::canonical(grammar, text, &number_of)
}

fn keyword_is(t: &[u8], word: &str) -> bool {
    strip(t).eq_ignore_ascii_case(word.as_bytes())
}

fn parsed_length(t: &[u8]) -> Option<Parsed> {
    calc_value(t).or_else(|| length(t).map(|(v, unit)| Parsed::Length(v, unit)))
}

fn length_value(def: &Def, t: &[u8]) -> Option<Value> {
    if let Some(kw) = grammar_canonical(def.grammar, t) {
        let px = match kw.as_str() {
            "thin" => 1.0,
            "medium" => 3.0,
            "thick" => 5.0,
            _ => return Some(Value::keyword(kw.into_bytes())),
        };
        return Some(Value::length(px, PX));
    }
    let allow_percent = def.grammar.contains("<length-percentage");
    let nonnegative = def.grammar.contains("[0,");
    match parsed_length(t)? {
        Parsed::Length(v, unit) => {
            let unit = if unit == NUMBER {
                if v != 0.0 {
                    return None;
                }
                PX
            } else {
                unit
            };
            if (unit == PERCENT && !allow_percent) || (nonnegative && v < 0.0) {
                return None;
            }
            Some(Value::length(v, unit))
        }
        Parsed::Calc(calc) => {
            (allow_percent || calc.pct == 0.0).then(|| Value::of(Body::Calc(calc)))
        }
    }
}

fn number_value(def: &Def, t: &[u8]) -> Option<Value> {
    if keyword_is(t, "normal") && southstar_css_grammar::has_keyword(def.grammar, "normal") {
        return Some(Value::length(1.0, NUMBER));
    }
    let mut num = match parsed_length(t)? {
        Parsed::Length(v, NUMBER) => v,
        Parsed::Length(v, PERCENT) => v / 100.0,
        Parsed::Calc(c)
            if c.func == 0
                && c.px == 0.0
                && c.em == 0.0
                && c.rem == 0.0
                && c.vw == 0.0
                && c.vh == 0.0
                && c.vmin == 0.0
                && c.vmax == 0.0 =>
        {
            c.pct / 100.0
        }
        _ => return None,
    };
    if def.grammar.contains("[0,") && num < 0.0 {
        return None;
    }
    if def.prop == Prop::Zoom && num == 0.0 {
        num = 1.0;
    }
    if def.prop == Prop::FloodOpacity {
        num = num.clamp(0.0, 1.0);
    }
    Some(Value::length(num, NUMBER))
}

fn color_scheme(t: &[u8]) -> Option<String> {
    if keyword_is(t, "normal") {
        return Some("normal".to_owned());
    }
    let names = grammar_canonical("<custom-ident>+", t)?;
    let words: Vec<&str> = names.split(' ').collect();
    let mut out: Vec<String> = Vec::new();
    let mut only = false;
    for (i, word) in words.iter().enumerate() {
        let lower = word.to_ascii_lowercase();
        match lower.as_str() {
            "only" => {
                if only || (i != 0 && i != words.len() - 1) || words.len() < 2 {
                    return None;
                }
                only = true;
            }
            "normal" => return None,
            "light" | "dark" => out.push(lower),
            _ => out.push((*word).to_owned()),
        }
    }
    if out.is_empty() {
        return None;
    }
    if only {
        out.push("only".to_owned());
    }
    Some(out.join(" "))
}

fn alternates_function(name: &str, args: &[u8], list: bool) -> Option<String> {
    let mut idents = Vec::new();
    for item in args.split(|&c| c == b',') {
        idents.push(grammar_canonical("<custom-ident>", item)?);
    }
    if idents.is_empty() || (!list && idents.len() > 1) {
        return None;
    }
    Some(format!("{name}({})", idents.join(", ")))
}

fn font_variant_alternates(t: &[u8]) -> Option<String> {
    const PARTS: [(&str, bool, bool); 7] = [
        ("stylistic", true, false),
        ("historical-forms", false, false),
        ("styleset", true, true),
        ("character-variant", true, true),
        ("swash", true, false),
        ("ornaments", true, false),
        ("annotation", true, false),
    ];
    if keyword_is(t, "normal") {
        return Some("normal".to_owned());
    }
    let tokens = split_ws_paren(t, 16);
    if tokens.is_empty() || tokens.len() >= 16 {
        return None;
    }
    let mut slots: [Option<String>; 7] = Default::default();
    for token in tokens {
        let open = token.iter().position(|&c| c == b'(');
        let name = &token[..open.unwrap_or(token.len())];
        let k = PARTS
            .iter()
            .position(|(part, _, _)| part.as_bytes().eq_ignore_ascii_case(name))?;
        let (part, function, list) = PARTS[k];
        if slots[k].is_some() || open.is_some() != function {
            return None;
        }
        slots[k] = Some(match open {
            None => part.to_owned(),
            Some(open) => {
                let args = token[open + 1..].strip_suffix(b")")?;
                alternates_function(part, args, list)?
            }
        });
    }
    Some(slots.into_iter().flatten().collect::<Vec<_>>().join(" "))
}

fn hyphenate_limit_chars_trim(canon: &str) -> String {
    let parts: Vec<&str> = canon.split(' ').collect();
    let mut n = parts.len();
    if n == 3 && parts[2] == parts[1] {
        n = 2;
    }
    if n == 2 && parts[1] == "auto" {
        n = 1;
    }
    parts[..n].join(" ")
}

fn keyword_canonical(def: &Def, t: &[u8]) -> Option<String> {
    match def.prop {
        Prop::ColorScheme => return color_scheme(t),
        Prop::FontVariantAlternates => return font_variant_alternates(t),
        _ => {}
    }
    let canon = grammar_canonical(def.grammar, t)?;
    Some(match def.prop {
        Prop::HyphenateLimitChars => hyphenate_limit_chars_trim(&canon),
        Prop::MarginTrim if canon == "block-start block-end" => "block".to_owned(),
        Prop::TextCombineUpright if canon == "digits" => "digits 2".to_owned(),
        _ => canon,
    })
}

pub(crate) fn keywords_specified(def: &Def, t: &[u8]) -> Option<Vec<u8>> {
    (def.kind == Kind::Keywords)
        .then(|| keyword_canonical(def, t))
        .flatten()
        .map(String::into_bytes)
}

pub(crate) fn parse(def: &Def, t: &[u8]) -> Option<Value> {
    match def.kind {
        Kind::Color => color_prop(def.prop, t),
        Kind::Length => length_value(def, t),
        Kind::Number => number_value(def, t),
        Kind::Keywords => keyword_canonical(def, t).map(|kw| Value::keyword(kw.into_bytes())),
    }
}
