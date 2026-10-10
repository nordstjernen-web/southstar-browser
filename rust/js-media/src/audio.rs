//! Southstar — the Web Audio API surface: AudioContext, OfflineAudioContext, AudioNodes, AudioParams and AudioBuffers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{ElementType, Scope, Value};

use crate::{
    JsResult, array_length, bind, defined_error, dom_exception, ffi, get, make_ctor, rejected,
    resolved, set, set_str, truthy,
};

const MAX_CHANNELS: i32 = 32;
const MAX_SAMPLES: u64 = 1 << 26;
const DEFAULT_RATE: f64 = 44100.0;

const PARAM_METHODS: [(&str, u32); 7] = [
    ("setValueAtTime", 2),
    ("linearRampToValueAtTime", 2),
    ("exponentialRampToValueAtTime", 2),
    ("setTargetAtTime", 3),
    ("setValueCurveAtTime", 3),
    ("cancelScheduledValues", 1),
    ("cancelAndHoldAtTime", 1),
];

const NODE_PARAMS: [(&str, f64); 4] = [
    ("gain", 1.0),
    ("frequency", 440.0),
    ("detune", 0.0),
    ("playbackRate", 1.0),
];

const SHAPING_PARAMS: [(&str, f64); 9] = [
    ("offset", 1.0),
    ("pan", 0.0),
    ("Q", 1.0),
    ("delayTime", 0.0),
    ("threshold", -24.0),
    ("knee", 30.0),
    ("ratio", 12.0),
    ("attack", 0.003),
    ("release", 0.25),
];

const NODE_INTERFACES: [(&str, &str); 15] = [
    ("destination", "AudioDestinationNode"),
    ("gain", "GainNode"),
    ("oscillator", "OscillatorNode"),
    ("compressor", "DynamicsCompressorNode"),
    ("biquad", "BiquadFilterNode"),
    ("delay", "DelayNode"),
    ("analyser", "AnalyserNode"),
    ("buffersource", "AudioBufferSourceNode"),
    ("constant", "ConstantSourceNode"),
    ("waveshaper", "WaveShaperNode"),
    ("convolver", "ConvolverNode"),
    ("panner", "PannerNode"),
    ("stereopanner", "StereoPannerNode"),
    ("merger", "ChannelMergerNode"),
    ("splitter", "ChannelSplitterNode"),
];

const NODE_FACTORIES: [(&str, u32, &str); 19] = [
    ("createGain", 0, "gain"),
    ("createAnalyser", 0, "analyser"),
    ("createMediaElementSource", 1, "mediaelementsource"),
    ("createMediaStreamSource", 1, "mediastreamsource"),
    ("createBufferSource", 0, "buffersource"),
    ("createOscillator", 0, "oscillator"),
    ("createDelay", 1, "delay"),
    ("createBiquadFilter", 0, "biquad"),
    ("createChannelMerger", 1, "merger"),
    ("createChannelSplitter", 1, "splitter"),
    ("createPanner", 0, "panner"),
    ("createStereoPanner", 0, "stereopanner"),
    ("createDynamicsCompressor", 0, "compressor"),
    ("createWaveShaper", 0, "waveshaper"),
    ("createConvolver", 0, "convolver"),
    ("createConstantSource", 0, "constant"),
    ("createScriptProcessor", 3, "scriptprocessor"),
    ("createJavaScriptNode", 3, "scriptprocessor"),
    ("createPeriodicWave", 2, "periodicwave"),
];

fn shape_ok(channels: i32, length: i32, sample_rate: f64) -> bool {
    (1..=MAX_CHANNELS).contains(&channels)
        && length >= 1
        && channels as u64 * length as u64 <= MAX_SAMPLES
        && (3000.0..=768000.0).contains(&sample_rate)
}

fn return_this(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(this.clone())
}

