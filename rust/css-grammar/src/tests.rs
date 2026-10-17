//! Southstar — unit tests for the CSS value grammar matcher.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{canonical, has_keyword};

fn no_math(_: &[u8]) -> Option<f64> {
    None
}

fn canon(grammar: &str, text: &str) -> Option<String> {
    canonical(grammar, text.as_bytes(), &no_math)
}

const TOUCH_ACTION: &str =
    "auto | none | manipulation | [pan-x pan-left pan-right] [pan-y pan-up pan-down] [pinch-zoom]";

#[test]
fn keyword_alternatives_match_one_token_case_insensitively() {
    assert_eq!(canon(TOUCH_ACTION, "none").as_deref(), Some("none"));
    assert_eq!(
        canon(TOUCH_ACTION, "MANIPULATION").as_deref(),
        Some("manipulation")
    );
    assert_eq!(canon(TOUCH_ACTION, "none auto"), None);
    assert_eq!(canon(TOUCH_ACTION, "sideways"), None);
    assert_eq!(canon(TOUCH_ACTION, ""), None);
}

#[test]
fn any_order_groups_serialize_in_grammar_order() {
    assert_eq!(
        canon(TOUCH_ACTION, "PAN-Y pan-x").as_deref(),
        Some("pan-x pan-y")
    );
    assert_eq!(
        canon(TOUCH_ACTION, "pinch-zoom pan-up pan-left").as_deref(),
        Some("pan-left pan-up pinch-zoom")
    );
    assert_eq!(canon(TOUCH_ACTION, "pan-x pan-left"), None);
    assert_eq!(
        canon("auto | [from-font under] [left right]", "right under").as_deref(),
        Some("under right")
    );
}

#[test]
fn required_groups_and_omitted_defaults() {
    let grammar = "auto | ![over under] [right* left]";
    assert_eq!(canon(grammar, "over").as_deref(), Some("over"));
    assert_eq!(canon(grammar, "over right").as_deref(), Some("over"));
    assert_eq!(canon(grammar, "left under").as_deref(), Some("under left"));
    assert_eq!(canon(grammar, "right"), None);
    assert_eq!(
        canon("none | all | [start] [end]", "end start").as_deref(),
        Some("start end")
    );
}

#[test]
fn repeated_groups_respect_their_bounds() {
    let grammar = "[auto <integer [1,inf]>]{1,3}";
    assert_eq!(canon(grammar, "auto 2 3").as_deref(), Some("auto 2 3"));
    assert_eq!(canon(grammar, "5").as_deref(), Some("5"));
    assert_eq!(canon(grammar, "0"), None);
    assert_eq!(canon(grammar, "1 2 3 4"), None);
    assert_eq!(canon(grammar, "2.5"), None);
}

#[test]
fn typed_terms_check_ranges() {
    let grammar = "none | all | ![digits] [<integer [2,4]>]";
    assert_eq!(canon(grammar, "digits 3").as_deref(), Some("digits 3"));
    assert_eq!(canon(grammar, "digits").as_deref(), Some("digits"));
    assert_eq!(canon(grammar, "digits 5"), None);
    assert_eq!(canon(grammar, "3"), None);
    assert_eq!(canon("<number [0,inf]>", "2").as_deref(), Some("2"));
    assert_eq!(canon("<number [0,inf]>", "-1"), None);
    assert_eq!(
        canon("auto | none | <percentage [0,inf]>", "50%").as_deref(),
        Some("50%")
    );
    assert_eq!(canon("<angle>", "0.5turn").as_deref(), Some("180deg"));
    assert_eq!(canon("<angle>", "0").as_deref(), Some("0deg"));
}

#[test]
fn math_functions_go_through_the_number_callback() {
    let two = |text: &[u8]| (text == b"calc(1 + 1)").then_some(2.0);
    assert_eq!(
        canonical("<integer [1,inf]>", b"calc(1 + 1)", &two).as_deref(),
        Some("2")
    );
    assert_eq!(canonical("<integer [1,inf]>", b"calc(0)", &two), None);
}

#[test]
fn custom_idents_exclude_css_wide_and_grammar_keywords() {
    let grammar = "none | <custom-ident>+";
    assert_eq!(canon(grammar, "card hero").as_deref(), Some("card hero"));
    assert_eq!(canon(grammar, "none").as_deref(), Some("none"));
    assert_eq!(canon(grammar, "card none"), None);
    assert_eq!(canon(grammar, "inherit"), None);
    assert_eq!(
        canon("normal | light | dark | <dashed-ident>", "--brand").as_deref(),
        Some("--brand")
    );
    assert_eq!(
        canon("normal | light | dark | <dashed-ident>", "brand"),
        None
    );
}

#[test]
fn strings_and_urls_are_quoted_canonically() {
    assert_eq!(canon("auto | <string>", "'-'").as_deref(), Some("\"-\""));
    assert_eq!(
        canon("auto | <string>", "'\\2010'").as_deref(),
        Some("\"\u{2010}\"")
    );
    assert_eq!(
        canon("auto | <string>", "'say \"hi\"'").as_deref(),
        Some("\"say \\\"hi\\\"\"")
    );
    assert_eq!(
        canon("none | <url>", "url(markers.svg#dot)").as_deref(),
        Some("url(\"markers.svg#dot\")")
    );
    assert_eq!(
        canon("none | <url>", "url( 'm.svg' )").as_deref(),
        Some("url(\"m.svg\")")
    );
}

#[test]
fn comma_lists_match_each_item() {
    let grammar = "normal | multiply | screen #";
    assert_eq!(
        canon(grammar, "multiply,Screen").as_deref(),
        Some("multiply, screen")
    );
    assert_eq!(canon(grammar, "multiply,"), None);
    assert_eq!(canon(grammar, "multiply, overlay"), None);
}

#[test]
fn keywords_are_found_as_whole_terms() {
    assert!(has_keyword(TOUCH_ACTION, "pan-x"));
    assert!(has_keyword(TOUCH_ACTION, "auto"));
    assert!(has_keyword("auto | ![over under] [right* left]", "right"));
    assert!(!has_keyword(TOUCH_ACTION, "pan"));
    assert!(!has_keyword(TOUCH_ACTION, ""));
}
