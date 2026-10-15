//! Boa's implementation of the ECMAScript `Iterator` constructor.
//!
//! The `Iterator` constructor is designed to be subclassed. It may be used as the
//! value of an extends clause of a class definition.
//!
//! More information:
//!  - [ECMAScript reference][spec]
//!
//! [spec]: https://tc39.es/ecma262/#sec-iterator-constructor

use std::collections::VecDeque;

use crate::{
    Context, JsArgs, JsData, JsResult, JsString, JsValue,
    builtins::{
        BuiltInBuilder, BuiltInConstructor, BuiltInObject, IntrinsicObject,
        iterable::{
            IteratorRecord,
            iterator_helper::{self, IterableRecord, ZipMode, iterator_close_all},
        },
        object::OrdinaryObject,
    },
    context::intrinsics::{Intrinsics, StandardConstructor, StandardConstructors},
    error::JsNativeError,
    js_error, js_string,
    object::{JsFunction, JsObject, PROTOTYPE, internal_methods::get_prototype_from_constructor},
    property::Attribute,
    property::PropertyKey,
    realm::Realm,
    string::StaticJsStrings,
    symbol::JsSymbol,
};
use boa_gc::{Finalize, Trace};

use super::{iterator_helper::IteratorHelper, wrap_for_valid_iterator::WrapForValidIterator};

/// The `Iterator` constructor.
///
/// More information:
///  - [ECMAScript reference][spec]
///
/// [spec]: https://tc39.es/ecma262/#sec-iterator-constructor
#[derive(Debug, Clone, Trace, Finalize, JsData)]
pub(crate) struct IteratorConstructor;

impl IntrinsicObject for IteratorConstructor {
    fn init(realm: &Realm) {
        let iterator_prototype = realm.intrinsics().constructors().iterator().prototype();
        BuiltInBuilder::from_standard_constructor::<Self>(realm)
            .inherits(Some(iterator_prototype.clone()))
            // Static methods
            .static_method(Self::from, js_string!("from"), 1)
            .static_method(Self::concat, js_string!("concat"), 0)
            .static_method(Self::zip, js_string!("zip"), 1)
            .static_method(Self::zip_keyed, js_string!("zipKeyed"), 1)
            .static_property(PROTOTYPE, iterator_prototype, Attribute::empty())
            .build_without_prototype();
    }

    fn get(intrinsics: &Intrinsics) -> JsObject {
        intrinsics.constructors().iterator().constructor()
    }
}

impl BuiltInObject for IteratorConstructor {
    const NAME: JsString = StaticJsStrings::ITERATOR;
}

impl BuiltInConstructor for IteratorConstructor {
    const PROTOTYPE_STORAGE_SLOTS: usize = 0;
    const CONSTRUCTOR_STORAGE_SLOTS: usize = 5;
    const CONSTRUCTOR_ARGUMENTS: usize = 0;
    const STANDARD_CONSTRUCTOR: fn(&StandardConstructors) -> &StandardConstructor =
        StandardConstructors::iterator;

    /// `Iterator ( )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator
    fn constructor(
        new_target: &JsValue,
        _args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // 1. If NewTarget is undefined or the active function object, throw a TypeError exception.
        if new_target.is_undefined()
            || new_target
                == &context
                    .active_function_object()
                    .unwrap_or_else(|| context.intrinsics().constructors().iterator().constructor())
                    .into()
        {
            return Err(JsNativeError::typ()
                .with_message(if new_target.is_undefined() {
                    "Iterator constructor requires 'new'"
                } else {
                    "Abstract class Iterator not directly constructable"
                })
                .into());
        }

        // 2. Return ? OrdinaryCreateFromConstructor(NewTarget, "%Iterator.prototype%").
        let prototype =
            get_prototype_from_constructor(new_target, StandardConstructors::iterator, context)?;

        // Create an ordinary object (Iterator instances have no internal data slots).
        Ok(JsObject::from_proto_and_data_with_shared_shape(
            context.root_shape(),
            prototype,
            OrdinaryObject,
        )
        .upcast()
        .into())
    }
}

