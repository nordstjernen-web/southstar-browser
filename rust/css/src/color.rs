//! Southstar — CSS colour values parsed to RGBA: hex, named and system colours, rgb(), hsl(), hwb(), lab(), lch(), oklab(), oklch(), color-mix(), light-dark() and calc() inside them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;
use std::ffi::{CStr, CString};

use crate::ffi;
use crate::scan::{
    byte, is_ident, is_ws, match_close_paren, skip_ws, split_args, split_ws_limit, starts_with_ci,
};

pub(crate) type Channels = [Option<u8>; 4];

const MAX_CALC_DEPTH: i32 = 64;
const MAX_COLOR_DEPTH: i32 = 32;

const NAMED: &[(&str, [u8; 3])] = &[
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgrey", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgrey", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
    ("transparent", [0, 0, 0]),
    ("accentcolor", [0, 120, 215]),
    ("accentcolortext", [255, 255, 255]),
    ("activetext", [255, 0, 0]),
    ("buttonborder", [140, 140, 140]),
    ("buttonface", [240, 240, 240]),
    ("buttontext", [0, 0, 0]),
    ("canvas", [255, 255, 255]),
    ("canvastext", [0, 0, 0]),
    ("field", [255, 255, 255]),
    ("fieldtext", [0, 0, 0]),
    ("graytext", [128, 128, 128]),
    ("highlight", [51, 153, 255]),
    ("highlighttext", [255, 255, 255]),
    ("linktext", [0, 0, 238]),
    ("mark", [255, 255, 0]),
    ("marktext", [0, 0, 0]),
    ("selecteditem", [51, 153, 255]),
    ("selecteditemtext", [255, 255, 255]),
    ("visitedtext", [85, 26, 139]),
    ("window", [255, 255, 255]),
    ("windowtext", [0, 0, 0]),
    ("activeborder", [140, 140, 140]),
    ("activecaption", [255, 255, 255]),
    ("appworkspace", [255, 255, 255]),
    ("background", [255, 255, 255]),
    ("buttonhighlight", [240, 240, 240]),
    ("buttonshadow", [240, 240, 240]),
    ("captiontext", [0, 0, 0]),
    ("inactiveborder", [140, 140, 140]),
    ("inactivecaption", [255, 255, 255]),
    ("inactivecaptiontext", [128, 128, 128]),
    ("infobackground", [255, 255, 255]),
    ("infotext", [0, 0, 0]),
    ("menu", [255, 255, 255]),
    ("menutext", [0, 0, 0]),
    ("scrollbar", [255, 255, 255]),
    ("threeddarkshadow", [140, 140, 140]),
    ("threedface", [140, 140, 140]),
    ("threedhighlight", [140, 140, 140]),
    ("threedlightshadow", [140, 140, 140]),
    ("threedshadow", [140, 140, 140]),
    ("windowframe", [140, 140, 140]),
];

pub fn parse_color(text: &CStr) -> Option<[u8; 4]> {
    let mut out = [None; 4];
    parse_into(text, &mut out).then(|| out.map(|channel| channel.unwrap_or(0)))
}

pub(crate) fn parse_into(text: &CStr, out: &mut Channels) -> bool {
    parse_depth(text, out, 0)
}

fn gclamp(value: f64, low: f64, high: f64) -> f64 {
    if value > high {
        high
    } else if value < low {
        low
    } else {
        value
    }
}

fn channel(value: f64) -> u8 {
    ((value + 0.5) as i32).clamp(0, 255) as u8
}

fn set_rgb(out: &mut Channels, rgb: [f64; 3]) {
    for (slot, value) in out.iter_mut().zip(rgb) {
        *slot = Some(channel(value * 255.0));
    }
}

fn opening(s: &[u8]) -> Option<usize> {
    s.iter().position(|&c| c == b'(').map(|open| open + 1)
}

fn none_at(s: &[u8], p: usize) -> bool {
    starts_with_ci(&s[p..], b"none") && !is_ident(byte(s, p + 4))
}

