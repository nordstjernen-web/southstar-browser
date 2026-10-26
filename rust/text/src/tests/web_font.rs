//! Southstar text — differential test of the web-font index against fontconfig's FreeType query.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::BTreeMap;
use std::ffi::CString;

use super::patterns_for;
use crate::ffi::fontconfig::{FC_INDEX, format, freetype_query_all};

const FIELDS: [&str; 31] = [
    "family",
    "familylang",
    "style",
    "stylelang",
    "fullname",
    "fullnamelang",
    "postscriptname",
    "slant",
    "weight",
    "width",
    "foundry",
    "fontversion",
    "index",
    "outline",
    "scalable",
    "color",
    "variable",
    "fonthashint",
    "decorative",
    "symbol",
    "spacing",
    "capability",
    "fontformat",
    "namedinstance",
    "fontwrapper",
    "order",
    "size",
    "pixelsize",
    "antialias",
    "charset",
    "lang",
];

fn font_files() -> Vec<String> {
    let mut dirs: Vec<String> = std::env::var("SOUTHSTAR_FONT_DIRS")
        .map(|dirs| dirs.split(';').map(str::to_owned).collect())
        .unwrap_or_default();
    if dirs.is_empty() {
        dirs.push("C:/Windows/Fonts".into());
        dirs.push("/usr/share/fonts".into());
    }
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path.to_string_lossy().into_owned());
                continue;
            }
            let name = path.to_string_lossy().to_lowercase();
            if [".ttf", ".otf", ".ttc"]
                .iter()
                .any(|ext| name.ends_with(ext))
            {
                files.push(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn indexes_like_fontconfig() {
    let mut fields_differing: BTreeMap<&str, usize> = BTreeMap::new();
    let mut samples: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let (mut files, mut patterns, mut missing, mut extra) = (0, 0, 0, 0);
    for path in font_files() {
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let Ok(c_path) = CString::new(path.as_str()) else {
            continue;
        };
        let theirs = freetype_query_all(&c_path);
        let ours = patterns_for(&path, &data);
        if theirs.is_empty() && ours.is_empty() {
            continue;
        }
        files += 1;
        let by_index = |list: &[crate::ffi::fontconfig::Pattern]| {
            list.iter()
                .map(|p| {
                    (
                        p.get_integer(FC_INDEX, 0).unwrap_or(-1),
                        p.get_bool(c"variable", 0),
                    )
                })
                .collect::<Vec<_>>()
        };
        let our_keys = by_index(&ours);
        for (theirs_pattern, key) in theirs.iter().zip(by_index(&theirs)) {
            let Some(position) = our_keys.iter().position(|k| *k == key) else {
                missing += 1;
                samples
                    .entry("missing")
                    .or_default()
                    .push(format!("{path} {key:?}"));
                continue;
            };
            patterns += 1;
            let ours_pattern = &ours[position];
            for field in FIELDS {
                let spec = CString::new(format!("%{{{field}}}")).expect("no NUL");
                let a = format(ours_pattern, &spec);
                let b = format(theirs_pattern, &spec);
                if a != b {
                    *fields_differing.entry(field).or_default() += 1;
                    let list = samples.entry(field).or_default();
                    if list.len() < 4 {
                        let clip = |s: &str| s.chars().take(160).collect::<String>();
                        list.push(format!("{path} {key:?}: ours {} fc {}", clip(&a), clip(&b)));
                    }
                }
            }
        }
        extra += ours.len().saturating_sub(theirs.len());
    }
    eprintln!("{files} files, {patterns} patterns compared, {missing} missing, {extra} extra");
    for (field, count) in &fields_differing {
        eprintln!("{field}: {count}");
    }
    for (field, list) in &samples {
        for line in list.iter().take(4) {
            eprintln!("  {field}: {line}");
        }
    }
    assert_eq!(missing, 0);
    assert!(fields_differing.is_empty(), "{fields_differing:?}");
}
