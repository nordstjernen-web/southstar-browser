//! Southstar — HTMLMediaElement: playback state, play/pause/load/seek, volume, mute and rate, TimeRanges, srcObject and the events the video pipeline reports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{Element, Js};
use crate::{JsResult, dom_exception_code, ffi, get, hooks, resolved, set, set_str, truthy};

const OWN_DATA: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

const MEDIA_STREAM_ATTR: &core::ffi::CStr = c"data-nd-media-stream";

fn is_media(element: Option<Element>) -> bool {
    element
        .and_then(|element| element.element_name())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case(b"video") || name.eq_ignore_ascii_case(b"audio")
        })
}

fn page_element(scope: &Scope<'_>, this: &Value) -> Option<(Js, Element)> {
    let js = ffi::js_of(scope);
    let element = ffi::element(this)?;
    (!js.is_null()).then_some((js, element))
}

fn number_or(scope: &mut Scope<'_>, this: &Value, key: &str, fallback: f64) -> Value {
    let value = get(scope, this, key);
    if value.is_number() {
        value
    } else {
        Value::number(fallback)
    }
}

fn stored_number(scope: &mut Scope<'_>, this: &Value, key: &str, fallback: f64) -> f64 {
    let value = get(scope, this, key);
    if value.is_number() {
        scope.to_number(&value).unwrap_or(fallback)
    } else {
        fallback
    }
}

fn int_or_zero(scope: &mut Scope<'_>, this: &Value, key: &str) -> Value {
    let value = get(scope, this, key);
    if value.is_undefined() {
        return Value::int(0);
    }
    Value::int(scope.to_int32(&value).unwrap_or(0))
}

fn define_int(scope: &mut Scope<'_>, object: &Value, key: &str, value: i32) {
    let _ = scope.define(object, key, Value::int(value), OWN_DATA);
}

fn resolve_src(js: Js, element: Element) -> Option<String> {
    let nonempty = |src: &core::ffi::CStr| !src.is_empty();
    let src = element
        .attr(c"src")
        .filter(|src| nonempty(src))
        .or_else(|| {
            core::iter::successors(element.first_child(), |child| child.next_sibling())
                .filter(|child| child.element_name() == Some(b"source"))
                .find_map(|child| child.attr(c"src").filter(|src| nonempty(src)))
        })?;
    let src = src.to_string_lossy();
    match ffi::current_url(js) {
        Some(base) => ffi::url_resolve(&base, &src),
        None => Some(src.into_owned()),
    }
}

fn plays_through_helper(url: &str) -> bool {
    let path = &url[..url.find('?').unwrap_or(url.len())];
    let extensions: &[&str] = if ffi::native_formats().libav {
        &[".mp3", ".opus", ".weba", ".webm", ".ogg", ".oga"]
    } else {
        &[".mp3"]
    };
    extensions.iter().any(|extension| {
        path.len() >= extension.len()
            && path.as_bytes()[path.len() - extension.len()..]
                .eq_ignore_ascii_case(extension.as_bytes())
    })
}

fn token_valid(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.first() == Some(&b'a')
        && (2..=11).contains(&bytes.len())
        && bytes[1..].iter().all(u8::is_ascii_digit)
}

fn existing_token(scope: &mut Scope<'_>, element: &Value) -> Option<String> {
    let token = get(scope, element, "_nd_audio_token");
    if !token.is_string() {
        return None;
    }
    scope
        .to_string(&token)
        .ok()
        .filter(|token| token_valid(token))
}

fn audio_token(scope: &mut Scope<'_>, js: Js, element: Element) -> String {
    let wrapper = ffi::wrap(scope, element);
    if let Some(token) = existing_token(scope, &wrapper) {
        return token;
    }
    let token = format!("a{}", hooks::next_audio_token(js));
    set_str(scope, &wrapper, "_nd_audio_token", &token);
    token
}

pub(crate) fn play(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if truthy(scope, this, "_nd_playing") {
        return resolved(scope, Value::undefined());
    }
    set(scope, this, "_nd_playing", Value::boolean(true));
    set(scope, this, "_nd_ended", Value::boolean(false));
    if let Some((js, element)) = page_element(scope, this) {
        ffi::dispatch(js, element, "play");
        let hooks = hooks::hooks(js);
        hooks.notify_play(element, true);
        let url = resolve_src(js, element);
        let is_video = element
            .name()
            .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"video"));
        match url {
            Some(url)
                if hooks.audio.is_some()
                    && !(is_video && ffi::video_url_is_inline(&url))
                    && plays_through_helper(&url) =>
            {
                let token = audio_token(scope, js, element);
                if !truthy(scope, this, "_nd_audio_opened") {
                    hooks.emit_audio(&format!("open {token} {url}"));
                    set(scope, this, "_nd_audio_opened", Value::boolean(true));
                }
                hooks.emit_audio(&format!("play {token}"));
            }
            _ => ffi::dispatch(js, element, "playing"),
        }
    }
    resolved(scope, Value::undefined())
}

