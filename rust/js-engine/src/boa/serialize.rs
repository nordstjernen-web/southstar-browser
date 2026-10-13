//! Southstar — object serialization on Boa: a self-contained byte encoding of structured-cloneable values, with shared objects and cycles written as back-references.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use boa_engine::builtins::error::Error as BoaError;
use boa_engine::object::builtins::{
    AlignedVec, JsArray, JsArrayBuffer, JsDataView, JsDate, JsMap, JsRegExp, JsSet,
};
use boa_engine::property::PropertyKey as BoaPropertyKey;
use boa_engine::{Context, JsBigInt, JsObject, JsString, JsValue};

use super::{HostData, Scope, Value};
use crate::ElementType;

const MAGIC: [u8; 2] = [b'S', 1];
const MAX_DEPTH: u32 = 1000;

const UNDEFINED: u8 = 0;
const NULL: u8 = 1;
const FALSE: u8 = 2;
const TRUE: u8 = 3;
const NUMBER: u8 = 4;
const BIGINT: u8 = 5;
const STRING: u8 = 6;
const REFERENCE: u8 = 7;
const OBJECT: u8 = 8;
const ARRAY: u8 = 9;
const DATE: u8 = 10;
const REGEXP: u8 = 11;
const BOOLEAN_OBJECT: u8 = 12;
const NUMBER_OBJECT: u8 = 13;
const STRING_OBJECT: u8 = 14;
const BIGINT_OBJECT: u8 = 15;
const MAP: u8 = 16;
const SET: u8 = 17;
const ARRAY_BUFFER: u8 = 18;
const RESIZABLE_ARRAY_BUFFER: u8 = 19;
const TYPED_ARRAY: u8 = 20;
const DATA_VIEW: u8 = 21;
const ERROR: u8 = 22;

const ERROR_NAMES: [&str; 8] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
];

