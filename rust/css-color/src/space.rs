//! Southstar — CSS colour spaces and the conversions between them, from the CSS Color 4 sample code.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

type Matrix = [[f64; 3]; 3];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Space {
    Srgb,
    SrgbLinear,
    DisplayP3,
    DisplayP3Linear,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    XyzD50,
    XyzD65,
    Lab,
    Lch,
    Oklab,
    Oklch,
    Hsl,
    Hwb,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Analog {
    Red,
    Green,
    Blue,
    Lightness,
    Colorfulness,
    Hue,
    OpponentA,
    OpponentB,
    Other,
}

const LCH_ACHROMATIC: f64 = 0.0015;
const OKLCH_ACHROMATIC: f64 = 0.000004;
const HSL_ACHROMATIC: f64 = 1.05e-5;
const HWB_ACHROMATIC: f64 = 1.05e-5;
const LIGHTNESS_EDGE: f64 = 1e-9;

const ALL: [Space; 15] = [
    Space::Srgb,
    Space::SrgbLinear,
    Space::DisplayP3,
    Space::DisplayP3Linear,
    Space::A98Rgb,
    Space::ProphotoRgb,
    Space::Rec2020,
    Space::XyzD50,
    Space::XyzD65,
    Space::Lab,
    Space::Lch,
    Space::Oklab,
    Space::Oklch,
    Space::Hsl,
    Space::Hwb,
];

impl Space {
    pub fn index(self) -> u8 {
        ALL.iter().position(|&s| s == self).unwrap_or(0) as u8
    }

    pub fn from_index(index: u8) -> Option<Space> {
        ALL.get(usize::from(index)).copied()
    }

    pub fn from_predefined(name: &str) -> Option<Space> {
        Some(match name {
            "srgb" => Space::Srgb,
            "srgb-linear" => Space::SrgbLinear,
            "display-p3" => Space::DisplayP3,
            "display-p3-linear" => Space::DisplayP3Linear,
            "a98-rgb" => Space::A98Rgb,
            "prophoto-rgb" => Space::ProphotoRgb,
            "rec2020" => Space::Rec2020,
            "xyz" | "xyz-d65" => Space::XyzD65,
            "xyz-d50" => Space::XyzD50,
            _ => return None,
        })
    }

    pub fn from_interpolation(name: &str) -> Option<Space> {
        Space::from_predefined(name).or(match name {
            "lab" => Some(Space::Lab),
            "lch" => Some(Space::Lch),
            "oklab" => Some(Space::Oklab),
            "oklch" => Some(Space::Oklch),
            "hsl" => Some(Space::Hsl),
            "hwb" => Some(Space::Hwb),
            _ => None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Space::Srgb => "srgb",
            Space::SrgbLinear => "srgb-linear",
            Space::DisplayP3 => "display-p3",
            Space::DisplayP3Linear => "display-p3-linear",
            Space::A98Rgb => "a98-rgb",
            Space::ProphotoRgb => "prophoto-rgb",
            Space::Rec2020 => "rec2020",
            Space::XyzD50 => "xyz-d50",
            Space::XyzD65 => "xyz-d65",
            Space::Lab => "lab",
            Space::Lch => "lch",
            Space::Oklab => "oklab",
            Space::Oklch => "oklch",
            Space::Hsl => "hsl",
            Space::Hwb => "hwb",
        }
    }

    pub fn is_predefined(self) -> bool {
        !matches!(
            self,
            Space::Lab | Space::Lch | Space::Oklab | Space::Oklch | Space::Hsl | Space::Hwb
        )
    }

    pub fn is_xyz(self) -> bool {
        matches!(self, Space::XyzD50 | Space::XyzD65)
    }

    pub fn hue_index(self) -> Option<usize> {
        match self {
            Space::Hsl | Space::Hwb => Some(0),
            Space::Lch | Space::Oklch => Some(2),
            _ => None,
        }
    }

    pub(crate) fn analogs(self) -> [Analog; 3] {
        use Analog::*;
        match self {
            Space::Lab | Space::Oklab => [Lightness, OpponentA, OpponentB],
            Space::Lch | Space::Oklch => [Lightness, Colorfulness, Hue],
            Space::Hsl => [Hue, Colorfulness, Lightness],
            Space::Hwb => [Hue, Other, Other],
            _ => [Red, Green, Blue],
        }
    }

    pub(crate) fn to_xyz_d65(self, c: [f64; 3]) -> [f64; 3] {
        match self {
            Space::Srgb => mul(&SRGB_TO_XYZ, c.map(srgb_to_linear)),
            Space::SrgbLinear => mul(&SRGB_TO_XYZ, c),
            Space::DisplayP3 => mul(&P3_TO_XYZ, c.map(srgb_to_linear)),
            Space::DisplayP3Linear => mul(&P3_TO_XYZ, c),
            Space::A98Rgb => mul(&A98_TO_XYZ, c.map(a98_to_linear)),
            Space::ProphotoRgb => mul(
                &D50_TO_D65,
                mul(&PROPHOTO_TO_XYZ50, c.map(prophoto_to_linear)),
            ),
            Space::Rec2020 => mul(&REC2020_TO_XYZ, c.map(rec2020_to_linear)),
            Space::XyzD50 => mul(&D50_TO_D65, c),
            Space::XyzD65 => c,
            Space::Lab => mul(&D50_TO_D65, lab_to_xyz50(c)),
            Space::Lch => mul(&D50_TO_D65, lab_to_xyz50(lch_to_lab(c, LCH_ACHROMATIC))),
            Space::Oklab => oklab_to_xyz(c),
            Space::Oklch => oklab_to_xyz(lch_to_lab(c, OKLCH_ACHROMATIC)),
            Space::Hsl => Space::Srgb.to_xyz_d65(hsl_to_srgb(c)),
            Space::Hwb => Space::Srgb.to_xyz_d65(hwb_to_srgb(c)),
        }
    }

    pub(crate) fn xyz_d65_into(self, xyz: [f64; 3]) -> [Option<f64>; 3] {
        let all = |c: [f64; 3]| c.map(Some);
        match self {
            Space::Srgb => all(mul(&XYZ_TO_SRGB, xyz).map(srgb_from_linear)),
            Space::SrgbLinear => all(mul(&XYZ_TO_SRGB, xyz)),
            Space::DisplayP3 => all(mul(&XYZ_TO_P3, xyz).map(srgb_from_linear)),
            Space::DisplayP3Linear => all(mul(&XYZ_TO_P3, xyz)),
            Space::A98Rgb => all(mul(&XYZ_TO_A98, xyz).map(a98_from_linear)),
            Space::ProphotoRgb => {
                all(mul(&XYZ50_TO_PROPHOTO, mul(&D65_TO_D50, xyz)).map(prophoto_from_linear))
            }
            Space::Rec2020 => all(mul(&XYZ_TO_REC2020, xyz).map(rec2020_from_linear)),
            Space::XyzD50 => all(mul(&D65_TO_D50, xyz)),
            Space::XyzD65 => all(xyz),
            Space::Lab => all(xyz50_to_lab(mul(&D65_TO_D50, xyz))),
            Space::Lch => lab_to_lch(xyz50_to_lab(mul(&D65_TO_D50, xyz)), LCH_ACHROMATIC),
            Space::Oklab => all(xyz_to_oklab(xyz)),
            Space::Oklch => lab_to_lch(xyz_to_oklab(xyz), OKLCH_ACHROMATIC),
            Space::Hsl => srgb_to_hsl(Space::Srgb.xyz_d65_into(xyz).map(|c| c.unwrap_or(0.0))),
            Space::Hwb => srgb_to_hwb(Space::Srgb.xyz_d65_into(xyz).map(|c| c.unwrap_or(0.0))),
        }
    }
}

const SRGB_TO_XYZ: Matrix = [
    [0.41239079926595934, 0.357584339383878, 0.1804807884018343],
    [0.21263900587151027, 0.715168678767756, 0.07219231536073371],
    [0.01933081871559182, 0.11919477979462598, 0.9505321522496607],
];
const XYZ_TO_SRGB: Matrix = [
    [3.2409699419045226, -1.537383177570094, -0.4986107602930034],
    [-0.9692436362808796, 1.8759675015077202, 0.04155505740717559],
    [
        0.05563007969699366,
        -0.20397695888897652,
        1.0569715142428786,
    ],
];
const P3_TO_XYZ: Matrix = [
    [0.4865709486482162, 0.26566769316909306, 0.1982172852343625],
    [0.2289745640697488, 0.6917385218365064, 0.079286914093745],
    [0.0, 0.04511338185890264, 1.043944368900976],
];
const XYZ_TO_P3: Matrix = [
    [2.493496911941425, -0.9313836179191239, -0.40271078445071684],
    [
        -0.8294889695615747,
        1.7626640603183463,
        0.023624685841943577,
    ],
    [
        0.03584583024378447,
        -0.07617238926804182,
        0.9568845240076872,
    ],
];
const A98_TO_XYZ: Matrix = [
    [0.5766690429101305, 0.1855582379065463, 0.1882286462349947],
    [0.29734497525053605, 0.6273635662554661, 0.07529145849399788],
    [0.02703136138641234, 0.07068885253582723, 0.9913375368376388],
];
const XYZ_TO_A98: Matrix = [
    [
        2.0415879038107465,
        -0.5650069742788596,
        -0.34473135077832956,
    ],
    [-0.9692436362808795, 1.8759675015077202, 0.04155505740717557],
    [
        0.013444280632031142,
        -0.11836239223101838,
        1.0151749943912054,
    ],
];
const PROPHOTO_TO_XYZ50: Matrix = [
    [0.7977666449006423, 0.13518129740053308, 0.0313477341283922],
    [0.2880748288194013, 0.711835234241873, 0.00008993693872564],
    [0.0, 0.0, 0.8251046025104602],
];
const XYZ50_TO_PROPHOTO: Matrix = [
    [
        1.3457868816471583,
        -0.25557208737979464,
        -0.05110186497554526,
    ],
    [-0.5446307051249019, 1.5082477428451468, 0.02052744743642139],
    [0.0, 0.0, 1.2119675456389452],
];
const REC2020_TO_XYZ: Matrix = [
    [0.6369580483012914, 0.14461690358620832, 0.1688809751641721],
    [0.2627002120112671, 0.6779980715188708, 0.05930171646986196],
    [0.0, 0.028072693049087428, 1.060985057710791],
];
const XYZ_TO_REC2020: Matrix = [
    [1.716651187971268, -0.355670783776392, -0.25336628137366],
    [-0.666684351832489, 1.616481236634939, 0.0157685458139111],
    [0.017639857445311, -0.042770613257809, 0.942103121235474],
];
const D65_TO_D50: Matrix = [
    [
        1.0479297925449969,
        0.022946870601609652,
        -0.05019226628920524,
    ],
    [
        0.02962780877005599,
        0.9904344267538799,
        -0.017073799063418826,
    ],
    [
        -0.009243040646204504,
        0.015055191490298152,
        0.7518742814281371,
    ],
];
const D50_TO_D65: Matrix = [
    [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
    [
        -0.0283697093338637,
        1.0099953980813041,
        0.021041441191917323,
    ],
    [
        0.012314014864481998,
        -0.020507649298898964,
        1.330365926242124,
    ],
];
const XYZ_TO_LMS: Matrix = [
    [
        0.819_022_437_996_703,
        0.3619062600528904,
        -0.1288737815209879,
    ],
    [0.0329836539323885, 0.9292868615863434, 0.0361446663506424],
    [0.0481771893596242, 0.2642395317527308, 0.6335478284694309],
];
const LMS_TO_OKLAB: Matrix = [
    [
        0.210_454_268_309_314,
        0.7936177747023054,
        -0.0040720430116193,
    ],
    [
        1.9779985324311684,
        -2.428_592_242_048_58,
        0.450_593_709_617_411,
    ],
    [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
];
const OKLAB_TO_LMS: Matrix = [
    [1.0, 0.3963377773761749, 0.2158037573099136],
    [1.0, -0.1055613458156586, -0.0638541728258133],
    [1.0, -0.0894841775298119, -1.2914855480194092],
];
const LMS_TO_XYZ: Matrix = [
    [1.2268798758459243, -0.5578149944602171, 0.2813910456659647],
    [
        -0.0405757452148008,
        1.112_286_803_280_317,
        -0.0717110580655164,
    ],
    [-0.0763729366746601, -0.4214933324022432, 1.5869240198367816],
];

const D50_WHITE: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
const LAB_KAPPA: f64 = 24389.0 / 27.0;
const LAB_EPSILON: f64 = 216.0 / 24389.0;

fn mul(m: &Matrix, v: [f64; 3]) -> [f64; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

fn signed(c: f64, f: impl Fn(f64) -> f64) -> f64 {
    let magnitude = f(c.abs());
    if c < 0.0 { -magnitude } else { magnitude }
}

pub(crate) fn srgb_to_linear(c: f64) -> f64 {
    signed(c, |a| {
        if a <= 0.04045 {
            a / 12.92
        } else {
            ((a + 0.055) / 1.055).powf(2.4)
        }
    })
}

pub(crate) fn srgb_from_linear(c: f64) -> f64 {
    signed(c, |a| {
        if a > 0.0031308 {
            1.055 * a.powf(1.0 / 2.4) - 0.055
        } else {
            12.92 * a
        }
    })
}

fn a98_to_linear(c: f64) -> f64 {
    signed(c, |a| a.powf(563.0 / 256.0))
}

fn a98_from_linear(c: f64) -> f64 {
    signed(c, |a| a.powf(256.0 / 563.0))
}

fn prophoto_to_linear(c: f64) -> f64 {
    signed(c, |a| {
        if a <= 16.0 / 512.0 {
            a / 16.0
        } else {
            a.powf(1.8)
        }
    })
}

fn prophoto_from_linear(c: f64) -> f64 {
    signed(c, |a| {
        if a >= 1.0 / 512.0 {
            a.powf(1.0 / 1.8)
        } else {
            16.0 * a
        }
    })
}

fn rec2020_to_linear(c: f64) -> f64 {
    signed(c, |a| a.powf(2.4))
}

fn rec2020_from_linear(c: f64) -> f64 {
    signed(c, |a| a.powf(1.0 / 2.4))
}

fn lab_to_xyz50(lab: [f64; 3]) -> [f64; 3] {
    let [l, a, b] = lab;
    let f1 = (l + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let inverse = |f: f64| {
        let cube = f * f * f;
        if cube > LAB_EPSILON {
            cube
        } else {
            (116.0 * f - 16.0) / LAB_KAPPA
        }
    };
    let y = if l > LAB_KAPPA * LAB_EPSILON {
        f1 * f1 * f1
    } else {
        l / LAB_KAPPA
    };
    [
        inverse(f0) * D50_WHITE[0],
        y * D50_WHITE[1],
        inverse(f2) * D50_WHITE[2],
    ]
}

fn xyz50_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    let f = |i: usize| {
        let v = xyz[i] / D50_WHITE[i];
        if v > LAB_EPSILON {
            v.cbrt()
        } else {
            (LAB_KAPPA * v + 16.0) / 116.0
        }
    };
    let (f0, f1, f2) = (f(0), f(1), f(2));
    [116.0 * f1 - 16.0, 500.0 * (f0 - f1), 200.0 * (f1 - f2)]
}

fn oklab_to_xyz(lab: [f64; 3]) -> [f64; 3] {
    mul(&LMS_TO_XYZ, mul(&OKLAB_TO_LMS, lab).map(|c| c * c * c))
}

fn xyz_to_oklab(xyz: [f64; 3]) -> [f64; 3] {
    mul(&LMS_TO_OKLAB, mul(&XYZ_TO_LMS, xyz).map(f64::cbrt))
}

pub(crate) fn normalize_hue(degrees: f64) -> f64 {
    if !degrees.is_finite() {
        return 0.0;
    }
    let h = degrees % 360.0;
    let h = if h < 0.0 { h + 360.0 } else { h };
    if h >= 360.0 { 0.0 } else { h }
}

fn lch_to_lab(lch: [f64; 3], achromatic: f64) -> [f64; 3] {
    let [l, c, h] = lch;
    if c <= achromatic {
        return [l, 0.0, 0.0];
    }
    let rad = h.to_radians();
    [l, c * rad.cos(), c * rad.sin()]
}

fn lab_to_lch(lab: [f64; 3], achromatic: f64) -> [Option<f64>; 3] {
    let [l, a, b] = lab;
    let c = (a * a + b * b).sqrt();
    if c <= achromatic {
        return [Some(l), Some(0.0), None];
    }
    [
        Some(l),
        Some(c),
        Some(normalize_hue(b.atan2(a).to_degrees())),
    ]
}

pub(crate) fn hsl_to_srgb(hsl: [f64; 3]) -> [f64; 3] {
    let h = normalize_hue(hsl[0]);
    let s = hsl[1] / 100.0;
    let l = hsl[2] / 100.0;
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [f(0.0), f(8.0), f(4.0)]
}

pub(crate) fn hwb_to_srgb(hwb: [f64; 3]) -> [f64; 3] {
    let w = hwb[1] / 100.0;
    let b = hwb[2] / 100.0;
    if w + b >= 1.0 {
        let gray = w / (w + b);
        return [gray; 3];
    }
    hsl_to_srgb([hwb[0], 100.0, 50.0]).map(|c| c * (1.0 - w - b) + w)
}

fn hsl_parts(rgb: [f64; 3]) -> (Option<f64>, f64, f64) {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let light = (min + max) / 2.0;
    let d = max - min;
    let mut hue = None;
    let mut sat = 0.0;
    if d != 0.0 {
        sat = if light.abs() < LIGHTNESS_EDGE || (light - 1.0).abs() < LIGHTNESS_EDGE {
            0.0
        } else {
            (max - light) / light.min(1.0 - light)
        };
        let h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        hue = Some(h * 60.0);
    }
    (hue, sat, light)
}

fn srgb_to_hsl(rgb: [f64; 3]) -> [Option<f64>; 3] {
    let (mut hue, mut sat, light) = hsl_parts(rgb);
    if sat < 0.0 {
        hue = hue.map(|h| h + 180.0);
        sat = -sat;
    }
    if sat.abs() < HSL_ACHROMATIC {
        return [None, Some(0.0), Some(light * 100.0)];
    }
    [
        hue.map(normalize_hue),
        Some(sat * 100.0),
        Some(light * 100.0),
    ]
}

fn srgb_to_hwb(rgb: [f64; 3]) -> [Option<f64>; 3] {
    let hue = hsl_parts(rgb).0.map(normalize_hue);
    let white = rgb[0].min(rgb[1]).min(rgb[2]);
    let black = 1.0 - rgb[0].max(rgb[1]).max(rgb[2]);
    let hue = if white + black >= 1.0 - HWB_ACHROMATIC {
        None
    } else {
        hue
    };
    [hue, Some(white * 100.0), Some(black * 100.0)]
}
