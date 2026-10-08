//! Southstar — offline Web Audio rendering: walks the AudioNode graph of an OfflineAudioContext and mixes it into one channel.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "quickjs")]
mod ffi;

use core::f64::consts::PI;

use southstar_js_engine::{Scope, Value};

const MAX_DEPTH: i32 = 32;
const MAX_NODE_RENDERS: u32 = 4096;

struct Walk {
    depth: i32,
    renders_left: u32,
}

pub(crate) fn positive(x: f64) -> bool {
    x > 0.0
}

fn c_trunc_u32(d: f64) -> u32 {
    if cfg!(any(target_arch = "x86", target_arch = "x86_64")) {
        let wide = if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&d) {
            d as i64
        } else {
            i64::MIN
        };
        return wide as u32;
    }
    d as u32
}

fn number(scope: &mut Scope, object: &Value, name: &str, default: f64) -> f64 {
    match scope.get(object, name) {
        Ok(value) if !value.is_undefined() && !value.is_null() => {
            scope.to_number(&value).unwrap_or(default)
        }
        _ => default,
    }
}

fn param(scope: &mut Scope, node: &Value, name: &str, default: f64) -> f64 {
    match scope.get(node, name) {
        Ok(param) if param.is_object() => number(scope, &param, "value", default),
        _ => default,
    }
}

fn string(scope: &mut Scope, object: &Value, name: &str) -> Vec<u8> {
    let mut text = scope
        .get(object, name)
        .and_then(|value| scope.to_bytes(&value))
        .unwrap_or_default();
    if let Some(nul) = text.iter().position(|&b| b == 0) {
        text.truncate(nul);
    }
    text
}

struct Samples<'a>(&'a [u8]);

impl Samples<'_> {
    fn len(&self) -> u32 {
        (self.0.len() / 4) as u32
    }

    fn get(&self, index: u32) -> f32 {
        let at = index as usize * 4;
        f32::from_ne_bytes([self.0[at], self.0[at + 1], self.0[at + 2], self.0[at + 3]])
    }
}

fn with_float32<R>(scope: &mut Scope, array: &Value, f: impl FnOnce(&Samples) -> R) -> Option<R> {
    if !array.is_object() {
        return None;
    }
    scope
        .with_typed_array(array, |view| {
            (view.element_size == 4 && view.byte_offset % 4 == 0).then(|| f(&Samples(view.bytes)))
        })
        .flatten()
}

fn window(scope: &mut Scope, node: &Value, frames: u32, rate: f64) -> (u32, u32) {
    let start = number(scope, node, "_startTime", -1.0);
    let stop = number(scope, node, "_stopTime", -1.0);
    let mut first = 0;
    let mut last = frames;
    if start > 0.0 {
        let f = start * rate;
        first = if f >= f64::from(frames) {
            frames
        } else {
            c_trunc_u32(f)
        };
    }
    if stop >= 0.0 {
        let f = stop * rate;
        let end = if f >= f64::from(frames) {
            frames
        } else {
            c_trunc_u32(f)
        };
        last = last.min(end);
    }
    (first, last.max(first))
}

fn oscillator(scope: &mut Scope, node: &Value, frames: u32, rate: f64, out: &mut [f32]) {
    let frequency = param(scope, node, "frequency", 440.0);
    let detune = param(scope, node, "detune", 0.0);
    let mut f = frequency * 2.0f64.powf(detune / 1200.0);
    if !positive(f) || f > rate * 0.5 {
        f = if f > 0.0 { rate * 0.5 } else { 0.0 };
    }
    let wave = string(scope, node, "type");
    let (first, last) = window(scope, node, frames, rate);
    let step = f / rate;
    let mut phase = 0.0;
    for sample in &mut out[first as usize..last as usize] {
        let v = match wave.as_slice() {
            b"square" => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            b"sawtooth" => 2.0 * phase - 1.0,
            b"triangle" => {
                if phase < 0.5 {
                    4.0 * phase - 1.0
                } else {
                    3.0 - 4.0 * phase
                }
            }
            _ => (2.0 * PI * phase).sin(),
        };
        *sample = v as f32;
        phase += step;
        if phase >= 1.0 {
            phase -= phase.floor();
        }
    }
}

