//! Southstar — unit tests for long var() chains between custom properties, declared in either order.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::custom_props::{self, Own, VarMatch};

fn chain(links: usize, reversed: bool) -> Vec<(String, String)> {
    let mut decls: Vec<(String, String)> = (1..=links)
        .map(|i| (format!("--a{i}"), format!("var(--a{})", i - 1)))
        .collect();
    decls.insert(0, ("--a0".to_owned(), "end".to_owned()));
    if reversed {
        decls.reverse();
    }
    decls
}

fn cascade(decls: &[(String, String)]) -> Own {
    let mut matches: Vec<VarMatch<'_>> = decls
        .iter()
        .enumerate()
        .map(|(i, (name, text))| VarMatch {
            origin: 2,
            specificity: (0, 1, 0),
            sheet_index: 1,
            layer_order: i32::MAX,
            scope_order: 0,
            source_order: i as i32,
            decl_order: i as i32,
            important: false,
            inline_style: false,
            rule: 1,
            name: name.as_bytes(),
            text: text.as_bytes(),
        })
        .collect();
    custom_props::sort(&mut matches);
    custom_props::unregistered(&matches, None, None)
}

fn value(own: &Own, name: &str) -> String {
    String::from_utf8_lossy(&own[name.as_bytes()]).into_owned()
}

#[test]
fn short_chains_resolve_in_either_order() {
    for reversed in [false, true] {
        let own = cascade(&chain(200, reversed));
        assert_eq!(value(&own, "--a200"), "end");
        assert_eq!(value(&own, "--a1"), "end");
    }
}

#[test]
fn long_forward_chain_resolves() {
    let own = cascade(&chain(20_000, false));
    assert_eq!(value(&own, "--a20000"), "end");
}

#[test]
fn long_reversed_chain_does_not_overflow_the_stack() {
    let own = cascade(&chain(20_000, true));
    assert_eq!(value(&own, "--a20000"), "initial");
    assert_eq!(value(&own, "--a1"), "end");
}
