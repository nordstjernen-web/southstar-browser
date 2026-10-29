//! Southstar — computed values read through rust/style, selector lists, box hit testing and the box's DOM node.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint};

use southstar_css::StyleMap;
use southstar_dom::Node;
use southstar_layout::BoxRef;
use southstar_style::Kind;

pub enum Value<'a> {
    Keyword(Option<&'a CStr>),
    Length(f64, c_uint),
    Calc(f64, f64),
    Color([u8; 4]),
    Url(Option<&'a CStr>),
    Other,
}

#[derive(Clone, Copy)]
pub struct StyleRef<'a>(southstar_style::StyleRef<'a>);

impl<'a> StyleRef<'a> {
    pub fn value(self, name: &CStr) -> Option<Value<'a>> {
        let prop = southstar_css::Prop::from_name(name.to_bytes())?.id();
        let v = self.0.value_at(prop)?.data();
        Some(match v.value_kind() {
            Kind::Keyword => Value::Keyword(v.keyword()),
            Kind::Length => v
                .length()
                .map_or(Value::Other, |(v, unit)| Value::Length(v, unit)),
            Kind::Color => v.color_rgba().map_or(Value::Other, Value::Color),
            Kind::Calc => v
                .calc()
                .map_or(Value::Other, |calc| Value::Calc(calc.pct, calc.px)),
            Kind::Url => Value::Url(v.url()),
            _ => Value::Other,
        })
    }
}

pub fn box_style(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b).map(StyleRef)
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn hit_test(root: Option<BoxRef<'_>>, x: f64, y: f64) -> Option<BoxRef<'_>> {
    root.and_then(|root| southstar_layout_hit::hit_test(root, x, y))
}

#[derive(Clone, Copy)]
pub struct StyleTable<'a>(Option<&'a StyleMap>);

impl<'a> StyleTable<'a> {
    pub fn new(table: Option<&'a StyleMap>) -> StyleTable<'a> {
        StyleTable(table)
    }

    pub fn get(self, node: Node<'_>) -> Option<StyleRef<'a>> {
        let style = self.0?.get(node.as_ptr())?;
        Some(StyleRef(southstar_style::StyleRef::from_data(style)))
    }
}

pub struct Selectors(southstar_css::SelectorList);

impl Selectors {
    pub fn parse(text: &CStr) -> Option<Selectors> {
        southstar_css::SelectorList::parse_lenient(text.to_bytes()).map(Selectors)
    }

    pub fn any_matches(&self, el: Node) -> bool {
        self.0.matches_any(el)
    }
}