impl IteratorConstructor {
    /// `Iterator.from ( O )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.from
    fn from(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let o = args.get_or_undefined(0);

        // 1. Let iteratorRecord be ? GetIteratorFlattenable(O, iterate-strings).
        let iterator_record = super::get_iterator_flattenable(o, true, context)?;

        // 2. Let hasInstance be ? OrdinaryHasInstance(%Iterator%, iteratorRecord.[[Iterator]]).
        let iterator_constructor = context.intrinsics().constructors().iterator().constructor();
        let has_instance = JsValue::ordinary_has_instance(
            &iterator_constructor.clone().into(),
            &iterator_record.iterator().clone().into(),
            context,
        )?;

        // 3. If hasInstance is true, then
        if has_instance {
            // a. Return iteratorRecord.[[Iterator]].
            return Ok(iterator_record.iterator().clone().into());
        }

        // 4. Let wrapper be OrdinaryObjectCreate(%WrapForValidIteratorPrototype%, « [[Iterated]] »).
        // 5. Set wrapper.[[Iterated]] to iteratorRecord.
        // 6. Return wrapper.
        let wrapper = JsObject::from_proto_and_data_with_shared_shape(
            context.root_shape(),
            context
                .intrinsics()
                .objects()
                .iterator_prototypes()
                .wrap_for_valid_iterator(),
            WrapForValidIterator {
                iterated: iterator_record,
            },
        );

        Ok(wrapper.into())
    }

    /// `Iterator.concat ( ...items )`
    ///
    /// More information:
    ///  - [ECMAScript reference][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-iterator.concat
    fn concat(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1. Let iterables be a new empty List.
        let mut iterables = VecDeque::with_capacity(args.len());

        // 2. For each element item of items, do
        for item in args {
            // a. If item is not an Object, throw a TypeError exception.
            let Some(item) = item.as_object() else {
                return Err(js_error!(TypeError: "Iterator.concat requires iterable objects"));
            };

            // b. Let method be ? GetMethod(item, %Symbol.iterator%).
            // c. If method is undefined, throw a TypeError exception.
            let method = item.get_method(JsSymbol::iterator(), context)?.ok_or_else(
                || js_error!(TypeError: "Iterator.concat requires objects with @@iterator"),
            )?;

            // d. Append the Record { [[OpenMethod]]: method, [[Iterable]]: item } to iterables.
            iterables.push_back(IterableRecord {
                iterable: item,
                open_method: JsFunction::from_object_unchecked(method),
            });
        }

        // 3. Let closure be a new Abstract Closure with no parameters that captures iterables
        //    and performs the following steps when called:
        //    (implemented via IteratorHelperOp::Concat in execute_next)
        // 4-5. Let result be CreateIteratorFromClosure(closure, "Iterator Helper", ...)
        //      with [[UnderlyingIterators]] set to a new empty List.
        let helper = IteratorHelper::create(iterator_helper::Concat::new(iterables), context);

        // 6. Return result.
        Ok(helper.into())
    }

    /// The option checks shared by `Iterator.zip` and `Iterator.zipKeyed`: steps 1 to 7.
    fn zip_options(
        iterables: &JsValue,
        options: &JsValue,
        context: &mut Context,
    ) -> JsResult<(JsObject, ZipMode, JsValue)> {
        // 1. If iterables is not an Object, throw a TypeError exception.
        let iterables = iterables
            .as_object()
            .ok_or_else(|| js_error!(TypeError: "Iterator.zip: iterables must be an object"))?;

        // 2. Set options to ? GetOptionsObject(options).
        let options = if options.is_undefined() {
            JsObject::with_null_proto()
        } else {
            options
                .as_object()
                .ok_or_else(|| js_error!(TypeError: "Iterator.zip: options must be an object"))?
        };

        // 3. Let mode be ? Get(options, "mode").
        let mode = options.get(js_string!("mode"), context)?;

        // 4. If mode is undefined, set mode to "shortest".
        // 5. If mode is not one of "shortest", "longest", or "strict", throw a TypeError exception.
        let mode = if mode.is_undefined() {
            ZipMode::Shortest
        } else {
            match mode.as_string() {
                Some(s) if s == js_string!("shortest") => ZipMode::Shortest,
                Some(s) if s == js_string!("longest") => ZipMode::Longest,
                Some(s) if s == js_string!("strict") => ZipMode::Strict,
                _ => {
                    return Err(js_error!(
                        TypeError: "Iterator.zip: mode must be \"shortest\", \"longest\" or \"strict\""
                    ));
                }
            }
        };

        // 6. Let paddingOption be undefined.
        // 7. If mode is "longest", then
        let padding = if mode == ZipMode::Longest {
            // a. Set paddingOption to ? Get(options, "padding").
            let padding = options.get(js_string!("padding"), context)?;
            // b. If paddingOption is not undefined and paddingOption is not an Object, throw a TypeError exception.
            if !padding.is_undefined() && !padding.is_object() {
                return Err(js_error!(TypeError: "Iterator.zip: padding must be an object"));
            }
            padding
        } else {
            JsValue::undefined()
        };

        Ok((iterables, mode, padding))
    }

