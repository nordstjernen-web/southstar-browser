//! Southstar — Intl.Segmenter: grapheme, word and sentence segments from Pango's Unicode text segmentation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::Cell;
use std::rc::Rc;

use southstar_js_engine::{Attributes, Scope, Trace, Value};

use super::service::{self, Options};
use crate::{Method, arg, ffi};

pub(crate) const METHODS: [Method; 1] = [("resolvedOptions", resolved_options, 0)];

const GRANULARITIES: [&str; 3] = ["grapheme", "word", "sentence"];

#[derive(Clone)]
struct Segmenter {
    locale: String,
    granularity: &'static str,
}

struct Boundary {
    unit: usize,
    byte: usize,
}

struct Segmentation {
    granularity: &'static str,
    text: String,
    exact: bool,
    boundaries: Vec<Boundary>,
}

impl Segmentation {
    fn new(segmenter: &Segmenter, text: String) -> Segmentation {
        let mut boundaries = vec![Boundary { unit: 0, byte: 0 }];
        let count = text.chars().count();
        if count > 0 {
            let without_nul: Vec<u8> = text.bytes().map(|c| c.max(1)).collect();
            let language = ffi::pango_language(service::language_of(&segmenter.locale).as_bytes());
            let attrs = ffi::log_attrs(&ffi::c_text(&without_nul), count, language.as_ref());
            let (mut unit, mut byte) = (0, 0);
            for (index, c) in text.chars().enumerate() {
                unit += c.len_utf16();
                byte += c.len_utf8();
                let position = index + 1;
                let boundary = position == count
                    || match segmenter.granularity {
                        "word" => attrs.word_boundary(position),
                        "sentence" => attrs.sentence_boundary(position),
                        _ => attrs.cursor_position(position),
                    };
                if boundary {
                    boundaries.push(Boundary { unit, byte });
                }
            }
        }
        Segmentation {
            granularity: segmenter.granularity,
            exact: !text.contains('\u{fffd}'),
            text,
            boundaries,
        }
    }

    fn length(&self) -> usize {
        self.boundaries.last().map_or(0, |boundary| boundary.unit)
    }

    fn around(&self, index: usize) -> (usize, usize) {
        let after = self
            .boundaries
            .partition_point(|boundary| boundary.unit <= index);
        (after - 1, after)
    }
}

#[derive(Clone)]
struct Segments {
    segmentation: Rc<Segmentation>,
    input: Value,
}

impl Trace for Segments {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.input);
    }
}

#[derive(Clone)]
struct SegmentIterator {
    segments: Segments,
    position: Rc<Cell<usize>>,
}

impl Trace for SegmentIterator {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        self.segments.trace(visit);
    }
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::require_new_target(scope, this, "Segmenter")?;
    let prototype = service::prototype_from(scope, this, "Segmenter")?;
    let requested = service::requested_locales(scope, &arg(args, 0))?;
    let options = Options::object(scope, &arg(args, 1))?;
    options.locale_matcher(scope)?;
    let locale = service::resolve(&requested, service::COMMON_LANGUAGES);
    let granularity = options.choice_or(scope, "granularity", &GRANULARITIES, "grapheme")?;
    let segmenter = Segmenter {
        locale,
        granularity,
    };
    Ok(scope.new_host_object(Some(&prototype), segmenter))
}

pub(crate) fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::supported_locales(scope, args, service::COMMON_LANGUAGES)
}

fn resolved_options(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let segmenter: Segmenter =
        service::receiver(scope, this, "Intl.Segmenter.prototype.resolvedOptions")?;
    let options = scope.new_object();
    service::text_property(scope, &options, "locale", &segmenter.locale);
    service::text_property(scope, &options, "granularity", segmenter.granularity);
    Ok(options)
}

fn substring(
    scope: &mut Scope<'_>,
    input: &Value,
    start: usize,
    end: usize,
) -> Result<Value, Value> {
    let global = scope.global();
    let string = scope.get(&global, "String")?;
    let prototype = scope.get(&string, "prototype")?;
    let slice = scope.get(&prototype, "slice")?;
    let bounds = [Value::number(start as f64), Value::number(end as f64)];
    scope.call(&slice, input, &bounds)
}