fn angle_degrees(s: &[u8], value: f64, end: &mut usize) -> f64 {
    let rest = &s[*end..];
    let unit = |name: &[u8]| starts_with_ci(rest, name) && !is_ident(byte(rest, name.len()));
    if unit(b"deg") {
        *end += 3;
        value
    } else if unit(b"turn") {
        *end += 4;
        value * 360.0
    } else if unit(b"grad") {
        *end += 4;
        value * 0.9
    } else if unit(b"rad") {
        *end += 3;
        value * 180.0 / PI
    } else {
        value
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Component {
    Rgb,
    Hsl,
    Hwb,
}

struct Components {
    values: [f64; 4],
    percent: [bool; 4],
    count: usize,
}

fn read_components(text: &CStr, open: usize, kind: Component) -> Option<Components> {
    let s = text.to_bytes();
    let mut parsed = Components {
        values: [0.0, 0.0, 0.0, 1.0],
        percent: [false; 4],
        count: 0,
    };
    let mut p = open;
    while byte(s, p) != 0 && byte(s, p) != b')' && parsed.count < 4 {
        while matches!(byte(s, p), b' ' | b',' | b'/') {
            p += 1;
        }
        if byte(s, p) == 0 || byte(s, p) == b')' {
            break;
        }
        let count = parsed.count;
        if none_at(s, p) {
            parsed.values[count] = if count == 3 { 1.0 } else { 0.0 };
            parsed.count += 1;
            p += 4;
            continue;
        }
        let (mut value, mut end) = ffi::strtod(text, p);
        if end == p {
            return None;
        }
        match kind {
            Component::Rgb => {
                if byte(s, end) == b'%' {
                    parsed.percent[count] = true;
                    end += 1;
                }
            }
            Component::Hsl | Component::Hwb if count == 0 => {
                value = angle_degrees(s, value, &mut end);
                if is_ident(byte(s, end)) {
                    return None;
                }
            }
            Component::Hsl if count < 3 => {
                if byte(s, end) != b'%' {
                    return None;
                }
                end += 1;
            }
            Component::Hsl => {
                if byte(s, end) == b'%' {
                    parsed.percent[count] = true;
                    end += 1;
                }
            }
            Component::Hwb => {
                if byte(s, end) == b'%' {
                    parsed.percent[count] = true;
                    end += 1;
                } else if is_ident(byte(s, end)) {
                    return None;
                }
            }
        }
        parsed.values[count] = value;
        parsed.count += 1;
        p = end;
    }
    (parsed.count >= 3).then_some(parsed)
}

fn alpha_channel(parsed: &Components) -> u8 {
    if parsed.count == 4 {
        let alpha = if parsed.percent[3] {
            parsed.values[3] / 100.0
        } else {
            parsed.values[3]
        };
        channel(alpha * 255.0)
    } else {
        255
    }
}

fn rgb_function(text: &CStr, out: &mut Channels) -> bool {
    let s = text.to_bytes();
    if !starts_with_ci(s, b"rgba(") && !starts_with_ci(s, b"rgb(") {
        return false;
    }
    let Some(open) = opening(s) else { return false };
    let Some(parsed) = read_components(text, open, Component::Rgb) else {
        return false;
    };
    for (i, slot) in out.iter_mut().take(3).enumerate() {
        let value = if parsed.percent[i] {
            parsed.values[i] * 255.0 / 100.0
        } else {
            parsed.values[i]
        };
        *slot = Some(channel(value));
    }
    out[3] = Some(alpha_channel(&parsed));
    true
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

fn hue_turns(degrees: f64) -> f64 {
    let h = degrees / 360.0;
    if h.is_finite() { h - h.floor() } else { 0.0 }
}

fn hsl_function(text: &CStr, out: &mut Channels) -> bool {
    let s = text.to_bytes();
    if !starts_with_ci(s, b"hsla(") && !starts_with_ci(s, b"hsl(") {
        return false;
    }
    let Some(open) = opening(s) else { return false };
    let Some(parsed) = read_components(text, open, Component::Hsl) else {
        return false;
    };
    let h = hue_turns(parsed.values[0]);
    let sat = (parsed.values[1] / 100.0).clamp(0.0, 1.0);
    let light = (parsed.values[2] / 100.0).clamp(0.0, 1.0);
    let rgb = if sat == 0.0 {
        [light; 3]
    } else {
        let q = if light < 0.5 {
            light * (1.0 + sat)
        } else {
            light + sat - light * sat
        };
        let p = 2.0 * light - q;
        [
            hue_to_rgb(p, q, h + 1.0 / 3.0),
            hue_to_rgb(p, q, h),
            hue_to_rgb(p, q, h - 1.0 / 3.0),
        ]
    };
    set_rgb(out, rgb);
    out[3] = Some(alpha_channel(&parsed));
    true
}

fn hwb_function(text: &CStr, out: &mut Channels) -> bool {
    let s = text.to_bytes();
    if !starts_with_ci(s, b"hwb(") {
        return false;
    }
    let Some(open) = opening(s) else { return false };
    let Some(parsed) = read_components(text, open, Component::Hwb) else {
        return false;
    };
    let h = hue_turns(parsed.values[0]);
    let white = gclamp(parsed.values[1] / 100.0, 0.0, 1.0);
    let black = gclamp(parsed.values[2] / 100.0, 0.0, 1.0);
    let mut rgb = [
        hue_to_rgb(0.0, 1.0, h + 1.0 / 3.0),
        hue_to_rgb(0.0, 1.0, h),
        hue_to_rgb(0.0, 1.0, h - 1.0 / 3.0),
    ];
    let sum = white + black;
    if sum >= 1.0 {
        rgb = [if sum > 0.0 { white / sum } else { 0.0 }; 3];
    } else {
        let scale = 1.0 - white - black;
        for value in &mut rgb {
            *value = *value * scale + white;
        }
    }
    set_rgb(out, rgb);
    out[3] = Some(alpha_channel(&parsed));
    true
}

fn srgb_encode_linear(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_decode_gamma(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn oklab_to_srgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let lp = l + 0.3963377774 * a + 0.2158037573 * b;
    let mp = l - 0.1055613458 * a - 0.0638541728 * b;
    let sp = l - 0.0894841775 * a - 1.2914855480 * b;
    let ll = lp * lp * lp;
    let mm = mp * mp * mp;
    let ss = sp * sp * sp;
    let r = 4.0767416621 * ll - 3.3077115913 * mm + 0.2309699292 * ss;
    let g = -1.2684380046 * ll + 2.6097574011 * mm - 0.3413193965 * ss;
    let b = -0.0041960863 * ll - 0.7034186147 * mm + 1.7076147010 * ss;
    [
        srgb_encode_linear(r),
        srgb_encode_linear(g),
        srgb_encode_linear(b),
    ]
}

fn srgb_to_oklab(rgb: [u8; 3]) -> [f64; 3] {
    let r = srgb_decode_gamma(f64::from(rgb[0]) / 255.0);
    let g = srgb_decode_gamma(f64::from(rgb[1]) / 255.0);
    let b = srgb_decode_gamma(f64::from(rgb[2]) / 255.0);
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let (lp, mp, sp) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.2104542553 * lp + 0.7936177850 * mp - 0.0040720468 * sp,
        1.9779984951 * lp - 2.4285922050 * mp + 0.4505937099 * sp,
        0.0259040371 * lp + 0.7827717662 * mp - 0.8086757660 * sp,
    ]
}

fn lab_inv_f(t: f64) -> f64 {
    let t3 = t * t * t;
    if t3 > 0.008856451679 {
        t3
    } else {
        (116.0 * t - 16.0) / 903.2962963
    }
}

fn lab_to_srgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let x50 = 0.96422 * lab_inv_f(fx);
    let y50 = lab_inv_f(fy);
    let z50 = 0.82521 * lab_inv_f(fz);
    let x = 0.9555766 * x50 - 0.0230393 * y50 + 0.0631636 * z50;
    let y = -0.0282895 * x50 + 1.0099416 * y50 + 0.0210077 * z50;
    let z = 0.0122982 * x50 - 0.0204830 * y50 + 1.3299098 * z50;
    let r = 3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let g = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z;
    let b = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;
    [
        srgb_encode_linear(r),
        srgb_encode_linear(g),
        srgb_encode_linear(b),
    ]
}

#[derive(Clone, Copy)]
struct LabSpace {
    oklab: bool,
    polar: bool,
}

impl LabSpace {
    fn chroma_percent(self) -> f64 {
        if self.oklab { 0.004 } else { 1.25 }
    }

    fn lightness_max(self) -> f64 {
        if self.oklab { 1.0 } else { 100.0 }
    }
}

fn lab_like_function(text: &CStr, out: &mut Channels) -> bool {
    let s = text.to_bytes();
    let space = if starts_with_ci(s, b"lch(") || starts_with_ci(s, b"lab(") {
        LabSpace {
            oklab: false,
            polar: starts_with_ci(s, b"lch("),
        }
    } else if starts_with_ci(s, b"oklch(") || starts_with_ci(s, b"oklab(") {
        LabSpace {
            oklab: true,
            polar: starts_with_ci(s, b"oklch("),
        }
    } else {
        return false;
    };
    if s.contains(&b',') {
        return false;
    }
    let Some(open) = opening(s) else { return false };
    let mut values = [0.0, 0.0, 0.0, 1.0];
    let mut count = 0;
    let mut p = open;
    while byte(s, p) != 0 && byte(s, p) != b')' && count < 4 {
        while matches!(byte(s, p), b' ' | b'/') {
            p += 1;
        }
        if byte(s, p) == 0 || byte(s, p) == b')' {
            break;
        }
        if none_at(s, p) {
            values[count] = if count == 3 { 1.0 } else { 0.0 };
            count += 1;
            p += 4;
            continue;
        }
        let (mut value, mut end) = ffi::strtod(text, p);
        if end == p {
            return false;
        }
        let percent = byte(s, end) == b'%';
        match count {
            0 if percent => {
                if space.oklab {
                    value /= 100.0;
                }
                end += 1;
            }
            1 => {
                if percent {
                    value *= space.chroma_percent();
                    end += 1;
                }
                if space.polar && value < 0.0 {
                    value = 0.0;
                }
            }
            2 if space.polar => {
                value = angle_degrees(s, value, &mut end);
                if is_ident(byte(s, end)) {
                    return false;
                }
            }
            2 if percent => {
                value *= space.chroma_percent();
                end += 1;
            }
            3 if percent => {
                value /= 100.0;
                end += 1;
            }
            _ => {}
        }
        values[count] = value;
        count += 1;
        p = end;
    }
    if count < 3 {
        return false;
    }
    let l = gclamp(values[0], 0.0, space.lightness_max());
    let (a, b) = if space.polar {
        let rad = values[2] * PI / 180.0;
        (values[1] * rad.cos(), values[1] * rad.sin())
    } else {
        (values[1], values[2])
    };
    let rgb = if space.oklab {
        oklab_to_srgb(l, a, b)
    } else {
        lab_to_srgb(l, a, b)
    };
    set_rgb(out, rgb);
    let alpha = if count >= 4 { values[3] } else { 1.0 };
    out[3] = Some(channel(gclamp(alpha, 0.0, 1.0) * 255.0));
    true
}

fn mix_percent(token: &[u8]) -> Option<f64> {
    let text = CString::new(token).ok()?;
    let s = text.to_bytes();
    let (value, mut end) = ffi::strtod(&text, 0);
    if end == 0 {
        return None;
    }
    end = skip_ws(s, end, s.len());
    if byte(s, end) != b'%' {
        return None;
    }
    end = skip_ws(s, end + 1, s.len());
    (end == s.len()).then(|| gclamp(value, 0.0, 100.0))
}

struct MixStop {
    color: Channels,
    percent: Option<f64>,
}

fn mix_stop(text: &[u8], depth: i32) -> Option<MixStop> {
    let tokens = split_ws_limit(text, 3);
    if tokens.len() != 1 && tokens.len() != 2 {
        return None;
    }
    let percent = match tokens.get(1) {
        Some(token) => Some(mix_percent(token)?),
        None => None,
    };
    let mut color = [None; 4];
    let ok = parse_bytes(tokens[0], &mut color, depth + 1);
    ok.then_some(MixStop { color, percent })
}

fn color_mix_function(text: &CStr, out: &mut Channels, depth: i32) -> bool {
    let s = text.to_bytes();
    if !starts_with_ci(s, b"color-mix(") {
        return false;
    }
    let Some(open) = opening(s) else { return false };
    let Some(body_end) = match_close_paren(s, open, s.len()) else {
        return false;
    };
    let parts = split_args(s, open, body_end, 3);
    if parts.len() != 3 {
        return false;
    }
    let space = parts[0];
    let mut i = skip_ws(space, 0, space.len());
    if !(starts_with_ci(&space[i..], b"in") && is_ws(byte(space, i + 2))) {
        return false;
    }
    i = skip_ws(space, i + 2, space.len());
    let name_len = space[i..].iter().take_while(|&&c| !is_ws(c)).count();
    let name = &space[i..i + name_len];
    let in_oklab = name.eq_ignore_ascii_case(b"oklab") || name.eq_ignore_ascii_case(b"oklch");
    let known = in_oklab
        || [
            &b"srgb"[..],
            b"srgb-linear",
            b"hsl",
            b"hwb",
            b"lab",
            b"lch",
            b"xyz",
        ]
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known));
    if !known {
        return false;
    }
    let Some(first) = mix_stop(parts[1], depth) else {
        return false;
    };
    let Some(second) = mix_stop(parts[2], depth) else {
        return false;
    };
    let (p1, p2) = match (first.percent, second.percent) {
        (Some(p1), None) => (p1, 100.0 - p1),
        (None, Some(p2)) => (100.0 - p2, p2),
        (Some(p1), Some(p2)) => (p1, p2),
        (None, None) => (50.0, 50.0),
    };
    let sum = p1 + p2;
    if sum <= 0.0 {
        return false;
    }
    let c1 = first.color.map(|channel| channel.unwrap_or(0));
    let c2 = second.color.map(|channel| channel.unwrap_or(0));
    let w1 = p1 / sum;
    let w2 = p2 / sum;
    let a1 = f64::from(c1[3]) / 255.0;
    let a2 = f64::from(c2[3]) / 255.0;
    let alpha = a1 * w1 + a2 * w2;
    if in_oklab {
        let lab1 = srgb_to_oklab([c1[0], c1[1], c1[2]]);
        let lab2 = srgb_to_oklab([c2[0], c2[1], c2[2]]);
        let mut mixed = [0.0; 3];
        if alpha > 0.0 {
            for (k, value) in mixed.iter_mut().enumerate() {
                *value = (lab1[k] * a1 * w1 + lab2[k] * a2 * w2) / alpha;
            }
        }
        set_rgb(out, oklab_to_srgb(mixed[0], mixed[1], mixed[2]));
    } else {
        for k in 0..3 {
            let value = if alpha > 0.0 {
                (f64::from(c1[k]) * a1 * w1 + f64::from(c2[k]) * a2 * w2) / alpha
            } else {
                0.0
            };
            out[k] = Some(channel(value));
        }
    }
    out[3] = Some(channel(alpha * 255.0));
    true
}

