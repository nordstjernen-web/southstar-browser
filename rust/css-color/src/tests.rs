//! Southstar — unit tests for CSS colour parsing, conversion, mixing and serialization.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{Color, Context, computed_text, specified_text};

fn spec(input: &str) -> Option<String> {
    specified_text(input.as_bytes())
}

fn comp(input: &str) -> Option<String> {
    let red = Color::from_rgba8([255, 0, 0, 255]);
    computed_text(
        input.as_bytes(),
        &Context {
            current: Some(red),
            dark: false,
        },
    )
}

fn numbers(text: &str) -> Vec<f64> {
    text.split([' ', '(', ',', ')', '/'])
        .filter_map(|p| p.trim_end_matches('%').parse().ok())
        .collect()
}

fn shape(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_ascii_digit() && *c != '.')
        .collect()
}

fn fuzzy(actual: Option<String>, expected: &str, epsilon: f64) {
    let actual = actual.unwrap_or_else(|| panic!("no value, expected {expected}"));
    assert_eq!(shape(&actual), shape(expected), "{actual} vs {expected}");
    let (a, e) = (numbers(&actual), numbers(expected));
    assert_eq!(a.len(), e.len(), "{actual} vs {expected}");
    for (x, y) in a.iter().zip(&e) {
        assert!((x - y).abs() <= epsilon, "{actual} vs {expected}");
    }
}

#[test]
fn legacy_colors_serialize_as_rgb() {
    assert_eq!(spec("RED").as_deref(), Some("red"));
    assert_eq!(comp("red").as_deref(), Some("rgb(255, 0, 0)"));
    assert_eq!(comp("#f008").as_deref(), Some("rgba(255, 0, 0, 0.533)"));
    assert_eq!(
        comp("rgb(42%, 3%, 50%)").as_deref(),
        Some("rgb(107, 8, 128)")
    );
    assert_eq!(
        comp("rgba(0, 51, 255, 37)").as_deref(),
        Some("rgb(0, 51, 255)")
    );
    assert_eq!(
        comp("r\\gb(00, 51, 102)").as_deref(),
        Some("rgb(0, 51, 102)")
    );
    assert_eq!(
        comp("hsl(120 30 50% / 0.5)").as_deref(),
        Some("rgba(89, 166, 89, 0.5)")
    );
    assert_eq!(comp("transparent").as_deref(), Some("rgba(0, 0, 0, 0)"));
    assert_eq!(
        comp("rgb(calc(infinity), 0, 0)").as_deref(),
        Some("rgb(255, 0, 0)")
    );
}

#[test]
fn invalid_colors_are_rejected() {
    for bad in [
        "rgb(none, none, none)",
        "rgb(0 0, 0)",
        "rgba(0, 0, 0, 0deg)",
        "rgb(10%, 50%, 0)",
        "rgb(0, 0, 0,)",
        "hsl(0, 0% 0%)",
        "hwb(90deg, 50%, 50%)",
        "lab(0% 0 0 1)",
        "lch(20% 10 10deg 10)",
        "color(srgb 1 2)",
        "color-mix(in srgb, red -10%, blue)",
        "color-mix(in lab longer hue, red, blue)",
    ] {
        assert_eq!(spec(bad), None, "{bad}");
    }
}

#[test]
fn none_components_keep_their_space() {
    assert_eq!(
        comp("rgb(none none none)").as_deref(),
        Some("color(srgb none none none)")
    );
    fuzzy(
        comp("rgb(128 none none)"),
        "color(srgb 0.501961 none none)",
        1e-6,
    );
    assert_eq!(
        comp("hsla(none none none)").as_deref(),
        Some("hsl(none none none)")
    );
    assert_eq!(
        comp("hsl(120 100% 50% / none)").as_deref(),
        Some("hsl(120 100% 50% / none)")
    );
    assert_eq!(
        comp("hwb(120 80% none)").as_deref(),
        Some("hwb(120 80% none)")
    );
}