pub(crate) fn pause(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if !truthy(scope, this, "_nd_playing") {
        return Ok(Value::undefined());
    }
    set(scope, this, "_nd_playing", Value::boolean(false));
    if let Some((js, element)) = page_element(scope, this) {
        let hooks = hooks::hooks(js);
        if let Some(token) = existing_token(scope, this) {
            hooks.emit_audio(&format!("pause {token}"));
        }
        ffi::dispatch(js, element, "pause");
        hooks.notify_play(element, false);
    }
    Ok(Value::undefined())
}

pub(crate) fn load(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let element = ffi::element(this);
    if !is_media(element) {
        return Ok(Value::undefined());
    }
    set(scope, this, "_nd_pos", Value::number(0.0));
    set(scope, this, "_nd_duration", Value::undefined());
    set(scope, this, "_nd_buffered", Value::undefined());
    set(scope, this, "_nd_ended", Value::boolean(false));
    set(scope, this, "_nd_playing", Value::boolean(false));
    set(scope, this, "_nd_played_end", Value::number(0.0));
    set(scope, this, "_nd_seeking", Value::boolean(false));
    set(scope, this, "_nd_error", Value::undefined());
    set(scope, this, "_nd_readyState", Value::int(0));
    set(scope, this, "_nd_networkState", Value::int(2));
    define_int(scope, this, "readyState", 0);
    define_int(scope, this, "videoWidth", 0);
    define_int(scope, this, "videoHeight", 0);
    if let Some((js, element)) = page_element(scope, this) {
        ffi::dispatch(js, element, "emptied");
        ffi::dispatch(js, element, "loadstart");
    }
    Ok(Value::undefined())
}

pub(crate) fn paused(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(!truthy(scope, this, "_nd_playing")))
}

pub(crate) fn ended(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(truthy(scope, this, "_nd_ended")))
}

pub(crate) fn seeking(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(truthy(scope, this, "_nd_seeking")))
}

pub(crate) fn ready_state(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(int_or_zero(scope, this, "_nd_readyState"))
}

pub(crate) fn network_state(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(int_or_zero(scope, this, "_nd_networkState"))
}

pub(crate) fn current_time(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(number_or(scope, this, "_nd_pos", 0.0))
}

pub(crate) fn duration(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(number_or(scope, this, "_nd_duration", f64::NAN))
}

pub(crate) fn error(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let error = get(scope, this, "_nd_error");
    if error.is_object() {
        return Ok(error);
    }
    Ok(if is_media(ffi::element(this)) {
        Value::null()
    } else {
        Value::undefined()
    })
}

fn range_edge(scope: &mut Scope<'_>, args: &[Value], data: &[Value], end: bool) -> JsResult {
    let duration = scope.to_number(&data[0]).unwrap_or(0.0);
    let length = scope.to_int32(&data[1]).unwrap_or(0);
    let index = match args.first() {
        Some(index) => scope.to_int32(index).unwrap_or(0),
        None => 0,
    };
    if index < 0 || index >= length {
        return Err(dom_exception_code(
            scope,
            "IndexSizeError",
            "index out of TimeRanges bounds",
            1,
        ));
    }
    Ok(Value::number(if end { duration } else { 0.0 }))
}

fn range_start(scope: &mut Scope<'_>, _: &Value, args: &[Value], data: &[Value]) -> JsResult {
    range_edge(scope, args, data, false)
}

fn range_end(scope: &mut Scope<'_>, _: &Value, args: &[Value], data: &[Value]) -> JsResult {
    range_edge(scope, args, data, true)
}

fn time_ranges(scope: &mut Scope<'_>, end: f64) -> Value {
    let length = i32::from(end > 0.0);
    let ranges = scope.new_object();
    let global = scope.global();
    let constructor = get(scope, &global, "TimeRanges");
    if constructor.is_object() {
        let proto = get(scope, &constructor, "prototype");
        if proto.is_object() {
            let _ = scope.set_prototype(&ranges, &proto);
        }
    }
    set(scope, &ranges, "length", Value::int(length));
    let data = [Value::number(end), Value::int(length)];
    let start = scope.bound_function("", 1, range_start, &data);
    set(scope, &ranges, "start", start);
    let end = scope.bound_function("", 1, range_end, &data);
    set(scope, &ranges, "end", end);
    ranges
}

