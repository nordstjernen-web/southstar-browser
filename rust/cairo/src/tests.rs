//! Southstar — unit tests for the Cairo values and the path walking that need no libcairo.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::{PathData, PathHeader, decode, transform_points};
use crate::{
    Antialias, Content, Extend, FillRule, Filter, Format, HintMetrics, LineCap, LineJoin, Matrix,
    Operator, PathSegment, PdfMetadata, SubpixelOrder,
};

fn header(kind: i32, length: i32) -> PathData {
    PathData {
        header: PathHeader { kind, length },
    }
}

fn point(x: f64, y: f64) -> PathData {
    PathData { point: [x, y] }
}

#[test]
fn enums_match_cairo_h() {
    assert_eq!(Format::Argb32 as i32, 0);
    assert_eq!(Format::Rgb24 as i32, 1);
    assert_eq!(Format::A8 as i32, 2);
    assert_eq!(Content::Alpha as i32, 0x2000);
    assert_eq!(Content::ColorAlpha as i32, 0x3000);
    assert_eq!(Operator::Clear as i32, 0);
    assert_eq!(Operator::Source as i32, 1);
    assert_eq!(Operator::Over as i32, 2);
    assert_eq!(Operator::Xor as i32, 11);
    assert_eq!(Operator::Add as i32, 12);
    assert_eq!(Operator::Multiply as i32, 14);
    assert_eq!(Operator::HslLuminosity as i32, 28);
    assert_eq!(FillRule::EvenOdd as i32, 1);
    assert_eq!(LineCap::Square as i32, 2);
    assert_eq!(LineJoin::Bevel as i32, 2);
    assert_eq!(Extend::Pad as i32, 3);
    assert_eq!(Filter::Nearest as i32, 3);
    assert_eq!(Antialias::Fast as i32, 4);
    assert_eq!(SubpixelOrder::Default as i32, 0);
    assert_eq!(HintMetrics::Off as i32, 1);
    assert_eq!(PdfMetadata::Creator as i32, 4);
    assert_eq!(PdfMetadata::CreateDate as i32, 5);
}

#[test]
fn operators_round_trip() {
    for op in 0..=28 {
        assert_eq!(Operator::from_raw(op) as i32, op);
    }
    assert_eq!(Operator::from_raw(99), Operator::Over);
    assert_eq!(FillRule::from_raw(1), FillRule::EvenOdd);
    assert_eq!(FillRule::from_raw(0), FillRule::Winding);
}

#[test]
fn matrix_layout_and_constructors() {
    assert_eq!(core::mem::size_of::<Matrix>(), 48);
    assert_eq!(core::mem::size_of::<PathData>(), 16);
    assert_eq!(
        Matrix::identity(),
        Matrix::from([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    );
    assert_eq!(
        Matrix::translation(3.0, 4.0).to_array(),
        [1.0, 0.0, 0.0, 1.0, 3.0, 4.0]
    );
    assert_eq!(
        Matrix::scaling(2.0, 5.0).to_array(),
        [2.0, 0.0, 0.0, 5.0, 0.0, 0.0]
    );
}

#[test]
fn decodes_every_segment_kind() {
    let data = [
        header(0, 2),
        point(1.0, 2.0),
        header(1, 2),
        point(3.0, 4.0),
        header(2, 4),
        point(5.0, 6.0),
        point(7.0, 8.0),
        point(9.0, 10.0),
        header(3, 1),
    ];
    assert_eq!(
        decode(&data),
        vec![
            PathSegment::MoveTo(1.0, 2.0),
            PathSegment::LineTo(3.0, 4.0),
            PathSegment::CurveTo([5.0, 6.0, 7.0, 8.0, 9.0, 10.0]),
            PathSegment::ClosePath,
        ]
    );
}

#[test]
fn decode_stops_at_truncated_data() {
    let data = [header(0, 2), point(1.0, 2.0), header(2, 4), point(5.0, 6.0)];
    assert_eq!(decode(&data), vec![PathSegment::MoveTo(1.0, 2.0)]);
    assert!(decode(&[header(1, 0)]).is_empty());
}

#[test]
fn transforms_points_but_not_headers() {
    let mut data = [
        header(0, 2),
        point(1.0, 2.0),
        header(1, 2),
        point(3.0, 4.0),
        header(3, 1),
    ];
    transform_points(&mut data, &Matrix::new(2.0, 0.0, 0.0, 3.0, 10.0, 20.0));
    assert_eq!(
        decode(&data),
        vec![
            PathSegment::MoveTo(12.0, 26.0),
            PathSegment::LineTo(16.0, 32.0),
            PathSegment::ClosePath,
        ]
    );
}
