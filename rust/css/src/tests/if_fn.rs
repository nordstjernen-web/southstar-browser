//! Southstar — unit tests for if(): branch choice, style() queries against custom properties, cycles and invalid syntax.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::borrow::Cow;
use std::collections::HashMap;

use southstar_css_prop_syntax::{self as prop_syntax, Context, SyntaxDef};

use crate::custom_props::{self, Inherited, Own, Registered, Registry, VarMatch};
use crate::vars::{self, Lookup};

type Case = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
);

struct Parent(HashMap<Vec<u8>, Vec<u8>>);

impl Inherited for Parent {
    fn lookup(&self, name: &[u8]) -> Option<&[u8]> {
        self.0.get(name).map(Vec::as_slice)
    }
}

struct Rule {
    name: &'static str,
    syntax: SyntaxDef,
    inherits: bool,
    initial: &'static str,
}

struct Rules(Vec<Rule>);

impl Rules {
    fn rule(&self, name: &[u8]) -> Option<&Rule> {
        self.0.iter().find(|r| r.name.as_bytes() == name)
    }
}

fn registered(rule: &Rule) -> Registered<'_> {
    Registered {
        inherits: rule.inherits,
        initial: Some(rule.initial.as_bytes()),
        syntax: Some(&rule.syntax),
    }
}

impl Registry for Rules {
    fn get(&self, name: &[u8]) -> Option<Registered<'_>> {
        self.rule(name).map(registered)
    }

    fn each(&self, f: &mut dyn FnMut(&[u8], Registered<'_>)) {
        for rule in &self.0 {
            f(rule.name.as_bytes(), registered(rule));
        }
    }

    fn rejects(&self, name: &[u8], value: &[u8]) -> bool {
        self.rule(name)
            .is_some_and(|rule| !prop_syntax::matches(&rule.syntax, value))
    }

    fn compute(&self, syntax: &SyntaxDef, value: &[u8]) -> Option<Vec<u8>> {
        let ctx = Context {
            font_size: 30.0,
            root_font_size: 16.0,
            ..Context::default()
        };
        prop_syntax::compute(syntax, value, Some(&ctx))
    }
}

fn rules() -> Rules {
    let rule = |name, syntax: &str, inherits, initial| Rule {
        name,
        syntax: prop_syntax::parse(syntax.as_bytes()).expect("syntax"),
        inherits,
        initial,
    };
    Rules(vec![
        rule("--string", "<string>", true, "\"\""),
        rule("--color", "<color>", true, "blue"),
        rule("--length", "<length>", false, "3px"),
        rule("--length-inherited", "<length>", true, "3px"),
        rule("--percentage", "<percentage>", true, "30%"),
        rule("--number", "<number>", true, "3"),
        rule("--angle", "<angle>", true, "3deg"),
        rule("--time", "<time>", true, "3s"),
        rule("--resolution", "<resolution>", true, "3dpi"),
    ])
}

fn decl<'a>(name: &'a str, text: &'a str, order: i32, inline: bool) -> VarMatch<'a> {
    VarMatch {
        origin: 2,
        specificity: (0, if inline { 0 } else { 1 }, 0),
        sheet_index: 1,
        layer_order: i32::MAX,
        scope_order: 0,
        source_order: order,
        decl_order: order,
        important: false,
        inline_style: inline,
        rule: if inline { 1 } else { 2 },
        name: name.as_bytes(),
        text: text.as_bytes(),
    }
}

fn shown(own: &Own, parent: &Parent, name: &str) -> String {
    let value = own
        .get(name.as_bytes())
        .map(Vec::as_slice)
        .or_else(|| parent.lookup(name.as_bytes()))
        .unwrap_or(b"initial");
    if value.eq_ignore_ascii_case(b"initial") {
        String::new()
    } else {
        String::from_utf8_lossy(value).into_owned()
    }
}