fn buffer_source(scope: &mut Scope, node: &Value, frames: u32, rate: f64, out: &mut [f32]) {
    let Ok(buffer) = scope.get(node, "buffer") else {
        return;
    };
    if !buffer.is_object() {
        return;
    }
    let mut ratio = param(scope, node, "playbackRate", 1.0);
    let buffer_rate = number(scope, &buffer, "sampleRate", rate);
    if !positive(ratio) {
        ratio = 1.0;
    }
    let mut step = ratio
        * if buffer_rate > 0.0 {
            buffer_rate / rate
        } else {
            1.0
        };
    if !step.is_finite() {
        step = 1.0;
    }
    let looping = match scope.get(node, "loop") {
        Ok(value) => scope.to_bool(&value),
        Err(_) => true,
    };
    let (first, last) = window(scope, node, frames, rate);
    let Ok(channel) = scope
        .get(&buffer, "_chans")
        .and_then(|chans| scope.get_index(&chans, 0))
    else {
        return;
    };
    with_float32(scope, &channel, |samples| {
        let n = samples.len();
        if n == 0 {
            return;
        }
        let mut pos = 0.0;
        for sample in &mut out[first as usize..last as usize] {
            if pos >= f64::from(n) {
                if !looping {
                    break;
                }
                pos %= f64::from(n);
            }
            *sample = samples.get(pos as u32);
            pos += step;
        }
    });
}

fn sum_inputs(
    scope: &mut Scope,
    node: &Value,
    frames: u32,
    rate: f64,
    out: &mut [f32],
    walk: &mut Walk,
) {
    let Ok(inputs) = scope.get(node, "_inputs") else {
        return;
    };
    if !inputs.is_object() {
        return;
    }
    let count = match scope.get(&inputs, "length") {
        Ok(length) => scope.to_int32(&length).unwrap_or(0) as u32,
        Err(_) => 0,
    };
    if count == 0 {
        return;
    }
    let mut scratch = Vec::new();
    if scratch.try_reserve_exact(frames as usize).is_err() {
        return;
    }
    scratch.resize(frames as usize, 0.0f32);
    walk.depth += 1;
    let mut index = 0;
    while index < count && walk.renders_left > 0 {
        if let Ok(source) = scope.get_index(&inputs, index) {
            if source.is_object() {
                scratch.fill(0.0);
                render(scope, &source, frames, rate, &mut scratch, walk);
                for (sample, input) in out.iter_mut().zip(&scratch) {
                    *sample += input;
                }
            }
        }
        index += 1;
    }
    walk.depth -= 1;
}

fn compressor(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32]) {
    let threshold = param(scope, node, "threshold", -24.0);
    let mut knee = param(scope, node, "knee", 30.0);
    let mut ratio = param(scope, node, "ratio", 12.0);
    let attack = param(scope, node, "attack", 0.003);
    let release = param(scope, node, "release", 0.25);
    if ratio < 1.0 {
        ratio = 1.0;
    }
    if knee < 0.0 {
        knee = 0.0;
    }
    let atk = if attack > 0.0 {
        (-1.0 / (attack * rate)).exp()
    } else {
        0.0
    };
    let rel = if release > 0.0 {
        (-1.0 / (release * rate)).exp()
    } else {
        0.0
    };
    let mut env = 0.0;
    for sample in out.iter_mut() {
        let x = f64::from(*sample).abs();
        let db = if x > 1e-9 { 20.0 * x.log10() } else { -180.0 };
        let over = db - threshold;
        let reduction = if knee > 0.0 && over > -knee * 0.5 && over < knee * 0.5 {
            let t = over + knee * 0.5;
            (1.0 / ratio - 1.0) * t * t / (2.0 * knee)
        } else if over <= 0.0 {
            0.0
        } else {
            over * (1.0 / ratio - 1.0)
        };
        let coeff = if reduction < env { atk } else { rel };
        env = coeff * env + (1.0 - coeff) * reduction;
        *sample = (f64::from(*sample) * 10.0f64.powf(env / 20.0)) as f32;
    }
}