fn positive(scope: &mut Scope<'_>, this: &Value, key: &str) -> f64 {
    let value = stored_number(scope, this, key, 0.0);
    if value.is_nan() || value <= 0.0 {
        0.0
    } else {
        value
    }
}

pub(crate) fn seekable(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let end = positive(scope, this, "_nd_duration");
    Ok(time_ranges(scope, end))
}

pub(crate) fn buffered(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let end = positive(scope, this, "_nd_buffered");
    Ok(time_ranges(scope, end))
}

pub(crate) fn played(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let end = positive(scope, this, "_nd_played_end");
    Ok(time_ranges(scope, end))
}

fn set_rate(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    key: &str,
    name: &str,
) -> JsResult {
    let rate = match args.first() {
        Some(value) => scope.to_number(value)?,
        None => f64::NAN,
    };
    if !rate.is_finite() {
        return Err(scope.type_error(&format!("{name} must be finite")));
    }
    if stored_number(scope, this, key, 1.0) == rate {
        return Ok(Value::undefined());
    }
    set(scope, this, key, Value::number(rate));
    if let Some((js, element)) = page_element(scope, this)
        && is_media(Some(element))
    {
        ffi::dispatch(js, element, "ratechange");
    }
    Ok(Value::undefined())
}

pub(crate) fn playback_rate(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(number_or(scope, this, "_nd_rate", 1.0))
}

pub(crate) fn set_playback_rate(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    set_rate(scope, this, args, "_nd_rate", "playbackRate")
}

pub(crate) fn default_playback_rate(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(number_or(scope, this, "_nd_default_rate", 1.0))
}

pub(crate) fn set_default_playback_rate(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    set_rate(scope, this, args, "_nd_default_rate", "defaultPlaybackRate")
}

pub(crate) fn volume(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(number_or(scope, this, "_nd_volume", 1.0))
}

pub(crate) fn set_volume(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let volume = match args.first() {
        Some(value) => scope.to_number(value)?,
        None => f64::NAN,
    };
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err(dom_exception_code(
            scope,
            "IndexSizeError",
            "volume must be between 0 and 1",
            1,
        ));
    }
    if stored_number(scope, this, "_nd_volume", 1.0) == volume {
        return Ok(Value::undefined());
    }
    set(scope, this, "_nd_volume", Value::number(volume));
    if let Some((js, element)) = page_element(scope, this) {
        hooks::hooks(js).notify_volume(element, volume);
        ffi::dispatch(js, element, "volumechange");
    }
    Ok(Value::undefined())
}

fn is_muted(scope: &mut Scope<'_>, this: &Value) -> bool {
    let stored = get(scope, this, "_nd_muted");
    if !stored.is_undefined() {
        return scope.to_bool(&stored);
    }
    ffi::element(this).is_some_and(|element| element.attr(c"muted").is_some())
}

pub(crate) fn muted(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(is_muted(scope, this)))
}

pub(crate) fn set_muted(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let muted = args.first().is_some_and(|value| scope.to_bool(value));
    if is_muted(scope, this) == muted {
        return Ok(Value::undefined());
    }
    set(scope, this, "_nd_muted", Value::boolean(muted));
    if let Some((js, element)) = page_element(scope, this) {
        hooks::hooks(js).notify_muted(element, muted);
        ffi::dispatch(js, element, "volumechange");
    }
    Ok(Value::undefined())
}

pub(crate) fn set_current_time(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let mut seconds = match args.first() {
        Some(value) => scope.to_number(value)?,
        None => f64::NAN,
    };
    if !seconds.is_finite() {
        return Err(scope.type_error("currentTime must be finite"));
    }
    if seconds < 0.0 {
        seconds = 0.0;
    }
    let Some((js, element)) = page_element(scope, this) else {
        return Ok(Value::undefined());
    };
    let hooks = hooks::hooks(js);
    let mut handled = hooks.seek(element, seconds);
    if !handled
        && hooks.audio.is_some()
        && let Some(token) = existing_token(scope, this)
    {
        hooks.emit_audio(&format!("seek {token} {seconds:.3}"));
        handled = true;
    }
    set(scope, this, "_nd_pos", Value::number(seconds));
    if handled {
        set(scope, this, "_nd_ended", Value::boolean(false));
        set(scope, this, "_nd_seeking", Value::boolean(true));
        ffi::dispatch(js, element, "seeking");
        ffi::dispatch(js, element, "timeupdate");
        set(scope, this, "_nd_seeking", Value::boolean(false));
        ffi::dispatch(js, element, "seeked");
    }
    Ok(Value::undefined())
}

