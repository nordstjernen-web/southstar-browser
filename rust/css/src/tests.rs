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
