//! Southstar — the about: pages' base stylesheet, taken from the NS_ABOUT_BASE_CSS literals in src/about_style.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

const HEADER: &str = include_str!("../../../src/about_style.h");
const MACRO: &str = "#define NS_ABOUT_BASE_CSS";

fn macro_literals(header: &str) -> String {
    let body = header.split_once(MACRO).map_or("", |(_, rest)| rest);
    let mut css = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => loop {
                match chars.next() {
                    None | Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some('n') => css.push('\n'),
                        Some('t') => css.push('\t'),
                        Some(escaped) => css.push(escaped),
                        None => break,
                    },
                    Some(literal) => css.push(literal),
                }
            },
            '\\' => {
                chars.find(|&skipped| skipped == '\n');
            }
            '\n' => break,
            _ => {}
        }
    }
    css
}

pub fn base_css() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| macro_literals(HEADER))
}
