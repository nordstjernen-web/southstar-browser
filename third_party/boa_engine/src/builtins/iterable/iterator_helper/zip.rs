use std::cell::Cell;

use boa_gc::{Finalize, Trace};

use crate::{
    Context, JsResult, JsValue,
    builtins::{Array, iterable::IteratorRecord},
    js_error,
    native_function::{CoroutineBranch, CoroutineState, NativeCoroutine},
    object::JsObject,
    vm::CompletionRecord,
};

/// The `mode` option of `Iterator.zip` and `Iterator.zipKeyed`.
#[derive(Clone, Copy, PartialEq, Eq, Trace, Finalize)]
#[boa_gc(empty_trace)]
pub(crate) enum ZipMode {
    Shortest,
    Longest,
    Strict,
}

/// `IteratorCloseAll ( iters, completion )`, over the iterators still marked open.
///
/// More information:
///  - [Proposal reference][spec]
///
/// [spec]: https://tc39.es/proposal-joint-iteration/#sec-closeall
pub(crate) fn iterator_close_all(
    iters: &[IteratorRecord],
    open: &[bool],
    mut completion: JsResult<JsValue>,
    context: &mut Context,
) -> JsResult<JsValue> {
    // 1. For each element iter of iters, in reverse List order, do
    for (iter, _) in iters.iter().zip(open).rev().filter(|(_, open)| **open) {
        // a. Set completion to Completion(IteratorClose(iter, completion)).
        completion = iter.close(completion, context);
    }
    // 2. Return ? completion.
    completion
}

/// [`IteratorZip ( iters, mode, padding, finishResults )`][spec]
///
/// The iterator helper behind `Iterator.zip` and `Iterator.zipKeyed`: each step reads one
/// value from every underlying iterator and yields them together, as an array or, when
/// `keys` is present, as a null-prototype object.
///
/// [spec]: https://tc39.es/proposal-joint-iteration/#sec-IteratorZip
#[derive(Trace, Finalize, Default)]
#[boa_gc(unsafe_no_drop)]
pub(crate) enum Zip {
    #[default]
    Completed,
    Running {
        iters: Vec<IteratorRecord>,
        open: Vec<bool>,
        mode: ZipMode,
        padding: Vec<JsValue>,
        keys: Option<Vec<JsValue>>,
    },
}

