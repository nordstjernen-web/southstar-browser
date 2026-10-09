//! Southstar — css.c ported to Rust section by section: so far the colour parser, lengths with calc() and the other math functions, container queries, gradients, positions and image values, transforms, grid values, font values, shadows, time values, easing functions and animation lists, display, counter and list values, border-image, the property table, the per-property value parser, shorthand expansion, the inline style text behind element.style, serializing, interpolating and comparing values, reading declaration blocks, parsing selectors, flattening nesting, evaluating @supports, parsing style sheets with their at-rules, scoping shadow trees' style sheets to their hosts, turning legacy HTML attributes into presentational hints, the element state behind pseudo-classes, attr() and var() substitution, the custom-property cascade and registered-property values, answering layout's and paint's computed-style queries, resolving pending var() and attr() declarations, applying matched declarations to a style, resolving units in computed styles, the computed text of grid track lists, cascade layer order, the rule index, matching selectors against elements and the restyle invalidation DOM mutations trigger.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod animation;
mod attr_fn;
mod border_image;
mod calc;
mod cascade;
mod color;
mod computed_units;
mod container;
mod content;
mod counter;
mod custom_props;
mod declarations;
mod display;
mod element_state;
mod ffi;
mod font;
mod gradient;
mod grid;
mod hints;
mod host_scope;
mod image;
mod initial;
mod inline;
mod layers;
mod lex;
mod matcher;
mod math;
mod nesting;
mod pending;
mod position;
mod prop;
mod property;
mod restyle;
mod rule_index;
mod scan;
mod selector;
mod shadow;
mod sheet;
mod shorthand;
mod style_query;
mod supports;
mod text;
mod time;
mod timing;
mod track_text;
mod transform;
mod units;
mod values;
mod vars;

pub use color::parse_color;
pub use ffi::NsCssValue;