fn light_dark_function(text: &CStr, out: &mut Channels, depth: i32) -> bool {
    let s = text.to_bytes();
    if !starts_with_ci(s, b"light-dark(") {
        return false;
    }
    let Some(open) = opening(s) else { return false };
    let Some(body_end) = match_close_paren(s, open, s.len()) else {
        return false;
    };
    let parts = split_args(s, open, body_end, 2);
    if parts.len() != 2 {
        return false;
    }
    let choice = if ffi::prefers_dark() {
        parts[1]
    } else {
        parts[0]
    };
    parse_bytes(choice, out, depth + 1)
}

#[derive(Default)]
struct CalcTerm {
    value: f64,
    unit: Vec<u8>,
}

const MAX_UNIT: usize = 7;

fn calc_factor(text: &CStr, pos: &mut usize, end: usize, out: &mut CalcTerm, depth: i32) -> bool {
    let s = text.to_bytes();
    if depth > MAX_CALC_DEPTH {
        return false;
    }
    let mut p = skip_ws(s, *pos, end);
    let nested = if p < end && s[p] == b'(' {
        Some(p + 1)
    } else if p + 5 <= end && starts_with_ci(&s[p..], b"calc(") {
        Some(p + 5)
    } else {
        None
    };
    if let Some(inner) = nested {
        p = inner;
        if !calc_sum(text, &mut p, end, out, depth + 1) {
            return false;
        }
        p = skip_ws(s, p, end);
        if p >= end || s[p] != b')' {
            return false;
        }
        *pos = p + 1;
        return true;
    }
    let (value, number_end) = ffi::strtod(text, p);
    if number_end == p || number_end > end {
        return false;
    }
    out.value = value;
    out.unit.clear();
    p = number_end;
    while p < end && (is_ident(s[p]) || s[p] == b'%') && out.unit.len() < MAX_UNIT {
        out.unit.push(s[p]);
        p += 1;
    }
    *pos = p;
    true
}