pub(crate) fn fast_seek(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Ok(Value::undefined());
    }
    set_current_time(scope, this, args)
}

pub(crate) fn video_playback_quality(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let position = stored_number(scope, this, "_nd_pos", 0.0);
    let quality = scope.new_object();
    let now = ffi::perf_now(scope);
    set(scope, &quality, "creationTime", Value::number(now));
    set(
        scope,
        &quality,
        "totalVideoFrames",
        Value::int((position * 30.0) as i32),
    );
    set(scope, &quality, "droppedVideoFrames", Value::int(0));
    set(scope, &quality, "corruptedVideoFrames", Value::int(0));
    Ok(quality)
}

pub(crate) fn src_object(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let value = get(scope, this, "_nd_srcObject");
    Ok(if value.is_undefined() {
        Value::null()
    } else {
        value
    })
}

pub(crate) fn set_src_object(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let value = args.first().cloned().unwrap_or_else(Value::undefined);
    set(scope, this, "_nd_srcObject", value.clone());
    let camera = value.is_object() && truthy(scope, &value, "_nd_camera");
    let js = ffi::js_of(scope);
    if let Some(element) = ffi::element(this) {
        let marker: &[u8] = if camera { b"camera" } else { b"" };
        southstar_dom::attrs::set_len(element, MEDIA_STREAM_ATTR, Some(marker), marker.len());
        ffi::mark_mutated(js);
        if camera && !js.is_null() {
            ffi::dispatch(js, element, "loadedmetadata");
        }
    }
    Ok(Value::undefined())
}

fn record_played(scope: &mut Scope<'_>, element: &Value, position: f64) {
    if position > stored_number(scope, element, "_nd_played_end", 0.0) {
        set(scope, element, "_nd_played_end", Value::number(position));
    }
}

pub(crate) fn video_event(scope: &mut Scope<'_>, js: Js, node: Element, kind: &str, value: f64) {
    let element = ffi::wrap(scope, node);
    let fire = |kinds: &[&str]| {
        for kind in kinds {
            ffi::dispatch(js, node, kind);
        }
    };
    match kind {
        "meta" => {
            let previous = stored_number(scope, &element, "_nd_duration", f64::NAN);
            let duration = if !previous.is_nan() && previous > value {
                previous
            } else {
                value
            };
            set(scope, &element, "_nd_duration", Value::number(duration));
            set(scope, &element, "_nd_readyState", Value::int(4));
            set(scope, &element, "_nd_networkState", Value::int(1));
            define_int(scope, &element, "readyState", 4);
            fire(&[
                "loadedmetadata",
                "durationchange",
                "canplay",
                "canplaythrough",
            ]);
        }
        "vwidth" => define_int(scope, &element, "videoWidth", value as i32),
        "vheight" => {
            define_int(scope, &element, "videoHeight", value as i32);
            fire(&["resize"]);
        }
        "waiting" => {
            set(scope, &element, "_nd_pos", Value::number(value));
            define_int(scope, &element, "readyState", 2);
            fire(&["timeupdate", "waiting"]);
        }
        "resumed" => {
            define_int(scope, &element, "readyState", 4);
            fire(&["canplay", "playing"]);
        }
        "unmuted" => {
            set(scope, &element, "_nd_muted", Value::boolean(false));
            fire(&["volumechange"]);
        }
        "buf" => {
            set(scope, &element, "_nd_buffered", Value::number(value));
            fire(&["progress"]);
        }
        "pos" => {
            set(scope, &element, "_nd_pos", Value::number(value));
            if truthy(scope, &element, "_nd_playing") {
                record_played(scope, &element, value);
            }
            fire(&["timeupdate"]);
        }
        "play" => {
            set(scope, &element, "_nd_playing", Value::boolean(true));
            set(scope, &element, "_nd_ended", Value::boolean(false));
            fire(&["play", "playing"]);
        }
        "pause" => {
            set(scope, &element, "_nd_playing", Value::boolean(false));
            fire(&["pause"]);
        }
        "ended" => {
            let was_playing = truthy(scope, &element, "_nd_playing");
            set(scope, &element, "_nd_pos", Value::number(value));
            record_played(scope, &element, value);
            set(scope, &element, "_nd_playing", Value::boolean(false));
            set(scope, &element, "_nd_ended", Value::boolean(true));
            fire(&["timeupdate"]);
            if was_playing {
                fire(&["pause"]);
            }
            fire(&["ended"]);
        }
        _ => {}
    }
}