fn biquad(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32]) {
    let mut f0 = param(scope, node, "frequency", 350.0);
    let mut q = param(scope, node, "Q", 1.0);
    let gain_db = param(scope, node, "gain", 0.0);
    let kind = string(scope, node, "type");
    if !positive(f0) {
        f0 = 350.0;
    }
    if f0 > rate * 0.5 {
        f0 = rate * 0.5;
    }
    if !positive(q) {
        q = 1e-4;
    }
    let w0 = 2.0 * PI * f0 / rate;
    let (cw, sw) = (w0.cos(), w0.sin());
    let alpha = sw / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = match kind.as_slice() {
        b"highpass" => (
            (1.0 + cw) / 2.0,
            -(1.0 + cw),
            (1.0 + cw) / 2.0,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        ),
        b"bandpass" => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
        b"notch" => (1.0, -2.0 * cw, 1.0, 1.0 + alpha, -2.0 * cw, 1.0 - alpha),
        b"allpass" => (
            1.0 - alpha,
            -2.0 * cw,
            1.0 + alpha,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        ),
        b"peaking" => {
            let a = 10.0f64.powf(gain_db / 40.0);
            (
                1.0 + alpha * a,
                -2.0 * cw,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cw,
                1.0 - alpha / a,
            )
        }
        _ => (
            (1.0 - cw) / 2.0,
            1.0 - cw,
            (1.0 - cw) / 2.0,
            1.0 + alpha,
            -2.0 * cw,
            1.0 - alpha,
        ),
    };
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for sample in out.iter_mut() {
        let x = f64::from(*sample);
        let y = (b0 / a0) * x + (b1 / a0) * x1 + (b2 / a0) * x2 - (a1 / a0) * y1 - (a2 / a0) * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
        *sample = y as f32;
    }
}

fn delay(scope: &mut Scope, node: &Value, rate: f64, out: &mut [f32]) {
    let t = param(scope, node, "delayTime", 0.0);
    if !positive(t) {
        return;
    }
    let d = c_trunc_u32(t * rate) as usize;
    if d == 0 {
        return;
    }
    if d >= out.len() {
        out.fill(0.0);
        return;
    }
    out.copy_within(..out.len() - d, d);
    out[..d].fill(0.0);
}

fn waveshaper(scope: &mut Scope, node: &Value, out: &mut [f32]) {
    let Ok(curve) = scope.get(node, "curve") else {
        return;
    };
    with_float32(scope, &curve, |curve| {
        let n = curve.len();
        if n < 2 {
            return;
        }
        for sample in out.iter_mut() {
            let x = f64::from(*sample).clamp(-1.0, 1.0);
            let pos = (x + 1.0) * 0.5 * f64::from(n - 1);
            let k = c_trunc_u32(pos);
            if k >= n - 1 {
                *sample = curve.get(n - 1);
                continue;
            }
            let frac = pos - f64::from(k);
            *sample = (f64::from(curve.get(k)) * (1.0 - frac) + f64::from(curve.get(k + 1)) * frac)
                as f32;
        }
    });
}

fn scale(out: &mut [f32], gain: f64) {
    for sample in out.iter_mut() {
        *sample = (f64::from(*sample) * gain) as f32;
    }
}

fn render(
    scope: &mut Scope,
    node: &Value,
    frames: u32,
    rate: f64,
    out: &mut [f32],
    walk: &mut Walk,
) {
    if walk.depth > MAX_DEPTH || walk.renders_left == 0 || !node.is_object() {
        return;
    }
    walk.renders_left -= 1;
    let kind = string(scope, node, "_kind");
    match kind.as_slice() {
        b"oscillator" => oscillator(scope, node, frames, rate, out),
        b"buffersource" => buffer_source(scope, node, frames, rate, out),
        b"constant" => {
            let v = param(scope, node, "offset", 1.0);
            let (first, last) = window(scope, node, frames, rate);
            out[first as usize..last as usize].fill(v as f32);
        }
        _ => {
            sum_inputs(scope, node, frames, rate, out, walk);
            match kind.as_slice() {
                b"gain" => {
                    let gain = param(scope, node, "gain", 1.0);
                    scale(out, gain);
                }
                b"compressor" => compressor(scope, node, rate, out),
                b"biquad" => biquad(scope, node, rate, out),
                b"delay" => delay(scope, node, rate, out),
                b"waveshaper" => waveshaper(scope, node, out),
                b"stereopanner" | b"panner" => {
                    let gain = 1.0 - param(scope, node, "pan", 0.0).abs() * 0.5;
                    scale(out, gain);
                }
                _ => {}
            }
        }
    }
}

pub fn render_offline(scope: &mut Scope, destination: &Value, rate: f64, out: &mut [f32]) -> bool {
    out.fill(0.0);
    if !destination.is_object() {
        return false;
    }
    let mut walk = Walk {
        depth: 0,
        renders_left: MAX_NODE_RENDERS,
    };
    render(scope, destination, out.len() as u32, rate, out, &mut walk);
    for sample in out.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }
    true
}