fn calc_product(text: &CStr, pos: &mut usize, end: usize, out: &mut CalcTerm, depth: i32) -> bool {
    let s = text.to_bytes();
    if !calc_factor(text, pos, end, out, depth) {
        return false;
    }
    loop {
        let mut p = skip_ws(s, *pos, end);
        if p >= end || (s[p] != b'*' && s[p] != b'/') {
            return true;
        }
        let op = s[p];
        p += 1;
        let mut rhs = CalcTerm::default();
        if !calc_factor(text, &mut p, end, &mut rhs, depth) {
            return false;
        }
        if op == b'*' {
            if !out.unit.is_empty() && !rhs.unit.is_empty() {
                return false;
            }
            out.value *= rhs.value;
            if !rhs.unit.is_empty() {
                out.unit = rhs.unit;
            }
        } else {
            if !rhs.unit.is_empty() || rhs.value == 0.0 {
                return false;
            }
            out.value /= rhs.value;
        }
        *pos = p;
    }
}

fn calc_sum(text: &CStr, pos: &mut usize, end: usize, out: &mut CalcTerm, depth: i32) -> bool {
    let s = text.to_bytes();
    if !calc_product(text, pos, end, out, depth) {
        return false;
    }
    loop {
        let mut p = skip_ws(s, *pos, end);
        if p >= end || (s[p] != b'+' && s[p] != b'-') {
            return true;
        }
        let op = s[p];
        p += 1;
        let mut rhs = CalcTerm::default();
        if !calc_product(text, &mut p, end, &mut rhs, depth) {
            return false;
        }
        if !out.unit.eq_ignore_ascii_case(&rhs.unit) {
            if out.unit.is_empty() && out.value == 0.0 {
                out.unit.clone_from(&rhs.unit);
            } else if !(rhs.unit.is_empty() && rhs.value == 0.0) {
                return false;
            }
        }
        out.value = if op == b'+' {
            out.value + rhs.value
        } else {
            out.value - rhs.value
        };
        *pos = p;
    }
}