fn segment_data(
    scope: &mut Scope<'_>,
    segments: &Segments,
    from: usize,
    to: usize,
) -> Result<Value, Value> {
    let segmentation = &segments.segmentation;
    let (start, end) = (&segmentation.boundaries[from], &segmentation.boundaries[to]);
    let text = &segmentation.text[start.byte..end.byte];
    let segment = if segmentation.exact {
        scope.string(text)
    } else {
        substring(scope, &segments.input, start.unit, end.unit)?
    };
    let result = scope.new_object();
    service::data_property(scope, &result, "segment", segment);
    service::data_property(scope, &result, "index", Value::number(start.unit as f64));
    service::data_property(scope, &result, "input", segments.input.clone());
    if segmentation.granularity == "word" {
        let word_like = text.chars().any(char::is_alphanumeric);
        service::data_property(scope, &result, "isWordLike", Value::boolean(word_like));
    }
    Ok(result)
}

fn segment(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let segmenter: Segmenter = service::receiver(scope, this, "Intl.Segmenter.prototype.segment")?;
    let input = scope.to_string_value(&arg(args, 0))?;
    let text = service::text_of(scope, &input)?;
    let segments = Segments {
        segmentation: Rc::new(Segmentation::new(&segmenter, text)),
        input,
    };
    Ok(scope.new_traced_host_object(Some(&arg(data, 0)), segments))
}

fn containing(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let segments: Segments = service::receiver(scope, this, "%Segments.prototype%.containing")?;
    let number = scope.to_number(&arg(args, 0))?;
    let index = if number.is_nan() { 0.0 } else { number.trunc() };
    if index < 0.0 || index >= segments.segmentation.length() as f64 {
        return Ok(Value::undefined());
    }
    let (from, to) = segments.segmentation.around(index as usize);
    segment_data(scope, &segments, from, to)
}

fn iterate(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let segments: Segments =
        service::receiver(scope, this, "%Segments.prototype%[Symbol.iterator]")?;
    let iterator = SegmentIterator {
        segments,
        position: Rc::new(Cell::new(0)),
    };
    Ok(scope.new_traced_host_object(Some(&arg(data, 0)), iterator))
}

fn next(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let iterator: SegmentIterator =
        service::receiver(scope, this, "%SegmentIterator.prototype%.next")?;
    let start = iterator.position.get();
    let segmentation = &iterator.segments.segmentation;
    if start >= segmentation.length() {
        return Ok(service::iter_result(scope, Value::undefined(), true));
    }
    let (from, to) = segmentation.around(start);
    iterator.position.set(segmentation.boundaries[to].unit);
    let data = segment_data(scope, &iterator.segments, from, to)?;
    Ok(service::iter_result(scope, data, false))
}

fn intrinsic_iterator_prototype(scope: &mut Scope<'_>) -> Result<Value, Value> {
    let array = scope.new_array();
    let symbol = service::well_known_symbol(scope, "iterator")?;
    let values = scope.get_key(&array, &symbol)?;
    let iterator = scope.call(&values, &array, &[])?;
    let array_iterator = scope.get_prototype(&iterator)?;
    scope.get_prototype(&array_iterator)
}

pub(crate) fn install(scope: &mut Scope<'_>, intl: &Value) -> Result<(), Value> {
    let constructor = scope.get(intl, "Segmenter")?;
    let prototype = scope.get(&constructor, "prototype")?;
    let iterator_prototype = intrinsic_iterator_prototype(scope)?;
    let segment_iterator = scope.new_object_with_proto(&iterator_prototype);
    let next = scope.function("next", 0, next);
    scope.define(&segment_iterator, "next", next, Attributes::METHOD)?;
    scope.define_to_string_tag(&segment_iterator, "Segmenter String Iterator")?;
    let segments = scope.new_object();
    let containing = scope.function("containing", 1, containing);
    scope.define(&segments, "containing", containing, Attributes::METHOD)?;
    let symbol = service::well_known_symbol(scope, "iterator")?;
    let iterate = scope.bound_function("[Symbol.iterator]", 0, iterate, &[segment_iterator]);
    scope.define_with_key(&segments, &symbol, iterate, Attributes::METHOD)?;
    let segment = scope.bound_function("segment", 1, segment, &[segments]);
    scope.define(&prototype, "segment", segment, Attributes::METHOD)
}