#[test]
fn lab_family_specified_and_computed() {
    assert_eq!(
        spec("lab(400 0 10/50%)").as_deref(),
        Some("lab(100 0 10 / 0.5)")
    );
    assert_eq!(
        spec("lab(50% 50% -20%)").as_deref(),
        Some("lab(50 62.5 -25)")
    );
    assert_eq!(
        spec("oklab(50% 50% -20%)").as_deref(),
        Some("oklab(0.5 0.2 -0.08)")
    );
    assert_eq!(
        spec("lch(10 20 1.28rad)").as_deref(),
        Some("lch(10 20 73.3386)")
    );
    assert_eq!(spec("lch(0.5 -20% -20)").as_deref(), Some("lch(0.5 0 340)"));
    assert_eq!(
        spec("lab(calc(50 * 3) calc(0.5 - 1) calc(1.5) / calc(-0.5 + 1))").as_deref(),
        Some("lab(calc(150) calc(-0.5) calc(1.5) / calc(0.5))")
    );
    assert_eq!(
        spec("lab(calc(50%) 50% 0.5)").as_deref(),
        Some("lab(calc(50%) 62.5 0.5)")
    );
    assert_eq!(
        spec("lab(calc(0 / 0) 0 0)").as_deref(),
        Some("lab(calc(NaN) 0 0)")
    );
    assert_eq!(
        comp("lab(calc(50 * 3) calc(0.5 - 1) calc(1.5) / calc(-0.5 + 1))").as_deref(),
        Some("lab(100 -0.5 1.5 / 0.5)")
    );
    assert_eq!(comp("lab(calc(NaN) 0 0)").as_deref(), Some("lab(0 0 0)"));
}

#[test]
fn color_function() {
    assert_eq!(
        comp("color(srgb 100% 0.5 20%)").as_deref(),
        Some("color(srgb 1 0.5 0.2)")
    );
    assert_eq!(
        comp("color(xyz 1 none 0.2 / 23.7%)").as_deref(),
        Some("color(xyz-d65 1 none 0.2 / 0.237)")
    );
    fuzzy(
        comp("color(from lab(67.5345 -8.6911 -41.6019) srgb r g b)"),
        "color(srgb 0.382862 0.672716 0.938469)",
        0.001,
    );
}

#[test]
fn relative_colors() {
    assert_eq!(
        spec("rgba(from rgb(20%, 40%, 60%, 80%) r g b / alpha)").as_deref(),
        Some("rgb(from rgba(51, 102, 153, 0.8) r g b / alpha)")
    );
    assert_eq!(
        spec("lab(from lab(50 -30 40) calc(l - 20) a b)").as_deref(),
        Some("lab(from lab(50 -30 40) calc(-20 + l) a b)")
    );
    assert_eq!(
        spec("lab(from lab(50 -30 40) l calc(a / 3) calc(b / 2))").as_deref(),
        Some("lab(from lab(50 -30 40) l calc(0.333333 * a) calc(0.5 * b))")
    );
    assert_eq!(
        spec("rgb(from rebeccapurple r calc(g * .5 + g * .5) 10)").as_deref(),
        Some("rgb(from rebeccapurple r calc((0.5 * g) + (0.5 * g)) 10)")
    );
    assert_eq!(
        spec("rgb(from rebeccapurple r calc(b * .5 - g * .5) 10)").as_deref(),
        Some("rgb(from rebeccapurple r calc((0.5 * b) - (0.5 * g)) 10)")
    );
    assert_eq!(
        spec("oklch(from red calc(1 / l) c h)").as_deref(),
        Some("oklch(from red calc(1 / l) c h)")
    );
    assert_eq!(
        spec("hsl(from rebeccapurple 0deg s l / alpha)").as_deref(),
        Some("hsl(from rebeccapurple 0deg s l / alpha)")
    );
    fuzzy(
        comp("rgb(from rebeccapurple 20% g b / alpha)"),
        "color(srgb 0.2 0.2 0.6)",
        0.01,
    );
    fuzzy(
        comp("hsl(from rebeccapurple 25 s l / alpha)"),
        "color(srgb 0.6 0.3667 0.2)",
        0.01,
    );
    fuzzy(
        comp("lab(from lab(0.7 45 30 / 40%) -200 -300 -400 / -500)"),
        "lab(0 -300 -400 / 0)",
        0.01,
    );
    fuzzy(
        comp("lch(from lch(-200 -300 -400 / -500%) l c h / alpha)"),
        "lch(0 0 320 / 0)",
        0.01,
    );
    fuzzy(
        comp("rgb(from currentColor r g b)"),
        "color(srgb 1 0 0)",
        0.01,
    );
    fuzzy(
        comp("alpha(from red / 0.5)"),
        "color(srgb 1 0 0 / 0.5)",
        0.001,
    );
}