fn resolve_calcs(text: &CStr) -> Option<Vec<u8>> {
    let s = text.to_bytes();
    let mut flat = Vec::with_capacity(s.len());
    let mut p = 0;
    while p < s.len() {
        if starts_with_ci(&s[p..], b"calc(") {
            let body = p + 5;
            let close = match_close_paren(s, body, s.len())?;
            let mut q = body;
            let mut term = CalcTerm::default();
            if !calc_sum(text, &mut q, close, &mut term, 0) {
                return None;
            }
            flat.extend_from_slice(&ffi::format_g6(term.value));
            flat.extend_from_slice(&term.unit);
            p = close + 1;
        } else {
            flat.push(s[p]);
            p += 1;
        }
    }
    Some(flat)
}

fn hex_digit(c: u8) -> Option<u8> {
    char::from(c).to_digit(16).map(|digit| digit as u8)
}

fn hex_color(s: &[u8], out: &mut Channels) -> bool {
    let digits = &s[1..];
    match digits.len() {
        3 | 4 => {
            let (Some(r), Some(g), Some(b)) = (
                hex_digit(digits[0]),
                hex_digit(digits[1]),
                hex_digit(digits[2]),
            ) else {
                return false;
            };
            out[0] = Some(r * 17);
            out[1] = Some(g * 17);
            out[2] = Some(b * 17);
            if digits.len() == 4 {
                let Some(a) = hex_digit(digits[3]) else {
                    return false;
                };
                out[3] = Some(a * 17);
            }
            true
        }
        6 | 8 => {
            let Some(values) = digits
                .iter()
                .map(|&c| hex_digit(c))
                .collect::<Option<Vec<u8>>>()
            else {
                return false;
            };
            for (slot, pair) in out.iter_mut().zip(values.chunks(2)) {
                *slot = Some(pair[0] * 16 + pair[1]);
            }
            true
        }
        _ => false,
    }
}

