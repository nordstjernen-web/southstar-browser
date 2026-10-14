//! Southstar — unit tests for the CSS component-value parser and the statement scanners.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{Component, Kind, is_self_contained, parse, scan, value_valid};

fn kinds(components: &[Component]) -> Vec<Kind> {
    components.iter().filter(|c| c.kind != Kind::Whitespace).map(|c| c.kind).collect()
}

fn value<'a>(input: &'a [u8], component: &Component) -> &'a [u8] {
    &input[component.value.clone().expect("component has a value")]
}

#[test]
fn tokenizes_numbers_dimensions_and_percentages() {
    let input = b"12px -3.5e2 50% +.25 1e3em";
    let (components, valid) = parse(input);
    assert!(valid);
    let parts: Vec<&Component> = components.iter().filter(|c| c.kind != Kind::Whitespace).collect();
    assert_eq!(kinds(&components), [Kind::Dimension, Kind::Number, Kind::Percentage, Kind::Number, Kind::Dimension]);
    assert_eq!(parts[0].number, 12.0);
    assert_eq!(value(input, parts[0]), b"px");
    assert_eq!(parts[1].number, -350.0);
    assert_eq!(parts[2].number, 50.0);
    assert_eq!(parts[3].number, 0.25);
    assert_eq!(parts[4].number, 1000.0);
    assert_eq!(value(input, parts[4]), b"em");
}

#[test]
fn nests_functions_and_blocks() {
    let input = b"calc(1px + var(--x, [a] {b}))";
    let (components, valid) = parse(input);
    assert!(valid);
    assert_eq!(kinds(&components), [Kind::Function]);
    assert_eq!(value(input, &components[0]), b"calc");
    let inner = components[0].children.as_ref().expect("function arguments");
    let var = inner.iter().find(|c| c.kind == Kind::Function).expect("nested var()");
    assert_eq!(value(input, var), b"var");
    let blocks = var.children.as_ref().expect("var arguments").iter().filter(|c| c.kind == Kind::Block).count();
    assert_eq!(blocks, 2);
}

#[test]
fn reads_strings_hashes_and_at_keywords() {
    let input = br#"'a\'b' "x" #fff #1a @media url-ish"#;
    let (components, _) = parse(input);
    assert_eq!(
        kinds(&components),
        [Kind::String, Kind::String, Kind::Hash, Kind::Hash, Kind::AtKeyword, Kind::Ident]
    );
}

#[test]
fn flags_unbalanced_and_bad_input() {
    assert!(value_valid(b"rgb(1 2 3)"));
    assert!(value_valid(b"rgb(1 2 3"), "end of input closes open blocks");
    assert!(!value_valid(b"a ) b"));
    assert!(!value_valid(b"'unterminated\n"));
    let deep = "(".repeat(1000) + &")".repeat(1000);
    let _ = parse(deep.as_bytes());
}

#[test]
fn scans_to_a_top_level_terminator() {
    assert_eq!(scan(b"color: red; x", b";"), (10, b';'));
    assert_eq!(scan(b"a: f(;) ';' /*;*/ b; c", b";"), (19, b';'));
    assert_eq!(scan(b"no terminator", b";"), (13, 0));
}

#[test]
fn recognizes_self_contained_sheets() {
    assert!(is_self_contained(b"a { color: red } @media print { b { x: y } }"));
    assert!(!is_self_contained(b"a { color: red"));
    assert!(!is_self_contained(b"a { b: '}' "));
}
