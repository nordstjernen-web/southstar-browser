//! Southstar — timing functions: linear, the ease keywords, cubic-bezier() and steps().
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_glib::GBoolean;

const KIND_LINEAR: c_int = 0;
const KIND_EASE_IN: c_int = 2;
const KIND_EASE_OUT: c_int = 3;
const KIND_EASE_IN_OUT: c_int = 4;
const KIND_STEPS: c_int = 5;
const KIND_CUBIC: c_int = 6;

const STEP_JUMP_START: c_int = 1;
const STEP_JUMP_NONE: c_int = 2;
const STEP_JUMP_BOTH: c_int = 3;

const EASE: [f64; 4] = [0.25, 0.1, 0.25, 1.0];
const EASE_IN: [f64; 4] = [0.42, 0.0, 1.0, 1.0];
const EASE_OUT: [f64; 4] = [0.0, 0.0, 0.58, 1.0];
const EASE_IN_OUT: [f64; 4] = [0.42, 0.0, 0.58, 1.0];

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Timing {
    kind: c_int,
    steps: c_int,
    step_pos: c_int,
    jump_keyword: GBoolean,
    cb: [f64; 4],
}

#[cfg(target_pointer_width = "64")]
const _: () =
    assert!(core::mem::size_of::<Timing>() == 48 && core::mem::offset_of!(Timing, cb) == 16);

impl Timing {
    pub fn linear() -> Timing {
        Timing {
            kind: KIND_LINEAR,
            ..Timing::default()
        }
    }

    pub fn apply(&self, x: f64) -> f64 {
        if self.kind == KIND_STEPS {
            return steps(self.steps, self.step_pos, x);
        }
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        match self.kind {
            KIND_LINEAR => x,
            KIND_EASE_IN => cubic_bezier(&EASE_IN, x),
            KIND_EASE_OUT => cubic_bezier(&EASE_OUT, x),
            KIND_EASE_IN_OUT => cubic_bezier(&EASE_IN_OUT, x),
            KIND_CUBIC => cubic_bezier(&self.cb, x),
            _ => cubic_bezier(&EASE, x),
        }
    }
}

fn steps(n: c_int, pos: c_int, x: f64) -> f64 {
    let n = n.max(1);
    let x = x.clamp(0.0, 1.0);
    let mut step = (x * f64::from(n) + 1e-9).floor() as c_int;
    if pos == STEP_JUMP_START || pos == STEP_JUMP_BOTH {
        step += 1;
    }
    let jumps = match pos {
        STEP_JUMP_NONE if n > 1 => n - 1,
        STEP_JUMP_NONE => 1,
        STEP_JUMP_BOTH => n + 1,
        _ => n,
    };
    f64::from(step.clamp(0, jumps)) / f64::from(jumps)
}

fn bezier_axis(t: f64, p1: f64, p2: f64) -> f64 {
    let mt = 1.0 - t;
    3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
}

fn cubic_bezier(cb: &[f64; 4], x: f64) -> f64 {
    let mut t = x;
    for _ in 0..8 {
        let xt = bezier_axis(t, cb[0], cb[2]) - x;
        if xt.abs() < 1e-6 {
            break;
        }
        let mt = 1.0 - t;
        let d =
            3.0 * mt * mt * cb[0] + 6.0 * mt * t * (cb[2] - cb[0]) + 3.0 * t * t * (1.0 - cb[2]);
        if d.abs() < 1e-6 {
            break;
        }
        t = (t - xt / d).clamp(0.0, 1.0);
    }
    bezier_axis(t, cb[1], cb[3])
}
