//! Southstar — unit tests for the table-defined properties and the keyword shorthands.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::prop::Prop;
use crate::property;
use crate::shorthand;
use crate::values;

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn longhands(name: &str, value: &str) -> Vec<(String, String)> {
    let decls = shorthand::expand(name.as_bytes(), value.as_bytes());
    (0..decls.len())
        .map(|i| {
            let value = values::serialize(shorthand::slot_value(&decls, i), false);
            (text(decls[i].prop.name()), text(&value))
        })
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|&(p, v)| (p.to_owned(), v.to_owned()))
        .collect()
}

fn named(prop: &[u8]) -> Option<String> {
    Prop::from_name(prop).map(|p| text(p.name()))
}

fn specified(prop: &str, value: &str) -> Option<String> {
    values::specified_canonical(Some(prop.as_bytes()), value.as_bytes())
        .map(|v| String::from_utf8_lossy(&v).into_owned())
}

#[test]
fn white_space_expands_into_collapse_and_wrap_mode() {
    assert_eq!(
        longhands("white-space", "pre"),
        pairs(&[
            ("white-space-collapse", "preserve"),
            ("text-wrap-mode", "nowrap")
        ])
    );
    assert_eq!(
        longhands("white-space", "NOWRAP preserve-breaks"),
        pairs(&[
            ("white-space-collapse", "preserve-breaks"),
            ("text-wrap-mode", "nowrap")
        ])
    );
    assert_eq!(
        longhands("white-space", "preserve discard-before"),
        pairs(&[
            ("white-space-collapse", "preserve"),
            ("text-wrap-mode", "wrap")
        ])
    );
    assert!(longhands("white-space", "pre wrap").is_empty());
    assert!(longhands("white-space", "nowrap nowrap").is_empty());
}

#[test]
fn text_wrap_no_longer_touches_space_collapsing() {
    assert_eq!(
        longhands("text-wrap", "balance"),
        pairs(&[("text-wrap-mode", "wrap"), ("text-wrap-style", "balance")])
    );
    assert_eq!(
        longhands("text-wrap", "pretty nowrap"),
        pairs(&[("text-wrap-mode", "nowrap"), ("text-wrap-style", "pretty")])
    );
    assert_eq!(
        longhands("text-wrap-mode", "nowrap"),
        pairs(&[("text-wrap-mode", "nowrap")])
    );
}

#[test]
fn keyword_shorthands_share_css_wide_keywords() {
    assert_eq!(
        longhands("text-emphasis", "inherit"),
        pairs(&[
            ("text-emphasis-style", "inherit"),
            ("text-emphasis-color", "inherit")
        ])
    );
    assert!(longhands("text-emphasis", "dot inherit").is_empty());
}

#[test]
fn text_emphasis_and_text_stroke_split_style_from_color() {
    let emphasis = longhands("text-emphasis", "open dot");
    assert_eq!(
        emphasis[0],
        pairs(&[("text-emphasis-style", "open dot")])[0]
    );
    assert_eq!(emphasis[1].0, "text-emphasis-color");
    assert_eq!(longhands("text-emphasis", "dot open red").len(), 2);
    let stroke = longhands("-webkit-text-stroke", "2px");
    assert_eq!(stroke[0], pairs(&[("-webkit-text-stroke-width", "2px")])[0]);
    assert_eq!(stroke[1].0, "-webkit-text-stroke-color");
    assert!(longhands("-webkit-text-stroke", "2px 3px").is_empty());
}

#[test]
fn font_synthesis_sets_all_four_longhands() {
    assert_eq!(
        longhands("font-synthesis", "position style"),
        pairs(&[
            ("font-synthesis-weight", "none"),
            ("font-synthesis-style", "auto"),
            ("font-synthesis-small-caps", "none"),
            ("font-synthesis-position", "auto")
        ])
    );
    assert_eq!(
        longhands("font-synthesis", "oblique-only")[1].1,
        "oblique-only"
    );
    assert!(longhands("font-synthesis", "style oblique-only").is_empty());
    assert!(longhands("font-synthesis", "none weight").is_empty());
}