#[test]
fn powerless_hues() {
    fuzzy(
        comp("hsl(from hwb(180 100% 25%) h s l)"),
        "hsl(none 0% 80%)",
        0.01,
    );
    fuzzy(
        comp("hsl(from lch(20 0.0015 180) h s l)"),
        "hsl(none 0% 18.93757452%)",
        0.01,
    );
    fuzzy(
        comp("hsl(from lch(20 0.00151 180) h s l)"),
        "color(srgb 0.18936676 0.18937855 0.18937554)",
        0.01,
    );
    fuzzy(
        comp("hsl(from hwb(180 49.999% 50%) h s l)"),
        "hsl(none 0% 49.999%)",
        0.01,
    );
    fuzzy(
        comp("hsl(from hwb(180 49.998% 50%) h s l)"),
        "color(srgb 0.49998 0.5 0.5)",
        0.01,
    );
    fuzzy(
        comp("lch(from oklch(0.2 0 180) l c h)"),
        "lch(7.22637037 0 none)",
        0.01,
    );
    fuzzy(
        comp("hwb(from oklch(0 0 0) h w b)"),
        "hwb(none 0% 100%)",
        0.01,
    );
}

#[test]
fn color_mix() {
    assert_eq!(
        spec("color-mix(in srgb, 70% red, 50% blue)").as_deref(),
        Some("color-mix(in srgb, red 70%, blue 50%)")
    );
    assert_eq!(
        spec("color-mix(in hsl, red 50%, blue)").as_deref(),
        Some("color-mix(in hsl, red, blue)")
    );
    assert_eq!(
        spec("color-mix(in srgb, red 50%, green, blue)").as_deref(),
        Some("color-mix(in srgb, red 50%, green 25%, blue 25%)")
    );
    assert_eq!(
        spec("color-mix(in srgb, red 100%)").as_deref(),
        Some("color-mix(in srgb, red)")
    );
    assert_eq!(
        spec("color-mix(in lch decreasing hue, red, hsl(120, 100%, 50%))").as_deref(),
        Some("color-mix(in lch decreasing hue, red, rgb(0, 255, 0))")
    );
    assert_eq!(
        spec("color-mix(in xyz, color(xyz .1 .2 .3), color(xyz .5 .6 .7))").as_deref(),
        Some("color-mix(in xyz-d65, color(xyz-d65 0.1 0.2 0.3), color(xyz-d65 0.5 0.6 0.7))")
    );
    assert_eq!(
        spec("color-mix(contrast-color(blue) 100%, purple 0%)").as_deref(),
        Some("color-mix(contrast-color(blue) 100%, purple 0%)")
    );
    fuzzy(
        comp("color-mix(in hsl, hsl(120deg 10% 20%), hsl(30deg 30% 40%))"),
        "color(srgb 0.33 0.36 0.24)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in hwb, hwb(120deg 10% 20% / 0) 10%, hwb(30deg 30% 40%))"),
        "color(srgb 0.6 0.495 0.3 / 0.9)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in lch, white, blue)"),
        "lch(64.78 65.6 301.36)",
        0.1,
    );
    fuzzy(
        comp("color-mix(in lch longer hue, red, black)"),
        "lch(27.145 53.4186 220.858)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in oklch, oklab(0.5 0 0), black)"),
        "oklch(0.25 0 none)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in lab, lab(10 20 30) 12.5%, lab(50 60 70) 37.5%)"),
        "lab(40 50 60 / 0.5)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in srgb, red 0%, green 0%, blue 0%)"),
        "color(srgb 0.25 0.12549 0.5 / 0)",
        0.001,
    );
    fuzzy(
        comp("color-mix(in srgb, red 30%, green 60%, blue 90%)"),
        "color(srgb 0.166667 0.16732 0.5)",
        0.001,
    );
    fuzzy(
        comp("color-mix(in hsl, hsl(0 100% 100%), blue)"),
        "color(srgb 1 0.5 1)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in hsl, hwb(0 50.0 50.0), blue)"),
        "color(srgb 0.25 0.25 0.75)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in oklch, oklab(0.1 none none), oklch(0.3 0.2 90deg))"),
        "oklch(0.2 0.2 90)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in hsl, hwb(none none none), hsl(none 0.2 0.4))"),
        "hsl(none 0.2% 0.4%)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in hsl, hwb(none 40% none), hsl(none 0.2 0.4))"),
        "hsl(none 50.1% 35.2%)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in oklch, oklab(0.1 none 0.3), oklch(0.3 0.2 0deg))"),
        "oklch(0.2 0.25 45)",
        0.01,
    );
    fuzzy(
        comp("color-mix(in srgb, red calc(10%), blue 50%)"),
        "color(srgb 0.166667 0 0.833333 / 0.6)",
        0.001,
    );
}