fn noop(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

fn resolved_undefined(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    resolved(scope, Value::undefined())
}

fn rejected_not_supported(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let error = scope.new_error();
    set_str(scope, &error, "message", "not supported");
    rejected(scope, error)
}

fn analysis_throw(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Err(defined_error(
        scope,
        "NotSupportedError",
        "Southstar does not implement Web Audio analysis",
        Some(9),
    ))
}

fn is_mic(scope: &mut Scope<'_>, node: &Value) -> bool {
    truthy(scope, node, "_micSrc") || truthy(scope, node, "_micTaint")
}

fn fill_bytes(scope: &mut Scope<'_>, this: &Value, args: &[Value], mic: fn(&mut [u8]), idle: u8) {
    let mic_source = is_mic(scope, this);
    let Some(target) = args.first() else {
        return;
    };
    if scope.typed_array_element(target).is_none() {
        return;
    }
    scope.with_buffer_bytes_mut(target, |bytes| {
        if mic_source {
            mic(bytes);
        } else {
            bytes.fill(idle);
        }
    });
}

fn byte_time_domain(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    fill_bytes(scope, this, args, ffi::mic_time_domain, 128);
    Ok(Value::undefined())
}

fn byte_frequency(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    fill_bytes(scope, this, args, ffi::mic_frequency, 0);
    Ok(Value::undefined())
}

fn connect(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(target) = args
        .first()
        .filter(|target| !target.is_undefined() && !target.is_null())
    else {
        return Ok(this.clone());
    };
    if target.is_object() {
        if is_mic(scope, this) {
            set(scope, target, "_micTaint", Value::boolean(true));
        }
        let inputs = get(scope, target, "_inputs");
        if inputs.is_object() {
            let count = array_length(scope, &inputs);
            let _ = scope.set_index(&inputs, count, this.clone());
        }
    }
    Ok(target.clone())
}

fn disconnect(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(target) = args.first().filter(|target| target.is_object()) else {
        return Ok(Value::undefined());
    };
    let inputs = get(scope, target, "_inputs");
    if inputs.is_object() {
        let count = array_length(scope, &inputs);
        let kept = scope.new_array();
        let mut next = 0;
        for index in 0..count {
            let source = scope
                .get_index(&inputs, index)
                .unwrap_or_else(|_| Value::undefined());
            if !source.same_object(this) {
                let _ = scope.set_index(&kept, next, source);
                next += 1;
            }
        }
        set(scope, target, "_inputs", kept);
    }
    Ok(Value::undefined())
}

fn schedule(scope: &mut Scope<'_>, this: &Value, args: &[Value], field: &str) -> JsResult {
    let when = match args.first() {
        Some(when) if !when.is_undefined() => scope.to_number(when).unwrap_or(0.0),
        _ => 0.0,
    };
    let when = if when >= 0.0 { when } else { 0.0 };
    set(scope, this, field, Value::number(when));
    Ok(Value::undefined())
}

fn start(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    schedule(scope, this, args, "_startTime")
}

fn stop(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    schedule(scope, this, args, "_stopTime")
}

fn make_param(scope: &mut Scope<'_>, value: f64) -> Value {
    let param = scope.new_object();
    set(scope, &param, "value", Value::number(value));
    set(scope, &param, "defaultValue", Value::number(value));
    set(scope, &param, "minValue", Value::number(-3.4e38));
    set(scope, &param, "maxValue", Value::number(3.4e38));
    for (name, arity) in PARAM_METHODS {
        bind(scope, &param, name, arity, return_this);
    }
    param
}

fn make_node(scope: &mut Scope<'_>, kind: &str) -> Value {
    let node = scope.new_object();
    set(scope, &node, "numberOfInputs", Value::int(1));
    set(scope, &node, "numberOfOutputs", Value::int(1));
    set(scope, &node, "channelCount", Value::int(2));
    set_str(scope, &node, "channelCountMode", "max");
    set_str(scope, &node, "channelInterpretation", "speakers");
    for (name, value) in NODE_PARAMS {
        let param = make_param(scope, value);
        set(scope, &node, name, param);
    }
    set(scope, &node, "buffer", Value::null());
    set(scope, &node, "loop", Value::boolean(false));
    let wave = if kind == "biquad" { "lowpass" } else { "sine" };
    set_str(scope, &node, "type", wave);
    set(scope, &node, "fftSize", Value::int(2048));
    set(scope, &node, "frequencyBinCount", Value::int(1024));
    for (name, value) in SHAPING_PARAMS {
        let param = make_param(scope, value);
        set(scope, &node, name, param);
    }
    set(scope, &node, "curve", Value::null());
    set_str(scope, &node, "_kind", kind);
    let inputs = scope.new_array();
    set(scope, &node, "_inputs", inputs);
    let listeners = scope.new_array();
    set(scope, &node, "_listeners", listeners);
    bind(scope, &node, "connect", 1, connect);
    bind(scope, &node, "disconnect", 0, disconnect);
    bind(scope, &node, "start", 1, start);
    bind(scope, &node, "stop", 1, stop);
    bind(scope, &node, "getByteFrequencyData", 1, byte_frequency);
    bind(scope, &node, "getByteTimeDomainData", 1, byte_time_domain);
    bind(scope, &node, "getFloatFrequencyData", 1, analysis_throw);
    bind(scope, &node, "getFloatTimeDomainData", 1, analysis_throw);
    ffi::bind_event_target(scope, &node);
    if let Some((_, interface)) = NODE_INTERFACES.iter().find(|(name, _)| *name == kind) {
        let _ = scope.define_to_string_tag(&node, interface);
    }
    node
}

fn create_node(scope: &mut Scope<'_>, _: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let kind = match data.first() {
        Some(kind) => scope.to_string(kind)?,
        None => "gain".to_owned(),
    };
    Ok(make_node(scope, &kind))
}