const ELEMENTS: [ElementType; 12] = [
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

enum Failure {
    Unclonable(&'static str),
    Thrown(Value),
}

impl From<Value> for Failure {
    fn from(error: Value) -> Failure {
        Failure::Thrown(error)
    }
}

type Written = Result<(), Failure>;

struct Writer<'s, 'a> {
    scope: &'s mut Scope<'a>,
    out: Vec<u8>,
    seen: HashMap<JsObject, u32>,
    depth: u32,
}

impl Writer<'_, '_> {
    fn cx(&mut self) -> &mut Context {
        self.scope.cx()
    }

    fn u32(&mut self, number: u32) {
        self.out.extend_from_slice(&number.to_le_bytes());
    }

    fn u64(&mut self, number: u64) {
        self.out.extend_from_slice(&number.to_le_bytes());
    }

    fn f64(&mut self, number: f64) {
        self.out.extend_from_slice(&number.to_le_bytes());
    }

    fn length(&mut self, len: usize) -> Written {
        let len = u32::try_from(len).map_err(|_| Failure::Unclonable("value is too large"))?;
        self.u32(len);
        Ok(())
    }

    fn string(&mut self, text: &JsString) -> Written {
        let units = text.to_vec();
        self.length(units.len())?;
        for unit in units {
            self.out.extend_from_slice(&unit.to_le_bytes());
        }
        Ok(())
    }

    fn str(&mut self, text: &str) -> Written {
        self.string(&JsString::from(text))
    }

    fn bigint(&mut self, number: &JsBigInt) -> Written {
        self.str(&number.to_string_radix(16))
    }

    fn get(&mut self, object: &JsObject, key: &str) -> Result<JsValue, Failure> {
        let key = JsString::from(key);
        object
            .get(key, self.cx())
            .map_err(|e| Failure::Thrown(self.scope.error(e)))
    }

    fn own(&mut self, object: &JsObject, key: &str) -> Result<Option<JsValue>, Failure> {
        let key = BoaPropertyKey::from(JsString::from(key));
        let has = object
            .has_own_property(key.clone(), self.cx())
            .map_err(|e| Failure::Thrown(self.scope.error(e)))?;
        if !has {
            return Ok(None);
        }
        object
            .get(key, self.cx())
            .map(Some)
            .map_err(|e| Failure::Thrown(self.scope.error(e)))
    }

    fn value(&mut self, value: &JsValue) -> Written {
        if value.is_undefined() {
            self.out.push(UNDEFINED);
        } else if value.is_null() {
            self.out.push(NULL);
        } else if let Some(flag) = value.as_boolean() {
            self.out.push(if flag { TRUE } else { FALSE });
        } else if let Some(number) = value.as_number() {
            self.out.push(NUMBER);
            self.f64(number);
        } else if let Some(text) = value.as_string() {
            self.out.push(STRING);
            self.string(&text)?;
        } else if let Some(number) = value.as_bigint() {
            self.out.push(BIGINT);
            self.bigint(&number)?;
        } else if let Some(object) = value.as_object() {
            self.object(&object)?;
        } else {
            return Err(Failure::Unclonable("a Symbol could not be cloned"));
        }
        Ok(())
    }

    fn object(&mut self, object: &JsObject) -> Written {
        if let Some(&index) = self.seen.get(object) {
            self.out.push(REFERENCE);
            self.u32(index);
            return Ok(());
        }
        if self.depth >= MAX_DEPTH {
            return Err(Failure::Unclonable("value is nested too deeply"));
        }
        let index = u32::try_from(self.seen.len())
            .map_err(|_| Failure::Unclonable("value is too large"))?;
        self.seen.insert(object.clone(), index);
        self.depth += 1;
        let written = self.object_body(object);
        self.depth -= 1;
        written
    }

    fn object_body(&mut self, object: &JsObject) -> Written {
        if object.is_callable() {
            return Err(Failure::Unclonable("a function could not be cloned"));
        }
        if object.is::<HostData>() {
            return Err(Failure::Unclonable("a platform object could not be cloned"));
        }
        if let Ok(buffer) = JsArrayBuffer::from_object(object.clone()) {
            return self.array_buffer(object, &buffer);
        }
        let as_value = Value(object.clone().into());
        if let Some(view) = self.scope.typed_array_view(&as_value)? {
            let element = ELEMENTS
                .iter()
                .position(|&e| e == view.element)
                .unwrap_or_default();
            self.out.push(TYPED_ARRAY);
            self.out.push(element as u8);
            self.u64(view.byte_offset as u64);
            self.u64(view.length as u64);
            return self.value(&view.buffer.0);
        }
        if JsDataView::from_object(object.clone()).is_ok() {
            let buffer = self.get(object, "buffer")?;
            let offset = self.get(object, "byteOffset")?;
            let length = self.get(object, "byteLength")?;
            self.out.push(DATA_VIEW);
            self.f64(offset.as_number().unwrap_or_default());
            self.f64(length.as_number().unwrap_or_default());
            return self.value(&buffer);
        }
        if let Ok(date) = JsDate::from_object(object.clone()) {
            let time = date
                .get_time(self.cx())
                .map_err(|e| Failure::Thrown(self.scope.error(e)))?;
            self.out.push(DATE);
            self.f64(time.as_number().unwrap_or(f64::NAN));
            return Ok(());
        }
        if let Ok(regexp) = JsRegExp::from_object(object.clone()) {
            let source = regexp
                .source(self.cx())
                .map_err(|e| Failure::Thrown(self.scope.error(e)))?;
            let flags = regexp
                .flags(self.cx())
                .map_err(|e| Failure::Thrown(self.scope.error(e)))?;
            self.out.push(REGEXP);
            self.str(&source)?;
            return self.str(&flags);
        }
        if let Ok(map) = JsMap::from_object(object.clone()) {
            return self.map(&map);
        }
        if let Ok(set) = JsSet::from_object(object.clone()) {
            return self.set(&set);
        }
        if object.is::<BoaError>() {
            return self.error(object);
        }
        if let Some(flag) = object.downcast_ref::<bool>().map(|flag| *flag) {
            self.out.push(BOOLEAN_OBJECT);
            self.out.push(u8::from(flag));
            return Ok(());
        }
        if let Some(number) = object.downcast_ref::<f64>().map(|number| *number) {
            self.out.push(NUMBER_OBJECT);
            self.f64(number);
            return Ok(());
        }
        if let Some(text) = object.downcast_ref::<JsString>().map(|text| text.clone()) {
            self.out.push(STRING_OBJECT);
            return self.string(&text);
        }
        if let Some(number) = object
            .downcast_ref::<JsBigInt>()
            .map(|number| number.clone())
        {
            self.out.push(BIGINT_OBJECT);
            return self.bigint(&number);
        }
        if as_value.is_array() {
            let length = self.get(object, "length")?;
            self.out.push(ARRAY);
            self.f64(length.as_number().unwrap_or_default());
        } else {
            self.out.push(OBJECT);
        }
        self.properties(object)
    }

    fn properties(&mut self, object: &JsObject) -> Written {
        let keys = self
            .scope
            .own_enumerable_keys(&Value(object.clone().into()))?;
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let Some(name) = key.0.as_string() else {
                continue;
            };
            let property = object
                .get(BoaPropertyKey::from(name.clone()), self.cx())
                .map_err(|e| Failure::Thrown(self.scope.error(e)))?;
            entries.push((name, property));
        }
        self.length(entries.len())?;
        for (name, property) in entries {
            self.string(&name)?;
            self.value(&property)?;
        }
        Ok(())
    }

    fn array_buffer(&mut self, object: &JsObject, buffer: &JsArrayBuffer) -> Written {
        let Some(bytes) = buffer.data().map(|data| data.to_vec()) else {
            return Err(Failure::Unclonable(
                "a detached ArrayBuffer could not be cloned",
            ));
        };
        let resizable = self.get(object, "resizable")?.to_boolean();
        if resizable {
            let max = self.get(object, "maxByteLength")?;
            self.out.push(RESIZABLE_ARRAY_BUFFER);
            self.f64(max.as_number().unwrap_or_default());
        } else {
            self.out.push(ARRAY_BUFFER);
        }
        self.length(bytes.len())?;
        self.out.extend_from_slice(&bytes);
        Ok(())
    }

    fn drain(&mut self, iterator: &JsObject) -> Result<Vec<JsValue>, Failure> {
        let next = self.get(iterator, "next")?;
        let mut items = Vec::new();
        loop {
            let step = self
                .scope
                .call(&Value(next.clone()), &Value(iterator.clone().into()), &[])?
                .0;
            let Some(step) = step.as_object() else {
                return Ok(items);
            };
            if self.get(&step, "done")?.to_boolean() {
                return Ok(items);
            }
            items.push(self.get(&step, "value")?);
        }
    }

    fn map(&mut self, map: &JsMap) -> Written {
        let iterator = map
            .entries(self.cx())
            .map_err(|e| Failure::Thrown(self.scope.error(e)))?;
        let entries = self.drain(&iterator.into())?;
        let mut pairs = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let key = self.get(&entry, "0")?;
            let value = self.get(&entry, "1")?;
            pairs.push((key, value));
        }
        self.out.push(MAP);
        self.length(pairs.len())?;
        for (key, value) in pairs {
            self.value(&key)?;
            self.value(&value)?;
        }
        Ok(())
    }

    fn set(&mut self, set: &JsSet) -> Written {
        let iterator = set
            .values(self.cx())
            .map_err(|e| Failure::Thrown(self.scope.error(e)))?;
        let values = self.drain(&iterator.into())?;
        self.out.push(SET);
        self.length(values.len())?;
        for value in values {
            self.value(&value)?;
        }
        Ok(())
    }

    fn optional_string(&mut self, value: Option<JsValue>) -> Written {
        match value.and_then(|value| value.as_string()) {
            Some(text) => {
                self.out.push(1);
                self.string(&text)
            }
            None => {
                self.out.push(0);
                Ok(())
            }
        }
    }

    fn error(&mut self, object: &JsObject) -> Written {
        let name = self.get(object, "name")?;
        let name = name
            .as_string()
            .map(|name| name.to_std_string_escaped())
            .filter(|name| ERROR_NAMES.contains(&name.as_str()))
            .unwrap_or_else(|| "Error".to_owned());
        let message = match self.own(object, "message")? {
            Some(message) => Some(JsValue::from(
                message
                    .to_string(self.cx())
                    .map_err(|e| Failure::Thrown(self.scope.error(e)))?,
            )),
            None => None,
        };
        let stack = self.get(object, "stack")?;
        let cause = self.own(object, "cause")?;
        self.out.push(ERROR);
        self.str(&name)?;
        self.optional_string(message)?;
        self.optional_string(Some(stack))?;
        match cause {
            Some(cause) => {
                self.out.push(1);
                self.value(&cause)
            }
            None => {
                self.out.push(0);
                Ok(())
            }
        }
    }
}

