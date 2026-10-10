//! Southstar — the engine-neutral JavaScript layer: one API over the in-tree QuickJS-ng fork or Boa, picked by a Cargo feature.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(all(feature = "quickjs", feature = "boa"))]
compile_error!("enable exactly one JavaScript engine: the `quickjs` or the `boa` feature");
#[cfg(not(any(feature = "quickjs", feature = "boa")))]
compile_error!("enable a JavaScript engine: the `quickjs` or the `boa` feature");

#[cfg(feature = "boa")]
mod boa;
#[cfg(feature = "quickjs")]
mod quickjs_ffi;

#[cfg(feature = "boa")]
use boa as backend;
#[cfg(feature = "quickjs")]
use quickjs_ffi as backend;

#[cfg(feature = "quickjs")]
pub use backend::quickjs;
pub use backend::{ENGINE_NAME, Engine, Realm, Scope, Value, engine_version};

pub type NativeFn = for<'a> fn(&mut Scope<'a>, &Value, &[Value]) -> Result<Value, Value>;

pub type BoundFn = for<'a> fn(&mut Scope<'a>, &Value, &[Value], &[Value]) -> Result<Value, Value>;

pub type RealmInit = for<'a> fn(&mut Scope<'a>) -> Result<(), Value>;

pub type Job = for<'a> fn(&mut Scope<'a>);

pub trait Trace {
    fn trace(&self, visit: &mut dyn FnMut(&Value));
}

pub struct TypedArrayBytes<'a> {
    pub bytes: &'a [u8],
    pub byte_offset: usize,
    pub element_size: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElementType {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    BigInt64,
    BigUint64,
    Float16,
    Float32,
    Float64,
}

pub struct PropertyDescriptor {
    pub value: Value,
    pub getter: Value,
    pub setter: Value,
}

pub enum PromiseState {
    NotAPromise,
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Clone, Copy)]
pub struct Attributes {
    pub writable: bool,
    pub enumerable: bool,
    pub configurable: bool,
}

impl Attributes {
    pub const ENUMERABLE: Attributes = Attributes {
        writable: false,
        enumerable: true,
        configurable: false,
    };
    pub const CONFIGURABLE: Attributes = Attributes {
        writable: false,
        enumerable: false,
        configurable: true,
    };
    pub const METHOD: Attributes = Attributes {
        writable: true,
        enumerable: false,
        configurable: true,
    };
}

pub fn int64_modulo(number: f64) -> i64 {
    let bits = number.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i64;
    if exponent <= 1023 + 62 {
        number as i64
    } else if exponent <= 1023 + 62 + 53 {
        let mantissa = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
        let magnitude = (mantissa << (exponent - 1023 - 52)) as i64;
        if bits >> 63 != 0 && magnitude != i64::MIN {
            -magnitude
        } else {
            magnitude
        }
    } else {
        0
    }
}