impl Zip {
    #[allow(
        clippy::new_ret_no_self,
        reason = "slightly cleaner to have this be a `new` method"
    )]
    pub(crate) fn new(
        iters: Vec<IteratorRecord>,
        mode: ZipMode,
        padding: Vec<JsValue>,
        keys: Option<Vec<JsValue>>,
    ) -> NativeCoroutine {
        let open = vec![true; iters.len()];
        NativeCoroutine::from_copy_closure_with_captures(
            |completion, state, context| {
                let Self::Running {
                    mut iters,
                    mut open,
                    mode,
                    padding,
                    keys,
                } = state.take()
                else {
                    return CoroutineState::Break(Ok(()));
                };

                // vi. If completion is an abrupt completion, then
                //     1. Return ? IteratorCloseAll(openIters, completion).
                match completion {
                    CompletionRecord::Return(value) => {
                        iterator_close_all(&iters, &open, Ok(value), context).branch()?;
                        return CoroutineState::Break(Ok(()));
                    }
                    CompletionRecord::Throw(err) => {
                        return CoroutineState::Break(
                            iterator_close_all(&iters, &open, Err(err), context).map(|_| ()),
                        );
                    }
                    CompletionRecord::Normal(_) => {}
                }

                // a. If iterCount = 0, return ReturnCompletion(undefined).
                let iter_count = iters.len();
                if iter_count == 0 {
                    return CoroutineState::Break(Ok(()));
                }

                // b. Repeat,
                //    i. Let results be a new empty List.
                let mut results = Vec::with_capacity(iter_count);

                // iii. For each integer i such that 0 ≤ i < iterCount, in ascending order, do
                for i in 0..iter_count {
                    // 2. If iter is null, then
                    if !open[i] {
                        // b. Let result be padding[i].
                        results.push(padding[i].clone());
                        continue;
                    }

                    // 3. a. Let result be Completion(IteratorStepValue(iter)).
                    match iters[i].step_value(context) {
                        // b. If result is an abrupt completion, then
                        Err(err) => {
                            // i. Remove iter from openIters.
                            open[i] = false;
                            // ii. Return ? IteratorCloseAll(openIters, result).
                            return CoroutineState::Break(
                                iterator_close_all(&iters, &open, Err(err), context).map(|_| ()),
                            );
                        }
                        Ok(Some(value)) => results.push(value),
                        // d. If result is done, then
                        Ok(None) => {
                            // i. Remove iter from openIters.
                            open[i] = false;
                            match mode {
                                // ii. If mode is "shortest", then
                                ZipMode::Shortest => {
                                    // i. Return ? IteratorCloseAll(openIters, ReturnCompletion(undefined)).
                                    return CoroutineState::Break(
                                        iterator_close_all(
                                            &iters,
                                            &open,
                                            Ok(JsValue::undefined()),
                                            context,
                                        )
                                        .map(|_| ()),
                                    );
                                }
                                // iii. Else if mode is "strict", then
                                ZipMode::Strict => {
                                    return CoroutineState::Break(Self::finish_strict(
                                        &mut iters, &mut open, i, context,
                                    ));
                                }
                                // iv. Else,
                                ZipMode::Longest => {
                                    // ii. If openIters is empty, return ReturnCompletion(undefined).
                                    if !open.contains(&true) {
                                        return CoroutineState::Break(Ok(()));
                                    }
                                    // iii. Set iters[i] to null.
                                    // iv. Set result to padding[i].
                                    results.push(padding[i].clone());
                                }
                            }
                        }
                    }
                }

                // iv. Set results to finishResults(results).
                let results = Self::finish_results(results, keys.as_deref(), context);
                if let Err(err) = results {
                    return CoroutineState::Break(Err(err));
                }

                state.set(Self::Running {
                    iters,
                    open,
                    mode,
                    padding,
                    keys,
                });

                // v. Let completion be Completion(Yield(results)).
                CoroutineState::Continue(results.unwrap_or_default())
            },
            Cell::new(Self::Running {
                iters,
                open,
                mode,
                padding,
                keys,
            }),
        )
    }

    /// The `mode` is "strict" branch of `IteratorZip`, run when the iterator at `index` is done.
    fn finish_strict(
        iters: &mut [IteratorRecord],
        open: &mut [bool],
        index: usize,
        context: &mut Context,
    ) -> JsResult<()> {
        // i. If i ≠ 0, then
        if index != 0 {
            // i. Return ? IteratorCloseAll(openIters, ThrowCompletion(a newly created TypeError object)).
            return iterator_close_all(
                iters,
                open,
                Err(js_error!(TypeError: "Iterator.zip: iterators have different lengths")),
                context,
            )
            .map(|_| ());
        }

        // ii. For each integer k such that 1 ≤ k < iterCount, in ascending order, do
        for k in 1..iters.len() {
            // i. Let open be Completion(IteratorStep(iters[k])).
            match iters[k].step(context) {
                // ii. If open is an abrupt completion, then
                Err(err) => {
                    // i. Remove iters[k] from openIters.
                    open[k] = false;
                    // ii. Return ? IteratorCloseAll(openIters, open).
                    return iterator_close_all(iters, open, Err(err), context).map(|_| ());
                }
                // iv. If open is done, then
                //     i. Remove iters[k] from openIters.
                Ok(true) => open[k] = false,
                // v. Else,
                Ok(false) => {
                    // i. Return ? IteratorCloseAll(openIters, ThrowCompletion(a newly created TypeError object)).
                    return iterator_close_all(
                        iters,
                        open,
                        Err(js_error!(TypeError: "Iterator.zip: iterators have different lengths")),
                        context,
                    )
                    .map(|_| ());
                }
            }
        }

        // iii. Return ReturnCompletion(undefined).
        Ok(())
    }

    /// The `finishResults` closures of `Iterator.zip` and `Iterator.zipKeyed`.
    fn finish_results(
        results: Vec<JsValue>,
        keys: Option<&[JsValue]>,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        let Some(keys) = keys else {
            // a. Return CreateArrayFromList(results).
            return Ok(Array::create_array_from_list(results, context).into());
        };

        // a. Let obj be OrdinaryObjectCreate(null).
        let obj = JsObject::with_null_proto();
        // b. For each integer i such that 0 ≤ i < iterCount, in ascending order, do
        for (key, value) in keys.iter().zip(results) {
            // i. Perform ! CreateDataPropertyOrThrow(obj, keys[i], results[i]).
            let key = key.to_property_key(context)?;
            obj.create_data_property_or_throw(key, value, context)?;
        }
        // c. Return obj.
        Ok(obj.into())
    }
}