fn create_media_stream_source(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let node = make_node(scope, "mediastreamsource");
    if args.first().is_some_and(Value::is_object) {
        set(scope, &node, "_micSrc", Value::boolean(true));
    }
    Ok(node)
}

fn new_float32_array(scope: &mut Scope<'_>, length: i32) -> JsResult {
    let global = scope.global();
    let constructor = get(scope, &global, "Float32Array");
    if constructor.is_object() {
        scope.construct(&constructor, &[Value::int(length)])
    } else {
        Ok(scope.new_array())
    }
}

fn make_buffer(scope: &mut Scope<'_>, channels: i32, length: i32, sample_rate: f64) -> Value {
    let channels = channels.max(1);
    let length = length.max(1);
    let sample_rate = if sample_rate > 0.0 {
        sample_rate
    } else {
        DEFAULT_RATE
    };
    let buffer = scope.new_object();
    set(scope, &buffer, "numberOfChannels", Value::int(channels));
    set(scope, &buffer, "length", Value::int(length));
    set(scope, &buffer, "sampleRate", Value::number(sample_rate));
    let duration = f64::from(length) / sample_rate;
    set(scope, &buffer, "duration", Value::number(duration));
    let data = scope.new_array();
    for channel in 0..channels {
        let array = new_float32_array(scope, length).unwrap_or_else(|_| scope.new_array());
        let _ = scope.set_index(&data, channel as u32, array);
    }
    set(scope, &buffer, "_chans", data);
    let _ = scope.define_to_string_tag(&buffer, "AudioBuffer");
    bind(scope, &buffer, "copyFromChannel", 3, noop);
    bind(scope, &buffer, "copyToChannel", 3, noop);
    bind(scope, &buffer, "getChannelData", 1, channel_data);
    buffer
}

