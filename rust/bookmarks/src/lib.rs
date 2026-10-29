//! Southstar — bookmarks storage: the saved list, parsing and writing the tab-separated bookmarks file.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod ffi;

use core::ffi::CStr;
use std::ffi::CString;

pub struct Bookmark {
    url: CString,
    title: CString,
}

impl Bookmark {
    fn new(url: &[u8], title: &[u8]) -> Bookmark {
        Bookmark {
            url: CString::new(url).unwrap_or_default(),
            title: CString::new(title).unwrap_or_default(),
        }
    }

    pub fn url(&self) -> &CStr {
        &self.url
    }

    pub fn title(&self) -> &CStr {
        &self.title
    }
}

pub struct Bookmarks {
    items: Vec<Bookmark>,
    path: CString,
}

impl Bookmarks {
    pub fn load() -> Bookmarks {
        let path = ffi::bookmarks_path();
        let contents = ffi::read(&path).unwrap_or_default();
        let items = parse(&contents)
            .into_iter()
            .map(|(url, title)| Bookmark::new(url, title))
            .collect();
        Bookmarks { items, path }
    }

    pub fn items(&self) -> &[Bookmark] {
        &self.items
    }

    fn position(&self, url: &CStr) -> Option<usize> {
        self.items.iter().position(|b| b.url.as_c_str() == url)
    }

    pub fn contains(&self, url: &CStr) -> bool {
        self.position(url).is_some()
    }

    pub fn add(&mut self, url: &CStr, title: Option<&CStr>) {
        if self.contains(url) {
            return;
        }
        let title = title.unwrap_or(url);
        self.items
            .push(Bookmark::new(url.to_bytes(), title.to_bytes()));
        self.save();
    }

    pub fn remove(&mut self, url: &CStr) {
        if let Some(i) = self.position(url) {
            self.items.remove(i);
            self.save();
        }
    }

    fn save(&self) {
        let out = serialize(
            self.items
                .iter()
                .map(|b| (Some(b.url.to_bytes()), Some(b.title.to_bytes()))),
        );
        ffi::write(&self.path, &out);
    }
}

fn strip(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    &line[start..end.max(start)]
}

pub fn parse(contents: &[u8]) -> Vec<(&[u8], &[u8])> {
    let text = contents.split(|&c| c == 0).next().unwrap_or_default();
    text.split(|&c| c == b'\n')
        .map(strip)
        .filter(|line| !line.is_empty() && line[0] != b'#')
        .map(|line| match line.iter().position(|&c| c == b'\t') {
            Some(tab) => (&line[..tab], &line[tab + 1..]),
            None => (line, line),
        })
        .collect()
}

fn append_sanitized(out: &mut Vec<u8>, field: &[u8]) {
    out.extend(field.iter().map(|&c| match c {
        b'\t' | b'\n' | b'\r' => b' ',
        c => c,
    }));
}

pub fn serialize<'a>(items: impl Iterator<Item = (Option<&'a [u8]>, Option<&'a [u8]>)>) -> Vec<u8> {
    let mut out = Vec::new();
    for (url, title) in items {
        append_sanitized(&mut out, url.unwrap_or_default());
        out.push(b'\t');
        append_sanitized(&mut out, title.unwrap_or_default());
        out.push(b'\n');
    }
    out
}
