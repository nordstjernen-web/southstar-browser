use crate::{
    Context, JsArgs, JsObject, JsResult, JsSymbol, JsValue,
    builtins::{
        IntrinsicObject,
        array::Array,
        builder::BuiltInBuilder,
        iterable::{
            IteratorRecord, get_iterator_direct, if_abrupt_close_iterator,
            iterator_helper::{self, IteratorHelper},
        },
    },
    context::intrinsics::Intrinsics,
    js_error, js_string,
    object::{CONSTRUCTOR, JsFunction},
    property::{Attribute, PropertyKey},
    realm::Realm,
    value::{IntegerOrInfinity, TryFromJs},
};

/// `%IteratorPrototype%` object
///
/// More information:
///  - [ECMA reference][spec]
///
/// [spec]: https://tc39.es/ecma262/#sec-%iteratorprototype%-object
pub(crate) struct Iterator;

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

impl IntrinsicObject for Iterator {
    fn init(realm: &Realm) {
        let get_constructor = BuiltInBuilder::callable(realm, Self::get_constructor)
            .name(js_string!("get constructor"))
            .build();
        let set_constructor = BuiltInBuilder::callable(realm, Self::set_constructor)
            .name(js_string!("set constructor"))
            .build();
        let get_to_string_tag = BuiltInBuilder::callable(realm, Self::get_to_string_tag)
            .name(js_string!("get [Symbol.toStringTag]"))
            .build();
        let set_to_string_tag = BuiltInBuilder::callable(realm, Self::set_to_string_tag)
            .name(js_string!("set [Symbol.toStringTag]"))
            .build();
        let builder = BuiltInBuilder::with_intrinsic::<Self>(realm)
            .static_method(|v, _, _| Ok(v.clone()), JsSymbol::iterator(), 0)
            .static_method(Self::map, js_string!("map"), 1)
            .static_method(Self::filter, js_string!("filter"), 1)
            .static_method(Self::take, js_string!("take"), 1)
            .static_method(Self::drop, js_string!("drop"), 1)
            .static_method(Self::flat_map, js_string!("flatMap"), 1)
            .static_method(Self::reduce, js_string!("reduce"), 1)
            .static_method(Self::to_array, js_string!("toArray"), 0)
            .static_method(Self::for_each, js_string!("forEach"), 1)
            .static_method(Self::some, js_string!("some"), 1)
            .static_method(Self::every, js_string!("every"), 1)
            .static_method(Self::find, js_string!("find"), 1)
            .static_method(Self::join, js_string!("join"), 1)
            .static_method(Self::chunks, js_string!("chunks"), 1)
            .static_method(Self::windows, js_string!("windows"), 1)
            .static_method(Self::dispose, JsSymbol::dispose(), 0)
            .static_accessor(
                JsSymbol::to_string_tag(),
                Some(get_to_string_tag),
                Some(set_to_string_tag),
                Attribute::CONFIGURABLE,
            )
            .static_accessor(
                CONSTRUCTOR,
                Some(get_constructor),
                Some(set_constructor),
                Attribute::CONFIGURABLE,
            );

        #[cfg(feature = "experimental")]
        let builder = builder.static_method(Self::includes, js_string!("includes"), 1);

        builder.build();
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        intrinsics.constructors().iterator().prototype()
    }
}

impl Iterator {
    /// `get Iterator.prototype.constructor`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.constructor
    #[allow(clippy::unnecessary_wraps)]
    fn get_constructor(
        _this: &JsValue,
        _args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Ok(context
            .intrinsics()
            .constructors()
            .iterator()
            .constructor()
            .into())
    }

    /// `set Iterator.prototype.constructor`
    ///
    /// `SetterThatIgnoresPrototypeProperties(this, %Iterator.prototype%, "constructor", v)`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.constructor
    fn set_constructor(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::setter_that_ignores_prototype_properties(
            this,
            &context.intrinsics().constructors().iterator().prototype(),
            js_string!("constructor"),
            args.get_or_undefined(0),
            context,
        )
    }

    /// `get Iterator.prototype[@@toStringTag]`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype-%40%40tostringtag
    #[allow(clippy::unnecessary_wraps)]
    fn get_to_string_tag(
        _this: &JsValue,
        _args: &[JsValue],
        _context: &mut Context,
    ) -> JsResult<JsValue> {
        Ok(js_string!("Iterator").into())
    }

