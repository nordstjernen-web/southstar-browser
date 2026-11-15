//! Southstar text — FontFace families installed under a private name each, so a later face for the same family and style replaces the earlier one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::sync::{Mutex, PoisonError};

struct Alias {
    family: String,
    installed: String,
    styles: Vec<(i32, i32)>,
}

struct Aliases {
    entries: Vec<Alias>,
    next: u64,
}

static ALIASES: Mutex<Aliases> = Mutex::new(Aliases {
    entries: Vec::new(),
    next: 0,
});

fn aliases() -> std::sync::MutexGuard<'static, Aliases> {
    ALIASES.lock().unwrap_or_else(PoisonError::into_inner)
}

fn normalized(family: &str) -> String {
    family
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_lowercase()
}

pub fn font_face_install_name(family: &str, weight: i32, slant: i32) -> String {
    let family = normalized(family);
    let mut aliases = aliases();
    let reusable = aliases
        .entries
        .iter()
        .find(|alias| alias.family == family && !alias.styles.contains(&(weight, slant)))
        .map(|alias| alias.installed.clone());
    reusable.unwrap_or_else(|| {
        aliases.next += 1;
        format!("southstar-font-face-{}", aliases.next)
    })
}

pub fn commit_font_face(family: &str, installed: &str, weight: i32, slant: i32) {
    let family = normalized(family);
    let mut aliases = aliases();
    match aliases
        .entries
        .iter_mut()
        .find(|alias| alias.family == family)
    {
        Some(alias) if alias.installed == installed => alias.styles.push((weight, slant)),
        Some(alias) => {
            alias.installed = installed.to_owned();
            alias.styles = vec![(weight, slant)];
        }
        None => aliases.entries.push(Alias {
            family,
            installed: installed.to_owned(),
            styles: vec![(weight, slant)],
        }),
    }
}

pub(crate) fn installed_family(family: &str) -> Option<String> {
    let aliases = aliases();
    if aliases.entries.is_empty() {
        return None;
    }
    let family = normalized(family);
    aliases
        .entries
        .iter()
        .find(|alias| alias.family == family)
        .map(|alias| alias.installed.clone())
}