#[test]
fn keyword_shorthands_serialize_their_shortest_form() {
    assert_eq!(
        specified("white-space", "preserve-breaks wrap").as_deref(),
        Some("pre-line")
    );
    assert_eq!(
        specified("white-space", "preserve-spaces").as_deref(),
        Some("preserve-spaces")
    );
    assert_eq!(
        specified("white-space", "collapse nowrap").as_deref(),
        Some("nowrap")
    );
    assert_eq!(
        specified("text-wrap", "wrap balance").as_deref(),
        Some("balance")
    );
    assert_eq!(specified("text-wrap", "nowrap").as_deref(), Some("nowrap"));
    assert_eq!(
        specified("font-synthesis", "position weight").as_deref(),
        Some("weight position")
    );
    assert_eq!(specified("font-synthesis", "none").as_deref(), Some("none"));
}

#[test]
fn table_properties_parse_through_their_grammar() {
    let keyword = |name: &str, value: &str| {
        let prop = Prop::from_name(name.as_bytes())?;
        property::parse(prop, value.as_bytes()).map(|v| text(&values::serialize(&v, false)))
    };
    assert_eq!(
        keyword("touch-action", "pan-y pan-x").as_deref(),
        Some("pan-x pan-y")
    );
    assert_eq!(keyword("touch-action", "none auto"), None);
    assert_eq!(
        keyword("text-emphasis-position", "left under").as_deref(),
        Some("under left")
    );
    assert_eq!(keyword("resize", "both").as_deref(), Some("both"));
    assert_eq!(
        keyword("text-underline-offset", "-1px"),
        Some("-1px".to_owned())
    );
    assert_eq!(keyword("flood-opacity", "50%").as_deref(), Some("0.5"));
    assert_eq!(
        specified("text-emphasis-position", "LEFT over").as_deref(),
        Some("over left")
    );
}

#[test]
fn table_properties_carry_their_inheritance() {
    assert!(Prop::WhiteSpaceCollapse.inherits());
    assert!(Prop::TextEmphasisStyle.inherits());
    assert!(!Prop::TouchAction.inherits());
    assert!(!Prop::Zoom.inherits());
    assert_eq!(named(b"text-wrap"), None);
    assert_eq!(named(b"text-wrap-mode").as_deref(), Some("text-wrap-mode"));
    assert_eq!(named(b"overflow-block").as_deref(), Some("overflow-y"));
}

fn composed(name: &str, longhands: &[(&str, &str)]) -> Option<String> {
    crate::gaps::shorthand_compose(name.as_bytes(), &mut |prop| {
        longhands
            .iter()
            .find(|(p, _)| p.as_bytes() == prop)
            .map(|(_, v)| v.as_bytes().to_vec())
    })
    .flatten()
    .map(|v| text(&v))
}

#[test]
fn gap_rule_lists_keep_repeat_and_canonical_items() {
    assert_eq!(
        specified("column-rule-width", "0, repeat(calc(5 + 3), THIN, 2PX)").as_deref(),
        Some("0px, repeat(8, thin, 2px)")
    );
    assert_eq!(
        specified("row-rule-style", "repeat(auto, dotted), ridge").as_deref(),
        Some("repeat(auto, dotted), ridge")
    );
    assert_eq!(specified("column-rule-width", "30%"), None);
    assert_eq!(specified("column-rule-width", "repeat(2, -20px)"), None);
    assert_eq!(
        specified("row-rule-color", "repeat(auto, red), repeat(auto, blue)"),
        None
    );
    assert_eq!(specified("column-rule-style", "repeat(0, solid)"), None);
}

#[test]
fn gap_rule_shorthand_fills_each_longhand_list() {
    assert_eq!(
        longhands(
            "column-rule",
            "repeat(3, lime 16px, dashed purple, 10px dotted)"
        ),
        pairs(&[
            ("column-rule-width", "repeat(3, 16px, medium, 10px)"),
            ("column-rule-style", "repeat(3, none, dashed, dotted)"),
            ("column-rule-color", "repeat(3, lime, purple, currentcolor)"),
        ])
    );
    assert_eq!(longhands("rule", "red 5px solid red"), pairs(&[]));
    assert_eq!(
        longhands("rule-break", "intersection"),
        pairs(&[
            ("column-rule-break", "intersection"),
            ("row-rule-break", "intersection"),
        ])
    );
}

