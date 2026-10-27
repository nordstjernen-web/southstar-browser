//! Southstar — computed values read from ns_style, selector lists, box hit testing and the box's DOM node.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint};
use core::marker::PhantomData;
use core::ptr::NonNull;

use southstar_css::{StyleData, StyleMap, ValueKind};
use southstar_dom::Node;
use southstar_layout::{BoxRef, Style};

pub enum Value<'a> {
    Keyword(Option<&'a CStr>),
    Length(f64, c_uint),
    Calc(f64, f64),
    Color([u8; 4]),
    Url(Option<&'a CStr>),
    Other,
}

#[derive(Clone, Copy)]
pub struct StyleRef<'a>(NonNull<Style>, PhantomData<&'a Style>);

impl<'a> StyleRef<'a> {
    fn from_ptr(p: *const Style) -> Option<StyleRef<'a>> {
        NonNull::new(p.cast_mut()).map(|p| StyleRef(p, PhantomData))
    }

    pub fn value(self, name: &CStr) -> Option<Value<'a>> {
        let prop = southstar_css::Prop::from_name(name.to_bytes())?.id();
        // SAFETY: a StyleRef points at a computed style made by rust/css that lives for 'a.
        let style = unsafe { self.0.cast::<StyleData>().as_ref() };
        let v = style.value_at(prop)?;
        Some(match v.value_kind() {
            ValueKind::Keyword => Value::Keyword(v.keyword()),
            ValueKind::Length => v
                .length()
                .map_or(Value::Other, |(len, unit)| Value::Length(len, unit)),
            ValueKind::Color => v.color_rgba().map_or(Value::Other, Value::Color),
            ValueKind::Calc => v.calc().map_or(Value::Other, |c| Value::Calc(c.pct, c.px)),
            ValueKind::Url => Value::Url(v.url()),
            _ => Value::Other,
        })
    }
}

pub fn box_style(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    StyleRef::from_ptr(b.style())
}

pub fn box_dom(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
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

    pub fn get(self, node: Node<'_>) -> Option<StyleRef<'_>> {
        StyleRef::from_ptr(self.0?.lookup(node.as_ptr()).cast())
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