    /// `Iterator.zip ( iterables [ , options ] )`
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-joint-iteration/#sec-iterator.zip
    fn zip(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1-7.
        let (iterables, mode, padding_option) =
            Self::zip_options(args.get_or_undefined(0), args.get_or_undefined(1), context)?;

        // 8. Let iters be a new empty List.
        let mut iters: Vec<IteratorRecord> = Vec::new();

        // 10. Let inputIter be ? GetIterator(iterables, sync).
        let mut input_iter = JsValue::from(iterables)
            .get_iterator(crate::builtins::iterable::IteratorHint::Sync, context)?;

        // 11. Let next be not-started.
        // 12. Repeat, while next is not done,
        loop {
            // a. Set next to Completion(IteratorStepValue(inputIter)).
            // b. IfAbruptCloseIterators(next, iters).
            let next = match input_iter.step_value(context) {
                Ok(next) => next,
                Err(err) => {
                    let open = vec![true; iters.len()];
                    return iterator_close_all(&iters, &open, Err(err), context);
                }
            };
            // c. If next is not done, then
            let Some(next) = next else {
                break;
            };
            // i. Let iter be Completion(GetIteratorFlattenable(next, reject-strings)).
            match super::get_iterator_flattenable(&next, false, context) {
                // iii. Append iter to iters.
                Ok(iter) => iters.push(iter),
                // ii. IfAbruptCloseIterators(iter, the list-concatenation of « inputIter » and iters).
                Err(err) => {
                    let open = vec![true; iters.len()];
                    let completion = iterator_close_all(&iters, &open, Err(err), context);
                    return input_iter.close(completion, context);
                }
            }
        }

        // 13. Let iterCount be the number of elements in iters.
        let iter_count = iters.len();
        let open = vec![true; iter_count];

        // 9. Let padding be a new empty List.
        let mut padding = Vec::with_capacity(iter_count);

        // 14. If mode is "longest", then
        if mode == ZipMode::Longest {
            // a. If paddingOption is undefined, then
            if padding_option.is_undefined() {
                // i. Perform the following steps iterCount times:
                //    1. Append undefined to padding.
                padding.resize(iter_count, JsValue::undefined());
            } else {
                // b. Else,
                //    i. Let paddingIter be Completion(GetIterator(paddingOption, sync)).
                //    ii. IfAbruptCloseIterators(paddingIter, iters).
                let mut padding_iter = match padding_option
                    .get_iterator(crate::builtins::iterable::IteratorHint::Sync, context)
                {
                    Ok(iter) => iter,
                    Err(err) => return iterator_close_all(&iters, &open, Err(err), context),
                };

                // iii. Let usingIterator be true.
                let mut using_iterator = true;

                // iv. Perform the following steps iterCount times:
                for _ in 0..iter_count {
                    // 1. If usingIterator is true, then
                    if using_iterator {
                        // a. Set next to Completion(IteratorStepValue(paddingIter)).
                        // b. IfAbruptCloseIterators(next, iters).
                        match padding_iter.step_value(context) {
                            Err(err) => {
                                return iterator_close_all(&iters, &open, Err(err), context);
                            }
                            // c. If next is done, then
                            //    i. Set usingIterator to false.
                            Ok(None) => using_iterator = false,
                            // d. Else,
                            //    i. Append next to padding.
                            Ok(Some(value)) => padding.push(value),
                        }
                    }
                    // 2. If usingIterator is false, append undefined to padding.
                    if !using_iterator {
                        padding.push(JsValue::undefined());
                    }
                }

                // v. If usingIterator is true, then
                if using_iterator {
                    // 1. Let completion be Completion(IteratorClose(paddingIter, NormalCompletion(unused))).
                    // 2. IfAbruptCloseIterators(completion, iters).
                    if let Err(err) = padding_iter.close(Ok(JsValue::undefined()), context) {
                        return iterator_close_all(&iters, &open, Err(err), context);
                    }
                }
            }
        }

        // 15. Let finishResults be a new Abstract Closure ... CreateArrayFromList(results).
        // 16. Return IteratorZip(iters, mode, padding, finishResults).
        Ok(IteratorHelper::create(
            iterator_helper::Zip::new(iters, mode, padding, None),
            context,
        )
        .into())
    }