struct Reader<'s, 'a, 'b> {
    scope: &'s mut Scope<'a>,
    bytes: &'b [u8],
    at: usize,
    objects: Vec<Option<JsObject>>,
    depth: u32,
}

type Read<T> = Result<T, Value>;

impl Reader<'_, '_, '_> {
    fn cx(&mut self) -> &mut Context {
        self.scope.cx()
    }

    fn invalid(&mut self) -> Value {
        self.scope.type_error("invalid serialized value")
    }

    fn take(&mut self, len: usize) -> Read<&[u8]> {
        let end = self
            .at
            .checked_add(len)
            .filter(|&end| end <= self.bytes.len());
        let Some(end) = end else {
            return Err(self.invalid());
        };
        let start = self.at;
        self.at = end;
        Ok(&self.bytes[start..end])
    }

    fn u8(&mut self) -> Read<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Read<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&mut self) -> Read<u64> {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(raw))
    }

    fn f64(&mut self) -> Read<f64> {
        Ok(f64::from_bits(self.u64()?))
    }

    fn string(&mut self) -> Read<JsString> {
        let len = self.u32()? as usize;
        let Some(byte_len) = len.checked_mul(2) else {
            return Err(self.invalid());
        };
        let units: Vec<u16> = self
            .take(byte_len)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        Ok(JsString::from(units.as_slice()))
    }

    fn bigint(&mut self) -> Read<JsBigInt> {
        let text = self.string()?.to_std_string_escaped();
        match JsBigInt::from_string_radix(&text, 16) {
            Some(number) => Ok(number),
            None => Err(self.invalid()),
        }
    }

    fn optional_string(&mut self) -> Read<Option<JsString>> {
        match self.u8()? {
            0 => Ok(None),
            _ => self.string().map(Some),
        }
    }

    fn reserve(&mut self) -> usize {
        self.objects.push(None);
        self.objects.len() - 1
    }

    fn fill(&mut self, slot: usize, object: &JsObject) {
        self.objects[slot] = Some(object.clone());
    }

    fn thrown<T>(&mut self, result: boa_engine::JsResult<T>) -> Read<T> {
        result.map_err(|e| self.scope.error(e))
    }

    fn value(&mut self) -> Read<JsValue> {
        let tag = self.u8()?;
        Ok(match tag {
            UNDEFINED => JsValue::undefined(),
            NULL => JsValue::null(),
            FALSE => JsValue::from(false),
            TRUE => JsValue::from(true),
            NUMBER => JsValue::from(self.f64()?),
            BIGINT => JsValue::from(self.bigint()?),
            STRING => JsValue::from(self.string()?),
            REFERENCE => {
                let index = self.u32()? as usize;
                match self.objects.get(index).cloned().flatten() {
                    Some(object) => object.into(),
                    None => return Err(self.invalid()),
                }
            }
            _ => {
                if self.depth >= MAX_DEPTH {
                    return Err(self.invalid());
                }
                self.depth += 1;
                let object = self.object(tag);
                self.depth -= 1;
                object?.into()
            }
        })
    }

    fn object(&mut self, tag: u8) -> Read<JsObject> {
        let slot = self.reserve();
        let object = match tag {
            OBJECT => {
                let object = JsObject::with_object_proto(self.cx().intrinsics());
                self.fill(slot, &object);
                self.properties(&object)?;
                object
            }
            ARRAY => {
                let length = self.f64()?;
                let array: JsObject = JsArray::new(self.cx())
                    .map_err(|e| self.scope.error(e))?
                    .into();
                self.fill(slot, &array);
                let set = array.set(JsString::from("length"), length, true, self.cx());
                self.thrown(set)?;
                self.properties(&array)?;
                array
            }
            DATE => {
                let time = self.f64()?;
                let date = self.cx().intrinsics().constructors().date().constructor();
                let date = date.construct(&[time.into()], None, self.cx());
                self.thrown(date)?
            }
            REGEXP => {
                let source = self.string()?;
                let flags = self.string()?;
                let regexp = JsRegExp::new(source, flags, self.cx());
                self.thrown(regexp)?.into()
            }
            BOOLEAN_OBJECT => {
                let flag = self.u8()? != 0;
                self.wrap(JsValue::from(flag))?
            }
            NUMBER_OBJECT => {
                let number = self.f64()?;
                self.wrap(JsValue::from(number))?
            }
            STRING_OBJECT => {
                let text = self.string()?;
                self.wrap(JsValue::from(text))?
            }
            BIGINT_OBJECT => {
                let number = self.bigint()?;
                self.wrap(JsValue::from(number))?
            }
            MAP => {
                let map = JsMap::new(self.cx());
                self.fill(slot, &map.clone().into());
                for _ in 0..self.u32()? {
                    let key = self.value()?;
                    let value = self.value()?;
                    let set = map.set(key, value, self.cx());
                    self.thrown(set)?;
                }
                map.into()
            }
            SET => {
                let set = JsSet::new(self.cx());
                self.fill(slot, &set.clone().into());
                for _ in 0..self.u32()? {
                    let value = self.value()?;
                    let added = set.add(value, self.cx());
                    self.thrown(added)?;
                }
                set.into()
            }
            ARRAY_BUFFER => self.array_buffer(None)?,
            RESIZABLE_ARRAY_BUFFER => {
                let max = self.f64()?;
                self.array_buffer(Some(max))?
            }
            TYPED_ARRAY => self.typed_array()?,
            DATA_VIEW => {
                let offset = self.f64()?;
                let length = self.f64()?;
                let buffer = self.value()?;
                let view = self
                    .cx()
                    .intrinsics()
                    .constructors()
                    .data_view()
                    .constructor();
                let view = view.construct(&[buffer, offset.into(), length.into()], None, self.cx());
                self.thrown(view)?
            }
            ERROR => self.error(slot)?,
            _ => return Err(self.invalid()),
        };
        self.fill(slot, &object);
        Ok(object)
    }

    fn wrap(&mut self, primitive: JsValue) -> Read<JsObject> {
        let object = primitive.to_object(self.cx());
        self.thrown(object)
    }

    fn properties(&mut self, object: &JsObject) -> Read<()> {
        for _ in 0..self.u32()? {
            let key = self.string()?;
            let value = self.value()?;
            let defined =
                object.create_data_property_or_throw(BoaPropertyKey::from(key), value, self.cx());
            self.thrown(defined)?;
        }
        Ok(())
    }

    fn array_buffer(&mut self, max: Option<f64>) -> Read<JsObject> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?.to_vec();
        let Some(max) = max else {
            let block = AlignedVec::from_iter(0, bytes);
            let buffer = JsArrayBuffer::from_byte_block(block, self.cx());
            return Ok(self.thrown(buffer)?.into());
        };
        let options = JsObject::with_object_proto(self.cx().intrinsics());
        let set = options.set(JsString::from("maxByteLength"), max, true, self.cx());
        self.thrown(set)?;
        let constructor = self
            .cx()
            .intrinsics()
            .constructors()
            .array_buffer()
            .constructor();
        let created = constructor.construct(&[len.into(), options.into()], None, self.cx());
        let object = self.thrown(created)?;
        if let Ok(buffer) = JsArrayBuffer::from_object(object.clone())
            && let Some(mut data) = buffer.data_mut()
            && data.len() == bytes.len()
        {
            data.copy_from_slice(&bytes);
        }
        Ok(object)
    }

    fn typed_array(&mut self) -> Read<JsObject> {
        let element = self.u8()? as usize;
        let offset = self.u64()? as f64;
        let length = self.u64()? as f64;
        let buffer = self.value()?;
        let Some(&element) = ELEMENTS.get(element) else {
            return Err(self.invalid());
        };
        let constructors = self.cx().intrinsics().constructors();
        let constructor = match element {
            ElementType::Int8 => constructors.typed_int8_array(),
            ElementType::Uint8 => constructors.typed_uint8_array(),
            ElementType::Uint8Clamped => constructors.typed_uint8clamped_array(),
            ElementType::Int16 => constructors.typed_int16_array(),
            ElementType::Uint16 => constructors.typed_uint16_array(),
            ElementType::Int32 => constructors.typed_int32_array(),
            ElementType::Uint32 => constructors.typed_uint32_array(),
            ElementType::BigInt64 => constructors.typed_bigint64_array(),
            ElementType::BigUint64 => constructors.typed_biguint64_array(),
            ElementType::Float16 => constructors.typed_float16_array(),
            ElementType::Float32 => constructors.typed_float32_array(),
            ElementType::Float64 => constructors.typed_float64_array(),
        }
        .constructor();
        let array = constructor.construct(&[buffer, offset.into(), length.into()], None, self.cx());
        self.thrown(array)
    }

    fn error(&mut self, slot: usize) -> Read<JsObject> {
        let name = self.string()?.to_std_string_escaped();
        let message = self.optional_string()?;
        let stack = self.optional_string()?;
        let has_cause = self.u8()? != 0;
        let constructors = self.cx().intrinsics().constructors();
        let constructor = match name.as_str() {
            "EvalError" => constructors.eval_error(),
            "RangeError" => constructors.range_error(),
            "ReferenceError" => constructors.reference_error(),
            "SyntaxError" => constructors.syntax_error(),
            "TypeError" => constructors.type_error(),
            "URIError" => constructors.uri_error(),
            _ => constructors.error(),
        }
        .constructor();
        let args: Vec<JsValue> = message.into_iter().map(JsValue::from).collect();
        let created = constructor.construct(&args, None, self.cx());
        let error = self.thrown(created)?;
        self.fill(slot, &error);
        if name == "AggregateError" {
            let set = error.set(
                JsString::from("name"),
                JsString::from(name),
                true,
                self.cx(),
            );
            self.thrown(set)?;
        }
        if let Some(stack) = stack {
            let set = error.set(JsString::from("stack"), stack, true, self.cx());
            self.thrown(set)?;
        }
        if has_cause {
            let cause = self.value()?;
            let set = error.set(JsString::from("cause"), cause, true, self.cx());
            self.thrown(set)?;
        }
        Ok(error)
    }
}

impl Scope<'_> {
    pub fn write_object(&mut self, value: &Value) -> Result<Vec<u8>, Option<Value>> {
        let mut writer = Writer {
            scope: self,
            out: MAGIC.to_vec(),
            seen: HashMap::new(),
            depth: 0,
        };
        match writer.value(&value.0) {
            Ok(()) => Ok(writer.out),
            Err(Failure::Thrown(error)) => Err(Some(error)),
            Err(Failure::Unclonable(message)) => {
                Err(Some(self.dom_exception("DataCloneError", message)))
            }
        }
    }

    pub fn read_object(&mut self, bytes: &[u8]) -> Result<Value, Value> {
        let Some(body) = bytes.strip_prefix(&MAGIC) else {
            return Err(self.type_error("invalid serialized value"));
        };
        let mut reader = Reader {
            scope: self,
            bytes: body,
            at: 0,
            objects: Vec::new(),
            depth: 0,
        };
        let value = reader.value()?;
        if reader.at != body.len() {
            return Err(reader.invalid());
        }
        Ok(Value(value))
    }
}
