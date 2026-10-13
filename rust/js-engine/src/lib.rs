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
pub use backend::{
    ENGINE_NAME, Engine, ObjectKey, PropertyDefinition, PropertyKey, Realm, Scope, SetFlags, Value,
    engine_version,
};

mod handles;
mod table;

pub use handles::{
    ContextHandle, InterruptFn, JobOutcome, MemoryUsage, ModuleLoadFn, ModuleNormalizeFn,
    RejectionTrackerFn, RuntimeHandle, RuntimeOptions,
};
pub use table::{
    GetterFn, GetterMagicFn, Prop, SetterFn, SetterMagicFn, accessor, accessor_magic, getter,
    method, method_enumerable,
};

pub type GetOwnHook =
    for<'a> fn(&mut Scope<'a>, &Value, &PropertyKey) -> Result<Option<OwnProperty>, Value>;

pub type GetOwnReceiverHook =
    for<'a> fn(&mut Scope<'a>, &Value, &PropertyKey, &Value) -> Result<Option<OwnProperty>, Value>;

pub type OwnKeysHook = for<'a> fn(&mut Scope<'a>, &Value) -> Result<Vec<PropertyKey>, Value>;

pub type DefineHook =
    for<'a> fn(&mut Scope<'a>, &Value, &PropertyKey, &PropertyDefinition) -> Result<bool, Value>;

pub type DeleteHook = for<'a> fn(&mut Scope<'a>, &Value, &PropertyKey) -> Result<bool, Value>;

pub type SetHook = for<'a> fn(
    &mut Scope<'a>,
    &Value,
    &PropertyKey,
    &Value,
    &Value,
    SetFlags,
) -> Result<bool, Value>;

#[derive(Clone, Copy)]
pub struct ClassHooks {
    pub get_own_property: Option<GetOwnHook>,
    pub get_own_property_receiver: Option<GetOwnReceiverHook>,
    pub own_keys: Option<OwnKeysHook>,
    pub define_own_property: Option<DefineHook>,
    pub delete: Option<DeleteHook>,
    pub set: Option<SetHook>,
}

impl ClassHooks {
    pub const NONE: ClassHooks = ClassHooks {
        get_own_property: None,
        get_own_property_receiver: None,
        own_keys: None,
        define_own_property: None,
        delete: None,
        set: None,
    };
}

pub type RawFinalize = fn(*mut core::ffi::c_void);

pub struct HostClass {
    pub name: &'static core::ffi::CStr,
    pub hooks: ClassHooks,
    pub raw_finalize: Option<RawFinalize>,
    id: core::sync::atomic::AtomicU32,
}

impl HostClass {
    pub const fn new(name: &'static core::ffi::CStr, hooks: ClassHooks) -> HostClass {
        HostClass {
            name,
            hooks,
            raw_finalize: None,
            id: core::sync::atomic::AtomicU32::new(0),
        }
    }

    pub const fn new_raw(
        name: &'static core::ffi::CStr,
        hooks: ClassHooks,
        finalize: RawFinalize,
    ) -> HostClass {
        HostClass {
            name,
            hooks,
            raw_finalize: Some(finalize),
            id: core::sync::atomic::AtomicU32::new(0),
        }
    }

    #[cfg_attr(not(feature = "quickjs"), allow(dead_code))]
    pub(crate) fn assigned_id(&self) -> u32 {
        self.id.load(core::sync::atomic::Ordering::Acquire)
    }

    #[cfg_attr(not(feature = "quickjs"), allow(dead_code))]
    pub(crate) fn id_slot(&self) -> &core::sync::atomic::AtomicU32 {
        &self.id
    }
}

pub struct OwnProperty {
    pub value: Value,
    pub getter: Value,
    pub setter: Value,
    pub accessor: bool,
    pub writable: bool,
    pub enumerable: bool,
    pub configurable: bool,
}

impl OwnProperty {
    pub fn data(value: Value, writable: bool, enumerable: bool, configurable: bool) -> OwnProperty {
        OwnProperty {
            value,
            getter: Value::undefined(),
            setter: Value::undefined(),
            accessor: false,
            writable,
            enumerable,
            configurable,
        }
    }
}
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

impl ElementType {
    pub fn constructor_name(self) -> &'static str {
        match self {
            ElementType::Int8 => "Int8Array",
            ElementType::Uint8 => "Uint8Array",
            ElementType::Uint8Clamped => "Uint8ClampedArray",
            ElementType::Int16 => "Int16Array",
            ElementType::Uint16 => "Uint16Array",
            ElementType::Int32 => "Int32Array",
            ElementType::Uint32 => "Uint32Array",
            ElementType::BigInt64 => "BigInt64Array",
            ElementType::BigUint64 => "BigUint64Array",
            ElementType::Float16 => "Float16Array",
            ElementType::Float32 => "Float32Array",
            ElementType::Float64 => "Float64Array",
        }
    }

    pub fn size(self) -> usize {
        match self {
            ElementType::Int8 | ElementType::Uint8 | ElementType::Uint8Clamped => 1,
            ElementType::Int16 | ElementType::Uint16 | ElementType::Float16 => 2,
            ElementType::Int32 | ElementType::Uint32 | ElementType::Float32 => 4,
            ElementType::BigInt64 | ElementType::BigUint64 | ElementType::Float64 => 8,
        }
    }

    pub const ALL: [ElementType; 12] = [
        ElementType::Int8,
        ElementType::Uint8,
        ElementType::Uint8Clamped,
        ElementType::Int16,
        ElementType::Uint16,
        ElementType::Int32,
        ElementType::Uint32,
        ElementType::BigInt64,
        ElementType::BigUint64,
        ElementType::Float16,
        ElementType::Float32,
        ElementType::Float64,
    ];
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjectKind {
    ArrayBuffer,
    TypedArray(ElementType),
    DataView,
    Date,
    RegExp,
    Map,
    Set,
    Error,
    Number,
    String,
    Boolean,
    BigInt,
    Other,
}

pub struct TypedArrayView {
    pub buffer: Value,
    pub byte_offset: usize,
    pub length: usize,
    pub element: ElementType,
}

pub struct PropertyDescriptor {
    pub value: Value,
    pub getter: Value,
    pub setter: Value,
    pub accessor: bool,
}

pub enum PromiseState {
    NotAPromise,
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Clone, Copy, Default)]
pub struct ScriptOptions<'a> {
    pub name: Option<&'a [u8]>,
    pub line: i32,
    pub column: i32,
    pub hide_source: bool,
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
