//! Southstar — unit tests for anchor positioning values: anchor() and anchor-size() with calc() simplification, position-area and the position-try lists.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::anchor::{self, AnchorFn, AnchorSizeFn, Context, Resolver};

fn inset(text: &str) -> Option<String> {
    anchor::canonical(text.as_bytes(), Context::Inset)
}

fn size(text: &str) -> Option<String> {
    anchor::canonical(text.as_bytes(), Context::Size)
}

#[test]
fn anchor_functions_serialize_canonically() {
    assert_eq!(
        inset("anchor(top --foo, 1px)").as_deref(),
        Some("anchor(--foo top, 1px)")
    );
    assert_eq!(
        inset("anchor(--foo min(50%, 100%))").as_deref(),
        Some("anchor(--foo calc(50%))")
    );
    assert_eq!(
        inset("calc((anchor(--foo top) + anchor(--bar bottom)) / 2)").as_deref(),
        Some("calc(0.5 * (anchor(--foo top) + anchor(--bar bottom)))")
    );
    assert_eq!(
        inset("calc(anchor(--foo left, 1px) + 10%)").as_deref(),
        Some("calc(10% + anchor(--foo left, 1px))")
    );
    assert_eq!(
        inset("calc(anchor(--foo left, 0))").as_deref(),
        Some("anchor(--foo left, 0px)")
    );
    assert_eq!(
        size("anchor-size(width --foo)").as_deref(),
        Some("anchor-size(--foo width)")
    );
    assert_eq!(
        size("anchor-size(10px)").as_deref(),
        Some("anchor-size(10px)")
    );
}

#[test]
fn anchor_functions_reject_bad_arguments() {
    for bad in [
        "anchor(--foo, top)",
        "anchor(--foo top,)",
        "anchor(foo top)",
        "anchor(--foo 10em)",
        "anchor(--foo top, 1)",
        "anchor(2 * 20%)",
        "calc(anchor(--foo top, 1) + 1px)",
    ] {
        assert_eq!(inset(bad), None, "{bad}");
    }
    assert_eq!(size("anchor(--foo top)"), None);
    assert_eq!(size("anchor-size(--foo 50%)"), None);
    assert_eq!(size("anchor-size(, 10px)"), None);
}

struct Fixed;

impl Resolver for Fixed {
    fn anchor(&self, f: &AnchorFn) -> Option<f64> {
        (f.name.as_deref() == Some("--a")).then_some(100.0)
    }

    fn anchor_size(&self, _: &AnchorSizeFn) -> Option<f64> {
        None
    }

    fn length(&self, value: f64, unit: &str) -> f64 {
        if unit == "%" { value * 2.0 } else { value }
    }
}

#[test]
fn anchor_expressions_evaluate_with_fallbacks() {
    let e = anchor::parse(b"calc(anchor(--a top) + 10%)", Context::Inset).unwrap();
    assert_eq!(anchor::evaluate(&e, &Fixed), Some(120.0));
    let e = anchor::parse(b"anchor(--b top, 5px)", Context::Inset).unwrap();
    assert_eq!(anchor::evaluate(&e, &Fixed), Some(5.0));
    let e = anchor::parse(b"anchor(--b top)", Context::Inset).unwrap();
    assert_eq!(anchor::evaluate(&e, &Fixed), None);
}

#[test]
fn position_area_specified_and_computed() {
    let specified = |t: &str| anchor::position_area_specified(t.as_bytes());
    let computed = |t: &str| anchor::position_area_computed(t.as_bytes());
    assert_eq!(specified("top left").as_deref(), Some("left top"));
    assert_eq!(specified("span-all left").as_deref(), Some("left"));
    assert_eq!(
        specified("start span-all").as_deref(),
        Some("start span-all")
    );
    assert_eq!(specified("left right"), None);
    assert_eq!(specified("left inline-start"), None);
    assert_eq!(
        computed("inline-start block-end").as_deref(),
        Some("end start")
    );
    assert_eq!(computed("start span-all").as_deref(), Some("block-start"));
    assert_eq!(computed("span-all start").as_deref(), Some("inline-start"));
    assert_eq!(
        computed("inline-start center").as_deref(),
        Some("center start")
    );
}

#[test]
fn position_try_lists() {
    let fallbacks = |t: &str| anchor::position_try_fallbacks(t.as_bytes());
    assert_eq!(
        fallbacks("flip-inline flip-start --foo").as_deref(),
        Some("--foo flip-inline flip-start")
    );
    assert_eq!(fallbacks("top left, --a").as_deref(), Some("left top, --a"));
    assert_eq!(fallbacks("--a --b"), None);
    assert_eq!(fallbacks("flip-inline --bar flip-block"), None);
    assert_eq!(
        anchor::position_try_split(b"most-width --foo, --bar"),
        Some(("most-width".into(), "--foo, --bar".into()))
    );
    assert_eq!(
        anchor::position_visibility(b"no-overflow anchor-valid").as_deref(),
        Some("anchor-valid no-overflow")
    );
}
