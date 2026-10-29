//! Southstar — the video crate's bridge to GLib's clock and cache directory, the network, textures, image decoding, URLs, the DOM, the box tree and computed styles.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::Node;
use southstar_layout::BoxRef;
use southstar_style::{Kind, PropId, StyleRef};

pub use southstar_texture::TextureRef as Texture;

pub use crate::decode::{Player, probe_chunk_range};

pub struct Response(Option<southstar_net::Response>);

impl Response {
    pub fn body(&self) -> Option<&[u8]> {
        let resp = self.0.as_ref().filter(|resp| resp.error().is_none())?;
        resp.body().filter(|body| !body.is_empty())
    }

    pub fn final_url(&self) -> Option<&[u8]> {
        self.0.as_ref()?.final_url().map(CStr::to_bytes)
    }
}

pub fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

fn user_cache_dir() -> Vec<u8> {
    southstar_glib::user_cache_dir()
}

pub fn env_set(name: &CStr) -> bool {
    name.to_str()
        .is_ok_and(|name| std::env::var_os(name).is_some())
}

pub fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn is_dir_separator(b: u8) -> bool {
    b == b'/' || (cfg!(windows) && b == b'\\')
}

fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    let parts: Vec<&[u8]> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    let separator = if cfg!(windows) {
        parts
            .iter()
            .flat_map(|p| p.iter().copied())
            .find(|&b| is_dir_separator(b))
            .unwrap_or(b'\\')
    } else {
        b'/'
    };
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let first = i == 0;
        let last = i + 1 == parts.len();
        let start = if first {
            0
        } else {
            part.iter()
                .position(|&b| !is_dir_separator(b))
                .unwrap_or(part.len())
        };
        let end = if last {
            part.len()
        } else {
            part.iter()
                .rposition(|&b| !is_dir_separator(b))
                .map_or(start, |p| p + 1)
        };
        if start >= end && !first {
            continue;
        }
        if !first && !out.last().is_some_and(|&b| is_dir_separator(b)) {
            out.push(separator);
        }
        out.extend_from_slice(&part[start..end.max(start)]);
    }
    out
}

fn path_of(bytes: &[u8]) -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(std::ffi::OsStr::from_bytes(bytes).into())
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(bytes).ok().map(Into::into)
    }
}

fn make_private_dir(dir: &std::path::Path) {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let _ = builder.create(dir);
}

fn replace_file_contents(path: &std::path::Path, data: &[u8]) -> bool {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = std::path::PathBuf::from(temp);
    if std::fs::write(&temp, data).is_ok() && std::fs::rename(&temp, path).is_ok() {
        return true;
    }
    let _ = std::fs::remove_file(&temp);
    false
}

pub fn url_resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(base.map(until_nul), until_nul(href))
}

pub fn texture_from_bgra(width: i32, height: i32, pixels: &[u8], stride: usize) -> Option<Texture> {
    Texture::new(width, height, pixels, stride)
}

pub fn decode_image(bytes: &[u8]) -> Option<(Texture, i32, i32)> {
    southstar_image::decode_bytes(bytes)
}

pub fn write_stream_file(subdir: &CStr, prefix: u8, token: u32, data: &[u8]) -> Option<Vec<u8>> {
    let cache_dir = user_cache_dir();
    let dir = build_filename(&[&cache_dir, b"southstar", subdir.to_bytes()]);
    make_private_dir(&path_of(&dir)?);
    let mut path = dir;
    path.extend_from_slice(
        format!(
            "/{}{}-{}.dat",
            char::from(prefix),
            std::process::id() as i32,
            token
        )
        .as_bytes(),
    );
    if !replace_file_contents(&path_of(&path)?, data) {
        return None;
    }
    let mut url = b"file://".to_vec();
    url.extend_from_slice(&path);
    Some(url)
}

pub fn next_in_subtree<'a>(n: Node<'a>, root: Node<'a>) -> Option<Node<'a>> {
    southstar_dom::index::next_in_subtree(n, Some(root), true)
}

pub fn collect_videos(root: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    southstar_layout_query::collect_videos(root)
}

pub fn box_fit(b: BoxRef<'_>) -> c_int {
    // SAFETY: a box's style pointer is null or the computed style the box tree keeps alive while the box exists.
    let style = unsafe { StyleRef::from_ptr(b.style()) };
    let fit = match style.and_then(|s| s.get(PropId::ObjectFit)) {
        Some(v) if v.kind() == Kind::Keyword => v.keyword_text().map(CStr::to_bytes),
        _ => Some(&b"contain"[..]),
    };
    match fit {
        Some(b"fill") => 0,
        Some(b"cover") => 2,
        Some(b"none") => 3,
        Some(b"scale-down") => 4,
        _ => 1,
    }
}

pub fn request(url: &[u8], top_url: Option<&[u8]>, done: impl FnOnce(Response) + 'static) {
    let request = southstar_net::FetchRequest {
        url,
        top_url,
        method: Some(b"GET"),
        ..southstar_net::FetchRequest::default()
    };
    southstar_net::request_with(&request, None, move |outcome| done(Response(outcome.ok())));
}