    /// `set Iterator.prototype[@@toStringTag]`
    ///
    /// `SetterThatIgnoresPrototypeProperties(this, %Iterator.prototype%, @@toStringTag, v)`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype-%40%40tostringtag
    fn set_to_string_tag(
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        Self::setter_that_ignores_prototype_properties(
            this,
            &context.intrinsics().constructors().iterator().prototype(),
            JsSymbol::to_string_tag(),
            args.get_or_undefined(0),
            context,
        )
    }

    /// `SetterThatIgnoresPrototypeProperties ( this, home, p, v )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-SetterThatIgnoresPrototypeProperties
    pub(crate) fn setter_that_ignores_prototype_properties<K: Into<PropertyKey>>(
        this: &JsValue,
        home: &JsObject,
        p: K,
        v: &JsValue,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        let p = p.into();

        // 1. If this is not an Object, then
        let Some(this_obj) = this.as_object() else {
            // a. Throw a TypeError exception.
            return Err(js_error!(TypeError: "Cannot set property on a non-object"));
        };

        // 2. If this is home, then
        if JsObject::equals(&this_obj, home) {
            // a. NOTE: Throwing here emulates the behavior of a Set handler ...
            // b. Throw a TypeError exception.
            return Err(js_error!(TypeError: "Cannot set property directly on the prototype"));
        }

        // 3. Let desc be ? this.[[GetOwnProperty]](p).
        let desc = this_obj.__get_own_property__(&p, &mut context.into())?;

        // 4. If desc is undefined, then
        if desc.is_none() {
            // a. Perform ? CreateDataPropertyOrThrow(this, p, v).
            this_obj.create_data_property_or_throw(p, v.clone(), context)?;
        } else {
            // 5. Else,
            //    a. Perform ? Set(this, p, v, true).
            this_obj.set(p, v.clone(), true, context)?;
        }

        // 6. Return undefined.
        Ok(JsValue::undefined())
    }

    // ==================== Prototype Methods — Lazy ====================