fn outer() -> Parent {
    let pairs = [
        ("--inherited", "outer_value"),
        ("--number", "30"),
        ("--x", "11"),
        ("--length", "30px"),
        ("--length-inherited", "30px"),
        ("--string", "\"\""),
        ("--color", "blue"),
        ("--percentage", "30%"),
        ("--angle", "3deg"),
        ("--time", "3s"),
        ("--resolution", "3dpi"),
    ];
    Parent(
        pairs
            .iter()
            .map(|(n, v)| (n.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect(),
    )
}

fn conditional(value: &str, set: &[(&str, &str)]) -> String {
    let parent = outer();
    let mut matches = vec![decl("--inherited", "inner_value", 0, false)];
    for (i, (name, text)) in set.iter().enumerate() {
        if !text.is_empty() {
            matches.push(decl(name, text, i as i32 + 1, true));
        }
    }
    matches.push(decl("--property", value, 100, true));
    custom_props::sort(&mut matches);
    let registry = rules();
    let own = custom_props::registered(&matches, Some(&parent), &registry, None);
    shown(&own, &parent, "--property")
}

fn cyclic(value: &str, set: &[(&str, &str)]) -> String {
    let parent = Parent(HashMap::new());
    let mut matches: Vec<VarMatch<'_>> = set
        .iter()
        .enumerate()
        .map(|(i, (name, text))| decl(name, text, i as i32, true))
        .collect();
    matches.push(decl("--prop", value, 100, true));
    custom_props::sort(&mut matches);
    let own = custom_props::unregistered(&matches, Some(&parent), None);
    shown(&own, &parent, "--prop")
}

#[test]
fn style_queries_compare_unregistered_custom_properties() {
    let cases: &[Case] = &[
        (
            "if(style(--x: 3): true_value)",
            &[("--x", "3")],
            "true_value",
        ),
        (
            "if( style( --x : 3 ) : true_value )",
            &[("--x", "3")],
            "true_value ",
        ),
        ("if(style(--x): true_value;)", &[("--x", "3")], "true_value"),
        ("if(style(--x: 0): ;)", &[("--x", "0")], ""),
        (
            "if(style(--x: 3): true_value; else: false_value)",
            &[("--x", "calc(1 + 2)")],
            "false_value",
        ),
        (
            "if(style(--x: initial): true_value; else: false_value)",
            &[("--x", "initial")],
            "true_value",
        ),
        (
            "if(style(--x: initial): true_value; else: false_value)",
            &[],
            "false_value",
        ),
        (
            "if(style(--inherited: inherit): true_value; else: false_value)",
            &[("--inherited", "outer_value")],
            "true_value",
        ),
        (
            "if(style(--inherited: unset): true_value; else: false_value)",
            &[("--inherited", "inner_value")],
            "false_value",
        ),
        (
            "if(style(--x: revert): true_value; else: false_value)",
            &[("--x", "11")],
            "false_value",
        ),
        (
            "if(style(--x):a)if(style(--x):b)",
            &[("--x", "3")],
            "a/**/b",
        ),
        (
            "if(style(--x!): true_value; else: false_value)",
            &[("--x", "3")],
            "false_value",
        ),
        (
            "if(style(color: green): true_value; else: false_value)",
            &[],
            "false_value",
        ),
        (
            "if(style(--x: 0) or (style(--y: 3) and style(--z: 3)): true_value; else: false_value)",
            &[("--x", "3"), ("--y", "3"), ("--z", "3")],
            "true_value",
        ),
        (
            "if(style(--x: 1): value1; else: value2; style(--y: green): value3)",
            &[("--x", "3")],
            "value2",
        ),
        (
            "if(style((--x: 3) and ((not (--y: red)) or (--z: 10px))): true_value; else: false_value)",
            &[("--x", "3"), ("--y", "red"), ("--z", "10px")],
            "true_value",
        ),
        (
            "if(style(--x: var(--y)): true_value; else: false_value)",
            &[("--x", "1"), ("--y", "3")],
            "false_value",
        ),
        (
            "if(style(--x: 3): true_value; else: false_value)",
            &[("--x", "var(--y)"), ("--y", "3")],
            "true_value",
        ),
        (
            "if(var(--else): true_value; else: false_value)",
            &[("--else", "else")],
            "true_value",
        ),
    ];
    for (value, set, expected) in cases {
        assert_eq!(conditional(value, set), *expected, "{value}");
    }
}

#[test]
fn style_queries_compare_registered_custom_properties_by_computed_value() {
    let cases: &[Case] = &[
        (
            "if(style(--number: 3): true_value; else: false_value)",
            &[("--number", "calc(1 + 2)")],
            "true_value",
        ),
        (
            "if(style(--length: 1em): true_value; else: false_value)",
            &[("--length", "30px")],
            "true_value",
        ),
        (
            "if(style(--length: 3): true_value; else: false_value)",
            &[("--length", "3px")],
            "false_value",
        ),
        (
            "if(style(--length: inherit): true_value; else: false_value)",
            &[],
            "false_value",
        ),
        (
            "if(style(--length: unset): true_value; else: false_value)",
            &[("--length", "3px")],
            "true_value",
        ),
        (
            "if(style(--length-inherited: inherit): true_value; else: false_value)",
            &[],
            "true_value",
        ),
        (
            "if(style(--color: rgb(0, 128, 0)): true_value; else: false_value)",
            &[("--color", "green")],
            "true_value",
        ),
        (
            "if(style(--percentage: 90px): true_value; else: false_value)",
            &[("--percentage", "30%")],
            "false_value",
        ),
    ];
    for (value, set, expected) in cases {
        assert_eq!(conditional(value, set), *expected, "{value}");
    }
}

#[test]
fn style_range_queries_compare_numbers_of_one_type() {
    let cases: &[Case] = &[
        (
            "if(style(5 > 3): true_value; else: false_value)",
            &[],
            "true_value",
        ),
        (
            "if(style(0 = 0px): true_value; else: false_value)",
            &[],
            "true_value",
        ),
        (
            "if(style(0 = 0%): true_value; else: false_value)",
            &[],
            "false_value",
        ),
        (
            "if(style(1em > 1px): true_value; else: false_value)",
            &[],
            "true_value",
        ),
        (
            "if(style(3turn > 3deg): true_value; else: false_value)",
            &[],
            "true_value",
        ),
        (
            "if(style(3dppx > 96dpi): true_value; else: false_value)",
            &[],
            "true_value",
        ),
        (
            "if(style(3px > 3): true_value; else: false_value)",
            &[],
            "false_value",
        ),
        (
            "if(style(5 > 3 !): true_value; else: false_value)",
            &[],
            "false_value",
        ),
        (
            "if(style(10px <= 10px < 11px): true_value; else: false_value)",
            &[],
            "true_value",
        ),
        (
            "if(style(--x + 1 >= --y): true_value; else: false_value)",
            &[("--x", "5"), ("--y", "3")],
            "false_value",
        ),
        (
            "if(style(calc(var(--x) + 1) >= var(--y)): true_value; else: false_value)",
            &[("--x", "5"), ("--y", "3")],
            "true_value",
        ),
        (
            "if(style(--x >= --y > --z): true_value; else: false_value)",
            &[("--x", "3"), ("--y", "3"), ("--z", "1")],
            "true_value",
        ),
        (
            "if(style(--time <= 1000ms): true_value; else: false_value)",
            &[("--time", "1s")],
            "true_value",
        ),
    ];
    for (value, set, expected) in cases {
        assert_eq!(conditional(value, set), *expected, "{value}");
    }
}

#[test]
fn invalid_if_syntax_is_invalid_at_computed_value_time() {
    let cases: &[Case] = &[
        ("if()", &[], ""),
        ("if(style())", &[], ""),
        ("if(style(--x: 3) !)", &[("--x", "3")], ""),
        (
            "if(style(--x: 3) true_value; else: false_value)",
            &[("--x", "3")],
            "",
        ),
        (
            "if(style(--x: 3): true_value; else: false_value!)",
            &[("--x", "3")],
            "",
        ),
        (
            "if(!style(--x: 3): true_value; else: false_value)",
            &[("--x", "3")],
            "",
        ),
        (
            "if(style(--x) and invalid: true_value; else: false_value)",
            &[("--x", "3")],
            "false_value",
        ),
        ("if(style(--x: 0): true_value;)", &[("--x", "3")], ""),
        (
            "if(style(--invalid): var(--x))",
            &[("--x", "true_value")],
            "",
        ),
    ];
    for (value, set, expected) in cases {
        assert_eq!(conditional(value, set), *expected, "{value}");
    }
}

#[test]
fn cycles_through_if_make_the_custom_property_invalid() {
    let cases: &[Case] = &[
        (
            "if(style(--x: 3): var(--prop); else: value)",
            &[("--x", "3")],
            "",
        ),
        (
            "if(style(--x: 0): var(--prop); else: value)",
            &[("--x", "3")],
            "value",
        ),
        ("if(style(--prop): var(--prop); else: value)", &[], ""),
        (
            "if(style(--x: 3): true_value; else: false_value)",
            &[("--x", "var(--prop)")],
            "",
        ),
        (
            "if(style(not (--x: var(--y))): true_value; else: false_value)",
            &[("--x", "11"), ("--y", "var(--prop)")],
            "",
        ),
        (
            "if(style(--y): true_value; else: false_value)",
            &[("--y", "var(--x)"), ("--x", "var(--y)")],
            "false_value",
        ),
        (
            "if(style(not (--x: var(--y))): true_value; else: false_value)",
            &[("--x", "11"), ("--y", "var(--y)")],
            "true_value",
        ),
        (
            "if(style(--x: 0): value1; style(--prop): value2)",
            &[("--x", "0")],
            "value1",
        ),
        (
            "if(style(--x: var(--y)): value1; style(--z: 3): value2; else: value3;)",
            &[("--x", "var(--y)"), ("--y", "var(--y)"), ("--z", "3")],
            "value2",
        ),
        (
            "if(style(--x: 3): var(--x); else: value)",
            &[("--x", "3")],
            "3",
        ),
    ];
    for (value, set, expected) in cases {
        assert_eq!(cyclic(value, set), *expected, "{value}");
    }
}

struct Values(HashMap<&'static str, &'static str>);

impl Lookup for Values {
    fn value(&self, name: &[u8]) -> Option<Cow<'_, [u8]>> {
        let name = std::str::from_utf8(name).ok()?;
        self.0.get(name).map(|v| Cow::Borrowed(v.as_bytes()))
    }

    fn registered_initial(&self, _name: &[u8]) -> Option<Option<&[u8]>> {
        None
    }
}

fn substituted(text: &str) -> Option<String> {
    let values = Values(HashMap::from([("--c", "3"), ("--true", "true")]));
    vars::substitute(text.as_bytes(), Some(&values), 0)
        .map(|v| String::from_utf8_lossy(&v).into_owned())
}

#[test]
fn attr_in_a_condition_taints_the_value_if_chooses() {
    assert_eq!(
        substituted("image-set(if(style(--true): url(a.png);))").as_deref(),
        Some("image-set(url(a.png))")
    );
    assert_eq!(
        substituted(
            "image-set(if(style(--c: attr(data-x type(*))): url(a.png); else: url(b.png)))"
        ),
        None
    );
    assert_eq!(
        substituted("if(style(--c < attr(data-x type(*))): url(a.png); else: url(b.png))"),
        None
    );
    assert_eq!(
        substituted("if(style(--c: attr(data-x type(*))): 1px; else: 2px)").as_deref(),
        Some("2px")
    );
    assert_eq!(
        cyclic("if(style(--x: attr(data-x)): a; else: b)", &[("--x", "1")]),
        "if(style(--x: attr(data-x)): a; else: b)"
    );
}