fn channel_data(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let channel = match args.first() {
        Some(channel) => scope.to_int32(channel)?,
        None => 0,
    };
    let channel = channel.max(0);
    let data = get(scope, this, "_chans");
    if data.is_object() {
        let array = scope
            .get_index(&data, channel as u32)
            .unwrap_or_else(|_| Value::undefined());
        if array.is_object() {
            return Ok(array);
        }
    }
    let length = get(scope, this, "length");
    let length = scope.to_int32(&length).unwrap_or(0).max(0);
    new_float32_array(scope, length)
}

fn create_buffer(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let channels = match args.first() {
        Some(value) => scope.to_int32(value)?,
        None => 1,
    };
    let length = match args.get(1) {
        Some(value) => scope.to_int32(value)?,
        None => 1,
    };
    let sample_rate = match args.get(2) {
        Some(value) => scope.to_number(value)?,
        None => DEFAULT_RATE,
    };
    if !shape_ok(channels, length, sample_rate) {
        return Err(dom_exception(
            scope,
            "NotSupportedError",
            "AudioBuffer channel count, length or sample rate out of range",
        ));
    }
    Ok(make_buffer(scope, channels, length, sample_rate))
}

fn decode_audio_data(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let buffer = create_buffer(scope, &Value::undefined(), &[])?;
    if let Some(success) = args.get(1)
        && scope.is_function(success)
    {
        let _ = scope.call(success, &Value::undefined(), core::slice::from_ref(&buffer));
    }
    resolved(scope, buffer)
}

fn build_context(scope: &mut Scope<'_>, sample_rate: f64) -> Value {
    let sample_rate = if sample_rate > 0.0 {
        sample_rate
    } else {
        DEFAULT_RATE
    };
    let context = scope.new_object();
    set(scope, &context, "sampleRate", Value::number(sample_rate));
    set(scope, &context, "currentTime", Value::number(0.0));
    set_str(scope, &context, "state", "running");
    set(scope, &context, "baseLatency", Value::number(0.02));
    set(scope, &context, "outputLatency", Value::number(0.02));
    let destination = make_node(scope, "destination");
    set(scope, &context, "destination", destination);
    let listener = scope.new_object();
    set(scope, &context, "listener", listener);
    set(scope, &context, "onstatechange", Value::null());
    let listeners = scope.new_array();
    set(scope, &context, "_listeners", listeners);
    let worklet = scope.new_object();
    bind(scope, &worklet, "addModule", 1, rejected_not_supported);
    set(scope, &context, "audioWorklet", worklet);
    bind(scope, &context, "resume", 0, resolved_undefined);
    bind(scope, &context, "suspend", 0, resolved_undefined);
    bind(scope, &context, "close", 0, resolved_undefined);
    bind(scope, &context, "createBuffer", 3, create_buffer);
    bind(scope, &context, "decodeAudioData", 3, decode_audio_data);
    ffi::bind_event_target(scope, &context);
    for (name, arity, kind) in NODE_FACTORIES {
        let kind = scope.string(kind);
        let factory = scope.bound_function(name, arity, create_node, &[kind]);
        set(scope, &context, name, factory);
    }
    bind(
        scope,
        &context,
        "createMediaStreamSource",
        1,
        create_media_stream_source,
    );
    let _ = scope.define_to_string_tag(&context, "AudioContext");
    context
}

fn audio_context(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(build_context(scope, DEFAULT_RATE))
}

fn copy_into_channels(scope: &mut Scope<'_>, buffer: &Value, channels: i32, mix: &[f32]) {
    let data = get(scope, buffer, "_chans");
    let samples: Vec<u8> = mix.iter().flat_map(|sample| sample.to_ne_bytes()).collect();
    for channel in 0..channels {
        let array = scope
            .get_index(&data, channel as u32)
            .unwrap_or_else(|_| Value::undefined());
        if scope.typed_array_element(&array) != Some(ElementType::Float32) {
            continue;
        }
        scope.with_buffer_bytes_mut(&array, |bytes| {
            let count = bytes.len().min(samples.len());
            bytes[..count].copy_from_slice(&samples[..count]);
        });
    }
}

