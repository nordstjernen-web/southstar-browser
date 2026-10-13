//! Southstar — property tables on Boa: each entry becomes a native function or an accessor pair defined on the target when the table is installed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use boa_engine::object::FunctionObjectBuilder;
use boa_engine::{JsError, JsString, JsValue, NativeFunction};

use super::{Scope, Value, access};
use crate::{Attributes, Prop};

const ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

impl Scope<'_> {
    fn accessor_function(&mut self, name: String, arity: usize, native: NativeFunction) -> Value {
        let realm = self.cx().realm().clone();
        let function = FunctionObjectBuilder::new(&realm, native)
            .name(JsString::from(name.as_str()))
            .length(arity)
            .build();
        Value(function.into())
    }

    fn table_getter(&mut self, prop: &Prop) -> Option<Value> {
        let name = format!("get {}", prop.name());
        let native = match *prop {
            Prop::Accessor { get: Some(get), .. } => {
                NativeFunction::from_copy_closure(move |this, _args, ctx| {
                    access::in_native(ctx.frame_depth(), || {
                        get(&mut Scope::of(ctx), &Value(this.clone()))
                    })
                    .map(|value| value.0)
                    .map_err(|error| JsError::from_opaque(error.0))
                })
            }
            Prop::AccessorMagic {
                get: Some(get),
                magic,
                ..
            } => NativeFunction::from_copy_closure(move |this, _args, ctx| {
                access::in_native(ctx.frame_depth(), || {
                    get(&mut Scope::of(ctx), &Value(this.clone()), i32::from(magic))
                })
                .map(|value| value.0)
                .map_err(|error| JsError::from_opaque(error.0))
            }),
            _ => return None,
        };
        Some(self.accessor_function(name, 0, native))
    }

    fn table_setter(&mut self, prop: &Prop) -> Option<Value> {
        let name = format!("set {}", prop.name());
        let native = match *prop {
            Prop::Accessor { set: Some(set), .. } => {
                NativeFunction::from_copy_closure(move |this, args, ctx| {
                    let value = Value(args.first().cloned().unwrap_or_default());
                    access::in_native(ctx.frame_depth(), || {
                        set(&mut Scope::of(ctx), &Value(this.clone()), &value)
                    })
                    .map(|()| JsValue::undefined())
                    .map_err(|error| JsError::from_opaque(error.0))
                })
            }
            Prop::AccessorMagic {
                set: Some(set),
                magic,
                ..
            } => NativeFunction::from_copy_closure(move |this, args, ctx| {
                let value = Value(args.first().cloned().unwrap_or_default());
                access::in_native(ctx.frame_depth(), || {
                    set(
                        &mut Scope::of(ctx),
                        &Value(this.clone()),
                        &value,
                        i32::from(magic),
                    )
                })
                .map(|()| JsValue::undefined())
                .map_err(|error| JsError::from_opaque(error.0))
            }),
            _ => return None,
        };
        Some(self.accessor_function(name, 1, native))
    }

    pub fn install_props(&mut self, object: &Value, props: &'static [Prop]) {
        if !object.is_object() {
            return;
        }
        for prop in props {
            match *prop {
                Prop::Method {
                    name,
                    arity,
                    f,
                    enumerable,
                } => {
                    let function = self.function(name, u32::from(arity), f);
                    let attributes = Attributes {
                        writable: true,
                        enumerable,
                        configurable: true,
                    };
                    let _ = self.define(object, name, function, attributes);
                }
                Prop::Accessor { name, .. } | Prop::AccessorMagic { name, .. } => {
                    let getter = self.table_getter(prop);
                    let setter = self.table_setter(prop);
                    let _ = self.define_accessor(
                        object,
                        name,
                        getter.as_ref(),
                        setter.as_ref(),
                        ACCESSOR,
                    );
                }
            }
        }
    }
}