fn named_color(s: &[u8], out: &mut Channels) -> bool {
    let Some((_, rgb)) = NAMED
        .iter()
        .find(|(name, _)| name.as_bytes().eq_ignore_ascii_case(s))
    else {
        return false;
    };
    for (slot, value) in out.iter_mut().zip(rgb) {
        *slot = Some(*value);
    }
    true
}

fn parse_bytes(bytes: &[u8], out: &mut Channels, depth: i32) -> bool {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    let text = CString::new(&bytes[..len]).unwrap_or_default();
    parse_depth(&text, out, depth)
}

fn parse_depth(text: &CStr, out: &mut Channels, depth: i32) -> bool {
    out[3] = Some(255);
    let s = text.to_bytes();
    if s.is_empty() || depth > MAX_COLOR_DEPTH {
        return false;
    }
    if s.windows(5).any(|window| window == b"calc(") {
        return match resolve_calcs(text) {
            Some(flat) => parse_bytes(&flat, out, depth + 1),
            None => false,
        };
    }
    if s.eq_ignore_ascii_case(b"transparent") {
        *out = [Some(0); 4];
        return true;
    }
    rgb_function(text, out)
        || hsl_function(text, out)
        || hwb_function(text, out)
        || lab_like_function(text, out)
        || color_mix_function(text, out, depth)
        || light_dark_function(text, out, depth)
        || if s[0] == b'#' {
            hex_color(s, out)
        } else {
            named_color(s, out)
        }
}

fn alpha_text(a: u8) -> Vec<u8> {
    let f = f64::from(a) / 255.0;
    let mut text = Vec::new();
    for format in [c"%.1f", c"%.2f", c"%.3f", c"%.4f", c"%.5f"] {
        text = ffi::formatd(format, f);
        let back = CString::new(text.clone()).unwrap_or_default();
        if (ffi::strtod(&back, 0).0 * 255.0 + 0.5) as i32 == i32::from(a) {
            break;
        }
    }
    if let Some(dot) = text.iter().position(|&c| c == b'.') {
        let mut end = text.len();
        while end > dot + 1 && text[end - 1] == b'0' {
            end -= 1;
        }
        if end == dot + 1 {
            end = dot;
        }
        text.truncate(end);
    }
    text
}

pub(crate) fn color_text(rgba: [u8; 4]) -> Vec<u8> {
    let [r, g, b, a] = rgba;
    if a == 255 {
        format!("rgb({r}, {g}, {b})").into_bytes()
    } else {
        let mut out = format!("rgba({r}, {g}, {b}, ").into_bytes();
        out.extend_from_slice(&alpha_text(a));
        out.push(b')');
        out
    }
}