fn int_field(scope: &mut Scope<'_>, object: &Value, key: &str) -> i32 {
    let value = get(scope, object, key);
    scope.to_int32(&value).unwrap_or(0)
}

fn start_rendering(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let channels = int_field(scope, this, "_oacChannels");
    let length = int_field(scope, this, "length");
    let rate = get(scope, this, "sampleRate");
    let sample_rate = scope.to_number(&rate).unwrap_or(f64::NAN);
    if !shape_ok(channels, length, sample_rate) {
        let error = defined_error(
            scope,
            "NotSupportedError",
            "OfflineAudioContext channel count, length or sample rate out of range",
            None,
        );
        return rejected(scope, error);
    }
    let mut mix = Vec::new();
    if mix.try_reserve_exact(length as usize).is_err() {
        let error = defined_error(
            scope,
            "NotSupportedError",
            "OfflineAudioContext rendering buffer could not be allocated",
            None,
        );
        return rejected(scope, error);
    }
    mix.resize(length as usize, 0.0f32);
    let buffer = make_buffer(scope, channels, length, sample_rate);
    let destination = get(scope, this, "destination");
    if southstar_webaudio::render_offline(scope, &destination, sample_rate, &mut mix) {
        copy_into_channels(scope, &buffer, channels, &mix);
    }
    set_str(scope, this, "state", "closed");
    let oncomplete = get(scope, this, "oncomplete");
    if scope.is_function(&oncomplete) {
        let event = ffi::new_event(scope);
        ffi::adopt_interface(scope, &event, "OfflineAudioCompletionEvent");
        set_str(scope, &event, "type", "complete");
        set(scope, &event, "renderedBuffer", buffer.clone());
        let _ = scope.call(&oncomplete, this, &[event]);
    }
    resolved(scope, buffer)
}

fn offline_audio_context(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let (mut channels, mut length, mut sample_rate) = (1, 1, DEFAULT_RATE);
    match args {
        [options] if options.is_object() && !scope.is_function(options) => {
            let value = get(scope, options, "numberOfChannels");
            if !value.is_undefined() {
                channels = scope.to_int32(&value)?;
            }
            let value = get(scope, options, "length");
            if !value.is_undefined() {
                length = scope.to_int32(&value)?;
            }
            let value = get(scope, options, "sampleRate");
            if !value.is_undefined() {
                sample_rate = scope.to_number(&value)?;
            }
        }
        _ => {
            if let Some(value) = args.first() {
                channels = scope.to_int32(value)?;
            }
            if let Some(value) = args.get(1) {
                length = scope.to_int32(value)?;
            }
            if let Some(value) = args.get(2) {
                sample_rate = scope.to_number(value)?;
            }
        }
    }
    if !shape_ok(channels, length, sample_rate) {
        return Err(dom_exception(
            scope,
            "NotSupportedError",
            "OfflineAudioContext channel count, length or sample rate out of range",
        ));
    }
    let context = build_context(scope, sample_rate);
    set_str(scope, &context, "state", "suspended");
    set(scope, &context, "length", Value::int(length));
    set(scope, &context, "_oacChannels", Value::int(channels));
    set(scope, &context, "oncomplete", Value::null());
    bind(scope, &context, "startRendering", 0, start_rendering);
    let _ = scope.define_to_string_tag(&context, "OfflineAudioContext");
    Ok(context)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let realtime = make_ctor(scope, "AudioContext", 1, audio_context);
    set(scope, global, "AudioContext", realtime.clone());
    set(scope, global, "webkitAudioContext", realtime);
    let offline = make_ctor(scope, "OfflineAudioContext", 3, offline_audio_context);
    set(scope, global, "OfflineAudioContext", offline.clone());
    set(scope, global, "webkitOfflineAudioContext", offline);
}