    /// `Iterator.prototype.map ( mapper )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.map
    fn map(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.prototype.map called on non-object"))?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(mapper) is false, then
        //    a. Let error be ThrowCompletion(a newly created TypeError object).
        //    b. Return ? IteratorClose(iterated, error).
        let mapper = args.get_or_undefined(0);
        let Ok(mapper) = JsFunction::try_from_js(mapper, context) else {
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.map: mapper is not callable"
                )),
                context,
            );
        };
        // 5. Set iterated to ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 6-8 are deferred to `IteratorHelper::create` and `Map::new`.
        let result = IteratorHelper::create(iterator_helper::Map::new(iterated, mapper), context);

        // 9. Return result.
        Ok(result.into())
    }

    /// `Iterator.prototype.filter ( predicate )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.filter
    fn filter(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.filter called on non-object"),
        )?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(predicate) is false, then
        //    a. Let error be ThrowCompletion(a newly created TypeError object).
        //    b. Return ? IteratorClose(iterated, error).
        let predicate = args.get_or_undefined(0);
        let Ok(predicate) = JsFunction::try_from_js(predicate, context) else {
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.filter: predicate is not callable"
                )),
                context,
            );
        };

        // 5. Set iterated to ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 6-8 are deferred to `IteratorHelper::create` and `Filter::new`.
        let result =
            IteratorHelper::create(iterator_helper::Filter::new(iterated, predicate), context);

        // 9. Return result.
        Ok(result.into())
    }

    /// `Iterator.prototype.take ( limit )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.take
    fn take(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.prototype.take called on non-object"))?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. Let numLimit be Completion(ToNumber(limit)).
        // 5. IfAbruptCloseIterator(numLimit, iterated).
        let limit = args.get_or_undefined(0);
        let num_limit = if_abrupt_close_iterator!(limit.to_number(context), iterated, context);

        // 6. If numLimit is NaN, throw a RangeError exception.
        if num_limit.is_nan() {
            return iterated.close(
                Err(js_error!(
                    RangeError: "Iterator.prototype.take: limit cannot be NaN"
                )),
                context,
            );
        }

        // 7. If numLimit is finite and numLimit > 𝔽(2**53 - 1), then
        //    a. Let error be ThrowCompletion(a newly created RangeError object).
        //    b. Return ? IteratorClose(iterated, error).
        if num_limit.is_finite() && num_limit > MAX_SAFE_INTEGER {
            return iterated.close(
                Err(js_error!(
                    RangeError: "Iterator.prototype.take: limit cannot exceed 2^53 - 1"
                )),
                context,
            );
        }

        // 7. Let integerLimit be ! ToIntegerOrInfinity(numLimit).
        let integer_limit = IntegerOrInfinity::from(num_limit);

        // 8. If integerLimit < 0, then
        let integer_limit = match integer_limit {
            IntegerOrInfinity::Integer(n) if n >= 0 => Some(n as u64),
            IntegerOrInfinity::PositiveInfinity => None,
            _ => {
                // a. Let error be ThrowCompletion(a newly created RangeError object).
                // b. Return ? IteratorClose(iterated, error).
                return iterated.close(
                    Err(js_error!(
                        RangeError: "Iterator.prototype.take: limit cannot be negative"
                    )),
                    context,
                );
            }
        };

        // 9. Set iterated to ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 10-12 are deferred to `IteratorHelper::create` and `Take::new`.
        let result =
            IteratorHelper::create(iterator_helper::Take::new(iterated, integer_limit), context);

        // 13. Return result.
        Ok(result.into())
    }

    /// `Iterator.prototype.drop ( limit )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.drop
    fn drop(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.prototype.drop called on non-object"))?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o.clone(), JsValue::undefined());

        // 4. Let numLimit be Completion(ToNumber(limit)).
        // 5. IfAbruptCloseIterator(numLimit, iterated).
        let limit = args.get_or_undefined(0);
        let num_limit = if_abrupt_close_iterator!(limit.to_number(context), iterated, context);

        // 6. If numLimit is NaN, throw a RangeError exception.
        if num_limit.is_nan() {
            return iterated.close(
                Err(js_error!(
                    RangeError: "Iterator.prototype.drop: limit cannot be NaN"
                )),
                context,
            );
        }

        // 7. If numLimit is finite and numLimit > 𝔽(2**53 - 1), then
        //    a. Let error be ThrowCompletion(a newly created RangeError object).
        //    b. Return ? IteratorClose(iterated, error).
        if num_limit.is_finite() && num_limit > MAX_SAFE_INTEGER {
            return iterated.close(
                Err(js_error!(
                    RangeError: "Iterator.prototype.drop: limit cannot exceed 2^53 - 1"
                )),
                context,
            );
        }

        // 7. Let integerLimit be ! ToIntegerOrInfinity(numLimit).
        let integer_limit = IntegerOrInfinity::from(num_limit);

        // 8. If integerLimit < 0, then
        let integer_limit = match integer_limit {
            IntegerOrInfinity::Integer(n) if n >= 0 => Some(n as u64),
            IntegerOrInfinity::PositiveInfinity => None,
            _ => {
                // a. Let error be ThrowCompletion(a newly created RangeError object).
                // b. Return ? IteratorClose(iterated, error).
                return iterated.close(
                    Err(js_error!(
                        RangeError: "Iterator.prototype.drop: limit cannot be negative"
                    )),
                    context,
                );
            }
        };
        // 9. Set iterated to ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 10-12 are deferred to `IteratorHelper::create` and `Drop::new`.
        let result =
            IteratorHelper::create(iterator_helper::Drop::new(iterated, integer_limit), context);

        // 13. Return result.
        Ok(result.into())
    }

    /// `Iterator.prototype.flatMap ( mapper )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.flatmap
    fn flat_map(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.flatMap called on non-object"),
        )?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(mapper) is false, then
        //    a. Let error be ThrowCompletion(a newly created TypeError object).
        //    b. Return ? IteratorClose(iterated, error).
        let mapper = args.get_or_undefined(0);
        let Ok(mapper) = JsFunction::try_from_js(mapper, context) else {
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.flatMap: mapper is not callable"
                )),
                context,
            );
        };

        // 5. Set iterated to ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 6-8 are deferred to `IteratorHelper::create` and `FlatMap::new`.
        let helper =
            IteratorHelper::create(iterator_helper::FlatMap::new(iterated, mapper), context);

        // 9. Return result.
        Ok(helper.into())
    }

    /// `ValidateChunkSize ( iterated, size )`, the argument checks shared by
    /// `Iterator.prototype.chunks` and `Iterator.prototype.windows`.
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-iterator-chunking/
    fn validate_chunk_size(
        iterated: &IteratorRecord,
        size: &JsValue,
        method: &str,
        context: &mut Context,
    ) -> JsResult<usize> {
        // 4. If size is not a Number, throw a TypeError exception.
        // 5. If size is not an integral Number, throw a TypeError exception.
        let Some(size) = size
            .as_number()
            .filter(|n| n.is_finite() && n.trunc() == *n)
        else {
            iterated.close(
                Err(js_error!(TypeError: "Iterator.prototype.{}: size must be an integral Number", method)),
                context,
            )?;
            return Ok(0);
        };

        // 6. If size is not in the inclusive interval from 1𝔽 to 𝔽(2^32 - 1), then
        //    a. Let error be ThrowCompletion(a newly created RangeError object).
        //    b. Return ? IteratorClose(iterated, error).
        if !(1.0..=f64::from(u32::MAX)).contains(&size) {
            iterated.close(
                Err(js_error!(RangeError: "Iterator.prototype.{}: size must be between 1 and 2^32 - 1", method)),
                context,
            )?;
            return Ok(0);
        }

        Ok(size as usize)
    }

    /// `Iterator.prototype.chunks ( chunkSize )`
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-iterator-chunking/#sec-iterator.prototype.chunks
    fn chunks(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.chunks called on non-object"),
        )?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4-6. Validate chunkSize, closing iterated on failure.
        let size =
            Self::validate_chunk_size(&iterated, args.get_or_undefined(0), "chunks", context)?;

        // 7. Set iterated to ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 8-10. Let result be CreateIteratorFromClosure(closure, "Iterator Helper", ...).
        let result =
            IteratorHelper::create(iterator_helper::Chunks::new(iterated, size, None), context);

        Ok(result.into())
    }

    /// `Iterator.prototype.windows ( windowSize [ , undersized ] )`
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-iterator-chunking/#sec-iterator.prototype.windows
    fn windows(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.windows called on non-object"),
        )?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4-6. Validate windowSize, closing iterated on failure.
        let size =
            Self::validate_chunk_size(&iterated, args.get_or_undefined(0), "windows", context)?;

        // 7. If undersized is undefined, set undersized to "only-full".
        // 8. If undersized is neither "only-full" nor "allow-partial", then
        //    a. Let error be ThrowCompletion(a newly created TypeError object).
        //    b. Return ? IteratorClose(iterated, error).
        let undersized = args.get_or_undefined(1);
        let undersized = if undersized.is_undefined() {
            iterator_helper::Undersized::OnlyFull
        } else if let Some(s) = undersized.as_string() {
            if s == js_string!("only-full") {
                iterator_helper::Undersized::OnlyFull
            } else if s == js_string!("allow-partial") {
                iterator_helper::Undersized::AllowPartial
            } else {
                return iterated.close(
                    Err(js_error!(TypeError: "Iterator.prototype.windows: invalid undersized option")),
                    context,
                );
            }
        } else {
            return iterated.close(
                Err(js_error!(TypeError: "Iterator.prototype.windows: invalid undersized option")),
                context,
            );
        };

        // 9. Set iterated to ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 10-12. Let result be CreateIteratorFromClosure(closure, "Iterator Helper", ...).
        let result = IteratorHelper::create(
            iterator_helper::Chunks::new(iterated, size, Some(undersized)),
            context,
        );

        Ok(result.into())
    }

    /// `Iterator.prototype [ %Symbol.dispose% ] ( )`
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-explicit-resource-management/#sec-%iteratorprototype%-@@dispose
    fn dispose(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. Let return be ? GetMethod(O, "return").
        let r#return = this.get_method(js_string!("return"), context)?;

        // 3. If return is not undefined, then
        if let Some(r#return) = r#return {
            // a. Perform ? Call(return, O, « »).
            r#return.call(this, &[], context)?;
        }

        // 4. Return undefined.
        Ok(JsValue::undefined())
    }

    // ==================== Prototype Methods — Eager (Consuming) ====================

    /// `Iterator.prototype.includes ( searchElement [, skippedElements ] )`
    #[cfg(feature = "experimental")] // Stage 2.7 iterator-includes proposal
    fn includes(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let obj = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.includes called on non-object"),
        )?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(obj.clone(), JsValue::undefined());

        let search_element = args.get_or_undefined(0);

        // 4. If skippedElements is undefined, let toSkip be 0.
        // 5. Else if skippedElements is not one of +∞𝔽, -∞𝔽, or an integral Number, then
        //    a. Let error be ThrowCompletion(a newly created TypeError object).
        //    b. Return ? IteratorClose(iterated, error).
        // 6. Else, let toSkip be skippedElements.
        let skipped_elements = args.get_or_undefined(1);
        let to_skip = if skipped_elements.is_undefined() {
            0.0
        } else {
            match skipped_elements.as_number() {
                Some(number) if number.is_infinite() || number.trunc() == number => number,
                _ => {
                    let error = js_error!(TypeError: "skippedElements must be an integral number");
                    return iterated.close(Err(error), context);
                }
            }
        };

        // 7. If toSkip < -0𝔽, then
        //    a. Let error be ThrowCompletion(a newly created RangeError object).
        //    b. Return ? IteratorClose(iterated, error).
        // 8. If toSkip is finite and toSkip > 𝔽(2**53 - 1), then
        //    a. Let error be ThrowCompletion(a newly created RangeError object).
        //    b. Return ? IteratorClose(iterated, error).
        if to_skip < 0.0 || (to_skip.is_finite() && to_skip > MAX_SAFE_INTEGER) {
            let error = js_error!(RangeError: "skippedElements is out of range");
            return iterated.close(Err(error), context);
        }
        let to_skip = if to_skip.is_finite() {
            to_skip as u64
        } else {
            u64::MAX
        };

        // 9. Let skipped be +0𝔽.
        let mut skipped = 0;

        // 8. Set iterated to ? GetIteratorDirect(O).
        let mut iterated = get_iterator_direct(&obj, context)?;

        // 9. Repeat,
        while let Some(value) = iterated.step_value(context)? {
            // a. Let value be ? IteratorStepValue(iterated).
            // b. If value is done, return false.
            // c. If skipped < toSkip, then
            if skipped < to_skip {
                // i. Set skipped to skipped + 1.
                skipped += 1;
            // d. Else if SameValueZero(value, searchElement) is true, then
            } else if JsValue::same_value_zero(&value, search_element) {
                // i. Return ? IteratorClose(iterated, NormalCompletion(true)).
                return iterated.close(Ok(true.into()), context);
            }
        }
        // Step 9.b. return false
        Ok(false.into())
    }

    /// `Iterator.prototype.join ( separator )`
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-iterator-join/
    fn join(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.prototype.join called on non-object"))?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If separator is undefined, let sep be ",".
        // 5. Else, let sep be Completion(ToString(separator)).
        // 6. IfAbruptCloseIterator(sep, iterated).
        let separator = args.get_or_undefined(0);
        let sep = if separator.is_undefined() {
            js_string!(",")
        } else {
            if_abrupt_close_iterator!(separator.to_string(context), iterated, context)
        };

        // 7. Set iterated to ? GetIteratorDirect(O).
        let mut iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 8. Let R be the empty String.
        let mut r = Vec::new();
        let mut first = true;

        // 9. Repeat,
        //    a. Let value be ? IteratorStepValue(iterated).
        //    b. If value is done, return R.
        while let Some(value) = iterated.step_value(context)? {
            // c. If R is not the first element, set R to the string-concatenation of R and sep.
            if !first {
                r.extend(sep.iter());
            }
            first = false;

            // d. If value is either undefined or null, let next be the empty String.
            // e. Else, let next be Completion(ToString(value)).
            // f. IfAbruptCloseIterator(next, iterated).
            if !value.is_null_or_undefined() {
                let next = if_abrupt_close_iterator!(value.to_string(context), iterated, context);
                // g. Set R to the string-concatenation of R and next.
                r.extend(next.iter());
            }
        }

        Ok(crate::JsString::from(&r[..]).into())
    }

    /// `Iterator.prototype.reduce ( reducer [ , initialValue ] )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.reduce
    fn reduce(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.reduce called on non-object"),
        )?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(reducer) is false, then
        let Some(reducer) = args.get_or_undefined(0).as_callable() else {
            // a. Let error be ThrowCompletion(a newly created TypeError object).
            // b. Return ? IteratorClose(iterated, error).
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.reduce: reducer is not callable"
                )),
                context,
            );
        };

        // 5. Set iterated to ? GetIteratorDirect(O).
        let mut iterated = get_iterator_direct(iterated.iterator(), context)?;

        let (mut accumulator, mut counter) = if let Some(acc) = args.get(1).cloned() {
            // 7. Else,
            //    a. Let accumulator be initialValue.
            //    b. Let counter be 0.
            (acc, 0)
        } else if let Some(first) = iterated.step_value(context)? {
            // 6. If initialValue is not present, then
            //    a. Let accumulator be ? IteratorStepValue(iterated).
            //    c. Let counter be 1.
            (first, 1u64)
        } else {
            // b. If accumulator is done, throw a TypeError exception.
            return Err(js_error!(
                TypeError: "Iterator.prototype.reduce: cannot reduce empty iterator with no initial value"
            ));
        };

        // 8. Repeat,
        //    a. Let value be ? IteratorStepValue(iterated).
        //    b. If value is done, return accumulator.
        while let Some(value) = iterated.step_value(context)? {
            // c. Let result be Completion(Call(reducer, undefined, « accumulator, value, 𝔽(counter) »)).
            let result = reducer.call(
                &JsValue::undefined(),
                &[accumulator, value, JsValue::new(counter)],
                context,
            );

            // d. IfAbruptCloseIterator(result, iterated).
            // e. Set accumulator to result.
            accumulator = if_abrupt_close_iterator!(result, iterated, context);

            // f. Set counter to counter + 1.
            counter += 1;
        }

        // Step 8.b
        Ok(accumulator)
    }

    /// `Iterator.prototype.toArray ( )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.toarray
    fn to_array(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.toArray called on non-object"),
        )?;

        // 3. Let iterated be ? GetIteratorDirect(O).
        let iterated = get_iterator_direct(&o, context)?;

        // 4. Let items be a new empty List.
        // 5. Repeat ...
        let items = iterated.into_list(context)?;

        // b. If value is done, return CreateArrayFromList(items).
        Ok(Array::create_array_from_list(items, context).into())
    }

    /// `Iterator.prototype.forEach ( fn )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.foreach
    fn for_each(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this.as_object().ok_or_else(
            || js_error!(TypeError: "Iterator.prototype.forEach called on non-object"),
        )?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(fn) is false, then
        let Some(func) = args.get_or_undefined(0).as_callable() else {
            // a. Let error be ThrowCompletion(a newly created TypeError object).
            // b. Return ? IteratorClose(iterated, error).
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.forEach: argument is not callable"
                )),
                context,
            );
        };

        // 5. Set iterated to ? GetIteratorDirect(O).
        let mut iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 6. Let counter be 0.
        let mut counter = 0u64;

        // 7. Repeat,
        //    a. Let value be ? IteratorStepValue(iterated).
        //    b. If value is done, return undefined.
        while let Some(value) = iterated.step_value(context)? {
            // c. Let result be Completion(Call(procedure, undefined, « value, 𝔽(counter) »)).
            let result = func.call(
                &JsValue::undefined(),
                &[value, JsValue::new(counter)],
                context,
            );

            // d. IfAbruptCloseIterator(result, iterated).
            if_abrupt_close_iterator!(result, iterated, context);

            // e. Set counter to counter + 1.
            counter += 1;
        }

        // Step 7.b
        Ok(JsValue::undefined())
    }

    /// `Iterator.prototype.some ( predicate )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.some
    fn some(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.prototype.some called on non-object"))?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(predicate) is false, then
        let Some(predicate) = args.get_or_undefined(0).as_callable() else {
            // a. Let error be ThrowCompletion(a newly created TypeError object).
            // b. Return ? IteratorClose(iterated, error).
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.some: predicate is not callable"
                )),
                context,
            );
        };

        // 5. Set iterated to ? GetIteratorDirect(O).
        let mut iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 6. Let counter be 0.
        let mut counter = 0u64;
        // 7. Repeat,
        //    a. Let value be ? IteratorStepValue(iterated).
        //    b. If value is done, return false.
        while let Some(value) = iterated.step_value(context)? {
            // c. Let result be Completion(Call(predicate, undefined, « value, 𝔽(counter) »)).
            let result = predicate.call(
                &JsValue::undefined(),
                &[value, JsValue::new(counter)],
                context,
            );
            // d. IfAbruptCloseIterator(result, iterated).
            let result = if_abrupt_close_iterator!(result, iterated, context);

            // e. If ToBoolean(result) is true, return ? IteratorClose(iterated, NormalCompletion(true)).
            if result.to_boolean() {
                return iterated.close(Ok(JsValue::new(true)), context);
            }

            // f. Set counter to counter + 1.
            counter += 1;
        }

        // Step 7.b
        Ok(false.into())
    }

    /// `Iterator.prototype.every ( predicate )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.every
    fn every(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.prototype.every called on non-object"))?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(predicate) is false, then
        let Some(predicate) = args.get_or_undefined(0).as_callable() else {
            // a. Let error be ThrowCompletion(a newly created TypeError object).
            // b. Return ? IteratorClose(iterated, error).
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.every: predicate is not callable"
                )),
                context,
            );
        };

        // 5. Set iterated to ? GetIteratorDirect(O).
        let mut iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 6. Let counter be 0.
        let mut counter = 0u64;

        // 7. Repeat,
        //    a. Let value be ? IteratorStepValue(iterated).
        //    b. If value is done, return true.
        while let Some(value) = iterated.step_value(context)? {
            // c. Let result be Completion(Call(predicate, undefined, « value, 𝔽(counter) »)).
            let result = predicate.call(
                &JsValue::undefined(),
                &[value, JsValue::new(counter)],
                context,
            );
            // d. IfAbruptCloseIterator(result, iterated).
            let result = if_abrupt_close_iterator!(result, iterated, context);

            // e. If ToBoolean(result) is false, return ? IteratorClose(iterated, NormalCompletion(false)).
            if !result.to_boolean() {
                return iterated.close(Ok(JsValue::new(false)), context);
            }

            // f. Set counter to counter + 1.
            counter += 1;
        }

        // Step 7.b
        Ok(true.into())
    }

    /// `Iterator.prototype.find ( predicate )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.prototype.find
    fn find(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let O be the this value.
        // 2. If O is not an Object, throw a TypeError exception.
        let o = this
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.prototype.find called on non-object"))?;

        // 3. Let iterated be the Iterator Record { [[Iterator]]: O, [[NextMethod]]: undefined, [[Done]]: false }.
        let iterated = IteratorRecord::new(o, JsValue::undefined());

        // 4. If IsCallable(predicate) is false, then
        let Some(predicate) = args.get_or_undefined(0).as_callable() else {
            // a. Let error be ThrowCompletion(a newly created TypeError object).
            // b. Return ? IteratorClose(iterated, error).
            return iterated.close(
                Err(js_error!(
                    TypeError: "Iterator.prototype.find: predicate is not callable"
                )),
                context,
            );
        };
        // 5. Set iterated to ? GetIteratorDirect(O).
        let mut iterated = get_iterator_direct(iterated.iterator(), context)?;

        // 6. Let counter be 0.
        let mut counter = 0u64;

        // 7. Repeat,
        //    a. Let value be ? IteratorStepValue(iterated).
        //    b. If value is done, return undefined.
        while let Some(value) = iterated.step_value(context)? {
            // c. Let result be Completion(Call(predicate, undefined, « value, 𝔽(counter) »)).
            let result = predicate.call(
                &JsValue::undefined(),
                &[value.clone(), JsValue::new(counter)],
                context,
            );

            // d. IfAbruptCloseIterator(result, iterated).
            let result = if_abrupt_close_iterator!(result, iterated, context);

            // e. If ToBoolean(result) is true, return ? IteratorClose(iterated, NormalCompletion(value)).
            if result.to_boolean() {
                return iterated.close(Ok(value), context);
            }

            // f. Set counter to counter + 1.
            counter += 1;
        }

        // Step 7.b
        Ok(JsValue::undefined())
    }
}
