//! Southstar — the remote-control rules that need no GTK: key names and shortcuts, token comparison and the page-point conversion.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::procview::input as keys;

pub const MOD_SHIFT: i32 = 1;
pub const MOD_CONTROL: i32 = 2;
pub const MOD_ALT: i32 = 4;
pub const MOD_META: i32 = 8;

const KEY_F1: u32 = 0xffbe;
const UNICODE_KEYVAL: u32 = 0x0100_0000;

pub fn token_equal(given: &str, expected: &str) -> bool {
    let (a, b) = (given.as_bytes(), expected.as_bytes());
    let mut diff = u8::from(a.len() != b.len());
    for (i, &byte) in b.iter().enumerate() {
        diff |= byte ^ a.get(i).copied().unwrap_or(0);
    }
    diff == 0 && !b.is_empty()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn modifier(word: &str) -> Option<i32> {
    Some(match word.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => MOD_CONTROL,
        "shift" => MOD_SHIFT,
        "alt" | "option" => MOD_ALT,
        "meta" | "cmd" | "command" | "super" | "win" => MOD_META,
        _ => return None,
    })
}

pub fn char_keyval(c: char) -> u32 {
    let code = c as u32;
    if (0x20..0x7f).contains(&code) || (0xa0..0x100).contains(&code) {
        code
    } else {
        UNICODE_KEYVAL | code
    }
}

fn keyval(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let named = match lower.as_str() {
        "enter" | "return" => Some(keys::KEY_RETURN),
        "esc" | "escape" => Some(keys::KEY_ESCAPE),
        "tab" => Some(keys::KEY_TAB),
        "space" => Some(keys::KEY_SPACE),
        "backspace" => Some(keys::KEY_BACKSPACE),
        "delete" | "del" => Some(keys::KEY_DELETE),
        "insert" => Some(keys::KEY_INSERT),
        "up" | "arrowup" => Some(keys::KEY_UP),
        "down" | "arrowdown" => Some(keys::KEY_DOWN),
        "left" | "arrowleft" => Some(keys::KEY_LEFT),
        "right" | "arrowright" => Some(keys::KEY_RIGHT),
        "home" => Some(keys::KEY_HOME),
        "end" => Some(keys::KEY_END),
        "pageup" | "page_up" => Some(keys::KEY_PAGE_UP),
        "pagedown" | "page_down" => Some(keys::KEY_PAGE_DOWN),
        "plus" => Some(keys::KEY_PLUS),
        "minus" => Some(keys::KEY_MINUS),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(char_keyval(c));
    }
    let n: u32 = lower.strip_prefix('f')?.parse().ok()?;
    (1..=35).contains(&n).then(|| KEY_F1 + n - 1)
}

pub fn parse_combo(combo: &str) -> Result<(u32, i32), String> {
    let (mods_part, key) = if let Some(rest) = combo.strip_suffix("++") {
        (Some(rest), "+")
    } else {
        match combo.rsplit_once('+') {
            Some((mods, key)) if !mods.is_empty() && !key.is_empty() => (Some(mods), key),
            _ => (None, combo),
        }
    };
    let mut mods = 0;
    for word in mods_part.unwrap_or("").split('+').filter(|w| !w.is_empty()) {
        mods |= modifier(word).ok_or_else(|| format!("unknown modifier '{word}'"))?;
    }
    let mut kv = keyval(key).ok_or_else(|| format!("unknown key '{key}'"))?;
    if mods & MOD_SHIFT != 0 {
        if kv == keys::KEY_TAB {
            kv = keys::KEY_ISO_LEFT_TAB;
        } else if (u32::from(b'a')..=u32::from(b'z')).contains(&kv) {
            kv -= 0x20;
        }
    }
    Ok((kv, mods))
}

pub fn parse_keys(spec: &str) -> Result<Vec<(u32, i32)>, String> {
    spec.split(' ')
        .filter(|c| !c.is_empty())
        .map(parse_combo)
        .collect()
}
