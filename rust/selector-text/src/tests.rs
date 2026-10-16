//! Southstar — unit tests for selector serialization and An+B parsing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{canonical_anb, serialize_selector_list};

fn ser(text: &str) -> String {
    serialize_selector_list(text).unwrap_or_else(|| panic!("{text} did not serialize"))
}

#[test]
fn combinators_and_lists() {
    assert_eq!(ser("div ol>li p"), "div ol > li p");
    assert_eq!(ser("  a+b   ~ c  "), "a + b ~ c");
    assert_eq!(ser("a,b ,  c"), "a, b, c");
    assert_eq!(ser(":is(div )"), ":is(div)");
    assert_eq!(ser(":is(,,,)"), ":is(, , , )");
    assert_eq!(ser(":host(:where(div ))"), ":host(:where(div))");
    assert_eq!(ser(":has(> .a,+ b)"), ":has(> .a, + b)");
    assert_eq!(ser(":is(::before:HOVER  )"), ":is(::before:HOVER)");
    assert_eq!(ser("p:before"), "p::before");
    assert_eq!(ser("a /* c */ > b"), "a > b");
    assert_eq!(ser(":has(:is(:has(.a+.b)))"), ":has(:is(:has(.a+.b)))");
    assert_eq!(
        ser("::part(foo):is([attr='value'])"),
        "::part(foo):is([attr='value'])"
    );
    assert_eq!(ser(":is(.a+.b, c>d)"), ":is(.a + .b, c > d)");
    assert_eq!(
        ser(":host(:is(.a, .b+.c, .d))"),
        ":host(:is(.a, .b+.c, .d))"
    );
}

#[test]
fn attributes() {
    assert_eq!(ser("[att=val]"), "[att=\"val\"]");
    assert_eq!(ser("span[class='example']"), "span[class=\"example\"]");
    assert_eq!(ser("a[hreflang|='en']"), "a[hreflang|=\"en\"]");
    assert_eq!(ser("[ att  *= val  I ]"), "[att*=\"val\" i]");
    assert_eq!(ser("[ns|att]"), "[ns|att]");
    assert_eq!(ser("[|att=v]"), "[att=\"v\"]");
    assert_eq!(ser("[a=\"x\\\"y\"]"), "[a=\"x\\\"y\"]");
}

#[test]
fn an_plus_b() {
    assert_eq!(ser(":nth-child( 2n + 1 )"), ":nth-child(2n+1)");
    assert_eq!(
        ser(":NTH-child(odd of .a,.b)"),
        ":nth-child(2n+1 of .a, .b)"
    );
    assert_eq!(canonical_anb("even").as_deref(), Some("2n"));
    assert_eq!(canonical_anb("-n- 3").as_deref(), Some("-n-3"));
    assert_eq!(canonical_anb("+5").as_deref(), Some("5"));
    for bad in [
        "n- 1 2", "n-b1", "n-+1", "n-1n", "-n+n", "+ 1n", "12 n", "+12n-0+1", "+ n + 7",
    ] {
        assert_eq!(canonical_anb(bad), None, "{bad}");
    }
}
