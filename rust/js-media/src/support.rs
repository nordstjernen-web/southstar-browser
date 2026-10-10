//! Southstar — media type support: canPlayType, MediaSource.isTypeSupported and navigator.mediaCapabilities.decodingInfo.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi, get, set};

const MANIFESTS: [&str; 5] = [
    "application/x-mpegurl",
    "application/vnd.apple.mpegurl",
    "audio/x-mpegurl",
    "audio/mpegurl",
    "application/dash+xml",
];

const LIBAV_CONTAINERS: [&str; 15] = [
    "video/webm",
    "audio/webm",
    "video/mp4",
    "audio/mp4",
    "application/mp4",
    "video/x-m4v",
    "video/quicktime",
    "video/mp2t",
    "audio/aac",
    "audio/flac",
    "audio/wav",
    "audio/x-wav",
    "audio/ogg",
    "application/ogg",
    "audio/opus",
];

const SEGMENTED: [&str; 7] = [
    "video/webm",
    "audio/webm",
    "video/mp4",
    "audio/mp4",
    "video/mp2t",
    "audio/mpeg",
    "audio/aac",
];

fn is_c_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

fn strip(text: &str) -> &str {
    text.trim_matches(is_c_space)
}

fn until_nul(text: &str) -> &str {
    text.split('\0').next().unwrap_or_default()
}

fn codec_supported(codec: &str) -> bool {
    if codec.is_empty() {
        return false;
    }
    let formats = ffi::native_formats();
    if formats.vorbis && codec.contains("vorbis") {
        return true;
    }
    if formats.opus && codec.contains("opus") {
        return true;
    }
    if codec.contains("mp3") || codec.starts_with("mp4a.69") || codec.starts_with("mp4a.6b") {
        return true;
    }
    ffi::video_codec_available(codec)
}

fn split_type(lowered: &str) -> (&str, Option<&str>) {
    match lowered.find(';') {
        Some(semi) => (strip(&lowered[..semi]), Some(&lowered[semi..])),
        None => (strip(lowered), None),
    }
}

fn type_codecs(params: Option<&str>) -> Option<&str> {
    let params = params?;
    let after = &params[params.find("codecs")?..];
    let value = &after[after.find('=')? + 1..];
    let value = value.trim_start_matches([' ', '"', '\'']);
    let end = value.find(['"', '\'']).unwrap_or(value.len());
    Some(&value[..end])
}

fn container_supported(container: &str) -> bool {
    let formats = ffi::native_formats();
    if matches!(container, "audio/mpeg" | "audio/mp3" | "video/mpeg") {
        return true;
    }
    if (formats.vorbis || formats.opus) && matches!(container, "audio/ogg" | "application/ogg") {
        return true;
    }
    if formats.opus && container == "audio/opus" {
        return true;
    }
    formats.libav && LIBAV_CONTAINERS.contains(&container)
}

pub(crate) fn type_support(media_type: &str) -> &'static str {
    let media_type = until_nul(media_type);
    if media_type.is_empty() {
        return "";
    }
    let lowered = media_type.to_ascii_lowercase();
    let (container, params) = split_type(&lowered);
    if !MANIFESTS.contains(&container) && !container_supported(container) {
        return "";
    }
    match type_codecs(params) {
        Some(codecs) if !codecs.is_empty() => {
            if codecs.split(',').all(|codec| codec_supported(strip(codec))) {
                "probably"
            } else {
                ""
            }
        }
        _ => "maybe",
    }
}

pub(crate) fn can_play_type(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let support = match args.first().filter(|arg| arg.is_string()) {
        Some(arg) => match scope.to_string(arg) {
            Ok(text) => type_support(&text),
            Err(_) => "",
        },
        None => "",
    };
    Ok(scope.string(support))
}

pub(crate) fn is_type_supported(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first() else {
        return Ok(Value::boolean(false));
    };
    let Ok(text) = scope.to_string(arg) else {
        return Ok(Value::boolean(false));
    };
    let lowered = until_nul(&text).to_ascii_lowercase();
    let (container, _) = split_type(&lowered);
    let supported = SEGMENTED.contains(&container) && !type_support(&text).is_empty();
    Ok(Value::boolean(supported))
}

fn config_supported(scope: &mut Scope<'_>, config: &Value) -> bool {
    if !config.is_object() {
        return true;
    }
    let content_type = get(scope, config, "contentType");
    if !content_type.is_string() {
        return false;
    }
    scope
        .to_string(&content_type)
        .is_ok_and(|text| !type_support(&text).is_empty())
}

pub(crate) fn decoding_info(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let (promise, resolve, _) = scope.new_promise()?;
    let info = scope.new_object();
    let config = args.first().filter(|config| config.is_object()).cloned();
    let mut supported = false;
    if let Some(config) = &config {
        let kind = get(scope, config, "type");
        let kind = if kind.is_string() {
            scope.to_string(&kind).ok()
        } else {
            None
        };
        let video = get(scope, config, "video");
        let audio = get(scope, config, "audio");
        let kind_ok = kind
            .as_deref()
            .is_none_or(|kind| matches!(until_nul(kind), "file" | "media-source"));
        if kind_ok && (video.is_object() || audio.is_object()) {
            supported = config_supported(scope, &video) && config_supported(scope, &audio);
        }
    }
    set(scope, &info, "supported", Value::boolean(supported));
    set(scope, &info, "smooth", Value::boolean(supported));
    set(scope, &info, "powerEfficient", Value::boolean(supported));
    set(
        scope,
        &info,
        "supportedConfiguration",
        config.unwrap_or_else(Value::null),
    );
    let _ = scope.call(&resolve, &Value::undefined(), &[info]);
    Ok(promise)
}