#[test]
fn gap_rule_shorthand_serializes_only_matching_shapes() {
    let rule = [
        ("column-rule-width", "repeat(auto, 5px)"),
        ("column-rule-style", "repeat(auto, solid)"),
        ("column-rule-color", "repeat(auto, red)"),
    ];
    assert_eq!(
        composed("column-rule", &rule).as_deref(),
        Some("repeat(auto, 5px solid red)")
    );
    let defaults = [
        ("column-rule-width", "medium"),
        ("column-rule-style", "none"),
        ("column-rule-color", "currentcolor"),
    ];
    assert_eq!(
        composed("column-rule", &defaults).as_deref(),
        Some("medium")
    );
    let mismatched = [
        ("column-rule-width", "15px, 25px"),
        ("column-rule-style", "solid"),
        ("column-rule-color", "green"),
    ];
    assert_eq!(composed("column-rule", &mismatched).as_deref(), Some(""));
}

#[test]
fn gap_rule_inset_shorthands_expand_and_collapse() {
    assert_eq!(
        longhands("column-rule-inset", "10px / 20px 30px"),
        pairs(&[
            ("column-rule-inset-cap-start", "10px"),
            ("column-rule-inset-cap-end", "10px"),
            ("column-rule-inset-junction-start", "20px"),
            ("column-rule-inset-junction-end", "30px"),
        ])
    );
    assert_eq!(
        longhands("row-rule-inset", "overlap-join 10px 20px"),
        pairs(&[])
    );
    let insets = [
        ("column-rule-inset-cap-start", "10px"),
        ("column-rule-inset-cap-end", "20px"),
        ("column-rule-inset-junction-start", "10px"),
        ("column-rule-inset-junction-end", "20px"),
    ];
    assert_eq!(
        composed("column-rule-inset", &insets).as_deref(),
        Some("10px 20px / 10px 20px")
    );
    assert_eq!(
        composed("column-rule-inset-start", &insets).as_deref(),
        Some("10px")
    );
    assert_eq!(
        composed("column-rule-inset-cap", &insets).as_deref(),
        Some("10px 20px")
    );
}

#[test]
fn gradient_stops_keep_non_legacy_colour_spaces() {
    let computed = |text: &str| {
        property::parse(Prop::BackgroundImage, text.as_bytes())
            .map(|v| String::from_utf8_lossy(&values::serialize(&v, false)).into_owned())
    };
    assert_eq!(
        computed("linear-gradient(in lab, color(srgb 1 0 0), blue)").as_deref(),
        Some("linear-gradient(in lab, color(srgb 1 0 0), rgb(0, 0, 255))")
    );
    assert_eq!(
        computed("radial-gradient(red, oklch(0.5 0.1 none) 20% 40%)").as_deref(),
        Some("radial-gradient(rgb(255, 0, 0), oklch(0.5 0.1 none) 20% 40%)")
    );
}

fn parsed(prop: Prop, text: &str) -> Option<String> {
    property::parse(prop, text.as_bytes())
        .map(|v| String::from_utf8_lossy(&values::serialize(&v, false)).into_owned())
}

#[test]
fn svg_opacities_and_miterlimit_take_numbers() {
    assert_eq!(parsed(Prop::FillOpacity, "0.123").as_deref(), Some("0.123"));
    assert_eq!(parsed(Prop::StrokeOpacity, "50%").as_deref(), Some("0.5"));
    assert_eq!(parsed(Prop::StopOpacity, "2").as_deref(), Some("2"));
    assert_eq!(parsed(Prop::FillOpacity, "1px"), None);
    assert_eq!(
        parsed(Prop::StrokeMiterlimit, "123").as_deref(),
        Some("123")
    );
    assert_eq!(parsed(Prop::StrokeMiterlimit, "-1"), None);
    assert_eq!(parsed(Prop::StrokeMiterlimit, "4px"), None);
}

#[test]
fn math_and_text_justify_keywords() {
    assert_eq!(parsed(Prop::MathDepth, "123").as_deref(), Some("123"));
    assert_eq!(
        parsed(Prop::MathDepth, "add(-2)").as_deref(),
        Some("add(-2)")
    );
    assert_eq!(
        parsed(Prop::MathDepth, "AUTO-ADD").as_deref(),
        Some("auto-add")
    );
    assert_eq!(parsed(Prop::MathDepth, "1.5"), None);
    assert_eq!(
        parsed(Prop::MathStyle, "compact").as_deref(),
        Some("compact")
    );
    assert_eq!(
        parsed(Prop::TextJustify, "distribute").as_deref(),
        Some("inter-character")
    );
    assert_eq!(parsed(Prop::TextJustify, "inter-ideograph"), None);
}