#[test]
fn edge_cases() {
    fuzzy(
        comp("color(from color(srgb 0.25 0.5 0.75) rec2020 r g b)"),
        "color(rec2020 0.420493 0.518024 0.741381)",
        0.001,
    );
    fuzzy(
        comp("color-mix(in srgb, color(srgb -2 -3 -4 / -5), color(srgb -4 -6 -8 / -10))"),
        "color(srgb 0 0 0 / 0)",
        0.001,
    );
    fuzzy(
        comp("color-mix(in hsl, lch(100% 0 0), blue)"),
        "color(srgb 0.625 0.625 0.875)",
        0.01,
    );
    fuzzy(
        comp("hwb(from hsl(180 0.001% 50%) h w b)"),
        "hwb(none 50% 50%)",
        0.01,
    );
    fuzzy(
        comp("hwb(from lab(100 104.3 -50.9) h w b)"),
        "color(srgb 1.5935 0.58776 1.40555)",
        0.001,
    );
    assert_eq!(
        comp("hwb(0 200% 100%)").as_deref(),
        Some("rgb(170, 170, 170)")
    );
    assert_eq!(
        comp("lch(50 calc(infinity) 0)").as_deref(),
        Some("lch(50 calc(infinity) 0)")
    );
    assert_eq!(spec("alpha(from red)"), None);
    assert_eq!(
        spec("color-mix(in oklab, red, blue)").as_deref(),
        Some("color-mix(red, blue)")
    );
}

#[test]
fn light_dark_and_contrast() {
    assert_eq!(
        spec("light-dark(black, white)").as_deref(),
        Some("light-dark(black, white)")
    );
    assert_eq!(
        comp("light-dark(black, white)").as_deref(),
        Some("rgb(0, 0, 0)")
    );
    assert_eq!(
        comp("contrast-color(white)").as_deref(),
        Some("rgb(0, 0, 0)")
    );
    assert_eq!(
        comp("contrast-color(navy)").as_deref(),
        Some("rgb(255, 255, 255)")
    );
}