    /// `Iterator.zipKeyed ( iterables [ , options ] )`
    ///
    /// More information:
    ///  - [Proposal reference][spec]
    ///
    /// [spec]: https://tc39.es/proposal-joint-iteration/#sec-iterator.zipkeyed
    fn zip_keyed(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // 1-7.
        let (iterables, mode, padding_option) =
            Self::zip_options(args.get_or_undefined(0), args.get_or_undefined(1), context)?;

        // 8. Let iters be a new empty List.
        let mut iters: Vec<IteratorRecord> = Vec::new();

        // 10. Let allKeys be ? iterables.[[OwnPropertyKeys]]().
        let all_keys = iterables.__own_property_keys__(context)?;

        // 11. Let keys be a new empty List.
        let mut keys: Vec<PropertyKey> = Vec::new();

        // 12. For each element key of allKeys, do
        for key in all_keys {
            let open = vec![true; iters.len()];
            // a. Let desc be Completion(iterables.[[GetOwnProperty]](key)).
            // b. IfAbruptCloseIterators(desc, iters).
            let desc = match iterables.__get_own_property__(&key, &mut context.into()) {
                Ok(desc) => desc,
                Err(err) => return iterator_close_all(&iters, &open, Err(err), context),
            };

            // c. If desc is not undefined and desc.[[Enumerable]] is true, then
            if !desc.is_some_and(|desc| desc.expect_enumerable()) {
                continue;
            }

            // i. Let value be Completion(Get(iterables, key)).
            // ii. IfAbruptCloseIterators(value, iters).
            let value = match iterables.get(key.clone(), context) {
                Ok(value) => value,
                Err(err) => return iterator_close_all(&iters, &open, Err(err), context),
            };

            // iii. If value is not undefined, then
            if value.is_undefined() {
                continue;
            }

            // 1. Let iter be Completion(GetIteratorFlattenable(value, reject-strings)).
            // 2. IfAbruptCloseIterators(iter, iters).
            let iter = match super::get_iterator_flattenable(&value, false, context) {
                Ok(iter) => iter,
                Err(err) => return iterator_close_all(&iters, &open, Err(err), context),
            };

            // 3. Append key to keys.
            keys.push(key);
            // 4. Append iter to iters.
            iters.push(iter);
        }

        // 13. Let iterCount be the number of elements in iters.
        let iter_count = iters.len();
        let open = vec![true; iter_count];

        // 9. Let padding be a new empty List.
        let mut padding = Vec::with_capacity(iter_count);

        // 14. If mode is "longest", then
        if mode == ZipMode::Longest {
            match padding_option.as_object() {
                // a. If paddingOption is undefined, then
                //    i. Perform the following steps iterCount times:
                //       1. Append undefined to padding.
                None => padding.resize(iter_count, JsValue::undefined()),
                // b. Else,
                //    i. For each element key of keys, do
                Some(padding_option) => {
                    for key in &keys {
                        // 1. Let value be Completion(Get(paddingOption, key)).
                        // 2. IfAbruptCloseIterators(value, iters).
                        match padding_option.get(key.clone(), context) {
                            // 3. Append value to padding.
                            Ok(value) => padding.push(value),
                            Err(err) => {
                                return iterator_close_all(&iters, &open, Err(err), context);
                            }
                        }
                    }
                }
            }
        }

        // 15. Let finishResults be a new Abstract Closure ... that captures keys and iterCount.
        // 16. Return IteratorZip(iters, mode, padding, finishResults).
        let keys = keys.into_iter().map(JsValue::from).collect();
        Ok(IteratorHelper::create(
            iterator_helper::Zip::new(iters, mode, padding, Some(keys)),
            context,
        )
        .into())
    }
}
