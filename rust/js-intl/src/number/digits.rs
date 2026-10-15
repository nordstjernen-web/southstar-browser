//! Southstar — the digit options of NumberFormat and PluralRules: reading them from options and rounding a value with them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use super::decimal::{Decimal, RawFormat, UnsignedRounding, to_raw_fixed, to_raw_precision};
use crate::ecma402::{OptionsBag, default_number_option};

const ROUNDING_INCREMENTS: [u32; 15] = [
    1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 2500, 5000,
];
const ROUNDING_MODES: [&str; 9] = [
    "ceil",
    "floor",
    "expand",
    "trunc",
    "halfCeil",
    "halfFloor",
    "halfExpand",
    "halfTrunc",
    "halfEven",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RoundingType {
    FractionDigits,
    SignificantDigits,
    MorePrecision,
    LessPrecision,
}

#[derive(Clone, Debug)]
pub(crate) struct DigitOptions {
    pub minimum_integer_digits: u32,
    pub fraction_digits: Option<(u32, u32)>,
    pub significant_digits: Option<(u32, u32)>,
    pub rounding_type: RoundingType,
    pub rounding_priority: &'static str,
    pub rounding_increment: u32,
    pub rounding_mode: &'static str,
    pub trailing_zero_display: &'static str,
}

pub(crate) struct Rounded {
    pub text: String,
    pub value: Decimal,
}

pub(crate) fn read_digit_options(
    scope: &mut Scope<'_>,
    options: &OptionsBag,
    default_minimum_fraction: u32,
    mut default_maximum_fraction: u32,
    notation: &str,
) -> Result<DigitOptions, Value> {
    let minimum_integer_digits = options.number(scope, "minimumIntegerDigits", 1, 21, 1)?;
    let minimum_fraction = options.get(scope, "minimumFractionDigits")?;
    let maximum_fraction = options.get(scope, "maximumFractionDigits")?;
    let minimum_significant = options.get(scope, "minimumSignificantDigits")?;
    let maximum_significant = options.get(scope, "maximumSignificantDigits")?;
    let rounding_increment = options.number(scope, "roundingIncrement", 1, 5000, 1)?;
    if !ROUNDING_INCREMENTS.contains(&rounding_increment) {
        return Err(scope.range_error("roundingIncrement is not an allowed value"));
    }
    let rounding_mode = options.string(scope, "roundingMode", &ROUNDING_MODES, "halfExpand")?;
    let rounding_priority = options.string(
        scope,
        "roundingPriority",
        &["auto", "morePrecision", "lessPrecision"],
        "auto",
    )?;
    let trailing_zero_display = options.string(
        scope,
        "trailingZeroDisplay",
        &["auto", "stripIfInteger"],
        "auto",
    )?;
    if rounding_increment != 1 {
        default_maximum_fraction = default_minimum_fraction;
    }
    let has_significant =
        !minimum_significant.is_undefined() || !maximum_significant.is_undefined();
    let has_fraction = !minimum_fraction.is_undefined() || !maximum_fraction.is_undefined();
    let mut need_significant = true;
    let mut need_fraction = true;
    if rounding_priority == "auto" {
        need_significant = has_significant;
        if need_significant || (!has_fraction && notation == "compact") {
            need_fraction = false;
        }
    }
    let mut significant_digits = None;
    let mut fraction_digits = None;
    if need_significant {
        significant_digits = Some(if has_significant {
            let minimum = default_number_option(
                scope,
                &minimum_significant,
                1,
                21,
                "minimumSignificantDigits",
            )?
            .unwrap_or(1);
            let maximum = default_number_option(
                scope,
                &maximum_significant,
                minimum,
                21,
                "maximumSignificantDigits",
            )?
            .unwrap_or(21);
            (minimum, maximum)
        } else {
            (1, 21)
        });
    }
    if need_fraction {
        fraction_digits = Some(if has_fraction {
            let minimum =
                default_number_option(scope, &minimum_fraction, 0, 100, "minimumFractionDigits")?;
            let maximum =
                default_number_option(scope, &maximum_fraction, 0, 100, "maximumFractionDigits")?;
            match (minimum, maximum) {
                (None, Some(maximum)) => (default_minimum_fraction.min(maximum), maximum),
                (Some(minimum), None) => (minimum, default_maximum_fraction.max(minimum)),
                (Some(minimum), Some(maximum)) if minimum > maximum => {
                    return Err(scope.range_error(
                        "minimumFractionDigits is greater than maximumFractionDigits",
                    ));
                }
                (Some(minimum), Some(maximum)) => (minimum, maximum),
                (None, None) => (default_minimum_fraction, default_maximum_fraction),
            }
        } else {
            (default_minimum_fraction, default_maximum_fraction)
        });
    }
    let (rounding_type, computed_priority) = if !need_significant && !need_fraction {
        fraction_digits = Some((0, 0));
        significant_digits = Some((1, 2));
        (RoundingType::MorePrecision, "morePrecision")
    } else if rounding_priority == "morePrecision" {
        (RoundingType::MorePrecision, "morePrecision")
    } else if rounding_priority == "lessPrecision" {
        (RoundingType::LessPrecision, "lessPrecision")
    } else if has_significant {
        (RoundingType::SignificantDigits, "auto")
    } else {
        (RoundingType::FractionDigits, "auto")
    };
    if rounding_increment != 1 {
        if rounding_type != RoundingType::FractionDigits {
            return Err(scope.type_error("roundingIncrement requires fraction digit rounding"));
        }
        if let Some((minimum, maximum)) = fraction_digits
            && minimum != maximum
        {
            return Err(scope.range_error(
                "roundingIncrement requires equal minimum and maximum fraction digits",
            ));
        }
    }
    Ok(DigitOptions {
        minimum_integer_digits,
        fraction_digits,
        significant_digits,
        rounding_type,
        rounding_priority: computed_priority,
        rounding_increment,
        rounding_mode,
        trailing_zero_display,
    })
}

fn unsigned_rounding(mode: &str, negative: bool) -> UnsignedRounding {
    match (mode, negative) {
        ("ceil", false) | ("floor", true) | ("expand", _) => UnsignedRounding::Infinity,
        ("ceil", true) | ("floor", false) | ("trunc", _) => UnsignedRounding::Zero,
        ("halfCeil", false) | ("halfFloor", true) | ("halfExpand", _) => {
            UnsignedRounding::HalfInfinity
        }
        ("halfCeil", true) | ("halfFloor", false) | ("halfTrunc", _) => UnsignedRounding::HalfZero,
        _ => UnsignedRounding::HalfEven,
    }
}

impl DigitOptions {
    pub(crate) fn round(&self, negative: bool, x: &Decimal) -> Rounded {
        let mode = unsigned_rounding(self.rounding_mode, negative);
        let (minimum_fraction, maximum_fraction) = self.fraction_digits.unwrap_or((0, 3));
        let (minimum_significant, maximum_significant) = self.significant_digits.unwrap_or((1, 21));
        let fixed = || {
            to_raw_fixed(
                x,
                minimum_fraction,
                maximum_fraction,
                self.rounding_increment,
                mode,
            )
        };
        let precise = || to_raw_precision(x, minimum_significant, maximum_significant, mode);
        let result: RawFormat = match self.rounding_type {
            RoundingType::SignificantDigits => precise(),
            RoundingType::FractionDigits => fixed(),
            RoundingType::MorePrecision => {
                let (significant, fraction) = (precise(), fixed());
                if significant.rounding_magnitude <= fraction.rounding_magnitude {
                    significant
                } else {
                    fraction
                }
            }
            RoundingType::LessPrecision => {
                let (significant, fraction) = (precise(), fixed());
                if significant.rounding_magnitude <= fraction.rounding_magnitude {
                    fraction
                } else {
                    significant
                }
            }
        };
        let mut text = result.text;
        if self.trailing_zero_display == "stripIfInteger"
            && result.rounded.is_integer()
            && let Some(dot) = text.find('.')
        {
            text.truncate(dot);
        }
        let minimum_integer = self.minimum_integer_digits as usize;
        if result.integer_digits < minimum_integer {
            text.insert_str(0, &"0".repeat(minimum_integer - result.integer_digits));
        }
        Rounded {
            text,
            value: result.rounded,
        }
    }
}
