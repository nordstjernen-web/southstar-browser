//! Southstar — the GLib key-file text format the localStorage files are kept in: reading one group's keys and values and writing groups back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests;

fn until_nul(text: &str) -> &str {
    text.split('\0').next().unwrap_or_default()
}

fn is_key_name(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with([' ', '\t'])
        && !key.ends_with([' ', '\t'])
        && !key.contains(['=', '[', ']', '\n', '\r'])
}

fn escape_value(value: &str, out: &mut String) {
    let mut leading = true;
    for c in value.chars() {
        match c {
            ' ' if leading => out.push_str("\\s"),
            '\t' if leading => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
        leading &= matches!(c, ' ' | '\t');
    }
}

fn unescape_value(value: &str) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next()? {
            's' => ' ',
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '\\' => '\\',
            _ => return None,
        });
    }
    Some(out)
}

pub fn read_group(data: &[u8], wanted: &str) -> Option<Vec<(String, String)>> {
    let text = core::str::from_utf8(data).ok()?;
    let mut group: Option<&str> = None;
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line = line.trim_start_matches([' ', '\t']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[') {
            group = Some(name.trim_end_matches([' ', '\t']).strip_suffix(']')?);
            continue;
        }
        let (key, value) = line.split_once('=')?;
        group?;
        if group != Some(wanted) {
            continue;
        }
        let key = key.trim_end_matches([' ', '\t']);
        let Some(value) = unescape_value(value.trim_start_matches([' ', '\t'])) else {
            continue;
        };
        match entries.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value,
            None => entries.push((key.to_owned(), value)),
        }
    }
    Some(entries)
}

pub fn write_groups<'a>(
    groups: impl IntoIterator<Item = (&'a str, Vec<(&'a str, &'a str)>)>,
) -> String {
    let mut out = String::new();
    for (name, entries) in groups {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push('[');
        out.push_str(name);
        out.push_str("]\n");
        for (key, value) in entries {
            let key = until_nul(key);
            if !is_key_name(key) {
                continue;
            }
            out.push_str(key);
            out.push('=');
            escape_value(until_nul(value), &mut out);
            out.push('\n');
        }
    }
    out
}
